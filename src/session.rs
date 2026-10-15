//! A session is one directory: `session.json` (written by the client), the game's `ready.json`,
//! request and response files, `closed.json`, `game.log` and `shots/`.

use crate::error::{ClientError, Result};
use crate::process;
use crate::spool::write_json_atomically;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

const LOG_TAIL_LINES: usize = 40;

/// The client's record of the game it started.
const RECORD: &str = "session.json";

/// How long a directory without a record must have stayed unchanged before `prune` takes it for
/// the leftover of a failed start: a start writes its record a moment after it creates the directory.
pub const LEFTOVER_AGE: Duration = Duration::from_secs(60);

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
    write_json_atomically(&directory.join(RECORD), &serde_json::to_value(record)?)?;
    Ok(())
}

/// `session.json` of a session directory, or `None` when it is missing or unreadable.
pub fn read_record(directory: &Path) -> Option<SessionRecord> {
    let text = std::fs::read_to_string(directory.join(RECORD)).ok()?;
    serde_json::from_str(&text).ok()
}

/// Why the game of a session ended, as it wrote in `closed.json` (`quit`, `idle` or `game`), or
/// `None` when it wrote nothing: it is still running, or it crashed or was killed.
pub fn closed_reason(directory: &Path) -> Option<String> {
    let text = std::fs::read_to_string(directory.join("closed.json")).ok()?;
    let value: Value = serde_json::from_str(&text).ok()?;
    value.get("reason")?.as_str().map(str::to_string)
}

/// What a reason in `closed.json` means, for an error message.
fn closed_explanation(reason: &str) -> String {
    match reason {
        "quit" => "it ended when it was asked to quit".to_string(),
        "idle" => "it ended itself after no request came for its idle time (start --idle-quit, 30 minutes by default)".to_string(),
        "game" => "it ended on its own: its code quit, or its window was closed".to_string(),
        other => format!("closed.json gives the reason '{other}'"),
    }
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

/// Text to append to an error about a dead game: why it ended, if it said so, and the end of its
/// log, if it wrote one.
pub fn death_note(directory: &Path) -> String {
    let reason = closed_reason(directory)
        .map(|reason| format!(" ({})", closed_explanation(&reason)))
        .unwrap_or_default();
    let tail = log_tail(&log_path(directory), LOG_TAIL_LINES);
    if tail.trim().is_empty() {
        reason
    } else {
        format!("{reason}; the log ends:\n{tail}")
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

/// On Windows a new directory inherits the access rules of its parent, and a temporary directory
/// can grant other accounts write access. The directory is made private by replacing its rules with
/// ones for this user and the system alone (no inheritance), and one that is a link, or is owned by
/// somebody else, is refused: see `winsec`.
#[cfg(windows)]
pub fn ensure_private_dir(path: &Path) -> Result<()> {
    std::fs::create_dir_all(path)?;
    let meta = std::fs::symlink_metadata(path)?;
    if meta.file_type().is_symlink() {
        return Err(ClientError::msg(format!(
            "{} is a link (a symbolic link or a junction); refusing to use it as a session directory",
            path.display()
        )));
    }
    if !meta.is_dir() {
        return Err(ClientError::msg(format!(
            "{} is not a directory",
            path.display()
        )));
    }
    crate::winsec::make_private(path)
}

#[cfg(not(any(unix, windows)))]
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
        let record_path = directory.join(RECORD);
        return Err(ClientError::msg(if record_path.exists() {
            format!(
                "{} is unreadable; delete the directory, or start the session again",
                record_path.display()
            )
        } else {
            format!(
                "no session named '{name}' (looked in {})",
                directory.display()
            )
        }));
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

/// Every session under `root`, whether its game is alive and, when the game said so, why it ended
/// (`closed`), sorted by name.
pub fn list(root: &Path) -> Vec<Value> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut found: Vec<(String, PathBuf, SessionRecord)> = entries
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            read_record(&entry.path()).map(|record| (name, entry.path(), record))
        })
        .collect();
    found.sort_by(|a, b| a.0.cmp(&b.0));
    found
        .into_iter()
        .map(|(name, directory, record)| {
            let mut session =
                json!({"session": name, "pid": record.pid, "alive": game_alive(&record)});
            if let Some(reason) = closed_reason(&directory) {
                session["closed"] = json!(reason);
            }
            session
        })
        .collect()
}

