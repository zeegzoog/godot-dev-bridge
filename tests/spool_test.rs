mod common;

use common::{FakeGame, fail, ok};
use godot_dev_bridge::spool::{
    Wait, new_request_id, read_json, send_request, sweep_stale_answers, write_json_atomically,
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

#[test]
fn a_timeout_after_the_game_took_the_request_says_it_may_still_be_running() {
    let dir = tempfile::tempdir().unwrap();
    let directory = dir.path().to_path_buf();
    // A game that takes the request, as a running step does, and never answers in time.
    let taker = std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while std::time::Instant::now() < deadline {
            for entry in std::fs::read_dir(&directory).unwrap().flatten() {
                if entry.file_name().to_string_lossy().starts_with("cmd-")
                    && !entry.file_name().to_string_lossy().ends_with(".tmp")
                {
                    std::fs::remove_file(entry.path()).unwrap();
                    return;
                }
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    });
    let error = send_request(dir.path(), "step", &json!({"ticks": 600}), &wait(0.5))
        .unwrap_err()
        .to_string();
    taker.join().unwrap();
    assert!(
        error.contains("took the request") && error.contains("status"),
        "{error}"
    );
}

#[test]
fn answers_nobody_collected_are_swept_but_fresh_ones_stay() {
    let dir = tempfile::tempdir().unwrap();
    let old = dir.path().join("resp-old.json");
    let fresh = dir.path().join("resp-fresh.json");
    let other = dir.path().join("game.log");
    for path in [&old, &fresh, &other] {
        std::fs::write(path, "{}").unwrap();
    }
    let long_ago = std::time::SystemTime::now() - Duration::from_secs(3600);
    std::fs::File::options()
        .write(true)
        .open(&old)
        .unwrap()
        .set_modified(long_ago)
        .unwrap();
    std::fs::File::options()
        .write(true)
        .open(&other)
        .unwrap()
        .set_modified(long_ago)
        .unwrap();

    sweep_stale_answers(dir.path(), Duration::from_secs(60));

    assert!(!old.exists(), "an old answer is an orphan");
    assert!(fresh.exists(), "a fresh answer may still be wanted");
    assert!(other.exists(), "only answers are swept");
}

#[test]
fn an_answer_that_is_not_json_is_named_and_removed() {
    let dir = tempfile::tempdir().unwrap();
    let directory = dir.path().to_path_buf();
    let writer = std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while std::time::Instant::now() < deadline {
            for entry in std::fs::read_dir(&directory).unwrap().flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                if name.starts_with("cmd-") && !name.ends_with(".tmp") {
                    let id = name.rsplit('-').next().unwrap().trim_end_matches(".json");
                    std::fs::write(
                        directory.join(format!("resp-{id}.json")),
                        "{\"value\": 1e99999}",
                    )
                    .unwrap();
                    std::fs::remove_file(entry.path()).unwrap();
                    return;
                }
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    });
    let error = send_request(dir.path(), "eval", &json!({"expr": "x"}), &wait(5.0))
        .unwrap_err()
        .to_string();
    writer.join().unwrap();
    assert!(
        error.contains("not valid JSON") && error.contains("resp-"),
        "{error}"
    );
    let leftovers: Vec<_> = std::fs::read_dir(dir.path()).unwrap().collect();
    assert!(leftovers.is_empty(), "left behind: {leftovers:?}");
}
