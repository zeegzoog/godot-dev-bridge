//! The agent feedback channel: an agent records what it found missing, broken or misleading in
//! godot-dev-bridge itself, drafts one scrubbed report per problem for the user to review, and,
//! after the user's yes, submits it as an issue through the user's own `gh`. Nothing here sends a
//! request to a game.

pub mod issues;
pub mod notebook;
pub mod report;
pub mod scrub;

use crate::cli::{FeedbackAction, NoteArgs, verb_names};
use crate::config::{Context, FeedbackMode, find_project, validate_session_name};
use crate::error::{ClientError, Result};
use crate::session;
use notebook::{Draft, Event, Note, Notebook};
use report::{Environment, Observed};
use scrub::{FingerprintBasis, Known, scrub, shape};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

pub const HELP_FULL: &str = "Report what is missing, broken or misleading in godot-dev-bridge itself to its maintainers.

While you work, as soon as a verb, its message, its help or the README blocks or misleads the task, or a capability you need has no verb, record one note: feedback note --kind KIND --summary TEXT --details TEXT [--verb VERB]. Not worth a note: your own mistakes, problems in the user's game, project or adapter, anything not observed in this work. Write it in English: what you tried, what came back, what you expected; one problem per note.

When the user's request is done or abandoned, stop reports pending notes, or the user asks: run feedback draft (one draft per problem), show the user the whole report in the conversation, and offer to copy it, open its link, submit it, or discard it. Never run feedback submit without the user's explicit yes in this conversation.";

pub const HELP_DRAFT: &str = "Report what is missing, broken or misleading in godot-dev-bridge itself to its maintainers.

While you work, as soon as a verb, its message, its help or the README blocks or misleads the task, or a capability you need has no verb, record one note: feedback note --kind KIND --summary TEXT --details TEXT [--verb VERB]. Not worth a note: your own mistakes, problems in the user's game, project or adapter, anything not observed in this work. Write it in English: what you tried, what came back, what you expected; one problem per note.

When the user's request is done or abandoned, stop reports pending notes, or the user asks: run feedback draft (one draft per problem), show the user the whole report in the conversation, and offer to copy it, open its link, or discard it.";

pub const HELP_OFF: &str = "Report what is missing, broken or misleading in godot-dev-bridge itself to its maintainers.

The feedback channel is off (DEV_BRIDGE_FEEDBACK=off): nothing is recorded and every feedback command is refused.";

/// Added to the answer of `start`.
pub const START_HINT: &str = "Something in godot-dev-bridge itself missing, broken or misleading? Record it as you go: godot-dev-bridge feedback note --help.";

const STOP_NEXT: &str =
    "Run godot-dev-bridge feedback draft and show the whole report to the user.";

const MAX_SUMMARY: usize = 120;
const MAX_DETAILS: usize = 2_000;
const MAX_SUGGESTION: usize = 500;
const MAX_EVENT_MESSAGE: usize = 200;
const MAX_NAME: usize = 32;
const FAILURES_PER_VERB: usize = 5;
const MAX_ATTEMPTS: usize = 10;

/// Verbs whose failures are not the bridge's to report: the channel itself, and the three that
/// carry the project's own command and state names.
const UNOBSERVED: [&str; 5] = ["feedback", "do", "state", "raw", "help"];

fn refused_when_off(ctx: &Context) -> Result<()> {
    if ctx.feedback == FeedbackMode::Off {
        return Err(ClientError::msg(
            "the feedback channel is off (DEV_BRIDGE_FEEDBACK=off); unset it, or set it to draft \
             or full, to use feedback",
        ));
    }
    Ok(())
}

