//! The notebook: notes, drafts and evidence, one file each, in a private directory under the spool
//! root. Every entry records the session name it belongs to; a notebook is the entries of one name.

use crate::error::{ClientError, Result};
use crate::session::ensure_private_dir;
use crate::spool::{new_request_id, write_json_atomically};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// The directory below the root. `@` is not allowed in a session name, so no session can be started
/// over it, and it has neither a session record nor a game log, so listing and pruning pass over it.
pub const DIR_NAME: &str = "@feedback";

pub const MAX_NOTES: usize = 20;
pub const MAX_DRAFTS: usize = 10;
pub const MAX_EVIDENCE: usize = 50;
/// Entries unchanged for this long are removed.
pub const LIFETIME: Duration = Duration::from_secs(7 * 24 * 60 * 60);
/// A submission lock older than this was left by a run that did not finish.
pub const LOCK_STALE: Duration = Duration::from_secs(120);

const NOTE: &str = "note-";
const DRAFT: &str = "draft-";
const EVIDENCE: &str = "evidence-";
const LOCK_SUFFIX: &str = ".lock";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Note {
    pub id: String,
    pub session: String,
    pub created: f64,
    pub kind: String,
    pub summary: String,
    pub details: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verb: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub impact: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Draft {
    pub id: String,
    pub session: String,
    pub created: f64,
    pub notes: Vec<String>,
    pub title: String,
    pub body: String,
    pub fingerprint: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<String>,
    /// `created` or `unknown` once a submission ended that way; absent while the draft is pending.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// Something the client observed itself: a failure of a verb, or an attempt at a verb or option
/// that does not exist.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub session: String,
    /// `failure`, `unknown_verb` or `unknown_option`.
    pub kind: String,
    pub verb: String,
    pub code: String,
    /// The scrubbed first line of the message, or the attempted name.
    pub message: String,
    pub count: u64,
    pub first_seen: f64,
    pub last_seen: f64,
}

pub fn now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0.0, |elapsed| elapsed.as_secs_f64())
}

pub fn directory(root: &Path) -> PathBuf {
    root.join(DIR_NAME)
}

/// The entries of one session name.
pub struct Notebook {
    dir: PathBuf,
    session: String,
}

/// The regular files of `dir` (never a link) whose names start with `prefix` and end with `suffix`.
fn files(dir: &Path, prefix: &str, suffix: &str) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut found: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .filter(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            name.starts_with(prefix) && name.ends_with(suffix)
        })
        .map(|entry| entry.path())
        .collect();
    found.sort();
    found
}

fn read<T: for<'de> Deserialize<'de>>(path: &Path) -> Option<T> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

fn age(path: &Path) -> Option<Duration> {
    let modified = std::fs::symlink_metadata(path).ok()?.modified().ok()?;
    SystemTime::now().duration_since(modified).ok()
}

fn write<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    write_json_atomically(path, &serde_json::to_value(value)?)?;
    Ok(())
}

/// A short id: a letter and six hexadecimal digits.
fn short_id(letter: char) -> String {
    let long = new_request_id();
    format!("{letter}{}", &long[long.len() - 6..])
}

/// Whether the notebook directory exists as a real directory (not a link).
fn is_real_dir(dir: &Path) -> bool {
    std::fs::symlink_metadata(dir).is_ok_and(|meta| meta.is_dir() && !meta.file_type().is_symlink())
}

/// Removes the entries unchanged for `LIFETIME` and the stale locks. Only the channel's own regular
/// files are touched.
fn expire(dir: &Path) {
    for prefix in [NOTE, DRAFT, EVIDENCE] {
        for path in files(dir, prefix, ".json") {
            if age(&path).is_some_and(|age| age >= LIFETIME) {
                let _ = std::fs::remove_file(path);
            }
        }
    }
    for path in files(dir, DRAFT, LOCK_SUFFIX) {
        if age(&path).is_some_and(|age| age >= LOCK_STALE) {
            let _ = std::fs::remove_file(path);
        }
    }
}

