//! Everything the client learns from its environment: the spool root, the session name, the
//! Godot executable and the project directory. Resolution functions take an explicit `Context`,
//! so tests never touch the process environment.

use crate::error::{ClientError, Result};
use std::cell::OnceCell;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default)]
pub struct GodotEnv {
    pub godot_path: Option<String>,
    pub godot: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct Context {
    /// The directory every session lives under.
    pub root: PathBuf,
    /// `DEV_BRIDGE_SESSION`, when set.
    pub session_env: Option<String>,
    pub cwd: PathBuf,
    pub godot_env: GodotEnv,
    /// Running under an MSYS shell such as Git Bash (`MSYSTEM` is set).
    pub msystem: bool,
    /// `EXEPATH` (Git Bash): where the MSYS root can be found when `cygpath` is not at hand.
    pub exepath: Option<String>,
    /// The MSYS root, looked up on first use (see `Context::msys_root`).
    pub msys_root: OnceCell<Option<String>>,
    pub windows: bool,
    pub path_var: Option<OsString>,
}

/// A variable's value, where an empty value counts as "not set".
fn non_empty(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

impl Context {
    /// Reads the real environment, after loading a `.env` file from the working directory or a
    /// parent (variables that are already set win).
    pub fn from_env() -> Self {
        match dotenvy::dotenv() {
            Ok(_) => {}
            Err(error) if error.not_found() => {}
            Err(error) => eprintln!(
                "warning: ignoring the .env file because it could not be parsed ({error}); \
                 put Windows paths in single quotes, e.g. GODOT_PATH='C:\\Godot\\Godot.exe'"
            ),
        }
        let cwd = std::env::current_dir().unwrap_or_default();
        Self {
            // Absolute, because the game runs with the project as its working directory and
            // would otherwise look for the session directory in the wrong place.
            root: absolutize(
                &cwd,
                &non_empty("DEV_BRIDGE_ROOT")
                    .map(PathBuf::from)
                    .unwrap_or_else(|| {
                        let runtime = non_empty("XDG_RUNTIME_DIR")
                            .map(PathBuf::from)
                            .filter(|dir| dir.is_absolute());
                        default_root(&std::env::temp_dir(), runtime.as_deref(), effective_uid())
                    }),
            ),
            session_env: non_empty("DEV_BRIDGE_SESSION"),
            cwd,
            godot_env: GodotEnv {
                godot_path: non_empty("GODOT_PATH"),
                godot: non_empty("GODOT"),
            },
            msystem: std::env::var_os("MSYSTEM").is_some(),
            exepath: non_empty("EXEPATH"),
            msys_root: OnceCell::new(),
            windows: cfg!(windows),
            path_var: std::env::var_os("PATH"),
        }
    }

    /// The directory an MSYS shell maps `/` to, as a Windows path with forward slashes. Looked up
    /// once, and only by a command that was given a path that the shell rewrote.
    pub fn msys_root(&self) -> Option<String> {
        self.msys_root
            .get_or_init(|| crate::msys::detect_root(self.exepath.as_deref()))
            .clone()
    }

    /// The session name: the flag, else the environment, else `default`.
    pub fn session_name(&self, given: Option<&str>) -> String {
        given
            .filter(|name| !name.is_empty())
            .map(str::to_string)
            .or_else(|| self.session_env.clone())
            .unwrap_or_else(|| "default".to_string())
    }
}

/// Where sessions live when `DEV_BRIDGE_ROOT` is not set, private to the user: on Unix the
/// per-user runtime directory when there is one (`runtime_dir`), otherwise a directory named after
/// the user id in the shared temporary directory, since `/tmp` is shared and its names are
/// predictable. Windows and macOS (`uid` is `None`) give every user a temporary directory of their
/// own.
pub fn default_root(temp: &Path, runtime_dir: Option<&Path>, uid: Option<u32>) -> PathBuf {
    match (runtime_dir, uid) {
        (Some(runtime), Some(_)) => runtime.join("dev_bridge"),
        (None, Some(uid)) => temp.join(format!("dev_bridge-{uid}")),
        (_, None) => temp.join("dev_bridge"),
    }
}

#[cfg(unix)]
fn effective_uid() -> Option<u32> {
    // SAFETY: a plain query of the calling process's identity.
    Some(unsafe { libc::geteuid() })
}

#[cfg(not(unix))]
fn effective_uid() -> Option<u32> {
    None
}

/// `path` itself when it is absolute, otherwise `path` below `cwd`, with the platform's own
/// separators throughout (a `DEV_BRIDGE_ROOT` written with `/` would otherwise print mixed ones).
pub fn absolutize(cwd: &Path, path: &Path) -> PathBuf {
    let joined = if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    };
    joined.components().collect()
}

/// A session name becomes a directory name below the spool root, and starting a session clears
/// that directory, so a name must not be able to point anywhere else: 1 to 64 ASCII letters,
/// digits, `.`, `_` or `-`, and neither `.` nor `..`.
pub fn validate_session_name(name: &str) -> Result<()> {
    let plain = !name.is_empty()
        && name.len() <= 64
        && name != "."
        && name != ".."
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'));
    if plain {
        Ok(())
    } else {
        Err(ClientError::msg(format!(
            "invalid session name '{name}': use 1 to 64 letters, digits, '.', '_' or '-' (not '.' or '..')"
        )))
    }
}

fn find_in_path(ctx: &Context) -> Option<PathBuf> {
    let names: &[&str] = if ctx.windows {
        &["godot.exe", "godot"]
    } else {
        &["godot"]
    };
    std::env::split_paths(ctx.path_var.as_ref()?)
        .flat_map(|dir| names.iter().map(move |name| dir.join(name)))
        .find(|candidate| candidate.is_file())
}

/// The Godot executable: `--godot`, then `GODOT_PATH`, `GODOT`, then `godot` on `PATH`. An
/// explicit `--godot` that does not exist is an error.
pub fn resolve_godot(ctx: &Context, explicit: Option<&str>) -> Result<PathBuf> {
    if let Some(path) = explicit.filter(|path| !path.trim().is_empty()) {
        let path = PathBuf::from(path);
        return if path.is_file() {
            Ok(path)
        } else {
            Err(ClientError::msg(format!(
                "--godot {} does not exist",
                path.display()
            )))
        };
    }

    let candidates = [
        ("GODOT_PATH", &ctx.godot_env.godot_path),
        ("GODOT", &ctx.godot_env.godot),
    ];

    let mut tried: Vec<String> = Vec::new();
    for (name, value) in candidates {
        let Some(value) = value else { continue };
        let path = PathBuf::from(value);
        if path.is_file() {
            return Ok(path);
        }
        tried.push(format!("{name}={value} (not found)"));
    }
    if let Some(found) = find_in_path(ctx) {
        return Ok(found);
    }
    tried.push("`godot` on PATH (not found)".to_string());
    Err(ClientError::msg(format!(
        "no Godot executable: tried {}. Pass --godot or set GODOT_PATH.",
        tried.join(", ")
    )))
}

fn has_project(dir: &Path) -> bool {
    dir.join("project.godot").is_file()
}

/// The physics tick rate a project sets (`physics/common/physics_ticks_per_second` in
/// `project.godot`), or `None` when it keeps the engine default or the file cannot be read.
pub fn project_physics_rate(project: &Path) -> Option<u32> {
    let text = std::fs::read_to_string(project.join("project.godot")).ok()?;
    let mut in_physics = false;
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            in_physics = line == "[physics]";
        } else if in_physics {
            if let Some(value) = line.strip_prefix("common/physics_ticks_per_second=") {
                let rate: f64 = value.trim().parse().ok()?;
                return (rate >= 1.0 && rate.fract() == 0.0 && rate <= f64::from(u32::MAX))
                    .then_some(rate as u32);
            }
        }
    }
    None
}

