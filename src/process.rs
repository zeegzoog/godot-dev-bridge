//! Process liveness and termination without shelling out.

#[cfg(unix)]
pub fn is_alive(pid: u32) -> bool {
    if pid == 0 || pid > i32::MAX as u32 {
        return false;
    }
    // SAFETY: signal 0 only checks that the process exists; nothing is delivered.
    let result = unsafe { libc::kill(pid as i32, 0) };
    result == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
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

/// Ends the process and everything it started: Godot's console wrapper (`GODOT_CONSOLE_PATH`) runs
/// the engine as a child, and killing the wrapper alone would leave the game running.
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

    /// Godot's console wrapper starts the engine as a child, so ending a game means ending a tree.
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
