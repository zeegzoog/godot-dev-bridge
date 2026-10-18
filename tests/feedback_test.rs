//! The feedback channel: notes, drafts, evidence, modes and submission through a stand-in `gh`.

mod common;

use godot_dev_bridge::config::{Context, FeedbackMode};
use godot_dev_bridge::feedback::{self, issues, notebook};
use godot_dev_bridge::run;
use godot_dev_bridge::session::{SessionRecord, session_directory, write_record};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime};

fn context(dir: &Path) -> Context {
    let work = dir.join("work");
    std::fs::create_dir_all(&work).unwrap();
    Context {
        root: dir.join("spool"),
        cwd: work,
        ..Context::default()
    }
}

fn drive(ctx: &Context, args: &[&str]) -> (i32, Value) {
    let mut argv = vec!["godot-dev-bridge".to_string()];
    argv.extend(args.iter().map(|word| word.to_string()));
    let (status, output) = run(argv, ctx);
    (status, output.expect("an output object"))
}

fn message(answer: &Value) -> String {
    answer["error"]["message"]
        .as_str()
        .unwrap_or("")
        .to_string()
}

fn note(ctx: &Context, args: &[&str]) -> String {
    let mut all = vec![
        "feedback",
        "note",
        "--kind",
        "bug",
        "--summary",
        "step hangs",
        "--details",
        "ran step 5, nothing came back",
    ];
    all.extend_from_slice(args);
    let (status, answer) = drive(ctx, &all);
    assert_eq!(status, 0, "{answer}");
    answer["note"].as_str().unwrap().to_string()
}

fn feedback_dir(ctx: &Context) -> PathBuf {
    ctx.root.join(notebook::DIR_NAME)
}

fn files_with(dir: &Path, prefix: &str) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(|entry| entry.ok().map(|entry| entry.path()))
                .filter(|path| {
                    path.file_name()
                        .is_some_and(|name| name.to_string_lossy().starts_with(prefix))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn age_file(path: &Path, by: Duration) {
    let file = std::fs::OpenOptions::new().write(true).open(path).unwrap();
    file.set_modified(SystemTime::now() - by).unwrap();
}

fn now() -> f64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs_f64()
}

#[test]
fn a_note_is_recorded_and_listed_as_not_drafted() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = context(dir.path());
    let id = note(&ctx, &["--verb", "step", "--impact", "blocked"]);
    assert!(id.starts_with('n') && id.len() == 7, "{id}");

    let (status, listed) = drive(&ctx, &["feedback", "list"]);
    assert_eq!(status, 0);
    assert_eq!(listed["notes"][0]["id"], json!(id));
    assert_eq!(listed["notes"][0]["drafted"], false);
    assert_eq!(listed["drafts"], json!([]));
}

#[test]
fn notes_over_a_limit_are_refused_with_the_limit_named() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = context(dir.path());
    let long = "x".repeat(121);
    let (status, answer) = drive(
        &ctx,
        &[
            "feedback",
            "note",
            "--kind",
            "bug",
            "--summary",
            &long,
            "--details",
            "d",
        ],
    );
    assert_eq!(status, 2);
    assert!(message(&answer).contains("120"), "{answer}");

    let (status, answer) = drive(
        &ctx,
        &[
            "feedback",
            "note",
            "--kind",
            "bug",
            "--summary",
            "s",
            "--details",
            " ",
        ],
    );
    assert_eq!(status, 2, "{answer}");

    let (status, answer) = drive(
        &ctx,
        &[
            "feedback",
            "note",
            "--kind",
            "bug",
            "--summary",
            "s",
            "--details",
            "d",
            "--verb",
            "screenshot",
        ],
    );
    assert_eq!(status, 2);
    assert!(message(&answer).contains("not a verb"), "{answer}");

    for _ in 0..notebook::MAX_NOTES {
        note(&ctx, &[]);
    }
    let (status, answer) = drive(
        &ctx,
        &[
            "feedback",
            "note",
            "--kind",
            "bug",
            "--summary",
            "s",
            "--details",
            "d",
        ],
    );
    assert_eq!(status, 2);
    assert!(message(&answer).contains("20 notes"), "{answer}");
}