/// Removes the directories of the sessions under `root` that have ended and returns their names,
/// sorted. A session has ended when the game its record names is no longer running. A directory
/// without a readable record is the leftover of a start that failed before the game ran when it
/// holds a `game.log` and nothing a session does not write, and has not changed for `min_age`
/// (a start that is about to write its record is left alone). Links in the root, files, and
/// directories that are not a session's are never touched, and no link is followed.
pub fn prune(root: &Path, min_age: Duration) -> Result<Vec<String>> {
    match std::fs::symlink_metadata(root) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
        Ok(meta) if meta.file_type().is_symlink() || !meta.is_dir() => {
            return Err(ClientError::msg(format!(
                "{} is a link or not a directory; refusing to remove anything below it",
                root.display()
            )));
        }
        Ok(_) => {}
    }
    let now = SystemTime::now();
    let mut pruned = Vec::new();
    for entry in std::fs::read_dir(root)?.filter_map(|entry| entry.ok()) {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_symlink() || !kind.is_dir() {
            continue;
        }
        let directory = entry.path();
        let ended = match read_record(&directory) {
            Some(record) => !game_alive(&record),
            None => {
                let age = entry
                    .metadata()
                    .and_then(|meta| meta.modified())
                    .ok()
                    .and_then(|modified| now.duration_since(modified).ok());
                age.is_some_and(|age| age >= min_age) && is_leftover(&directory)
            }
        };
        if ended && remove_session_directory(&directory) {
            pruned.push(entry.file_name().to_string_lossy().into_owned());
        }
    }
    pruned.sort();
    Ok(pruned)
}

/// Whether `name`, a file directly in a session directory, is one that the client or the game
/// writes there (or the temporary name it is written under).
fn is_session_file(name: &str) -> bool {
    let name = name.strip_suffix(".tmp").unwrap_or(name);
    matches!(
        name,
        RECORD | "ready.json" | "closed.json" | "game.log" | ".probe"
    ) || ((name.starts_with("cmd-") || name.starts_with("resp-")) && name.ends_with(".json"))
}

/// Whether a directory without a record is what a start that failed leaves: a `game.log` (every
/// start opens one before it runs the game), and nothing but session files and a `shots` directory
/// of plain files.
fn is_leftover(directory: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return false;
    };
    let mut has_log = false;
    for entry in entries {
        let Ok(entry) = entry else {
            return false;
        };
        let Ok(kind) = entry.file_type() else {
            return false;
        };
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let fits = if kind.is_file() {
            has_log |= name == "game.log";
            is_session_file(&name)
        } else {
            name == "shots" && kind.is_dir() && holds_only_files(&entry.path())
        };
        if !fits {
            return false;
        }
    }
    has_log
}

fn holds_only_files(directory: &Path) -> bool {
    std::fs::read_dir(directory).is_ok_and(|mut entries| {
        entries.all(|entry| {
            entry
                .and_then(|entry| entry.file_type())
                .is_ok_and(|kind| kind.is_file())
        })
    })
}

/// Removes a session directory with everything in it, `session.json` last so that a directory that
/// cannot be removed whole is still known as a session's; returns whether it is gone. Links inside
/// are removed, never followed.
fn remove_session_directory(directory: &Path) -> bool {
    if let Ok(entries) = std::fs::read_dir(directory) {
        for entry in entries.filter_map(|entry| entry.ok()) {
            if entry.file_name() != RECORD {
                remove_entry(&entry, false);
            }
        }
    }
    let _ = std::fs::remove_file(directory.join(RECORD));
    std::fs::remove_dir(directory).is_ok()
}

/// Removes a link itself (a symbolic link or, on Windows, a junction), never what it points to.
fn remove_link(path: &Path) {
    if std::fs::remove_file(path).is_err() {
        let _ = std::fs::remove_dir(path);
    }
}

/// Removes one entry of a directory: a link itself, a file, or a directory with what it holds
/// (`keep_directories` empties it and leaves it in place).
fn remove_entry(entry: &std::fs::DirEntry, keep_directories: bool) {
    let path = entry.path();
    // `DirEntry::file_type` does not follow links: what a link points to is not ours to delete.
    let Ok(kind) = entry.file_type() else {
        return;
    };
    if kind.is_symlink() {
        remove_link(&path);
    } else if kind.is_dir() {
        remove_contents(&path, keep_directories);
        if !keep_directories {
            let _ = std::fs::remove_dir(&path);
        }
    } else {
        let _ = std::fs::remove_file(&path);
    }
}

fn remove_contents(directory: &Path, keep_directories: bool) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.filter_map(|entry| entry.ok()) {
        remove_entry(&entry, keep_directories);
    }
}

/// Removes what a finished session left, so a new one starts clean (the old log goes too). The
/// directories inside stay, emptied; links are removed, never followed.
pub fn clear_directory(directory: &Path) {
    remove_contents(directory, true);
}
