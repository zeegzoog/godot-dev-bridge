pub mod cli;
pub mod config;
pub mod error;
pub mod launch;
pub mod msys;
pub mod process;
pub mod session;
pub mod spool;

use clap::Parser;
use cli::{Cli, Verb};
use config::Context;
use error::{ClientError, Result};
use serde_json::{Value, json};
use spool::Wait;
use std::io::Write;
use std::time::Duration;

const DEFAULT_TIMEOUT: f64 = 30.0;
const START_TIMEOUT: f64 = 90.0;

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
    Ok(Duration::from_secs_f64(seconds))
}

fn execute(cli: &Cli, ctx: &Context) -> Result<(i32, Value)> {
    let timeout = timeout_for(cli)?;
    let session = cli.session.as_deref();
    match &cli.verb {
        Verb::Start(args) => Ok((0, launch::start_session(ctx, session, timeout, args)?)),
        Verb::Stop => Ok((0, launch::stop_session(ctx, session, timeout)?)),
        Verb::Sessions => Ok((0, json!({"ok": true, "sessions": session::list(&ctx.root)}))),
        verb => {
            let (command, args) = cli::command_for(verb, ctx)?;
            let name = ctx.session_name(session);
            let (directory, record) = session::require_live(&ctx.root, &name)?;
            let pid = record.pid;
            let alive = move || process::is_alive(pid);
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

/// Runs one command line. Returns the exit status and the JSON object to print; `None` means clap
/// already printed the usage or help text.
pub fn run(argv: Vec<String>, ctx: &Context) -> (i32, Option<Value>) {
    let cli = match Cli::try_parse_from(argv) {
        Ok(cli) => cli,
        Err(error) => {
            let _ = error.print();
            return (if error.use_stderr() { 2 } else { 0 }, None);
        }
    };
    match execute(&cli, ctx) {
        Ok((status, output)) => (status, Some(output)),
        Err(error) => (
            2,
            Some(json!({
                "ok": false,
                "error": {"code": "client", "message": error.to_string()},
            })),
        ),
    }
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