#[test]
fn two_session_names_keep_separate_notebooks() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = context(dir.path());
    let first = note(&ctx, &["--session", "one"]);
    let second = note(&ctx, &["--session", "two"]);
    let (_, one) = drive(&ctx, &["--session", "one", "feedback", "list"]);
    let (_, two) = drive(&ctx, &["feedback", "list", "--session", "two"]);
    assert_eq!(one["notes"].as_array().unwrap().len(), 1);
    assert_eq!(one["notes"][0]["id"], json!(first));
    assert_eq!(two["notes"][0]["id"], json!(second));
}

#[test]
fn entries_unchanged_for_a_week_expire() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = context(dir.path());
    note(&ctx, &[]);
    let file = files_with(&feedback_dir(&ctx), "note-").remove(0);
    age_file(&file, notebook::LIFETIME + Duration::from_secs(60));
    let (_, listed) = drive(&ctx, &["feedback", "list"]);
    assert_eq!(listed["notes"], json!([]));
    assert!(!file.exists());
}

#[test]
fn discard_removes_named_entries_or_the_whole_notebook_and_refuses_unknown_ids() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = context(dir.path());
    let keep = note(&ctx, &[]);
    let drop = note(&ctx, &[]);
    let other = note(&ctx, &["--session", "other"]);

    let (status, answer) = drive(&ctx, &["feedback", "discard", "nzzzzzz"]);
    assert_eq!(status, 2);
    assert!(message(&answer).contains(&keep), "{answer}");
    assert!(!message(&answer).contains(&other), "{answer}");

    let (status, answer) = drive(&ctx, &["feedback", "discard", &drop]);
    assert_eq!((status, answer["removed"].clone()), (0, json!([drop])));

    let (status, answer) = drive(&ctx, &["feedback", "discard", "--all"]);
    assert_eq!((status, answer["removed"].clone()), (0, json!([keep])));
    let (_, listed) = drive(&ctx, &["--session", "other", "feedback", "list"]);
    assert_eq!(
        listed["notes"][0]["id"],
        json!(other),
        "another notebook is untouched"
    );
}

#[test]
fn a_notebook_directory_that_is_a_link_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = context(dir.path());
    let elsewhere = dir.path().join("elsewhere");
    std::fs::create_dir_all(&elsewhere).unwrap();
    std::fs::create_dir_all(&ctx.root).unwrap();
    link_directory(&feedback_dir(&ctx), &elsewhere);
    let (status, answer) = drive(
        &ctx,
        &[
            "feedback",
            "note",
            "--kind",
            "bug",
            "--summary",
            "s",
            "--details",
            "d",
        ],
    );
    assert_eq!(status, 2);
    assert!(message(&answer).contains("link"), "{answer}");
    assert_eq!(std::fs::read_dir(&elsewhere).unwrap().count(), 0);
}

/// Makes `link` a directory link to `target` (a symbolic link on Unix, a junction on Windows).
fn link_directory(link: &Path, target: &Path) {
    #[cfg(unix)]
    std::os::unix::fs::symlink(target, link).unwrap();
    #[cfg(windows)]
    {
        let status = Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(link)
            .arg(target)
            .stdout(std::process::Stdio::null())
            .status()
            .unwrap();
        assert!(status.success(), "mklink /J failed");
    }
}

#[test]
fn sessions_and_prune_pass_over_the_notebook_and_no_session_can_take_its_name() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = context(dir.path());
    note(&ctx, &[]);
    let (status, listed) = drive(&ctx, &["sessions", "--prune"]);
    assert_eq!(status, 0);
    assert_eq!(listed["sessions"], json!([]));
    assert_eq!(listed["pruned"], json!([]));
    assert_eq!(files_with(&feedback_dir(&ctx), "note-").len(), 1);

    let (status, answer) = drive(&ctx, &["--session", notebook::DIR_NAME, "stop"]);
    assert_eq!(status, 2);
    assert!(
        message(&answer).contains("invalid session name"),
        "{answer}"
    );
}

fn events(ctx: &Context) -> Vec<Value> {
    files_with(&feedback_dir(ctx), "evidence-")
        .iter()
        .map(|path| serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap())
        .collect()
}