/// The project directory: `--project`, else the first directory at or above `cwd` that holds a
/// `project.godot`, else the single child directory of `cwd` that does.
pub fn find_project(cwd: &Path, explicit: Option<&Path>) -> Result<PathBuf> {
    if let Some(dir) = explicit {
        let dir = if dir.is_absolute() {
            dir.to_path_buf()
        } else {
            cwd.join(dir)
        };
        return if has_project(&dir) {
            Ok(dir)
        } else {
            Err(ClientError::msg(format!(
                "--project {} has no project.godot",
                dir.display()
            )))
        };
    }
    if let Some(found) = cwd.ancestors().find(|dir| has_project(dir)) {
        return Ok(found.to_path_buf());
    }
    let mut children: Vec<PathBuf> = std::fs::read_dir(cwd)
        .map(|entries| {
            entries
                .filter_map(|entry| entry.ok().map(|entry| entry.path()))
                .filter(|path| path.is_dir() && has_project(path))
                .collect()
        })
        .unwrap_or_default();
    children.sort();
    match children.len() {
        1 => Ok(children.remove(0)),
        0 => Err(ClientError::msg(format!(
            "no project.godot in {}, above it or one level below it; pass --project",
            cwd.display()
        ))),
        _ => Err(ClientError::msg(format!(
            "several Godot projects below {}: {}; pass --project",
            cwd.display(),
            children
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ))),
    }
}
