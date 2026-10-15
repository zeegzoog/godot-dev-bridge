mod common;

use common::{FakeGame, fail, ok};
use godot_dev_bridge::config::Context;
use godot_dev_bridge::run;
use godot_dev_bridge::session::{SessionRecord, session_directory, write_record};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

fn context(dir: &Path) -> Context {
    Context {
        root: dir.join("sessions"),
        cwd: dir.to_path_buf(),
        ..Context::default()
    }
}

fn now() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs_f64()
}

fn session(ctx: &Context, name: &str, pid: u32) -> PathBuf {
    let directory = session_directory(&ctx.root, name);
    std::fs::create_dir_all(&directory).unwrap();
    write_record(
        &directory,
        &SessionRecord {
            pid,
            command: vec![],
            started: now(),
        },
    )
    .unwrap();
    directory
}

fn drive(ctx: &Context, args: &[&str]) -> (i32, Value) {
    let mut argv = vec!["godot-dev-bridge".to_string()];
    argv.extend(args.iter().map(|word| word.to_string()));
    let (status, output) = run(argv, ctx);
    (status, output.expect("an output object"))
}

#[test]
fn a_command_gets_the_games_answer_and_exit_status_zero() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = context(dir.path());
    let directory = session(&ctx, "default", std::process::id());
    let _game = FakeGame::start(
        &directory,
        Box::new(|command, args| ok(json!({"command": command, "args": args}))),
    );
    let (status, answer) = drive(&ctx, &["step", "5"]);
    assert_eq!(status, 0);
    assert_eq!(answer["ok"], true);
    assert_eq!(answer["result"]["command"], "step");
    assert_eq!(answer["result"]["args"], json!({"ticks": 5}));
}

#[test]
fn an_error_answer_from_the_game_is_exit_status_one() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = context(dir.path());
    let directory = session(&ctx, "default", std::process::id());
    let _game = FakeGame::start(&directory, Box::new(|_, _| fail("not_found", "No node")));
    let (status, answer) = drive(&ctx, &["get", "/root/Nope"]);
    assert_eq!(status, 1);
    assert_eq!(answer["error"]["code"], "not_found");
}

#[test]
fn a_missing_session_is_exit_status_two_with_a_client_error() {
    let dir = tempfile::tempdir().unwrap();
    let (status, answer) = drive(&context(dir.path()), &["ping"]);
    assert_eq!(status, 2);
    assert_eq!(answer["ok"], false);
    assert_eq!(answer["error"]["code"], "client");
    assert!(
        answer["error"]["message"]
            .as_str()
            .unwrap()
            .contains("no session named 'default'")
    );
}

#[test]
fn a_dead_game_is_exit_status_two() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = context(dir.path());
    session(&ctx, "default", u32::MAX - 1);
    let (status, answer) = drive(&ctx, &["ping"]);
    assert_eq!(status, 2);
    assert!(
        answer["error"]["message"]
            .as_str()
            .unwrap()
            .contains("is not running")
    );
}

#[test]
fn no_answer_in_time_is_exit_status_two() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = context(dir.path());
    session(&ctx, "default", std::process::id());
    let (status, answer) = drive(&ctx, &["--timeout", "0.3", "ping"]);
    assert_eq!(status, 2);
    assert!(
        answer["error"]["message"]
            .as_str()
            .unwrap()
            .contains("no answer to 'ping'")
    );
}

#[test]
fn the_session_comes_from_the_flag_then_the_environment() {
    let dir = tempfile::tempdir().unwrap();
    let mut ctx = context(dir.path());
    ctx.session_env = Some("from-env".into());
    let directory = session(&ctx, "from-env", std::process::id());
    let _game = FakeGame::start(&directory, Box::new(|_, _| ok(json!({"pong": true}))));
    assert_eq!(drive(&ctx, &["ping"]).0, 0);
    assert_eq!(drive(&ctx, &["--session", "other", "ping"]).0, 2);
}

#[test]
fn sessions_lists_what_is_there() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = context(dir.path());
    session(&ctx, "alpha", std::process::id());
    let (status, answer) = drive(&ctx, &["sessions"]);
    assert_eq!(status, 0);
    assert_eq!(answer["sessions"][0]["session"], "alpha");
    assert_eq!(answer["sessions"][0]["alive"], true);
}

#[test]
fn a_non_positive_timeout_is_a_usage_error() {
    let dir = tempfile::tempdir().unwrap();
    let (status, answer) = drive(&context(dir.path()), &["--timeout", "0", "ping"]);
    assert_eq!(status, 2);
    assert!(
        answer["error"]["message"]
            .as_str()
            .unwrap()
            .contains("positive")
    );
}