#[test]
fn a_failing_core_verb_is_counted_once_per_distinct_failure() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = context(dir.path());
    assert_eq!(drive(&ctx, &["status"]).0, 2);
    assert_eq!(drive(&ctx, &["status"]).0, 2);
    let recorded = events(&ctx);
    assert_eq!(recorded.len(), 1, "{recorded:?}");
    let event = &recorded[0];
    assert_eq!(event["kind"], "failure");
    assert_eq!(event["verb"], "status");
    assert_eq!(event["code"], "client");
    assert_eq!(event["count"], 2);
    let text = event["message"].as_str().unwrap();
    assert!(
        text.contains("'{session}'") && text.contains("{session}"),
        "{text}"
    );
    assert!(
        !text.contains(&dir.path().to_string_lossy().to_string()),
        "{text}"
    );
}

#[test]
fn attempts_at_verbs_and_options_that_do_not_exist_are_recorded() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = context(dir.path());
    assert_eq!(drive(&ctx, &["screenshot", "--full"]).0, 2);
    assert_eq!(
        drive(&ctx, &["--session", "x", "step", "--seconds", "2"]).0,
        2
    );
    let mut recorded = events(&ctx);
    recorded.sort_by_key(|event| event["kind"].as_str().unwrap().to_string());
    assert_eq!(recorded.len(), 2, "{recorded:?}");
    assert_eq!(recorded[0]["kind"], "unknown_option");
    assert_eq!(recorded[0]["verb"], "step");
    assert_eq!(recorded[0]["message"], "--seconds");
    assert_eq!(recorded[0]["session"], "x");
    assert_eq!(recorded[1]["kind"], "unknown_verb");
    assert_eq!(recorded[1]["message"], "screenshot");
}

#[test]
fn project_commands_successes_help_and_version_record_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = context(dir.path());
    drive(&ctx, &["do", "spawn_enemy"]);
    drive(&ctx, &["state", "player"]);
    drive(&ctx, &["raw", r#"{"cmd": "heal"}"#]);
    drive(&ctx, &["feedback", "draft"]);
    drive(&ctx, &["sessions"]);
    let _ = run(vec!["godot-dev-bridge".into(), "--help".into()], &ctx);
    let _ = run(vec!["godot-dev-bridge".into(), "--version".into()], &ctx);
    assert_eq!(events(&ctx), Vec::<Value>::new());
}

fn dead_session(ctx: &Context, name: &str, log: &str) -> PathBuf {
    let directory = session_directory(&ctx.root, name);
    std::fs::create_dir_all(&directory).unwrap();
    write_record(
        &directory,
        &SessionRecord {
            pid: 0,
            command: vec![],
            started: now(),
        },
    )
    .unwrap();
    std::fs::write(directory.join("game.log"), log).unwrap();
    directory
}

#[test]
fn only_the_first_line_of_a_message_is_kept_so_the_log_tail_never_is() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = context(dir.path());
    dead_session(&ctx, "default", "PLAYER_SECRET_LINE\n");
    let (status, answer) = drive(&ctx, &["ping"]);
    assert_eq!(status, 2);
    assert!(message(&answer).contains("PLAYER_SECRET_LINE"), "{answer}");
    let recorded = events(&ctx);
    assert_eq!(recorded.len(), 1);
    assert!(
        !recorded[0]["message"]
            .as_str()
            .unwrap()
            .contains("PLAYER_SECRET_LINE")
    );
}

#[test]
fn recording_changes_neither_the_output_nor_the_exit_status() {
    let dir = tempfile::tempdir().unwrap();
    let full = context(dir.path());
    let off = Context {
        feedback: FeedbackMode::Off,
        root: dir.path().join("spool-off"),
        ..full.clone()
    };
    for args in [
        vec!["status"],
        vec!["screenshot"],
        vec!["step", "--seconds", "1"],
    ] {
        let a = drive(&full, &args);
        let b = drive(&off, &args);
        assert_eq!(a.0, b.0, "{args:?}");
        assert_eq!(
            message(&a.1).replace(&full.root.to_string_lossy().to_string(), "R"),
            message(&b.1).replace(&off.root.to_string_lossy().to_string(), "R"),
            "{args:?}"
        );
    }
    assert!(!events(&full).is_empty());
    assert!(!feedback_dir(&off).exists(), "off records nothing");
}

