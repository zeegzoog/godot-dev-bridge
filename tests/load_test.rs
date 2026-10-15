//! Load behaviour against a real headless Godot: many clients at once, a condition evaluated on
//! every tick, and a large answer. The limits are loose on purpose (they catch a regression of an
//! order of magnitude, not a slow machine).
//! Needs a Godot 4.5+ executable: `GODOT_PATH=/path/to/godot cargo test --test load_test -- --ignored`.

mod common;

use common::Bridge;
use godot_dev_bridge::spool::{Wait, send_request};
use serde_json::json;
use std::time::{Duration, Instant};

static ALIVE: fn() -> bool = || true;
static NO_NOTE: fn() -> String = String::new;

fn ask(directory: &std::path::Path, command: &str, args: serde_json::Value) -> serde_json::Value {
    send_request(
        directory,
        command,
        &args,
        &Wait {
            timeout: Duration::from_secs(60),
            is_alive: &ALIVE,
            death_note: &NO_NOTE,
        },
    )
    .unwrap_or_else(|error| panic!("{command}: {error}"))
}

/// Sixteen clients ask at the same time and every one gets the answer to its own question. A request
/// file that the game finds while Windows still holds it used to be answered as malformed.
#[test]
#[ignore = "needs a Godot executable: set GODOT_PATH"]
fn many_clients_at_once_all_get_the_answer_to_their_own_question() {
    let bridge = Bridge::new("load");
    bridge.start();
    let directory = bridge.root.path().join("load");

    let started = Instant::now();
    let workers: Vec<_> = (0..16)
        .map(|worker| {
            let directory = directory.clone();
            std::thread::spawn(move || {
                for call in 0..100 {
                    let expected = worker * 1000 + call;
                    let answer = ask(
                        &directory,
                        "eval",
                        json!({"expr": format!("{worker} * 1000 + {call}")}),
                    );
                    assert_eq!(answer["ok"], true, "worker {worker} call {call}: {answer}");
                    assert_eq!(
                        answer["result"]["value"], expected,
                        "worker {worker} call {call}"
                    );
                }
            })
        })
        .collect();
    for worker in workers {
        worker
            .join()
            .expect("a worker saw a wrong or failed answer");
    }
    let elapsed = started.elapsed();
    // 1600 requests take 3 to 4 s on a laptop; 60 s is a regression of an order of magnitude.
    assert!(
        elapsed < Duration::from_secs(60),
        "1600 requests took {elapsed:?}"
    );
    let left: Vec<_> = std::fs::read_dir(&directory)
        .unwrap()
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with("cmd-") || name.starts_with("resp-"))
        .collect();
    assert!(
        left.is_empty(),
        "request or answer files left behind: {left:?}"
    );
}

/// A condition is evaluated before every tick. It used to list every project setting each time
/// (5 ms a tick); now 600 ticks take a fraction of a second.
#[test]
#[ignore = "needs a Godot executable: set GODOT_PATH"]
fn a_step_with_a_condition_costs_little_per_tick() {
    let bridge = Bridge::new("load-until");
    bridge.start();
    let started = Instant::now();
    let answer = bridge.ok(&["step", "--until", "false", "--max", "600"]);
    let elapsed = started.elapsed();
    assert_eq!(answer["result"]["ticks_run"], 600);
    assert!(
        elapsed < Duration::from_millis(2500),
        "600 ticks with a condition took {elapsed:?} (it was about 3 s before the scope was cached)"
    );
}

/// An answer of several megabytes is delivered whole, and the game is fine afterwards.
#[test]
#[ignore = "needs a Godot executable: set GODOT_PATH"]
fn a_large_answer_is_delivered_whole() {
    let bridge = Bridge::new("load-large");
    bridge.start();
    let answer = bridge.ok(&["eval", "'x'.repeat(2000000)"]);
    assert_eq!(answer["result"]["value"].as_str().unwrap().len(), 2_000_000);
    assert_eq!(bridge.ok(&["ping"])["result"]["pong"], true);
}
