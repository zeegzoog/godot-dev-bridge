//! Process liveness, identity and termination.

/// How far after the recorded session start a process may have been created and still be the game
/// (the game is spawned just before the start time is written).
const START_SLACK_SECONDS: f64 = 5.0;

/// Whether `pid` is still the game of a session that started at `started` (seconds since the Unix
/// epoch). The operating system hands a pid to another program once its owner is gone, so a live pid
/// alone proves nothing: where the creation time is known, a process created after the session
/// started is somebody else, and must neither block a new start nor be killed by `stop`.
pub fn is_alive_since(pid: u32, started: f64) -> bool {
    is_alive(pid) && started_at(pid).is_none_or(|created| created <= started + START_SLACK_SECONDS)
}

#[cfg(unix)]
pub fn is_alive(pid: u32) -> bool {
    if pid == 0 || pid > i32::MAX as u32 {
        return false;
    }
    // SAFETY: signal 0 only checks that the process exists; nothing is delivered.
    let result = unsafe { libc::kill(pid as i32, 0) };
    let exists = result == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM);
    exists && !is_zombie(pid)
}

/// A process that has exited but was not waited for by its parent still answers `kill(pid, 0)`
/// (in a container whose first process never reaps, a game that quit stays one for good).
#[cfg(target_os = "linux")]
fn is_zombie(pid: u32) -> bool {
    proc_stat_fields(pid)
        .and_then(|fields| fields.into_iter().next())
        .is_some_and(|state| state == "Z" || state == "X")
}

#[cfg(all(unix, not(target_os = "linux")))]
fn is_zombie(_pid: u32) -> bool {
    false
}

/// The fields of `/proc/<pid>/stat` after the command name (which is in parentheses and may hold
/// spaces): the state first, `starttime` as the 20th.
#[cfg(target_os = "linux")]
fn proc_stat_fields(pid: u32) -> Option<Vec<String>> {
    let text = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let rest = text.get(text.rfind(')')? + 1..)?;
    Some(rest.split_whitespace().map(str::to_string).collect())
}

/// When the process was created, in seconds since the Unix epoch.
#[cfg(target_os = "linux")]
pub fn started_at(pid: u32) -> Option<f64> {
    let ticks: f64 = proc_stat_fields(pid)?.get(19)?.parse().ok()?;
    // SAFETY: a plain query of a system constant.
    let per_second = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
    if per_second <= 0 {
        return None;
    }
    let boot: f64 = std::fs::read_to_string("/proc/stat")
        .ok()?
        .lines()
        .find_map(|line| line.strip_prefix("btime ")?.trim().parse().ok())?;
    Some(boot + ticks / per_second as f64)
}

/// Not known on this platform: the pid alone has to do.
#[cfg(all(unix, not(target_os = "linux")))]
pub fn started_at(_pid: u32) -> Option<f64> {
    None
}

#[cfg(unix)]
pub fn kill(pid: u32) {
    if pid == 0 || pid > i32::MAX as u32 {
        return;
    }
    // SAFETY: SIGKILL on a process id the caller owns.
    unsafe {
        libc::kill(pid as i32, libc::SIGKILL);
    }
}

#[cfg(windows)]
pub fn is_alive(pid: u32) -> bool {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{GetExitCodeProcess, OpenProcess};

    const QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    const STILL_ACTIVE: u32 = 259;
    if pid == 0 {
        return false;
    }
    // SAFETY: plain Win32 calls on a handle that is opened and closed here.
    unsafe {
        let handle = OpenProcess(QUERY_LIMITED_INFORMATION, 0, pid);
        if (handle as usize) == 0 {
            return false;
        }
        let mut code: u32 = 0;
        let queried = GetExitCodeProcess(handle, &mut code);
        CloseHandle(handle);
        queried != 0 && code == STILL_ACTIVE
    }
}