#[test]
fn an_unwritable_root_does_not_change_the_answer() {
    let dir = tempfile::tempdir().unwrap();
    let mut ctx = context(dir.path());
    let blocker = dir.path().join("a-file");
    std::fs::write(&blocker, "").unwrap();
    ctx.root = blocker.join("spool");
    let (status, answer) = drive(&ctx, &["status"]);
    assert_eq!(status, 2);
    assert!(message(&answer).contains("no session named"), "{answer}");
}

/// A session directory as a finished game leaves it, with a project that has the addon installed.
fn reported_session(ctx: &Context, dir: &Path) -> PathBuf {
    let project = dir.join("Rocket Game");
    std::fs::create_dir_all(project.join("addons/godot_dev_bridge")).unwrap();
    std::fs::write(project.join("project.godot"), "").unwrap();
    std::fs::write(
        project.join("addons/godot_dev_bridge/plugin.cfg"),
        "[plugin]\nname=\"x\"\nversion=\"9.8.7\"\n",
    )
    .unwrap();
    let directory = dead_session(ctx, "rocket", "");
    std::fs::write(
        directory.join("ready.json"),
        json!({
            "protocol": 1, "project": project.to_string_lossy(), "godot_version": "4.5.1.stable",
            "headless": true, "deterministic": false,
            "adapter": {"configured": true, "path": "res://tools/rocket_adapter.gd", "loaded": false,
                        "error": "Parse error in rocket_adapter.gd"}
        })
        .to_string(),
    )
    .unwrap();
    project
}

#[test]
fn a_draft_is_scrubbed_labelled_and_quotes_the_named_verbs_failures() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = context(dir.path());
    let project = reported_session(&ctx, dir.path());
    let session = ["--session", "rocket"];
    drive(&ctx, &[&session[..], &["ping"]].concat());
    drive(&ctx, &[&session[..], &["status"]].concat());
    let details = format!(
        "ran in {} with token ghp_abcdefghijklmn, see @maintainer and #12; ``` fence; also /home/ann/notes.txt",
        project.join("main.gd").display()
    );
    let (status, noted) = drive(
        &ctx,
        &[
            &session[..],
            &[
                "feedback",
                "note",
                "--kind",
                "bug",
                "--verb",
                "ping",
                "--summary",
                "ping fails after the game ends",
                "--details",
                &details,
            ],
        ]
        .concat(),
    );
    assert_eq!(status, 0, "{noted}");

    let (status, drafted) = drive(&ctx, &[&session[..], &["feedback", "draft"]].concat());
    assert_eq!(status, 0, "{drafted}");
    let report = drafted["report"].as_str().unwrap();
    for absent in [
        "Rocket Game",
        "ghp_abcdefghijklmn",
        "/home/ann",
        "@maintainer",
        "rocket_adapter",
        "'rocket'",
    ] {
        assert!(!report.contains(absent), "{absent} leaked:\n{report}");
    }
    for present in [
        "{project}",
        "{redacted}",
        "{path}",
        "(written by the agent)",
        "(observed by the client)",
        "````text",
        "`ping` failed with `client`",
        "9.8.7",
        "4.5.1.stable",
        "configured, not loaded",
        "headless",
        "Fingerprint: gdbfb",
    ] {
        assert!(report.contains(present), "{present} missing:\n{report}");
    }
    assert!(
        !report.contains("`status` failed"),
        "only the named verb's failures:\n{report}"
    );
    assert!(
        drafted["title"]
            .as_str()
            .unwrap()
            .starts_with("[agent-feedback] bug: ping fails")
    );
    assert!(drafted["link"].as_str().unwrap().len() <= 6_000);
    assert_eq!(drafted["delivery"]["copy"], true);
    assert_eq!(drafted["delivery"]["link"], true);
    assert!(
        drafted["next"]
            .as_str()
            .unwrap()
            .contains("feedback submit")
    );
}

#[test]
fn a_missing_feature_draft_quotes_the_attempts_and_a_bug_draft_does_not() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = context(dir.path());
    drive(&ctx, &["screenshot"]);
    note(&ctx, &[]);
    let (_, bug) = drive(&ctx, &["feedback", "draft"]);
    assert!(!bug["report"].as_str().unwrap().contains("screenshot"));
    drive(
        &ctx,
        &[
            "feedback",
            "note",
            "--kind",
            "missing-feature",
            "--summary",
            "no screenshot verb",
            "--details",
            "wanted a picture",
        ],
    );
    let (_, wish) = drive(&ctx, &["feedback", "draft"]);
    assert!(
        wish["report"]
            .as_str()
            .unwrap()
            .contains("verb `screenshot`, which does not exist"),
        "{wish}"
    );
    assert_ne!(bug["fingerprint"], wish["fingerprint"]);
}