fn ready(ctx: &Context, name: &str) -> Option<Value> {
    let path = session::session_directory(&ctx.root, name).join("ready.json");
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

/// The project of the notebook's session as its game reported it, else the one `start` would find.
fn project_dir(ctx: &Context, name: &str) -> Option<PathBuf> {
    ready(ctx, name)
        .and_then(|ready| ready.get("project")?.as_str().map(PathBuf::from))
        .filter(|path| path.is_absolute())
        .or_else(|| find_project(&ctx.cwd, None).ok())
}

/// What the scrubber must replace for this session.
pub fn known_for(ctx: &Context, name: &str) -> Known {
    let project = project_dir(ctx, name);
    Known::new(project.as_deref(), Some(&ctx.root), Some(name), ctx.windows)
}

fn limited(text: &str, limit: usize, what: &str) -> Result<()> {
    let length = text.chars().count();
    if length > limit {
        return Err(ClientError::msg(format!(
            "{what} has {length} characters; the limit is {limit}. Shorten it, or split the \
             problem into several notes"
        )));
    }
    Ok(())
}

fn note(ctx: &Context, name: &str, args: &NoteArgs) -> Result<Value> {
    let known = known_for(ctx, name);
    let summary = scrub(args.summary.trim(), &known);
    let details = scrub(args.details.trim(), &known);
    let suggestion = args
        .suggestion
        .as_deref()
        .map(|text| scrub(text.trim(), &known))
        .filter(|text| !text.is_empty());
    if summary.is_empty() || details.is_empty() {
        return Err(ClientError::msg(
            "--summary and --details must not be empty: say what you tried and what came back",
        ));
    }
    if summary.contains('\n') {
        return Err(ClientError::msg(
            "--summary must be one line; put the rest in --details",
        ));
    }
    limited(&summary, MAX_SUMMARY, "--summary")?;
    limited(&details, MAX_DETAILS, "--details")?;
    if let Some(suggestion) = &suggestion {
        limited(suggestion, MAX_SUGGESTION, "--suggestion")?;
    }
    if let Some(verb) = &args.verb {
        let verbs = verb_names();
        if !verbs.contains(verb) {
            return Err(ClientError::msg(format!(
                "--verb {verb} is not a verb of godot-dev-bridge; use one of {}, or leave it out \
                 when the problem is a verb that does not exist",
                verbs.join(", ")
            )));
        }
    }
    let notebook = Notebook::open(&ctx.root, name)?;
    let note = notebook.add_note(Note {
        id: String::new(),
        session: String::new(),
        created: 0.0,
        kind: args.kind.name().to_string(),
        summary,
        details,
        verb: args.verb.clone(),
        impact: args.impact.map(|impact| impact.name().to_string()),
        suggestion,
    })?;
    let pending: Vec<Value> = notebook
        .undrafted()
        .iter()
        .map(|note| json!({"id": note.id, "kind": note.kind, "summary": note.summary}))
        .collect();
    Ok(json!({"ok": true, "note": note.id, "pending": pending}))
}

fn list(ctx: &Context, name: &str) -> Result<Value> {
    let notebook = Notebook::open(&ctx.root, name)?;
    let drafts = notebook.drafts();
    let notes: Vec<Value> = notebook
        .notes()
        .iter()
        .map(|note| {
            let drafted = drafts.iter().any(|draft| draft.notes.contains(&note.id));
            json!({"id": note.id, "kind": note.kind, "summary": note.summary, "drafted": drafted})
        })
        .collect();
    let drafts: Vec<Value> = drafts
        .iter()
        .map(|draft| {
            let mut entry = json!({"id": draft.id, "title": draft.title, "notes": draft.notes});
            if let Some(outcome) = &draft.outcome {
                entry["outcome"] = json!(outcome);
            }
            if let Some(url) = &draft.url {
                entry["url"] = json!(url);
            }
            entry
        })
        .collect();
    Ok(json!({"ok": true, "notes": notes, "drafts": drafts}))
}

fn addon_version(project: &Path) -> Option<String> {
    let text = std::fs::read_to_string(project.join("addons/godot_dev_bridge/plugin.cfg")).ok()?;
    text.lines()
        .find_map(|line| line.trim().strip_prefix("version="))
        .map(|value| value.trim().trim_matches('"').to_string())
        .filter(|value| !value.is_empty())
}

fn yes_no(value: Option<bool>) -> String {
    match value {
        Some(true) => "yes".into(),
        Some(false) => "no".into(),
        None => "unknown".into(),
    }
}

/// The environment block: only the facts the report may carry.
fn environment(ctx: &Context, name: &str) -> Environment {
    let unknown = || "unknown".to_string();
    let ready = ready(ctx, name);
    let field = |key: &str| ready.as_ref().and_then(|ready| ready.get(key)).cloned();
    let adapter = field("adapter").map(|adapter| {
        let configured = adapter.get("configured").and_then(Value::as_bool);
        let loaded = adapter.get("loaded").and_then(Value::as_bool);
        match (configured, loaded) {
            (Some(false), _) => "not configured".to_string(),
            (Some(true), Some(true)) => "configured, loaded".to_string(),
            (Some(true), Some(false)) => "configured, not loaded".to_string(),
            _ => unknown(),
        }
    });
    let record = session::read_record(&session::session_directory(&ctx.root, name));
    vec![
        ("Client".into(), env!("CARGO_PKG_VERSION").to_string()),
        (
            "Addon".into(),
            project_dir(ctx, name)
                .and_then(|project| addon_version(&project))
                .unwrap_or_else(unknown),
        ),
        (
            "Godot".into(),
            field("godot_version")
                .map(|value| match value {
                    Value::String(text) => text,
                    other => other.to_string(),
                })
                .unwrap_or_else(unknown),
        ),
        (
            "Platform".into(),
            format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
        ),
        ("Shell rewrites paths".into(), yes_no(Some(ctx.msystem))),
        (
            "Session".into(),
            match field("headless").and_then(|value| value.as_bool()) {
                Some(true) => "headless".into(),
                Some(false) => "windowed".into(),
                None => unknown(),
            },
        ),
        (
            "Exact stepping".into(),
            yes_no(field("deterministic").and_then(|value| value.as_bool())),
        ),
        ("Adapter".into(), adapter.unwrap_or_else(unknown)),
        (
            "Game running".into(),
            yes_no(record.map(|record| session::game_alive(&record))),
        ),
        ("Feedback mode".into(), ctx.feedback.name().to_string()),
    ]
}

fn attempt_name(event: &Event) -> String {
    if event.kind == "unknown_option" {
        format!("{} {}", event.verb, event.message)
            .trim()
            .to_string()
    } else {
        event.message.clone()
    }
}

fn draft(ctx: &Context, name: &str, chosen: &[String], given_title: Option<&str>) -> Result<Value> {
    let notebook = Notebook::open(&ctx.root, name)?;
    let all = notebook.notes();
    let notes: Vec<Note> = if chosen.is_empty() {
        notebook.undrafted()
    } else {
        if let Some(unknown) = chosen
            .iter()
            .find(|id| !all.iter().any(|note| &note.id == *id))
        {
            let ids: Vec<&str> = all.iter().map(|note| note.id.as_str()).collect();
            return Err(ClientError::msg(format!(
                "no note '{unknown}' in this notebook; its notes: {}",
                if ids.is_empty() {
                    "none".to_string()
                } else {
                    ids.join(", ")
                }
            )));
        }
        all.iter()
            .filter(|note| chosen.contains(&note.id))
            .cloned()
            .collect()
    };
    if notes.is_empty() {
        return Err(ClientError::msg(
            "no note to draft: record one with `feedback note` first",
        ));
    }

    let kind = report::draft_kind(&notes);
    let verbs: Vec<String> = notes.iter().filter_map(|note| note.verb.clone()).collect();
    let mut events = notebook.events();
    events.sort_by(|a, b| b.last_seen.total_cmp(&a.last_seen));
    let mut failures: Vec<&Event> = Vec::new();
    let mut named: Vec<&String> = verbs.iter().collect();
    named.sort();
    named.dedup();
    for verb in named {
        failures.extend(
            events
                .iter()
                .filter(|event| event.kind == "failure" && &event.verb == verb)
                .take(FAILURES_PER_VERB),
        );
    }
    let attempts: Vec<&Event> = if kind == "missing-feature" {
        events
            .iter()
            .filter(|event| event.kind == "unknown_verb" || event.kind == "unknown_option")
            .take(MAX_ATTEMPTS)
            .collect()
    } else {
        Vec::new()
    };

    let fingerprint = scrub::fingerprint(FingerprintBasis {
        kind: &kind,
        verbs: verbs.clone(),
        shapes: failures.iter().map(|event| shape(&event.message)).collect(),
        attempts: attempts.iter().map(|event| attempt_name(event)).collect(),
        first_summary: &notes[0].summary,
    });
    let known = known_for(ctx, name);
    let title = report::title(&kind, &notes[0].summary, given_title, &known);
    let body = report::build_body(
        &notes,
        &Observed { failures, attempts },
        &environment(ctx, name),
        &fingerprint,
        &known,
    );
    let repository = issues::repository();
    let link = match &repository {
        Some(repository) => report::build_link(repository, &title, &body),
        None => Err("the package names no GitHub repository".to_string()),
    };
    let saved = notebook.add_draft(Draft {
        id: String::new(),
        session: String::new(),
        created: 0.0,
        notes: notes.iter().map(|note| note.id.clone()).collect(),
        title: title.clone(),
        body: body.clone(),
        fingerprint: fingerprint.clone(),
        link: link.clone().ok(),
        outcome: None,
        url: None,
    })?;

    let submit = ctx.feedback == FeedbackMode::Full && issues::find_gh(ctx).is_ok();
    let left_out: Vec<String> = notebook
        .undrafted()
        .into_iter()
        .map(|note| note.id)
        .collect();
    let mut offers = vec!["copy it".to_string()];
    if link.is_ok() {
        offers.push("open its link".into());
    }
    if ctx.feedback == FeedbackMode::Full {
        offers.push(format!(
            "submit it (`godot-dev-bridge feedback submit {} --confirmed`, only after the user's \
             explicit yes)",
            saved.id
        ));
    }
    offers.push(format!(
        "or discard it (`godot-dev-bridge feedback discard {}`)",
        saved.id
    ));
    let next = format!(
        "Show the user this whole report in the conversation, then offer to {}.",
        offers.join(", ")
    );
    let mut answer = json!({
        "ok": true,
        "draft": saved.id,
        "title": title,
        "fingerprint": fingerprint,
        "report": body,
        "delivery": {"copy": true, "link": link.is_ok(), "submit": submit},
        "left_out": left_out,
        "next": next,
    });
    match link {
        Ok(link) => answer["link"] = json!(link),
        Err(reason) => answer["link_reason"] = json!(reason),
    }
    Ok(answer)
}

fn not_created(
    draft: &str,
    reason: &str,
    steps: Vec<String>,
    lookup: Option<&str>,
) -> (i32, Value) {
    let mut answer = json!({
        "ok": false,
        "error": {"code": "client", "message": format!("not submitted: {reason}. {}", steps.join(" "))},
        "outcome": "not_created",
        "draft": draft,
        "steps": steps,
    });
    if let Some(lookup) = lookup {
        answer["lookup"] = json!(lookup);
    }
    (2, answer)
}

fn unknown_outcome(draft: &Draft, repository: &str) -> (i32, Value) {
    (
        2,
        json!({
            "ok": false,
            "error": {"code": "client", "message": format!(
                "gh ended without naming an issue it created; check https://github.com/{repository}/issues \
                 for {} before submitting again",
                draft.fingerprint
            )},
            "outcome": "unknown",
            "draft": draft.id,
        }),
    )
}

fn created(draft: &str, url: &str, lookup: Option<&str>, repeated: bool) -> (i32, Value) {
    let mut answer = json!({
        "ok": true,
        "outcome": "created",
        "draft": draft,
        "url": url,
        "public": format!("The report is now public at {url}."),
    });
    if repeated {
        answer["repeated"] = json!(true);
    }
    if let Some(lookup) = lookup {
        answer["lookup"] = json!(lookup);
    }
    (0, answer)
}

fn submit(
    ctx: &Context,
    name: &str,
    id: &str,
    confirmed: bool,
    allow_duplicate: bool,
) -> Result<(i32, Value)> {
    if ctx.feedback != FeedbackMode::Full {
        return Err(ClientError::msg(
            "submission is off in draft mode (DEV_BRIDGE_FEEDBACK=draft); give the user the \
             report text or the link from `feedback draft` to file it by hand",
        ));
    }
    if !confirmed {
        return Err(ClientError::msg(format!(
            "not submitted: show the user the whole report of {id}, ask whether to submit it to \
             the public issue tracker, and run this again with --confirmed only after an explicit yes"
        )));
    }
    let notebook = Notebook::open(&ctx.root, name)?;
    let drafts = notebook.drafts();
    let Some(mut draft) = drafts.iter().find(|draft| draft.id == id).cloned() else {
        let ids: Vec<&str> = drafts.iter().map(|draft| draft.id.as_str()).collect();
        return Err(ClientError::msg(format!(
            "no draft '{id}' in this notebook; its drafts: {}",
            if ids.is_empty() {
                "none (build one with `feedback draft`)".to_string()
            } else {
                ids.join(", ")
            }
        )));
    };
    let Some(repository) = issues::repository() else {
        return Ok(not_created(
            id,
            "the package names no GitHub repository",
            vec![issues::fallback_step()],
            None,
        ));
    };
    match (draft.outcome.as_deref(), draft.url.as_deref()) {
        (Some("created"), Some(url)) => return Ok(created(id, url, None, true)),
        (Some("unknown"), _) => return Ok(unknown_outcome(&draft, &repository)),
        _ => {}
    }
    let _lock = notebook.lock(id)?;
    // Read again under the lock: a submission that ended a moment ago may have recorded its outcome.
    if let Some(latest) = notebook.drafts().into_iter().find(|draft| draft.id == id) {
        draft = latest;
    }
    match (draft.outcome.as_deref(), draft.url.as_deref()) {
        (Some("created"), Some(url)) => return Ok(created(id, url, None, true)),
        (Some("unknown"), _) => return Ok(unknown_outcome(&draft, &repository)),
        _ => {}
    }
    let gh = match issues::find_gh(ctx) {
        Ok(gh) => gh,
        Err(reason) => {
            return Ok(not_created(
                id,
                &reason,
                vec![
                    "Install the GitHub CLI (https://cli.github.com) and sign in with `gh auth \
                     login`, or set DEV_BRIDGE_GH to its path."
                        .into(),
                    issues::fallback_step(),
                ],
                None,
            ));
        }
    };
    let known = known_for(ctx, name);
    let quoted = |text: &str| scrub(&issues::first_line(text), &known);

    let mut lookup: Option<String> = None;
    if !allow_duplicate {
        match issues::run_gh(
            &gh,
            &issues::lookup_args(&repository, &draft.fingerprint),
            "",
            issues::LOOKUP_DEADLINE,
        ) {
            Ok(output) if output.status == Some(0) => {
                match issues::duplicate_in(&output.stdout, &draft.fingerprint) {
                    Ok(Some(url)) => {
                        return Ok((
                            0,
                            json!({
                                "ok": true,
                                "outcome": "duplicate",
                                "draft": id,
                                "url": url,
                                "next": format!(
                                    "Show the user the existing issue. To file this report anyway, run \
                                     `godot-dev-bridge feedback submit {id} --confirmed --allow-duplicate` \
                                     after the user's yes."
                                ),
                            }),
                        ));
                    }
                    Ok(None) => {}
                    Err(reason) => lookup = Some(format!("failed: {reason}")),
                }
            }
            Ok(output) if output.status.is_none() => {
                lookup = Some(format!(
                    "failed: gh did not answer within {} seconds",
                    issues::LOOKUP_DEADLINE.as_secs()
                ));
            }
            Ok(output) => {
                lookup = Some(format!("failed: {}", quoted(&output.stderr)));
            }
            Err(error) => lookup = Some(format!("failed: {error}")),
        }
    }

    let output = match issues::run_gh(
        &gh,
        &issues::create_args(&repository, &draft.title),
        &draft.body,
        issues::CREATE_DEADLINE,
    ) {
        Ok(output) => output,
        Err(error) => {
            return Ok(not_created(
                id,
                &format!("gh could not be started ({error})"),
                issues::steps_for(""),
                lookup.as_deref(),
            ));
        }
    };
    match output.status {
        None => Ok(not_created(
            id,
            &format!(
                "gh did not answer within {} seconds",
                issues::CREATE_DEADLINE.as_secs()
            ),
            vec![
                format!(
                    "Check https://github.com/{repository}/issues for {} before submitting again: \
                     the issue may have been created.",
                    draft.fingerprint
                ),
                issues::fallback_step(),
            ],
            lookup.as_deref(),
        )),
        Some(0) => match issues::created_url(&output.stdout, &repository) {
            Some(url) => {
                draft.outcome = Some("created".into());
                draft.url = Some(url.clone());
                notebook.save_draft(&draft)?;
                Ok(created(id, &url, lookup.as_deref(), false))
            }
            None => {
                draft.outcome = Some("unknown".into());
                notebook.save_draft(&draft)?;
                Ok(unknown_outcome(&draft, &repository))
            }
        },
        Some(_) => {
            let message = if output.stderr.trim().is_empty() {
                quoted(&output.stdout)
            } else {
                quoted(&output.stderr)
            };
            let reason = if message.is_empty() {
                "gh failed".to_string()
            } else {
                format!("gh failed: {message}")
            };
            Ok(not_created(
                id,
                &reason,
                issues::steps_for(&message),
                lookup.as_deref(),
            ))
        }
    }
}

fn discard(ctx: &Context, name: &str, ids: &[String], all: bool) -> Result<Value> {
    let notebook = Notebook::open(&ctx.root, name)?;
    let removed = if all {
        notebook.discard_all()
    } else {
        notebook.discard(ids)?
    };
    Ok(json!({"ok": true, "removed": removed}))
}

/// Runs one feedback subcommand for the session name `name` (already validated).
pub fn execute(ctx: &Context, name: &str, action: &FeedbackAction) -> Result<(i32, Value)> {
    refused_when_off(ctx)?;
    match action {
        FeedbackAction::Note(args) => Ok((0, note(ctx, name, args)?)),
        FeedbackAction::List => Ok((0, list(ctx, name)?)),
        FeedbackAction::Draft { notes, title } => {
            Ok((0, draft(ctx, name, notes, title.as_deref())?))
        }
        FeedbackAction::Submit {
            draft,
            confirmed,
            allow_duplicate,
        } => submit(ctx, name, draft, *confirmed, *allow_duplicate),
        FeedbackAction::Discard { ids, all } => Ok((0, discard(ctx, name, ids, *all)?)),
    }
}

/// The field the `stop` answer gains when the notebook has notes not yet drafted.
pub fn stop_hint(ctx: &Context, name: &str) -> Option<Value> {
    if ctx.feedback == FeedbackMode::Off {
        return None;
    }
    let pending = Notebook::existing(&ctx.root, name)?.undrafted().len();
    (pending > 0).then(|| json!({"pending": pending, "next": STOP_NEXT}))
}

/// The verb of a command line: its first word that is not an option or the value of a global one.
pub fn verb_of(argv: &[String]) -> Option<String> {
    let mut words = argv.iter().skip(1);
    while let Some(word) = words.next() {
        if word == "--session" || word == "--timeout" {
            words.next();
        } else if !word.starts_with('-') {
            return Some(word.clone());
        }
    }
    None
}

/// The session name of a command line as the parser would take it, else the environment's, else
/// `default`.
fn session_of(ctx: &Context, argv: &[String]) -> Option<String> {
    let mut words = argv.iter().skip(1);
    let mut given = None;
    while let Some(word) = words.next() {
        if word == "--session" {
            given = words.next().cloned();
        } else if let Some(value) = word.strip_prefix("--session=") {
            given = Some(value.to_string());
        }
    }
    let name = ctx.session_name(given.as_deref());
    validate_session_name(&name).is_ok().then_some(name)
}

/// An attempted verb or option name as the notebook keeps it.
fn attempted(name: &str) -> String {
    let name = name.split('=').next().unwrap_or("");
    let plain = !name.is_empty()
        && name.chars().count() <= MAX_NAME
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if plain {
        name.to_string()
    } else {
        "{invalid}".to_string()
    }
}

fn record(ctx: &Context, name: &str, kind: &str, verb: &str, code: &str, message: &str) {
    if let Ok(notebook) = Notebook::open(&ctx.root, name) {
        notebook.record_event(kind, verb, code, message, &shape(message));
    }
}

/// Records a usage error that names a verb or an option that does not exist. Never fails.
pub fn observe_usage(ctx: &Context, argv: &[String], error: &clap::Error) {
    use clap::error::{ContextKind, ContextValue, ErrorKind};

    if ctx.feedback == FeedbackMode::Off {
        return;
    }
    let verb = verb_of(argv);
    let known = verb
        .as_ref()
        .is_some_and(|verb| verb_names().contains(verb));
    if verb
        .as_deref()
        .is_some_and(|verb| UNOBSERVED.contains(&verb))
    {
        return;
    }
    let Some(name) = session_of(ctx, argv) else {
        return;
    };
    let value = |kind: ContextKind| match error.get(kind) {
        Some(ContextValue::String(text)) => Some(text.clone()),
        _ => None,
    };
    match error.kind() {
        ErrorKind::InvalidSubcommand if !known => {
            if let Some(attempt) = value(ContextKind::InvalidSubcommand) {
                record(ctx, &name, "unknown_verb", "", "", &attempted(&attempt));
            }
        }
        ErrorKind::UnknownArgument => {
            if let Some(attempt) = value(ContextKind::InvalidArg) {
                let verb = if known {
                    verb.unwrap_or_default()
                } else {
                    String::new()
                };
                record(
                    ctx,
                    &name,
                    "unknown_option",
                    &verb,
                    "",
                    &attempted(&attempt),
                );
            }
        }
        _ => {}
    }
}

/// Records the failure of a core verb: its error code and the scrubbed, shortened first line of
/// its message. The outcome is only read. Never fails.
pub fn observe_run(ctx: &Context, argv: &[String], status: i32, output: &Value) {
    if ctx.feedback == FeedbackMode::Off || status == 0 {
        return;
    }
    let Some(verb) = verb_of(argv) else {
        return;
    };
    if UNOBSERVED.contains(&verb.as_str()) {
        return;
    }
    let Some(name) = session_of(ctx, argv) else {
        return;
    };
    let error = output.get("error");
    let code = error
        .and_then(|error| error.get("code"))
        .and_then(Value::as_str)
        .unwrap_or("client");
    let message = error
        .and_then(|error| error.get("message"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let line = scrub(&issues::first_line(message), &known_for(ctx, &name));
    let line: String = line.chars().take(MAX_EVENT_MESSAGE).collect();
    record(ctx, &name, "failure", &verb, code, &line);
}