#[test]
fn usage_errors_are_status_two_with_the_usual_client_error_object() {
    let dir = tempfile::tempdir().unwrap();
    for (words, expected) in [
        (vec!["nonsense"], "unrecognized subcommand 'nonsense'"),
        (vec![], "no command given"),
        (vec!["step", "3", "--max", "1"], "--until"),
        (vec!["eval"], "required arguments"),
    ] {
        let mut argv = vec!["godot-dev-bridge".to_string()];
        argv.extend(words.iter().map(|word| word.to_string()));
        let (status, output) = run(argv, &context(dir.path()));
        assert_eq!(status, 2, "{words:?}");
        let output = output.unwrap_or_else(|| panic!("{words:?} printed no JSON object"));
        assert_eq!(output["ok"], false, "{words:?}");
        assert_eq!(output["error"]["code"], "client", "{words:?}");
        let message = output["error"]["message"].as_str().unwrap();
        assert!(message.contains(expected), "{words:?}: {message}");
        assert!(message.contains("--help"), "{words:?}: {message}");
    }
}

#[test]
fn help_and_version_are_a_success_with_no_json_object() {
    let dir = tempfile::tempdir().unwrap();
    for flag in ["--help", "--version"] {
        let (status, output) = run(
            vec!["godot-dev-bridge".into(), flag.into()],
            &context(dir.path()),
        );
        assert_eq!((status, output), (0, None), "{flag}");
    }
}

#[test]
fn a_timeout_that_does_not_fit_a_clock_is_a_usage_error_not_a_panic() {
    let dir = tempfile::tempdir().unwrap();
    for seconds in ["1e30", "1e300", "40000000"] {
        let (status, answer) = drive(&context(dir.path()), &["--timeout", seconds, "ping"]);
        assert_eq!(status, 2, "{seconds}");
        assert!(
            answer["error"]["message"]
                .as_str()
                .unwrap()
                .contains("at most"),
            "{seconds}: {answer}"
        );
    }
}

#[test]
fn start_without_a_godot_executable_names_the_places_it_looked() {
    let dir = tempfile::tempdir().unwrap();
    let (status, answer) = drive(&context(dir.path()), &["start", "--headless"]);
    assert_eq!(status, 2);
    assert!(
        answer["error"]["message"]
            .as_str()
            .unwrap()
            .contains("no Godot executable")
    );
}

#[test]
fn start_refuses_to_run_over_a_live_session() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = context(dir.path());
    session(&ctx, "default", std::process::id());
    let (status, answer) = drive(&ctx, &["start"]);
    assert_eq!(status, 2);
    assert!(
        answer["error"]["message"]
            .as_str()
            .unwrap()
            .contains("already running")
    );
}

#[test]
fn stop_of_an_unknown_session_is_a_client_error() {
    let dir = tempfile::tempdir().unwrap();
    let (status, answer) = drive(&context(dir.path()), &["stop"]);
    assert_eq!(status, 2);
    assert!(
        answer["error"]["message"]
            .as_str()
            .unwrap()
            .contains("no session named")
    );
}

#[test]
fn a_session_name_that_climbs_out_of_the_root_is_refused_and_deletes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = context(dir.path());
    std::fs::create_dir_all(&ctx.root).unwrap();
    // Next to the spool root: what `--session ..` would have cleared out.
    let victim = dir.path().join("victim.txt");
    std::fs::write(&victim, "keep me").unwrap();

    for name in ["..", ".", "../elsewhere"] {
        let (status, answer) = drive(&ctx, &["--session", name, "start", "--headless"]);
        assert_eq!(status, 2, "{name}: {answer}");
        assert!(
            answer["error"]["message"]
                .as_str()
                .unwrap()
                .contains("session name"),
            "{name}: {answer}"
        );
        let (status, _) = drive(&ctx, &["--session", name, "stop"]);
        assert_eq!(status, 2, "{name}");
    }
    assert!(victim.exists(), "a file outside the spool root was deleted");

    let mut from_env = context(dir.path());
    from_env.session_env = Some("..".into());
    assert_eq!(
        drive(&from_env, &["ping"]).0,
        2,
        "DEV_BRIDGE_SESSION is checked too"
    );
    assert!(victim.exists());
}

#[test]
fn start_refuses_a_project_that_was_never_imported_before_starting_anything() {
    let dir = tempfile::tempdir().unwrap();
    let mut ctx = context(dir.path());
    // A file that exists is all the client needs of "Godot" to get as far as the import check.
    let godot = dir.path().join("fake-godot");
    std::fs::write(&godot, "").unwrap();
    ctx.godot_env.godot_path = Some(godot.to_string_lossy().into_owned());
    let project = dir.path().join("game");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(
        project.join("project.godot"),
        "config_version=5
",
    )
    .unwrap();

    let (status, answer) = drive(&ctx, &["start", "--headless", "--project", "game"]);
    assert_eq!(status, 2, "{answer}");
    let message = answer["error"]["message"].as_str().unwrap();
    assert!(
        message.contains("has not been imported") && message.contains("--import"),
        "{message}"
    );
    assert!(
        message.contains(&project.to_string_lossy().into_owned()),
        "the command names the project: {message}"
    );
    assert!(
        !ctx.root.join("default").exists(),
        "nothing was started or created"
    );
}
