//! Git Bash (MSYS) rewrites an argument that starts with `/` into a Windows path under its own
//! root: `/root/App` becomes `C:/Program Files/Git/root/App`. Node paths start with `/root`, so
//! the client undoes the rewrite.

use crate::config::Context;
use std::process::Command;

fn looks_like_drive_path(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes[2] == b'/' || bytes[2] == b'\\')
}

/// Removes `root` from the front of `path`; a path that does not start with it is returned as is.
pub fn strip_root(path: &str, root: &str) -> String {
    let root = root.trim_end_matches('/');
    let normalized = path.replace('\\', "/");
    match normalized.strip_prefix(root) {
        Some(rest) if !root.is_empty() && rest.starts_with('/') => rest.to_string(),
        _ => path.to_string(),
    }
}

/// The directory an MSYS shell maps `/` to, as a Windows path with forward slashes: what `cygpath`
/// says, else what the shell's `EXEPATH` implies (`exepath`, read by the caller from the
/// environment).
pub fn detect_root(exepath: Option<&str>) -> Option<String> {
    if let Ok(output) = Command::new("cygpath").args(["-m", "/"]).output() {
        let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !text.is_empty() {
            return Some(text.trim_end_matches('/').to_string());
        }
    }
    let exepath = exepath?.replace('\\', "/");
    let exepath = exepath.trim_end_matches('/');
    let root = exepath.strip_suffix("/bin").unwrap_or(exepath);
    (!root.is_empty()).then(|| root.to_string())
}

/// A node path as the user typed it. Outside an MSYS shell, or for a path that was not rewritten,
/// it is returned unchanged.
pub fn unmangle_node_path(path: &str, ctx: &Context) -> String {
    if !ctx.msystem || !looks_like_drive_path(path) {
        return path.to_string();
    }
    match ctx.msys_root() {
        Some(root) => strip_root(path, &root),
        None => path.to_string(),
    }
}
