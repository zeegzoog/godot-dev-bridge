//! Submission through the user's own signed-in GitHub command-line client (`gh`): the duplicate
//! look-up, the creation of the issue, and what their output means.

use crate::config::{Context, find_program};
use serde_json::Value;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

pub const LOOKUP_DEADLINE: Duration = Duration::from_secs(20);
pub const CREATE_DEADLINE: Duration = Duration::from_secs(45);
const MAX_OUTPUT: usize = 1024 * 1024;
const POLL: Duration = Duration::from_millis(20);
/// How long to wait for the output pipes once the child is gone.
const PIPE_GRACE: Duration = Duration::from_secs(2);

/// `owner/name` of the tracker, from the package manifest: the one place the repository is named.
pub fn repository() -> Option<String> {
    let url = env!("CARGO_PKG_REPOSITORY");
    let path = url
        .strip_prefix("https://github.com/")?
        .trim_end_matches('/')
        .trim_end_matches(".git");
    let mut parts = path.split('/');
    let (owner, name) = (parts.next()?, parts.next()?);
    let plain = |part: &str| {
        !part.is_empty()
            && part
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    };
    (parts.next().is_none() && plain(owner) && plain(name)).then(|| format!("{owner}/{name}"))
}

/// The `gh` to run: `DEV_BRIDGE_GH` when it names a file, else `gh` on `PATH`.
pub fn find_gh(ctx: &Context) -> std::result::Result<PathBuf, String> {
    match &ctx.gh {
        Some(path) => {
            let path = PathBuf::from(path);
            if path.is_file() {
                Ok(path)
            } else {
                Err(format!("DEV_BRIDGE_GH={} does not exist", path.display()))
            }
        }
        None => find_program(ctx, "gh").ok_or_else(|| "gh is not on PATH".to_string()),
    }
}