#[test]
fn a_draft_takes_the_undrafted_notes_or_the_named_ones() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = context(dir.path());
    let (status, answer) = drive(&ctx, &["feedback", "draft"]);
    assert_eq!(status, 2);
    assert!(message(&answer).contains("feedback note"), "{answer}");

    let first = note(&ctx, &[]);
    let second = note(&ctx, &[]);
    let (_, one) = drive(&ctx, &["feedback", "draft", "--notes", &first]);
    assert_eq!(one["left_out"], json!([second]));
    let (_, rest) = drive(&ctx, &["feedback", "draft"]);
    assert_eq!(rest["left_out"], json!([]));
    let (_, listed) = drive(&ctx, &["feedback", "list"]);
    assert_eq!(listed["drafts"][1]["notes"], json!([second]));

    let (status, again) = drive(
        &ctx,
        &["feedback", "draft", "--notes", &format!("{first},{second}")],
    );
    assert_eq!(status, 0, "drafted notes may be drafted again: {again}");
    let (status, answer) = drive(&ctx, &["feedback", "draft", "--notes", "nzzzzzz"]);
    assert_eq!(status, 2);
    assert!(message(&answer).contains(&first), "{answer}");
}

#[test]
fn draft_mode_never_offers_submission() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = Context {
        feedback: FeedbackMode::Draft,
        ..context(dir.path())
    };
    note(&ctx, &[]);
    let (_, drafted) = drive(&ctx, &["feedback", "draft"]);
    assert_eq!(drafted["delivery"]["submit"], false);
    assert!(
        !drafted["next"].as_str().unwrap().contains("submit"),
        "{drafted}"
    );
    let id = drafted["draft"].as_str().unwrap();
    let (status, answer) = drive(&ctx, &["feedback", "submit", id, "--confirmed"]);
    assert_eq!(status, 2);
    assert!(message(&answer).contains("draft mode"), "{answer}");

    let mut command = godot_dev_bridge::cli::command(FeedbackMode::Draft);
    let help = command
        .find_subcommand_mut("feedback")
        .unwrap()
        .render_long_help()
        .to_string();
    assert!(!help.to_lowercase().contains("submit"), "{help}");
}

#[test]
fn off_mode_refuses_every_feedback_command_and_adds_no_hint() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = Context {
        feedback: FeedbackMode::Off,
        ..context(dir.path())
    };
    for args in [
        vec![
            "feedback",
            "note",
            "--kind",
            "bug",
            "--summary",
            "s",
            "--details",
            "d",
        ],
        vec!["feedback", "list"],
        vec!["feedback", "draft"],
        vec!["feedback", "submit", "d000000", "--confirmed"],
        vec!["feedback", "discard", "--all"],
    ] {
        let (status, answer) = drive(&ctx, &args);
        assert_eq!(status, 2, "{args:?}");
        assert!(
            message(&answer).contains("DEV_BRIDGE_FEEDBACK=off"),
            "{answer}"
        );
    }
    assert!(!feedback_dir(&ctx).exists());
    assert_eq!(feedback::stop_hint(&ctx, "default"), None);
}

#[test]
fn the_stop_hint_counts_the_notes_not_yet_drafted() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = context(dir.path());
    assert_eq!(feedback::stop_hint(&ctx, "default"), None);
    note(&ctx, &[]);
    let hint = feedback::stop_hint(&ctx, "default").expect("a hint with a pending note");
    assert_eq!(hint["pending"], 1);
    assert!(hint["next"].as_str().unwrap().contains("feedback draft"));
    assert_eq!(feedback::stop_hint(&ctx, "other"), None);
    drive(&ctx, &["feedback", "draft"]);
    assert_eq!(feedback::stop_hint(&ctx, "default"), None);
}

