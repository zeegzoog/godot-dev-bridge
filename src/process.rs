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

#[cfg(windows)]
pub fn kill(pid: u32) {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{OpenProcess, TerminateProcess};

    const PROCESS_TERMINATE: u32 = 0x0001;
    if pid == 0 {
        return;
    }
    // SAFETY: plain Win32 calls on a handle that is opened and closed here.
    unsafe {
        let handle = OpenProcess(PROCESS_TERMINATE, 0, pid);
        if (handle as usize) != 0 {
            TerminateProcess(handle, 1);
            CloseHandle(handle);
        }
    }
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
