//! A stand-in for the game: answers request files in a session directory from a thread.
#![allow(dead_code)]

use godot_dev_bridge::spool::write_json_atomically;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

/// Maps `(command, args)` to the body of the response (everything except `id`).
pub type Responder = Box<dyn Fn(&str, &Value) -> Value + Send>;

pub struct FakeGame {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

pub fn ok(result: Value) -> Value {
    json!({"ok": true, "result": result, "error": null, "physics_frames": 0, "process_frames": 0})
}

pub fn fail(code: &str, message: &str) -> Value {
    json!({
        "ok": false, "result": null,
        "error": {"code": code, "message": message},
        "physics_frames": 0, "process_frames": 0
    })
}

fn pending_requests(directory: &Path) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = std::fs::read_dir(directory)
        .map(|entries| {
            entries
                .filter_map(|entry| entry.ok().map(|entry| entry.path()))
                .filter(|path| {
                    path.file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| name.starts_with("cmd-") && name.ends_with(".json"))
                })
                .collect()
        })
        .unwrap_or_default();
    found.sort();
    found
}

impl FakeGame {
    pub fn start(directory: &Path, responder: Responder) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let directory = directory.to_path_buf();
        let thread = std::thread::spawn(move || {
            while !flag.load(Ordering::Relaxed) {
                for path in pending_requests(&directory) {
                    let Ok(text) = std::fs::read_to_string(&path) else {
                        continue;
                    };
                    let Ok(request) = serde_json::from_str::<Value>(&text) else {
                        continue;
                    };
                    let id = request["id"].as_str().unwrap_or("").to_string();
                    let command = request["cmd"].as_str().unwrap_or("").to_string();
                    let mut body = responder(&command, &request["args"]);
                    body["id"] = json!(id);
                    let _ =
                        write_json_atomically(&directory.join(format!("resp-{id}.json")), &body);
                    let _ = std::fs::remove_file(&path);
                }
                std::thread::sleep(Duration::from_millis(2));
            }
        });
        Self {
            stop,
            thread: Some(thread),
        }
    }
}

impl Drop for FakeGame {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::Instant;

/// The Godot executable for tests: `GODOT_PATH`, which a `.env` file fills in when the shell did
/// not export it.
pub fn godot_path() -> PathBuf {
    let _ = dotenvy::dotenv();
    let chosen = std::env::var("GODOT_PATH")
        .ok()
        .filter(|value| !value.trim().is_empty());
    PathBuf::from(chosen.expect("set GODOT_PATH to run the Godot tests"))
}

pub fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Runs Godot in the repository root with a deadline; returns (success, stdout + stderr).
pub fn run_godot(godot: &Path, args: &[&str], timeout: Duration) -> (bool, String) {
    let scratch = tempfile::tempdir().expect("temp dir");
    let log_path = scratch.path().join("godot.log");
    let log = std::fs::File::create(&log_path).expect("log file");
    let errors = log.try_clone().expect("clone log");
    let mut child = std::process::Command::new(godot)
        .args(args)
        .current_dir(project_root())
        .stdin(Stdio::null())
        .stdout(log)
        .stderr(errors)
        .spawn()
        .expect("Godot starts");
    let read_log =
        || String::from_utf8_lossy(&std::fs::read(&log_path).unwrap_or_default()).into_owned();
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait().expect("poll Godot") {
            return (status.success(), read_log());
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            return (
                false,
                format!("timed out after {timeout:?}\n{}", read_log()),
            );
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Imports the dev project once per test binary (a fresh `.godot/` cache is needed first).
pub fn ensure_imported(godot: &Path) {
    static IMPORTED: Mutex<bool> = Mutex::new(false);
    let mut done = IMPORTED.lock().unwrap_or_else(|error| error.into_inner());
    if !*done {
        let (ok, log) = run_godot(
            godot,
            &["--headless", "--path", ".", "--import"],
            Duration::from_secs(180),
        );
        assert!(ok, "importing the dev project failed:\n{log}");
        *done = true;
    }
}

/// Copies a directory tree.
pub fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("create target directory");
    for entry in std::fs::read_dir(from)
        .expect("read source directory")
        .filter_map(|entry| entry.ok())
    {
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).expect("copy file");
        }
    }
}