#[test]
fn submission_needs_the_confirmation_and_a_draft_of_this_notebook() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = context(dir.path());
    note(&ctx, &[]);
    let (_, drafted) = drive(&ctx, &["feedback", "draft"]);
    let id = drafted["draft"].as_str().unwrap().to_string();
    let (status, answer) = drive(&ctx, &["feedback", "submit", &id]);
    assert_eq!(status, 2);
    assert!(message(&answer).contains("--confirmed"), "{answer}");
    let (status, answer) = drive(&ctx, &["feedback", "submit", "dzzzzzz", "--confirmed"]);
    assert_eq!(status, 2);
    assert!(message(&answer).contains(&id), "{answer}");
}

#[test]
fn no_feedback_command_takes_a_destination_or_a_credential() {
    let command = godot_dev_bridge::cli::command(FeedbackMode::Full);
    let feedback = command.find_subcommand("feedback").unwrap();
    for sub in feedback.get_subcommands() {
        for argument in sub.get_arguments() {
            let id = argument.get_id().as_str().to_lowercase();
            for word in [
                "repo",
                "host",
                "url",
                "address",
                "token",
                "credential",
                "owner",
            ] {
                assert!(!id.contains(word), "{} --{id}", sub.get_name());
            }
        }
    }
}

#[test]
fn a_gh_that_does_not_answer_is_ended_at_its_deadline() {
    let fake = common::fake_gh();
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("behaviour.json"),
        json!({"list": {"sleep_ms": 20_000}}).to_string(),
    )
    .unwrap();
    // A copy of the stand-in reads the behaviour next to itself, so this test sets no environment;
    // the deadline is a parameter of the runner, so a short one keeps it quick.
    let copy = dir
        .path()
        .join(format!("gh{}", std::env::consts::EXE_SUFFIX));
    std::fs::copy(&fake, &copy).unwrap();
    let started = std::time::Instant::now();
    let output = issues::run_gh(
        &copy,
        &["issue".into(), "list".into()],
        "",
        Duration::from_millis(500),
    )
    .unwrap();
    assert_eq!(output.status, None);
    assert!(started.elapsed() < Duration::from_secs(10));
}

/// The built client with a spool, a stand-in `gh` and its behaviour of its own.
struct Shell {
    dir: tempfile::TempDir,
    gh: Option<PathBuf>,
}

impl Shell {
    fn new(behaviour: Value) -> Self {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("work")).unwrap();
        std::fs::create_dir_all(dir.path().join("gh")).unwrap();
        std::fs::write(dir.path().join("gh/behaviour.json"), behaviour.to_string()).unwrap();
        Self {
            dir,
            gh: Some(common::fake_gh()),
        }
    }

    fn run(&self, args: &[&str]) -> (i32, Value, String) {
        let mut command = Command::new(env!("CARGO_BIN_EXE_godot-dev-bridge"));
        command
            .args(["--session", "s"])
            .args(args)
            .current_dir(self.dir.path().join("work"))
            .env("DEV_BRIDGE_ROOT", self.dir.path().join("spool"))
            .env("FAKE_GH_DIR", self.dir.path().join("gh"))
            .env_remove("DEV_BRIDGE_FEEDBACK")
            .env_remove("DEV_BRIDGE_SESSION");
        match &self.gh {
            Some(gh) => command.env("DEV_BRIDGE_GH", gh),
            None => command.env("DEV_BRIDGE_GH", self.dir.path().join("no-such-gh")),
        };
        let (status, stdout, stderr) = common::run_client(command, Duration::from_secs(120));
        let value: Value = serde_json::from_str(stdout.trim())
            .unwrap_or_else(|error| panic!("one JSON object expected ({error}):\n{stdout}"));
        (status, value, stderr)
    }

    fn draft(&self) -> (String, Value) {
        let (status, noted, _) = self.run(&[
            "feedback",
            "note",
            "--kind",
            "bug",
            "--verb",
            "step",
            "--summary",
            "step hangs",
            "--details",
            "ran step 5",
        ]);
        assert_eq!(status, 0, "{noted}");
        let (status, drafted, _) = self.run(&["feedback", "draft"]);
        assert_eq!(status, 0, "{drafted}");
        (drafted["draft"].as_str().unwrap().to_string(), drafted)
    }

    fn calls(&self) -> Vec<Value> {
        let mut calls = Vec::new();
        for n in 1.. {
            let path = self.dir.path().join(format!("gh/call-{n}.json"));
            let Ok(text) = std::fs::read_to_string(path) else {
                break;
            };
            calls.push(serde_json::from_str(&text).unwrap());
        }
        calls
    }

    fn lock(&self, draft: &str) -> PathBuf {
        self.dir
            .path()
            .join("spool")
            .join(notebook::DIR_NAME)
            .join(format!("draft-{draft}.lock"))
    }
}