/// When the process was created, in seconds since the Unix epoch.
#[cfg(windows)]
pub fn started_at(pid: u32) -> Option<f64> {
    use windows_sys::Win32::Foundation::{CloseHandle, FILETIME};
    use windows_sys::Win32::System::Threading::{GetProcessTimes, OpenProcess};

    const QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    // Seconds between 1601-01-01 (the FILETIME epoch) and 1970-01-01.
    const SECONDS_1601_TO_1970: f64 = 11_644_473_600.0;
    if pid == 0 {
        return None;
    }
    // SAFETY: plain Win32 calls on a handle that is opened and closed here; the FILETIME structures
    // are plain data that the call fills in.
    unsafe {
        let handle = OpenProcess(QUERY_LIMITED_INFORMATION, 0, pid);
        if (handle as usize) == 0 {
            return None;
        }
        let zero = FILETIME {
            dwLowDateTime: 0,
            dwHighDateTime: 0,
        };
        let (mut created, mut exited, mut kernel, mut user) = (zero, zero, zero, zero);
        let known = GetProcessTimes(handle, &mut created, &mut exited, &mut kernel, &mut user);
        CloseHandle(handle);
        if known == 0 {
            return None;
        }
        let ticks = (u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime);
        // FILETIME counts 100-nanosecond ticks.
        Some(ticks as f64 / 10_000_000.0 - SECONDS_1601_TO_1970)
    }
}

/// Ends the process and everything it started: a wrapper executable (Godot's console build is one)
/// runs the engine as a child, and killing the wrapper alone would leave the game running.
#[cfg(windows)]
pub fn kill(pid: u32) {
    use std::process::{Command, Stdio};

    if pid == 0 {
        return;
    }
    let _ = Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_current_process_is_alive() {
        assert!(is_alive(std::process::id()));
    }

    #[test]
    fn zero_and_unused_ids_are_not_alive() {
        assert!(!is_alive(0));
        assert!(!is_alive(u32::MAX - 1));
    }

    /// A wrapper executable starts the engine as a child, so ending a game means ending a tree.
    #[cfg(windows)]
    #[test]
    fn killing_a_process_also_ends_its_children() {
        use std::io::Read;
        use std::process::Stdio;
        use std::sync::mpsc::channel;
        use std::time::Duration;

        let mut parent = std::process::Command::new("cmd")
            .args(["/C", "ping -n 60 127.0.0.1"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut stdout = parent.stdout.take().unwrap();
        std::thread::sleep(Duration::from_millis(700)); // let cmd start its child
        kill(parent.id());
        let (done, finished) = channel();
        std::thread::spawn(move || {
            let mut sink = Vec::new();
            let _ = stdout.read_to_end(&mut sink);
            let _ = done.send(());
        });
        // The child holds the pipe's write end: the pipe closes only when the whole tree is gone.
        assert!(
            finished.recv_timeout(Duration::from_secs(5)).is_ok(),
            "a child of the killed process is still running"
        );
        let _ = parent.wait();
    }

    fn now() -> f64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs_f64()
    }

    #[test]
    fn the_current_process_is_the_game_of_a_session_that_just_started() {
        assert!(is_alive_since(std::process::id(), now()));
    }

    /// A pid can be handed to a different program after the game died; it must not be mistaken for
    /// the game (and killed by `stop`).
    #[cfg(any(windows, target_os = "linux"))]
    #[test]
    fn a_process_created_after_the_session_started_is_not_its_game() {
        assert!(is_alive(std::process::id()));
        assert!(!is_alive_since(std::process::id(), 0.0));
    }

    #[cfg(any(windows, target_os = "linux"))]
    #[test]
    fn a_process_knows_when_it_was_created() {
        let created = started_at(std::process::id()).expect("the creation time is known");
        let current = now();
        assert!(
            created <= current + 1.0 && created > current - 3600.0,
            "{created} vs {current}"
        );
    }

    /// An exited child stays a zombie until its parent waits for it; `kill(pid, 0)` still succeeds.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_zombie_is_not_alive() {
        let mut child = std::process::Command::new("true").spawn().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(300));
        assert!(!is_alive(child.id()));
        child.wait().unwrap();
    }

    #[test]
    fn a_finished_child_is_not_alive() {
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .arg("--list")
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let pid = child.id();
        child.wait().unwrap();
        assert!(!is_alive(pid));
    }
}
