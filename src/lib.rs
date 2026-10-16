pub mod cli;
pub mod config;
pub mod error;
pub mod feedback;
pub mod launch;
pub mod msys;
pub mod process;
pub mod session;
pub mod spool;
#[cfg(windows)]
mod winsec;

use clap::FromArgMatches;
use cli::{Cli, Verb};
use config::Context;
use error::{ClientError, Result};
use serde_json::{Value, json};
use spool::Wait;
use std::io::Write;
use std::time::Duration;

const DEFAULT_TIMEOUT: f64 = 30.0;
const START_TIMEOUT: f64 = 90.0;
/// One year: far beyond any wait, and small enough for every clock arithmetic.
const MAX_TIMEOUT: f64 = 31_536_000.0;

fn timeout_for(cli: &Cli) -> Result<Duration> {
    let seconds = cli.timeout.unwrap_or(match cli.verb {
        Verb::Start(_) => START_TIMEOUT,
        _ => DEFAULT_TIMEOUT,
    });
    if !seconds.is_finite() || seconds <= 0.0 {
        return Err(ClientError::msg(
            "--timeout must be a positive number of seconds",
        ));
    }
    if seconds > MAX_TIMEOUT {
        return Err(ClientError::msg(format!(
            "--timeout must be at most {MAX_TIMEOUT} seconds (one year)"
        )));
    }
    Ok(Duration::from_secs_f64(seconds))
}

fn execute(cli: &Cli, ctx: &Context) -> Result<(i32, Value)> {
    let timeout = timeout_for(cli)?;
    let session = cli.session.as_deref();
    if !matches!(cli.verb, Verb::Sessions { .. }) {
        config::validate_session_name(&ctx.session_name(session))?;
    }
    match &cli.verb {
        Verb::Start(args) => {
            let mut answer = launch::start_session(ctx, session, timeout, args)?;
            if ctx.feedback != config::FeedbackMode::Off {
                answer["feedback"] = json!(feedback::START_HINT);
            }
            Ok((0, answer))
        }
        Verb::Stop => {
            let mut answer = launch::stop_session(ctx, session, timeout)?;
            if let Some(hint) = feedback::stop_hint(ctx, &ctx.session_name(session)) {
                answer["feedback"] = hint;
            }
            Ok((0, answer))
        }
        Verb::Feedback { action } => feedback::execute(ctx, &ctx.session_name(session), action),
        Verb::Sessions { prune } => {
            let mut answer = json!({"ok": true});
            if *prune {
                answer["pruned"] = json!(session::prune(&ctx.root, session::LEFTOVER_AGE)?);
            }
            answer["sessions"] = json!(session::list(&ctx.root));
            Ok((0, answer))
        }
        verb => {
            let (command, args) = cli::command_for(verb, ctx)?;
            let name = ctx.session_name(session);
            let (directory, record) = session::require_live(&ctx.root, &name)?;
            let alive = move || session::game_alive(&record);
            let note = {
                let directory = directory.clone();
                move || session::death_note(&directory)
            };
            let answer = spool::send_request(
                &directory,
                &command,
                &args,
                &Wait {
                    timeout,
                    is_alive: &alive,
                    death_note: &note,
                },
            )?;
            let succeeded = answer.get("ok").and_then(Value::as_bool).unwrap_or(false);
            Ok((if succeeded { 0 } else { 1 }, answer))
        }
    }
}

fn client_error(message: &str) -> Value {
    json!({"ok": false, "error": {"code": "client", "message": message}})
}

/// The one line of a usage error that names the problem, for the JSON object.
fn usage_message(error: &clap::Error) -> String {
    use clap::error::ErrorKind;
    if error.kind() == ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand {
        return "no command given; run with --help to list them".to_string();
    }
    let text = error.to_string();
    let mut lines = text.lines().map(str::trim).filter(|line| !line.is_empty());
    let first = lines.next().unwrap_or("usage error");
    let mut message = first.strip_prefix("error: ").unwrap_or(first).to_string();
    if message.ends_with(':') {
        // "the following required arguments were not provided:" names them on the next line.
        if let Some(detail) = lines.next() {
            message.push(' ');
            message.push_str(detail);
        }
    }
    format!("{message}; run with --help for the usage")
}

/// Runs one command line. Returns the exit status and the JSON object to print; `None` means clap
/// printed help or the version (a success). A usage error gets clap's explanation on standard error
/// and the usual client error object, like every other failure.
///
/// A failing run is also recorded for the feedback channel, after its outcome is final; recording
/// never changes the outcome.
pub fn run(argv: Vec<String>, ctx: &Context) -> (i32, Option<Value>) {
    let parsed = cli::command(ctx.feedback)
        .try_get_matches_from(&argv)
        .and_then(|matches| Cli::from_arg_matches(&matches));
    let cli = match parsed {
        Ok(cli) => cli,
        Err(error) => {
            let _ = error.print();
            return if error.use_stderr() {
                feedback::observe_usage(ctx, &argv, &error);
                (2, Some(client_error(&usage_message(&error))))
            } else {
                (0, None)
            };
        }
    };
    let (status, output) = match execute(&cli, ctx) {
        Ok((status, output)) => (status, output),
        Err(error) => (2, client_error(&error.to_string())),
    };
    feedback::observe_run(ctx, &argv, status, &output);
    (status, Some(output))
}

/// The binary's whole job: read the environment, run, print one JSON object, return the status.
pub fn run_from_env() -> i32 {
    let ctx = Context::from_env();
    let (status, output) = run(std::env::args().collect(), &ctx);
    if let Some(output) = output {
        let mut stdout = std::io::stdout().lock();
        let _ = writeln!(stdout, "{output}");
        let _ = stdout.flush();
    }
    status
}
