//! Starting the game with the bridge enabled and stopping it again.

use crate::cli::StartArgs;
use crate::config::{Context, find_project, project_physics_rate, resolve_godot};
use crate::error::{ClientError, Result};
use crate::process;
use crate::session::{
    SessionRecord, clear_directory, death_note, ensure_private_dir, game_alive, log_path,
    read_record, session_directory, write_record,
};
use crate::spool::{Wait, read_json, send_request};
use serde_json::{Value, json};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// The wire protocol version this client speaks.
pub const PROTOCOL: u64 = 1;

const POLL: Duration = Duration::from_millis(10);
const STOP_GRACE: Duration = Duration::from_secs(5);

/// The engine's default physics tick rate, for a project that does not set one.
const DEFAULT_FPS: u32 = 60;

/// The fixed frame rate: `--fps`, else the physics rate the project sets, else 60. Stepping is exact
/// only when it equals the physics rate, so a project that sets its own would otherwise start in a
/// state that is not deterministic.
pub fn effective_fps(project: &Path, args: &StartArgs) -> Result<u32> {
    match args.fps {
        Some(0) => Err(ClientError::msg("--fps must be a positive number")),
        Some(fps) => Ok(fps),
        None => Ok(project_physics_rate(project).unwrap_or(DEFAULT_FPS)),
    }
}

fn parse_window(window: &str) -> bool {
    match window.split_once('x') {
        Some((width, height)) => {
            !width.is_empty()
                && !height.is_empty()
                && width.bytes().all(|b| b.is_ascii_digit())
                && height.bytes().all(|b| b.is_ascii_digit())
        }
        None => false,
    }
}

/// The exact argument list that starts the game: the engine's own arguments, then `--` and ours.
/// The first element is the executable.
pub fn build_godot_command(
    godot: &Path,
    project: &Path,
    directory: &Path,
    args: &StartArgs,
) -> Result<Vec<String>> {
    let fps = effective_fps(project, args)?;
    if !args.headless && !parse_window(&args.window) {
        return Err(ClientError::msg(format!(
            "--window must look like 1152x648, got '{}'",
            args.window
        )));
    }
    if let Some(scene) = &args.scene {
        if !scene.starts_with("res://") {
            return Err(ClientError::msg(format!(
                "--scene must be a res:// path, got '{scene}'"
            )));
        }
    }

    let mut command = vec![
        godot.to_string_lossy().into_owned(),
        "--path".into(),
        project.to_string_lossy().into_owned(),
        "--fixed-fps".into(),
        fps.to_string(),
    ];
    if args.headless {
        command.push("--headless".into());
    } else {
        command.extend([
            "--windowed".into(),
            "--resolution".into(),
            args.window.clone(),
        ]);
    }
    if args.fast {
        command.push("--disable-vsync".into());
    }
    command.extend(args.engine_args.iter().cloned());
    if let Some(scene) = &args.scene {
        command.push(scene.clone());
    }
    command.push("--".into());
    command.push(format!("--bridge-dir={}", directory.to_string_lossy()));
    command.push(format!("--bridge-fixed-fps={fps}"));
    if let Some(seconds) = args.idle_quit {
        command.push(format!("--bridge-idle-quit={seconds}"));
    }
    if args.keep_mouse {
        command.push("--bridge-keep-mouse".into());
    }
    Ok(command)
}

/// Refuses to start over a live session of the same name; a stale one (its game is gone) is fine.
pub fn ensure_not_running(root: &Path, name: &str) -> Result<()> {
    let directory = session_directory(root, name);
    match read_record(&directory) {
        Some(record) if game_alive(&record) => Err(ClientError::msg(format!(
            "session '{name}' is already running (pid {}); stop it first",
            record.pid
        ))),
        _ => Ok(()),
    }
}

/// `text` as one shell word: in double quotes when it has a space.
fn shell_word(text: &str) -> String {
    if text.contains(' ') {
        format!("\"{text}\"")
    } else {
        text.to_string()
    }
}

/// The problem to report when the project was never imported (it has no `.godot` directory), or
/// `None`. Without the import the class cache and the imported resources are missing: the game still
/// starts and the bridge still answers, but the game is broken, so `start` refuses instead of
/// reporting a success. The message carries the command to run, with the real paths.
pub fn import_required(godot: &Path, project: &Path) -> Option<String> {
    if project.join(".godot").is_dir() {
        return None;
    }
    Some(format!(
        "the project has not been imported yet: run `{} --headless --path {} --import` once, then start again",
        shell_word(&godot.to_string_lossy()),
        shell_word(&project.to_string_lossy())
    ))
}

/// The game must speak the protocol this client speaks.
pub fn check_protocol(ready: &Value) -> Result<()> {
    match ready.get("protocol").and_then(Value::as_u64) {
        Some(PROTOCOL) => Ok(()),
        other => Err(ClientError::msg(format!(
            "the addon speaks protocol {}, this client speaks {PROTOCOL}: use the client and the addon of the same release",
            other.map_or("(none)".to_string(), |n| n.to_string())
        ))),
    }
}

