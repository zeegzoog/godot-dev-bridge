mod common;

use clap::Parser;
use common::{FakeGame, ok};
use godot_dev_bridge::cli::{Cli, StartArgs, Verb};
use godot_dev_bridge::launch::{
    build_godot_command, check_protocol, ensure_not_running, import_hint, stop_with, wait_ready,
};
use godot_dev_bridge::session::{SessionRecord, session_directory, write_record};
use serde_json::json;
use std::cell::Cell;
use std::path::Path;
use std::time::Duration;

fn start_args(extra: &[&str]) -> StartArgs {
    let mut argv = vec!["godot-dev-bridge", "start"];
    argv.extend_from_slice(extra);
    match Cli::try_parse_from(argv).unwrap().verb {
        Verb::Start(args) => args,
        other => panic!("{other:?}"),
    }
}

fn record(pid: u32) -> SessionRecord {
    SessionRecord {
        pid,
        command: vec![],
        started: 0.0,
    }
}

#[test]
fn a_headless_command_line_has_the_engine_arguments_then_the_separator_then_ours() {
    let command = build_godot_command(
        Path::new("godot"),
        Path::new("proj"),
        Path::new("spool"),
        &start_args(&["--headless"]),
    )
    .unwrap();
    assert_eq!(
        command,
        [
            "godot",
            "--path",
            "proj",
            "--fixed-fps",
            "60",
            "--headless",
            "--",
            "--bridge-dir=spool",
            "--bridge-fixed-fps=60"
        ]
    );
}

#[test]
fn a_windowed_command_line_carries_every_option_in_order() {
    let command = build_godot_command(
        Path::new("godot"),
        Path::new("proj"),
        Path::new("spool"),
        &start_args(&[
            "--fps",
            "30",
            "--window",
            "800x600",
            "--fast",
            "--scene",
            "res://a.tscn",
            "--idle-quit",
            "120",
            "--keep-mouse",
            "--",
            "--verbose",
        ]),
    )
    .unwrap();
    assert_eq!(
        command,
        [
            "godot",
            "--path",
            "proj",
            "--fixed-fps",
            "30",
            "--windowed",
            "--resolution",
            "800x600",
            "--disable-vsync",
            "--verbose",
            "res://a.tscn",
            "--",
            "--bridge-dir=spool",
            "--bridge-fixed-fps=30",
            "--bridge-idle-quit=120",
            "--bridge-keep-mouse"
        ]
    );
}

#[test]
fn a_bad_window_size_or_frame_rate_is_rejected_before_anything_starts() {
    for extra in [
        &["--window", "big"][..],
        &["--window", "10x"],
        &["--fps", "0"],
    ] {
        let error = build_godot_command(
            Path::new("godot"),
            Path::new("p"),
            Path::new("s"),
            &start_args(extra),
        )
        .unwrap_err()
        .to_string();
        assert!(
            error.contains("--window") || error.contains("--fps"),
            "{error}"
        );
    }
}

#[test]
fn a_scene_must_be_a_res_path_so_it_cannot_become_an_engine_option() {
    for scene in ["level.tscn", "--editor", "/abs/level.tscn"] {
        let flag = format!("--scene={scene}");
        let error = build_godot_command(
            Path::new("godot"),
            Path::new("p"),
            Path::new("s"),
            &start_args(&[flag.as_str()]),
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("res://"), "{scene}: {error}");
    }
}

#[test]
fn a_live_session_blocks_start_but_a_stale_one_does_not() {
    let root = tempfile::tempdir().unwrap();
    assert!(ensure_not_running(root.path(), "s").is_ok());

    let dir = session_directory(root.path(), "s");
    std::fs::create_dir_all(&dir).unwrap();
    write_record(&dir, &record(u32::MAX - 1)).unwrap();
    assert!(
        ensure_not_running(root.path(), "s").is_ok(),
        "stale session is replaced"
    );

    write_record(&dir, &record(std::process::id())).unwrap();
    let error = ensure_not_running(root.path(), "s")
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("already running") && error.contains("stop"),
        "{error}"
    );
}

#[test]
fn a_project_that_was_never_imported_gets_the_import_command_in_the_hint() {
    let project = tempfile::tempdir().unwrap();
    let hint = import_hint(project.path());
    assert!(
        hint.contains("--import") && hint.contains("--headless"),
        "{hint}"
    );
    std::fs::create_dir(project.path().join(".godot")).unwrap();
    assert_eq!(import_hint(project.path()), "");
}

#[test]
fn protocol_one_is_accepted_and_anything_else_is_named() {
    assert!(check_protocol(&json!({"protocol": 1})).is_ok());
    let error = check_protocol(&json!({"protocol": 2}))
        .unwrap_err()
        .to_string();
    assert!(
        error.contains('2') && error.contains("same release"),
        "{error}"
    );
    assert!(check_protocol(&json!({})).is_err());
}

#[test]
fn wait_ready_returns_the_ready_file_once_it_appears() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().to_path_buf();
    let writer = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(100));
        godot_dev_bridge::spool::write_json_atomically(
            &path.join("ready.json"),
            &json!({"protocol": 1, "pid": 7}),
        )
        .unwrap();
    });
    let ready = wait_ready(dir.path(), Duration::from_secs(5), &mut || None, "").unwrap();
    writer.join().unwrap();
    assert_eq!(ready["pid"], 7);
}

#[test]
fn wait_ready_reports_an_early_exit_with_the_log_and_the_hint() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("game.log"), "SCRIPT ERROR: boom\n").unwrap();
    let calls = Cell::new(0);
    let error = wait_ready(
        dir.path(),
        Duration::from_secs(5),
        &mut || {
            calls.set(calls.get() + 1);
            (calls.get() > 2).then(|| "exit status: 1".to_string())
        },
        "; run the import",
    )
    .unwrap_err()
    .to_string();
    assert!(
        error.contains("exit status: 1")
            && error.contains("boom")
            && error.contains("run the import"),
        "{error}"
    );
}

#[test]
fn wait_ready_times_out_with_the_log_and_the_hint() {
    let dir = tempfile::tempdir().unwrap();
    let error = wait_ready(
        dir.path(),
        Duration::from_millis(100),
        &mut || None,
        "; hint",
    )
    .unwrap_err()
    .to_string();
    assert!(
        error.contains("not ready within") && error.contains("hint"),
        "{error}"
    );
}

#[test]
fn stop_asks_the_game_to_quit_and_only_kills_a_game_that_stays() {
    let dir = tempfile::tempdir().unwrap();
    let _game = FakeGame::start(dir.path(), Box::new(|_, _| ok(json!({"quitting": true}))));

    let checks = Cell::new(0);
    let killed = Cell::new(false);
    let leaving = |_pid: u32| {
        checks.set(checks.get() + 1);
        checks.get() <= 3
    };
    let outcome = stop_with(
        "s",
        dir.path(),
        &record(1),
        Duration::from_secs(5),
        Duration::from_secs(2),
        &leaving,
        &|_| killed.set(true),
    );
    assert_eq!(outcome["killed"], false);
    assert!(!killed.get());

    let staying = |_pid: u32| true;
    let outcome = stop_with(
        "s",
        dir.path(),
        &record(1),
        Duration::from_secs(5),
        Duration::from_millis(150),
        &staying,
        &|_| killed.set(true),
    );
    assert_eq!(outcome["killed"], true);
    assert!(killed.get());
    assert_eq!(outcome["stopped"], true);
}
