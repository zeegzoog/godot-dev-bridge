//! The files a session talks through: atomic writes and the request/response exchange.

use crate::error::{ClientError, Result};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const POLL: Duration = Duration::from_millis(10);
/// Tries to read a file that is there but refused (Windows may briefly do that to a fresh file).
const READ_ATTEMPTS: u32 = 100;
/// Tries on a file that was read but is not JSON: it is written whole (`.tmp`, then renamed), so a
/// second look is all it deserves.
const PARSE_ATTEMPTS: u32 = 5;
/// How old an answer must be before nobody can still be waiting for it.
pub const STALE_ANSWER_AGE: Duration = Duration::from_secs(60);

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// Writes `value` finished-or-absent: under `<path>.tmp`, then renamed over `path`.
pub fn write_json_atomically(path: &Path, value: &Value) -> std::io::Result<()> {
    let mut name = path.as_os_str().to_owned();
    name.push(".tmp");
    let temporary = PathBuf::from(name);
    std::fs::write(&temporary, serde_json::to_vec(value)?)?;
    std::fs::rename(&temporary, path)
}

/// Reads a JSON file, retrying for about a second when it cannot be read: right after a file
/// appears Windows may refuse it. A file that was read but is not JSON is reported as such.
pub fn read_json(path: &Path) -> Result<Value> {
    let mut reason = String::from("it does not exist");
    for attempt in 0..READ_ATTEMPTS {
        match std::fs::read_to_string(path) {
            Ok(text) => match serde_json::from_str(&text) {
                Ok(value) => return Ok(value),
                Err(error) => {
                    reason = format!("its content is not valid JSON ({error})");
                    if attempt + 1 >= PARSE_ATTEMPTS {
                        break;
                    }
                }
            },
            Err(error) => reason = error.to_string(),
        }
        std::thread::sleep(POLL);
    }
    Err(ClientError::msg(format!(
        "cannot read {}: {reason}",
        path.display()
    )))
}

/// Removes answers (`resp-*.json`) older than `max_age`: the caller that wanted one gave up, so
/// nobody will collect it.
pub fn sweep_stale_answers(directory: &Path, max_age: Duration) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    let now = SystemTime::now();
    for entry in entries.filter_map(|entry| entry.ok()) {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if !(name.starts_with("resp-") && name.ends_with(".json")) {
            continue;
        }
        let age = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|modified| now.duration_since(modified).ok());
        if age.is_some_and(|age| age > max_age) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

fn since_epoch() -> Duration {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
}

/// A 12-character hexadecimal id, unique across runs and across calls within a run.
pub fn new_request_id() -> String {
    let nanos = since_epoch().as_nanos() as u64;
    let mixed = nanos.wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ (u64::from(std::process::id()) << 20)
        ^ COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{:012x}", mixed & 0xFFFF_FFFF_FFFF)
}

/// A sequence number that orders requests across runs: microseconds since the epoch.
pub fn new_sequence() -> u64 {
    since_epoch().as_micros() as u64
}

/// How a request waits for its answer.
pub struct Wait<'a> {
    pub timeout: Duration,
    /// Whether the game process is still running.
    pub is_alive: &'a dyn Fn() -> bool,
    /// Extra text for the error when the game died (for example the end of its log).
    pub death_note: &'a dyn Fn() -> String,
}

/// Sends one request and returns the game's whole answer (an error answer is data, not a failure).
///
/// On a timeout or a dead game the unread request file is removed, so a late game never
/// executes a command its caller has given up on. Answers that an earlier caller gave up on are
/// swept first.
pub fn send_request(directory: &Path, command: &str, args: &Value, wait: &Wait) -> Result<Value> {
    sweep_stale_answers(directory, STALE_ANSWER_AGE);
    let id = new_request_id();
    let request_path = directory.join(format!("cmd-{}-{id}.json", new_sequence()));
    write_json_atomically(
        &request_path,
        &json!({"id": id, "cmd": command, "args": args}),
    )?;
    let answer_path = directory.join(format!("resp-{id}.json"));
    let deadline = Instant::now() + wait.timeout;
    loop {
        if answer_path.exists() {
            let answer = read_json(&answer_path);
            let _ = std::fs::remove_file(&answer_path);
            return answer;
        }
        if Instant::now() >= deadline {
            // A request the game already took (a running `step` is) is out of reach: say so, so
            // that the caller checks before sending it again.
            let taken = !request_path.exists();
            let _ = std::fs::remove_file(&request_path);
            let seconds = wait.timeout.as_secs_f64();
            return Err(ClientError::msg(if taken {
                format!(
                    "no answer to '{command}' within {seconds} s; the game took the request and may still be running it (its answer will be discarded), so check `status` before sending it again, or raise --timeout"
                )
            } else {
                format!("no answer to '{command}' within {seconds} s")
            }));
        }
        if !(wait.is_alive)() {
            let _ = std::fs::remove_file(&request_path);
            return Err(ClientError::msg(format!(
                "the game process ended before answering '{command}'{}",
                (wait.death_note)()
            )));
        }
        std::thread::sleep(POLL);
    }
}