/// Waits for the game's `ready.json`. `poll_exit` returns a description once the game process
/// has exited. Both failures carry the end of `game.log`.
pub fn wait_ready(
    directory: &Path,
    timeout: Duration,
    poll_exit: &mut dyn FnMut() -> Option<String>,
) -> Result<Value> {
    let ready_path = directory.join("ready.json");
    let deadline = Instant::now() + timeout;
    loop {
        if ready_path.exists() {
            return read_json(&ready_path);
        }
        if let Some(status) = poll_exit() {
            return Err(ClientError::msg(format!(
                "the game ended ({status}) before it was ready{}",
                death_note(directory)
            )));
        }
        if Instant::now() >= deadline {
            return Err(ClientError::msg(format!(
                "the game was not ready within {} s{}",
                timeout.as_secs_f64(),
                death_note(directory)
            )));
        }
        std::thread::sleep(POLL);
    }
}

#[cfg(windows)]
fn detach(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    keep_own_handles_private();
    command.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
}

/// Windows lets a child inherit every inheritable handle of its parent, and the pipe this client's
/// output goes through is one. The game would then hold that pipe open for as long as it runs, and a
/// caller that waits for the pipe to close (an agent's shell tool, `Command::output()`) would wait
/// for the game to exit instead of for this client. The game's own standard handles are passed
/// explicitly, so only this client's are made private.
#[cfg(windows)]
fn keep_own_handles_private() {
    use windows_sys::Win32::Foundation::{HANDLE_FLAG_INHERIT, SetHandleInformation};
    use windows_sys::Win32::System::Console::{
        GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
    };

    for which in [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE] {
        // SAFETY: plain Win32 calls on this process's own standard handles; an invalid or missing
        // handle (no console, closed stream) is skipped.
        unsafe {
            let handle = GetStdHandle(which);
            let address = handle as isize;
            if address != 0 && address != -1 {
                SetHandleInformation(handle, HANDLE_FLAG_INHERIT, 0);
            }
        }
    }
}

#[cfg(unix)]
fn detach(command: &mut Command) {
    use std::os::unix::process::CommandExt;
    command.process_group(0);
}

/// Starts the game detached from this shell, its output appended to `log`.
fn spawn_game(command: &[String], log: &Path) -> Result<Child> {
    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log)?;
    let errors = log_file.try_clone()?;
    let mut process = Command::new(&command[0]);
    process
        .args(&command[1..])
        .stdin(Stdio::null())
        .stdout(Stdio::from(log_file))
        .stderr(Stdio::from(errors));
    detach(&mut process);
    process
        .spawn()
        .map_err(|error| ClientError::msg(format!("cannot start {}: {error}", command[0])))
}

/// Starts the game with the bridge enabled and waits until it announces itself.
pub fn start_session(
    ctx: &Context,
    session: Option<&str>,
    timeout: Duration,
    args: &StartArgs,
) -> Result<Value> {
    let name = ctx.session_name(session);
    ensure_not_running(&ctx.root, &name)?;
    let godot = resolve_godot(ctx, args.godot.as_deref())?;
    let project = find_project(&ctx.cwd, args.project.as_deref())?;
    if let Some(problem) = import_required(&godot, &project) {
        return Err(ClientError::msg(problem));
    }

    // Validated before anything is cleared: a typo must not cost the previous run's log.
    let directory = session_directory(&ctx.root, &name);
    let command = build_godot_command(&godot, &project, &directory, args)?;
    ensure_private_dir(&ctx.root)?;
    ensure_private_dir(&directory)?;
    clear_directory(&directory);

    let mut child = spawn_game(&command, &log_path(&directory))?;
    let pid = child.id();
    let started = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0.0, |elapsed| elapsed.as_secs_f64());
    write_record(
        &directory,
        &SessionRecord {
            pid,
            command,
            started,
        },
    )?;

    let ready = wait_ready(&directory, timeout, &mut || {
        child
            .try_wait()
            .ok()
            .flatten()
            .map(|status| status.to_string())
    })
    .and_then(|ready| check_protocol(&ready).map(|()| ready));
    match ready {
        Ok(ready) => Ok(json!({
            "ok": true,
            "session": name,
            "pid": pid,
            "directory": directory.to_string_lossy(),
            "ready": ready,
        })),
        Err(error) => {
            if process::is_alive(pid) {
                process::kill(pid);
            }
            Err(error)
        }
    }
}

/// Asks the game to quit, waits up to `grace`, and ends the process if it is still there.
pub fn stop_with(
    name: &str,
    directory: &Path,
    record: &SessionRecord,
    timeout: Duration,
    grace: Duration,
    is_alive: &dyn Fn(u32) -> bool,
    kill: &dyn Fn(u32),
) -> Value {
    let pid = record.pid;
    if is_alive(pid) {
        let alive_now = || is_alive(pid);
        let no_note = String::new;
        let wait = Wait {
            timeout: timeout.min(STOP_GRACE),
            is_alive: &alive_now,
            death_note: &no_note,
        };
        let _ = send_request(directory, "quit", &json!({}), &wait);
        let deadline = Instant::now() + grace;
        while is_alive(pid) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(50));
        }
    }
    let mut killed = false;
    if is_alive(pid) {
        kill(pid);
        killed = true;
    }
    json!({"ok": true, "session": name, "stopped": true, "killed": killed})
}

pub fn stop_session(ctx: &Context, session: Option<&str>, timeout: Duration) -> Result<Value> {
    let name = ctx.session_name(session);
    let directory = session_directory(&ctx.root, &name);
    let Some(record) = read_record(&directory) else {
        return Err(ClientError::msg(format!(
            "no session named '{name}' (looked in {})",
            directory.display()
        )));
    };
    Ok(stop_with(
        &name,
        &directory,
        &record,
        timeout,
        STOP_GRACE,
        &|pid| process::is_alive_since(pid, record.started),
        &process::kill,
    ))
}
