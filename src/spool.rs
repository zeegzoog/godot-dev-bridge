//! The files a session talks through: atomic writes and the request/response exchange.

use crate::error::{ClientError, Result};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const POLL: Duration = Duration::from_millis(10);
const READ_ATTEMPTS: u32 = 100;

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// Writes `value` finished-or-absent: under `<path>.tmp`, then renamed over `path`.
pub fn write_json_atomically(path: &Path, value: &Value) -> std::io::Result<()> {
    let mut name = path.as_os_str().to_owned();
    name.push(".tmp");
    let temporary = PathBuf::from(name);
    std::fs::write(&temporary, serde_json::to_vec(value)?)?;
    std::fs::rename(&temporary, path)
}

/// Reads a JSON file, retrying for about a second: right after a file appears Windows may refuse it.
pub fn read_json(path: &Path) -> Result<Value> {
    for _ in 0..READ_ATTEMPTS {
        if let Ok(text) = std::fs::read_to_string(path) {
            if let Ok(value) = serde_json::from_str(&text) {
                return Ok(value);
            }
        }
        std::thread::sleep(POLL);
    }
    Err(ClientError::msg(format!("cannot read {}", path.display())))
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
/// executes a command its caller has given up on.
pub fn send_request(directory: &Path, command: &str, args: &Value, wait: &Wait) -> Result<Value> {
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
            let answer = read_json(&answer_path)?;
            let _ = std::fs::remove_file(&answer_path);
            return Ok(answer);
        }
        if Instant::now() >= deadline {
            let _ = std::fs::remove_file(&request_path);
            return Err(ClientError::msg(format!(
                "no answer to '{command}' within {} s",
                wait.timeout.as_secs_f64()
            )));
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