/// Runs the client with piped output and a deadline; returns (exit status, stdout, stderr).
///
/// After the client has exited its output pipes must reach end-of-file. If they do not, a process
/// the client started (the game) inherited them, and every caller that waits for the pipe to close
/// (an agent's shell tool, `Command::output()`) would wait for the game to exit instead.
pub fn run_client(mut command: std::process::Command, deadline: Duration) -> (i32, String, String) {
    use std::io::Read;
    use std::sync::mpsc::channel;

    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().expect("the client starts");
    let mut stdout = child.stdout.take().expect("piped stdout");
    let mut stderr = child.stderr.take().expect("piped stderr");
    let (out_tx, out_rx) = channel();
    let (err_tx, err_rx) = channel();
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = stdout.read_to_end(&mut bytes);
        let _ = out_tx.send(bytes);
    });
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = stderr.read_to_end(&mut bytes);
        let _ = err_tx.send(bytes);
    });

    let end = Instant::now() + deadline;
    let status = loop {
        if let Some(status) = child.try_wait().expect("poll the client") {
            break status;
        }
        if Instant::now() > end {
            let _ = child.kill();
            panic!("the client did not exit within {deadline:?}");
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let grace = Duration::from_secs(5);
    let out = out_rx.recv_timeout(grace).unwrap_or_else(|_| {
        panic!(
            "the client exited but its stdout pipe stayed open: a process it started inherited it"
        )
    });
    let err = err_rx.recv_timeout(grace).unwrap_or_else(|_| {
        panic!(
            "the client exited but its stderr pipe stayed open: a process it started inherited it"
        )
    });
    (
        status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out).into_owned(),
        String::from_utf8_lossy(&err).into_owned(),
    )
}

/// The built client against a real Godot running the dev project's ticker scene, in a spool of its
/// own. Needs a Godot 4.5+ executable (see `godot_path`). Stops its session when dropped.
pub struct Bridge {
    pub root: tempfile::TempDir,
    pub session: String,
}

impl Bridge {
    pub fn new(session: &str) -> Self {
        ensure_imported(&godot_path());
        Self {
            root: tempfile::tempdir().unwrap(),
            session: session.to_string(),
        }
    }

    pub fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_godot-dev-bridge"));
        command
            .args(["--session", &self.session])
            .args(args)
            .current_dir(project_root())
            .env("DEV_BRIDGE_ROOT", self.root.path())
            .env("GODOT_PATH", godot_path());
        command
    }

    /// Runs the client; returns its exit status and the JSON object it printed.
    pub fn run(&self, args: &[&str]) -> (i32, Value) {
        let (status, stdout, stderr) = run_client(self.command(args), Duration::from_secs(120));
        let value = serde_json::from_str(stdout.trim())
            .unwrap_or_else(|error| panic!("not JSON ({error}): stdout={stdout} stderr={stderr}"));
        (status, value)
    }

    pub fn ok(&self, args: &[&str]) -> Value {
        let (status, answer) = self.run(args);
        assert_eq!(status, 0, "{args:?} -> {answer}");
        answer
    }

    pub fn start(&self) -> Value {
        self.ok(&["start", "--headless"])
    }

    pub fn ticker(&self) -> Value {
        self.ok(&["state", "ticker"])["result"]["ticker"].clone()
    }
}

impl Drop for Bridge {
    fn drop(&mut self) {
        let _ = self.command(&["stop"]).output();
    }
}

/// The stand-in `gh` of `examples/fake_gh.rs`, built for this run: a run that names one test target
/// does not build the examples, and a stale stand-in would test old behaviour.
pub fn fake_gh() -> PathBuf {
    static BUILT: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    BUILT
        .get_or_init(|| {
            let deps = std::env::current_exe().expect("the test executable");
            let profile = deps
                .parent()
                .and_then(Path::parent)
                .expect("the profile directory")
                .to_path_buf();
            let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
            let mut build = Command::new(cargo);
            build
                .args(["build", "--quiet", "--example", "fake_gh"])
                .current_dir(project_root());
            if profile.file_name().is_some_and(|name| name == "release") {
                build.arg("--release");
            }
            assert!(
                build.status().expect("cargo runs").success(),
                "building the stand-in gh failed"
            );
            profile
                .join("examples")
                .join(format!("fake_gh{}", std::env::consts::EXE_SUFFIX))
        })
        .clone()
}