const ISSUE: &str = "https://github.com/zenzoog/godot-dev-bridge/issues/7";

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item.as_str().unwrap().to_string())
        .collect()
}

#[test]
fn a_submission_looks_for_a_duplicate_then_creates_the_issue_once() {
    let shell = Shell::new(json!({
        "list": {"stdout": "[]"},
        "create": {"stdout": format!("Creating issue in zenzoog/godot-dev-bridge\n\n{ISSUE}\n")}
    }));
    let (id, drafted) = shell.draft();
    let (status, answer, _) = shell.run(&["feedback", "submit", &id, "--confirmed"]);
    assert_eq!(status, 0, "{answer}");
    assert_eq!(answer["outcome"], "created");
    assert_eq!(answer["url"], ISSUE);
    assert!(answer["public"].as_str().unwrap().contains(ISSUE));

    let calls = shell.calls();
    assert_eq!(calls.len(), 2, "{calls:?}");
    let repository = issues::repository().unwrap();
    let fingerprint = drafted["fingerprint"].as_str().unwrap();
    assert_eq!(
        strings(&calls[0]["args"]),
        issues::lookup_args(&repository, fingerprint)
    );
    assert_eq!(
        strings(&calls[1]["args"]),
        issues::create_args(&repository, drafted["title"].as_str().unwrap())
    );
    assert_eq!(calls[1]["stdin"], drafted["report"]);
    assert!(!shell.lock(&id).exists());

    let (status, again, _) = shell.run(&["feedback", "submit", &id, "--confirmed"]);
    assert_eq!((status, again["outcome"].clone()), (0, json!("created")));
    assert_eq!(again["repeated"], true);
    assert_eq!(shell.calls().len(), 2, "a repeat starts no gh");
    let (_, listed, _) = shell.run(&["feedback", "list"]);
    assert_eq!(listed["drafts"][0]["outcome"], "created");
}

#[test]
fn an_issue_with_the_fingerprint_is_reported_instead_of_a_new_one_unless_asked() {
    let shell = Shell::new(json!({"create": {"stdout": ISSUE}}));
    let (id, drafted) = shell.draft();
    let fingerprint = drafted["fingerprint"].as_str().unwrap();
    std::fs::write(
        shell.dir.path().join("gh/behaviour.json"),
        json!({
            "list": {"stdout": json!([
                {"number": 3, "title": "x", "url": "https://github.com/zenzoog/godot-dev-bridge/issues/3", "body": "unrelated"},
                {"number": 5, "title": "y", "url": "https://github.com/zenzoog/godot-dev-bridge/issues/5", "body": format!("- Fingerprint: {fingerprint}")}
            ]).to_string()},
            "create": {"stdout": ISSUE}
        })
        .to_string(),
    )
    .unwrap();
    let (status, answer, _) = shell.run(&["feedback", "submit", &id, "--confirmed"]);
    assert_eq!(status, 0, "{answer}");
    assert_eq!(answer["outcome"], "duplicate");
    assert_eq!(
        answer["url"],
        "https://github.com/zenzoog/godot-dev-bridge/issues/5"
    );
    assert_eq!(shell.calls().len(), 1, "nothing was created");

    let (status, forced, _) = shell.run(&[
        "feedback",
        "submit",
        &id,
        "--confirmed",
        "--allow-duplicate",
    ]);
    assert_eq!(
        (status, forced["outcome"].clone()),
        (0, json!("created")),
        "{forced}"
    );
    let calls = shell.calls();
    assert_eq!(calls.len(), 2);
    assert_eq!(
        calls[1]["args"][1], "create",
        "no look-up when a duplicate is allowed"
    );
}