impl Notebook {
    /// Opens the notebook of `session`, creating the root and the directory private to the user,
    /// and removes expired entries.
    pub fn open(root: &Path, session: &str) -> Result<Self> {
        ensure_private_dir(root)?;
        let dir = directory(root);
        ensure_private_dir(&dir)?;
        expire(&dir);
        Ok(Self {
            dir,
            session: session.to_string(),
        })
    }

    /// The notebook without creating anything, or `None` when there is none yet.
    pub fn existing(root: &Path, session: &str) -> Option<Self> {
        let dir = directory(root);
        is_real_dir(&dir).then(|| Self {
            dir,
            session: session.to_string(),
        })
    }

    pub fn session(&self) -> &str {
        &self.session
    }

    fn own<T: for<'de> Deserialize<'de>>(
        &self,
        prefix: &str,
        session_of: fn(&T) -> &str,
    ) -> Vec<T> {
        files(&self.dir, prefix, ".json")
            .iter()
            .filter_map(|path| read::<T>(path))
            .filter(|entry| session_of(entry) == self.session)
            .collect()
    }

    /// The notes, oldest first.
    pub fn notes(&self) -> Vec<Note> {
        let mut notes = self.own::<Note>(NOTE, |note| &note.session);
        notes.sort_by(|a, b| a.created.total_cmp(&b.created).then(a.id.cmp(&b.id)));
        notes
    }

    /// The drafts, oldest first.
    pub fn drafts(&self) -> Vec<Draft> {
        let mut drafts = self.own::<Draft>(DRAFT, |draft| &draft.session);
        drafts.sort_by(|a, b| a.created.total_cmp(&b.created).then(a.id.cmp(&b.id)));
        drafts
    }

    pub fn events(&self) -> Vec<Event> {
        self.own::<Event>(EVIDENCE, |event| &event.session)
    }

    /// The notes no draft lists yet.
    pub fn undrafted(&self) -> Vec<Note> {
        let drafted: Vec<String> = self
            .drafts()
            .into_iter()
            .flat_map(|draft| draft.notes)
            .collect();
        self.notes()
            .into_iter()
            .filter(|note| !drafted.contains(&note.id))
            .collect()
    }

    fn new_id(&self, letter: char, prefix: &str) -> String {
        loop {
            let id = short_id(letter);
            if !self.dir.join(format!("{prefix}{id}.json")).exists() {
                return id;
            }
        }
    }

    /// Adds a note (its `id`, `session` and `created` are filled in here).
    pub fn add_note(&self, mut note: Note) -> Result<Note> {
        if self.notes().len() >= MAX_NOTES {
            return Err(ClientError::msg(format!(
                "this notebook already holds {MAX_NOTES} notes, the most it keeps; draft them with \
                 `feedback draft` or remove some with `feedback discard`"
            )));
        }
        note.id = self.new_id('n', NOTE);
        note.session = self.session.clone();
        note.created = now();
        write(&self.dir.join(format!("{NOTE}{}.json", note.id)), &note)?;
        Ok(note)
    }

    /// Adds a draft (its `id`, `session` and `created` are filled in here).
    pub fn add_draft(&self, mut draft: Draft) -> Result<Draft> {
        if self.drafts().len() >= MAX_DRAFTS {
            return Err(ClientError::msg(format!(
                "this notebook already holds {MAX_DRAFTS} drafts, the most it keeps; remove some \
                 with `feedback discard`"
            )));
        }
        draft.id = self.new_id('d', DRAFT);
        draft.session = self.session.clone();
        draft.created = now();
        self.save_draft(&draft)?;
        Ok(draft)
    }

    pub fn save_draft(&self, draft: &Draft) -> Result<()> {
        write(&self.dir.join(format!("{DRAFT}{}.json", draft.id)), draft)
    }

