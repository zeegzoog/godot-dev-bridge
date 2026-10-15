//! A session is one directory: `session.json` (written by the client), the game's `ready.json`,
//! request and response files, `closed.json`, `game.log` and `shots/`.

use crate::error::{ClientError, Result};
use crate::process;
use crate::spool::write_json_atomically;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

const LOG_TAIL_LINES: usize = 40;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionRecord {
    pub pid: u32,
    pub command: Vec<String>,
    pub started: f64,
}

pub fn session_directory(root: &Path, name: &str) -> PathBuf {
    root.join(name)
}

pub fn log_path(directory: &Path) -> PathBuf {
    directory.join("game.log")
}

pub fn write_record(directory: &Path, record: &SessionRecord) -> Result<()> {
    write_json_atomically(
        &directory.join("session.json"),
        &serde_json::to_value(record)?,
    )?;
    Ok(())
}

/// `session.json` of a session directory, or `None` when it is missing or unreadable.
pub fn read_record(directory: &Path) -> Option<SessionRecord> {
    let text = std::fs::read_to_string(directory.join("session.json")).ok()?;
    serde_json::from_str(&text).ok()
}

/// The last `lines` lines of a text file, or an empty string when it cannot be read.
pub fn log_tail(path: &Path, lines: usize) -> String {
    let Ok(bytes) = std::fs::read(path) else {
        return String::new();
    };
    let text = String::from_utf8_lossy(&bytes);
    let all: Vec<&str> = text.lines().collect();
    all[all.len().saturating_sub(lines)..].join("\n")
}

/// Text to append to an error about a dead game: the end of its log, if it wrote one.
pub fn death_note(directory: &Path) -> String {
    let tail = log_tail(&log_path(directory), LOG_TAIL_LINES);
    if tail.trim().is_empty() {
        String::new()
    } else {
        format!("; the log ends:\n{tail}")
    }
}

/// The session's directory and record, or an error when no live session has that name.
pub fn require_live(root: &Path, name: &str) -> Result<(PathBuf, SessionRecord)> {
    let directory = session_directory(root, name);
    let Some(record) = read_record(&directory) else {
        return Err(ClientError::msg(format!(
            "no session named '{name}' (looked in {})",
            directory.display()
        )));
    };
    if !process::is_alive(record.pid) {
        return Err(ClientError::msg(format!(
            "the game of session '{name}' (pid {}) is not running{}",
            record.pid,
            death_note(&directory)
        )));
    }
    Ok((directory, record))
}

/// Every session under `root` and whether its game is alive, sorted by name.
pub fn list(root: &Path) -> Vec<Value> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut found: Vec<(String, SessionRecord)> = entries
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            read_record(&entry.path()).map(|record| (name, record))
        })
        .collect();
    found.sort_by(|a, b| a.0.cmp(&b.0));
    found
        .into_iter()
        .map(|(name, record)| {
            json!({"session": name, "pid": record.pid, "alive": process::is_alive(record.pid)})
        })
        .collect()
}

/// Removes what a finished session left, so a new one starts clean (the old log goes too).
pub fn clear_directory(directory: &Path) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.filter_map(|entry| entry.ok()) {
        let path = entry.path();
        if path.is_file() {
            let _ = std::fs::remove_file(&path);
        } else if path.is_dir() {
            if let Ok(inner) = std::fs::read_dir(&path) {
                for file in inner.filter_map(|file| file.ok()) {
                    let _ = std::fs::remove_file(file.path());
                }
            }
        }
    }
}
