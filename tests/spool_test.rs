mod common;

use common::{FakeGame, fail, ok};
use godot_dev_bridge::spool::{
    Wait, new_request_id, read_json, send_request, write_json_atomically,
};
use serde_json::json;
use std::time::Duration;

static ALIVE: fn() -> bool = || true;
static NO_NOTE: fn() -> String = String::new;

fn wait(seconds: f64) -> Wait<'static> {
    Wait {
        timeout: Duration::from_secs_f64(seconds),
        is_alive: &ALIVE,
        death_note: &NO_NOTE,
    }
}

#[test]
fn atomic_write_leaves_no_temporary_file_and_replaces_existing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ready.json");
    write_json_atomically(&path, &json!({"a": 1})).unwrap();
    write_json_atomically(&path, &json!({"a": 2})).unwrap();
    assert_eq!(read_json(&path).unwrap()["a"], 2);
    assert!(!dir.path().join("ready.json.tmp").exists());
}

#[test]
fn request_ids_are_distinct() {
    let ids: std::collections::HashSet<String> = (0..1000).map(|_| new_request_id()).collect();
    assert_eq!(ids.len(), 1000);
    assert!(ids.iter().all(|id| id.len() == 12));
}

#[test]
fn a_request_is_answered_and_the_files_are_cleaned_up() {
    let dir = tempfile::tempdir().unwrap();
    let _game = FakeGame::start(
        dir.path(),
        Box::new(|command, args| ok(json!({"echo": command, "args": args}))),
    );
    let answer = send_request(dir.path(), "ping", &json!({"x": 1}), &wait(5.0)).unwrap();
    assert_eq!(answer["ok"], true);
    assert_eq!(answer["result"]["echo"], "ping");
    assert_eq!(answer["result"]["args"]["x"], 1);
    let leftovers: Vec<_> = std::fs::read_dir(dir.path()).unwrap().collect();
    assert!(leftovers.is_empty(), "left behind: {leftovers:?}");
}

#[test]
fn an_error_answer_is_returned_as_data_not_as_a_client_failure() {
    let dir = tempfile::tempdir().unwrap();
    let _game = FakeGame::start(dir.path(), Box::new(|_, _| fail("bad_args", "no")));
    let answer = send_request(dir.path(), "step", &json!({}), &wait(5.0)).unwrap();
    assert_eq!(answer["ok"], false);
    assert_eq!(answer["error"]["code"], "bad_args");
}

#[test]
fn a_timeout_removes_the_unread_request_so_a_late_game_does_not_run_it() {
    let dir = tempfile::tempdir().unwrap();
    let error = send_request(dir.path(), "step", &json!({"ticks": 600}), &wait(0.2)).unwrap_err();
    assert!(error.to_string().contains("no answer to 'step'"), "{error}");
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
}

#[test]
fn a_dead_game_ends_the_wait_early_and_the_note_is_appended() {
    let dir = tempfile::tempdir().unwrap();
    let dead = || false;
    let note = || "; the log ends:\nboom".to_string();
    let started = std::time::Instant::now();
    let error = send_request(
        dir.path(),
        "ping",
        &json!({}),
        &Wait {
            timeout: Duration::from_secs(30),
            is_alive: &dead,
            death_note: &note,
        },
    )
    .unwrap_err();
    assert!(started.elapsed() < Duration::from_secs(5));
    let text = error.to_string();
    assert!(
        text.contains("ended before answering 'ping'") && text.contains("boom"),
        "{text}"
    );
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
}