    /// Removes the notes and drafts with these ids. Every id must belong to this notebook.
    pub fn discard(&self, ids: &[String]) -> Result<Vec<String>> {
        let notes: Vec<String> = self.notes().into_iter().map(|note| note.id).collect();
        let drafts: Vec<String> = self.drafts().into_iter().map(|draft| draft.id).collect();
        if let Some(unknown) = ids
            .iter()
            .find(|id| !notes.contains(id) && !drafts.contains(id))
        {
            let mut known = notes.clone();
            known.extend(drafts.clone());
            return Err(ClientError::msg(format!(
                "no note or draft '{unknown}' in this notebook; it holds: {}",
                if known.is_empty() {
                    "nothing".to_string()
                } else {
                    known.join(", ")
                }
            )));
        }
        let mut removed = Vec::new();
        for id in ids {
            let prefix = if notes.contains(id) { NOTE } else { DRAFT };
            let path = self.dir.join(format!("{prefix}{id}.json"));
            if std::fs::remove_file(&path).is_ok() && !removed.contains(id) {
                removed.push(id.clone());
            }
        }
        Ok(removed)
    }

    /// Removes every note, draft and piece of evidence of this notebook.
    pub fn discard_all(&self) -> Vec<String> {
        let mut removed = Vec::new();
        for path in files(&self.dir, NOTE, ".json") {
            if let Some(note) = read::<Note>(&path).filter(|note| note.session == self.session) {
                if std::fs::remove_file(&path).is_ok() {
                    removed.push(note.id);
                }
            }
        }
        for path in files(&self.dir, DRAFT, ".json") {
            if let Some(draft) = read::<Draft>(&path).filter(|draft| draft.session == self.session)
            {
                if std::fs::remove_file(&path).is_ok() {
                    removed.push(draft.id);
                }
            }
        }
        for path in files(&self.dir, EVIDENCE, ".json") {
            if read::<Event>(&path).is_some_and(|event| event.session == self.session) {
                let _ = std::fs::remove_file(&path);
            }
        }
        removed
    }

    /// Counts one occurrence of an event: the file of the same event grows its count, a new event
    /// takes the place of the least recently seen one when the directory is full.
    pub fn record_event(&self, kind: &str, verb: &str, code: &str, message: &str, shape: &str) {
        let key = crate::feedback::scrub::fnv1a64(
            format!("{}\n{kind}\n{verb}\n{code}\n{shape}", self.session).as_bytes(),
        );
        let path = self.dir.join(format!("{EVIDENCE}{key:016x}.json"));
        let seen = now();
        let event = match read::<Event>(&path) {
            Some(mut event) => {
                event.count = event.count.saturating_add(1);
                event.last_seen = seen;
                event.message = message.to_string();
                event
            }
            None => {
                let mut all: Vec<(f64, PathBuf)> = files(&self.dir, EVIDENCE, ".json")
                    .into_iter()
                    .map(|path| {
                        let last = read::<Event>(&path).map_or(0.0, |event| event.last_seen);
                        (last, path)
                    })
                    .collect();
                all.sort_by(|a, b| a.0.total_cmp(&b.0));
                let excess = (all.len() + 1).saturating_sub(MAX_EVIDENCE);
                for (_, old) in all.into_iter().take(excess) {
                    let _ = std::fs::remove_file(old);
                }
                Event {
                    session: self.session.clone(),
                    kind: kind.to_string(),
                    verb: verb.to_string(),
                    code: code.to_string(),
                    message: message.to_string(),
                    count: 1,
                    first_seen: seen,
                    last_seen: seen,
                }
            }
        };
        let _ = write(&path, &event);
    }

    /// Takes the submission lock of a draft; it is released when the guard is dropped. A lock left
    /// by a run that did not finish is replaced once it is stale.
    pub fn lock(&self, draft: &str) -> Result<SubmitLock> {
        let path = self.dir.join(format!("{DRAFT}{draft}{LOCK_SUFFIX}"));
        for _ in 0..2 {
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(_) => return Ok(SubmitLock { path }),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    if age(&path).is_some_and(|age| age >= LOCK_STALE) {
                        let _ = std::fs::remove_file(&path);
                        continue;
                    }
                    return Err(ClientError::msg(format!(
                        "a submission of draft {draft} is in progress; wait for it to end (a run \
                         that was interrupted releases it after {} seconds)",
                        LOCK_STALE.as_secs()
                    )));
                }
                Err(error) => return Err(error.into()),
            }
        }
        Err(ClientError::msg(format!(
            "the submission lock of draft {draft} could not be taken"
        )))
    }
}

/// Held while a draft is being submitted.
pub struct SubmitLock {
    path: PathBuf,
}

impl Drop for SubmitLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}