/// What a `gh` run produced. `status` is `None` when it was ended at its deadline.
#[derive(Debug, Default)]
pub struct Output {
    pub status: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

fn drain(mut stream: impl Read + Send + 'static) -> Receiver<String> {
    let (sender, receiver) = channel();
    std::thread::spawn(move || {
        let mut kept = Vec::new();
        let mut buffer = [0u8; 8192];
        while let Ok(read) = stream.read(&mut buffer) {
            if read == 0 {
                break;
            }
            let room = MAX_OUTPUT.saturating_sub(kept.len());
            kept.extend_from_slice(&buffer[..read.min(room)]);
        }
        let _ = sender.send(String::from_utf8_lossy(&kept).into_owned());
    });
    receiver
}

#[cfg(windows)]
fn prepare(_command: &mut Command) {
    crate::launch::keep_own_handles_private();
}

#[cfg(not(windows))]
fn prepare(_command: &mut Command) {}

/// Runs `gh` with `args`, `stdin` written to it, and a deadline after which it is ended with its
/// process tree. Its output goes through pipes and never reaches this client's own.
pub fn run_gh(
    gh: &Path,
    args: &[String],
    stdin: &str,
    deadline: Duration,
) -> std::io::Result<Output> {
    let mut command = Command::new(gh);
    command
        .args(args)
        .env("GH_PROMPT_DISABLED", "1")
        .env("NO_COLOR", "1")
        .env("GH_NO_UPDATE_NOTIFIER", "1")
        .env("GH_SPINNER_DISABLED", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    prepare(&mut command);
    let mut child = command.spawn()?;
    let out = child.stdout.take().map(drain);
    let err = child.stderr.take().map(drain);
    if let Some(mut input) = child.stdin.take() {
        let text = stdin.to_string();
        std::thread::spawn(move || {
            let _ = input.write_all(text.as_bytes());
        });
    }
    let end = Instant::now() + deadline;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break Some(status.code().unwrap_or(-1));
        }
        if Instant::now() >= end {
            crate::process::kill(child.id());
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        std::thread::sleep(POLL);
    };
    let collect = |receiver: Option<Receiver<String>>| {
        receiver
            .and_then(|receiver| receiver.recv_timeout(PIPE_GRACE).ok())
            .unwrap_or_default()
    };
    Ok(Output {
        status,
        stdout: collect(out),
        stderr: collect(err),
    })
}

/// The arguments of the duplicate look-up.
pub fn lookup_args(repository: &str, fingerprint: &str) -> Vec<String> {
    [
        "issue",
        "list",
        "--repo",
        repository,
        "--state",
        "all",
        "--search",
        &format!("{fingerprint} in:body"),
        "--json",
        "number,title,url,body",
        "--limit",
        "5",
    ]
    .iter()
    .map(|arg| arg.to_string())
    .collect()
}

/// The arguments of the creation: the title as one `--title=` argument, so a leading `-` cannot
/// be read as an option; the body comes on standard input.
pub fn create_args(repository: &str, title: &str) -> Vec<String> {
    vec![
        "issue".into(),
        "create".into(),
        "--repo".into(),
        repository.into(),
        format!("--title={title}"),
        "--body-file".into(),
        "-".into(),
    ]
}

/// The address of an issue that already carries the fingerprint, from the look-up's output.
pub fn duplicate_in(
    stdout: &str,
    fingerprint: &str,
) -> std::result::Result<Option<String>, String> {
    let issues: Value = serde_json::from_str(stdout.trim())
        .map_err(|_| "the look-up answer could not be read".to_string())?;
    let issues = issues
        .as_array()
        .ok_or_else(|| "the look-up answer is not a list".to_string())?;
    Ok(issues
        .iter()
        .filter(|issue| {
            issue
                .get("body")
                .and_then(Value::as_str)
                .is_some_and(|body| body.contains(fingerprint))
        })
        .find_map(|issue| issue.get("url").and_then(Value::as_str).map(str::to_string)))
}

/// The issue address on the last line of the creation's output, when it is one of this repository.
pub fn created_url(stdout: &str, repository: &str) -> Option<String> {
    let line = stdout
        .lines()
        .map(str::trim)
        .rfind(|line| !line.is_empty())?;
    let prefix = format!("https://github.com/{repository}/issues/");
    let number = line.strip_prefix(&prefix)?;
    (!number.is_empty() && number.chars().all(|c| c.is_ascii_digit())).then(|| line.to_string())
}

/// The first non-empty line of a message.
pub fn first_line(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("")
        .to_string()
}

/// What to do next after a creation that failed with `message` (already scrubbed).
pub fn steps_for(message: &str) -> Vec<String> {
    let lower = message.to_lowercase();
    let mut steps = Vec::new();
    if [
        "auth login",
        "gh_token",
        "not logged",
        "authenticat",
        "log in",
    ]
    .iter()
    .any(|sign| lower.contains(sign))
    {
        steps.push("Ask the user to run `gh auth login` in a terminal, then submit again.".into());
    } else if [
        "network",
        "connect",
        "timeout",
        "timed out",
        "rate limit",
        "dial tcp",
        "could not resolve",
    ]
    .iter()
    .any(|sign| lower.contains(sign))
    {
        steps.push("The tracker could not be reached; submit again later.".into());
    } else if message.is_empty() {
        steps.push("The tracker client gave no reason; submit again later.".into());
    } else {
        steps.push(format!("The tracker client said: {message}"));
    }
    steps.push(fallback_step());
    steps
}

/// The text path, which always works.
pub fn fallback_step() -> String {
    "Or give the user the report text or the link from `feedback draft` to file it by hand.".into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_repository_comes_from_the_manifest() {
        assert_eq!(repository().as_deref(), Some("zigenzoog/godot-dev-bridge"));
    }

    #[test]
    fn a_created_issue_is_recognized_only_by_an_address_of_this_repository() {
        let repo = "o/n";
        assert_eq!(
            created_url("Creating issue\n\nhttps://github.com/o/n/issues/42\n", repo).as_deref(),
            Some("https://github.com/o/n/issues/42")
        );
        assert_eq!(created_url("https://github.com/x/y/issues/42", repo), None);
        assert_eq!(created_url("https://github.com/o/n/issues/42x", repo), None);
        assert_eq!(created_url("done", repo), None);
    }

    #[test]
    fn a_duplicate_needs_the_fingerprint_in_its_body() {
        let stdout = r#"[{"number":1,"title":"a","url":"u1","body":"no"},{"number":2,"title":"b","url":"u2","body":"x gdbfb1 y"}]"#;
        assert_eq!(duplicate_in(stdout, "gdbfb1"), Ok(Some("u2".into())));
        assert_eq!(duplicate_in("[]", "gdbfb1"), Ok(None));
        assert!(duplicate_in("oops", "gdbfb1").is_err());
    }

    #[test]
    fn the_steps_follow_the_message() {
        assert!(
            steps_for("To get started with GitHub CLI, please run:  gh auth login")[0]
                .contains("gh auth login")
        );
        assert!(steps_for("error connecting to api.github.com")[0].contains("later"));
        assert!(steps_for("label not found")[0].contains("label not found"));
        assert_eq!(steps_for("x").last().cloned(), Some(fallback_step()));
    }

    #[test]
    fn the_title_travels_as_one_argument() {
        let args = create_args("o/n", "-x title");
        assert!(args.contains(&"--title=-x title".to_string()));
        assert_eq!(&args[args.len() - 2..], ["--body-file", "-"]);
    }
}
