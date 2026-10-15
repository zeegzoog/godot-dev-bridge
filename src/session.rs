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

/// Creates `path` (and its parents) for a session and makes sure nobody else can use it. On Unix
/// the directory is private to the user (mode 0700); one that exists is accepted only when it is a
/// real directory owned by this user, and is tightened if it was more open. The spool is a command
/// channel into the game (`eval` can run anything), so a directory another user can write to would
/// let them drive it.
#[cfg(unix)]
pub fn ensure_private_dir(path: &Path) -> Result<()> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};

    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(path)?;
    let meta = std::fs::symlink_metadata(path)?;
    if meta.file_type().is_symlink() {
        return Err(ClientError::msg(format!(
            "{} is a symbolic link; refusing to use it as a session directory",
            path.display()
        )));
    }
    if !meta.is_dir() {
        return Err(ClientError::msg(format!(
            "{} is not a directory",
            path.display()
        )));
    }
    // SAFETY: a plain query of the calling process's identity.
    if meta.uid() != unsafe { libc::geteuid() } {
        return Err(ClientError::msg(format!(
            "{} belongs to another user; refusing to use it. Set DEV_BRIDGE_ROOT to a directory of your own",
            path.display()
        )));
    }
    if meta.mode() & 0o077 != 0 {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

#[cfg(not(unix))]
pub fn ensure_private_dir(path: &Path) -> Result<()> {
    std::fs::create_dir_all(path)?;
    Ok(())
}

/// Whether the game a session recorded is still running (and is still that game: see
/// `process::is_alive_since`).
pub fn game_alive(record: &SessionRecord) -> bool {
    process::is_alive_since(record.pid, record.started)
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
    if !game_alive(&record) {
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
            json!({"session": name, "pid": record.pid, "alive": game_alive(&record)})
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