#[test]
fn a_failed_look_up_is_reported_and_does_not_block_creation() {
    let shell = Shell::new(json!({
        "list": {"exit": 1, "stderr": "search is down"},
        "create": {"stdout": ISSUE}
    }));
    let (id, _) = shell.draft();
    let (status, answer, _) = shell.run(&["feedback", "submit", &id, "--confirmed"]);
    assert_eq!(status, 0, "{answer}");
    assert_eq!(answer["outcome"], "created");
    assert_eq!(answer["lookup"], "failed: search is down");
}

#[test]
fn a_user_who_is_not_signed_in_is_told_how_and_the_draft_stays_pending() {
    let shell = Shell::new(json!({
        "list": {"stdout": "[]"},
        "create": {"exit": 4, "stderr": "To get started with GitHub CLI, please run:  gh auth login\n"}
    }));
    let (id, _) = shell.draft();
    let (status, answer, _) = shell.run(&["feedback", "submit", &id, "--confirmed"]);
    assert_eq!(status, 2, "{answer}");
    assert_eq!(answer["ok"], false);
    assert_eq!(answer["error"]["code"], "client");
    assert_eq!(answer["outcome"], "not_created");
    let steps = strings(&answer["steps"]);
    assert!(steps[0].contains("gh auth login"), "{steps:?}");
    assert!(
        steps.last().unwrap().contains("feedback draft"),
        "{steps:?}"
    );
    let (_, listed, _) = shell.run(&["feedback", "list"]);
    assert_eq!(listed["drafts"][0].get("outcome"), None);
}

#[test]
fn without_gh_the_report_is_not_created_and_the_text_path_is_named() {
    let mut shell = Shell::new(json!({}));
    shell.gh = None;
    let (id, drafted) = shell.draft();
    assert_eq!(drafted["delivery"]["submit"], false);
    let (status, answer, _) = shell.run(&["feedback", "submit", &id, "--confirmed"]);
    assert_eq!(status, 2);
    assert_eq!(answer["outcome"], "not_created");
    assert!(
        strings(&answer["steps"])[0].contains("cli.github.com"),
        "{answer}"
    );
}

#[test]
fn a_creation_that_names_no_issue_is_unknown_and_stays_so() {
    let shell = Shell::new(json!({"list": {"stdout": "[]"}, "create": {"stdout": "done\n"}}));
    let (id, _) = shell.draft();
    let (status, answer, _) = shell.run(&["feedback", "submit", &id, "--confirmed"]);
    assert_eq!(status, 2, "{answer}");
    assert_eq!(answer["outcome"], "unknown");
    assert!(
        message(&answer).contains("before submitting again"),
        "{answer}"
    );
    let (status, again, _) = shell.run(&["feedback", "submit", &id, "--confirmed"]);
    assert_eq!((status, again["outcome"].clone()), (2, json!("unknown")));
    assert_eq!(shell.calls().len(), 2, "a repeat starts no gh");
}

#[test]
fn a_fresh_lock_refuses_a_second_submission_and_a_stale_one_is_replaced() {
    let shell = Shell::new(json!({"list": {"stdout": "[]"}, "create": {"stdout": ISSUE}}));
    let (id, _) = shell.draft();
    let lock = shell.lock(&id);
    std::fs::write(&lock, "").unwrap();
    let (status, answer, _) = shell.run(&["feedback", "submit", &id, "--confirmed"]);
    assert_eq!(status, 2);
    assert!(message(&answer).contains("in progress"), "{answer}");
    assert!(shell.calls().is_empty());

    age_file(&lock, notebook::LOCK_STALE + Duration::from_secs(5));
    let (status, answer, _) = shell.run(&["feedback", "submit", &id, "--confirmed"]);
    assert_eq!(
        (status, answer["outcome"].clone()),
        (0, json!("created")),
        "{answer}"
    );
}

#[test]
fn the_clients_output_holds_its_object_and_nothing_from_gh() {
    let shell = Shell::new(json!({
        "list": {"stdout": "[]", "stderr": "warning from gh\n"},
        "create": {"stdout": format!("noise line\n{ISSUE}\n"), "stderr": "more noise\n"}
    }));
    let (id, _) = shell.draft();
    let (status, answer, stderr) = shell.run(&["feedback", "submit", &id, "--confirmed"]);
    assert_eq!(status, 0, "{answer}");
    assert!(
        !stderr.contains("noise") && !stderr.contains("warning from gh"),
        "{stderr}"
    );
}
