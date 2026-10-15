//! The built client against a real headless Godot running the dev project's ticker scene.
//! Needs a Godot 4.5+ executable: `GODOT_PATH=/path/to/godot cargo test --test e2e_test -- --ignored`.

mod common;

use common::Bridge;
use godot_dev_bridge::process::is_alive;
use serde_json::Value;

#[test]
#[ignore = "needs a Godot executable: set GODOT_PATH"]
fn a_session_holds_the_game_still_and_steps_it_by_exact_ticks() {
    let bridge = Bridge::new("stepping");
    let started = bridge.start();
    assert_eq!(started["ready"]["protocol"], 1);
    assert_eq!(started["ready"]["headless"], true);
    assert_eq!(started["ready"]["deterministic"], true, "{started}");

    let status = bridge.ok(&["status"])["result"].clone();
    assert_eq!(status["deterministic"], true);
    assert!(
        status["commands"]
            .as_array()
            .unwrap()
            .iter()
            .any(|name| name == "add")
    );

    let before = bridge.ticker();
    assert_eq!(before["ticks"], 0, "the game is held before the first tick");
    std::thread::sleep(std::time::Duration::from_millis(300));
    assert_eq!(
        bridge.ticker()["ticks"],
        0,
        "and stays still while the driver thinks"
    );

    let step = bridge.ok(&["step", "5"])["result"].clone();
    assert_eq!(step["ticks_run"], 5);
    let after = bridge.ticker();
    assert_eq!(after["ticks"], 5);
    assert_eq!(
        after["frames"], 5,
        "every tick is paired with one process frame"
    );

    let until = bridge.ok(&[
        "step",
        "--until",
        "root.get_node('Ticker').ticks >= 12",
        "--max",
        "100",
    ])["result"]
        .clone();
    assert_eq!(until["until_met"], true);
    assert_eq!(until["ticks_run"], 7);

    let already = bridge.ok(&["step", "--until", "true"])["result"].clone();
    assert_eq!(
        already["ticks_run"], 0,
        "a condition that already holds runs no tick"
    );
    assert_eq!(already["until_met"], true);

    let bounded = bridge.ok(&["step", "--until", "false", "--max", "3"])["result"].clone();
    assert_eq!(
        bounded["ticks_run"], 3,
        "a condition that never holds is bounded"
    );
    assert_eq!(bounded["until_met"], false);

    let value = bridge.ok(&["eval", "root.get_node('Ticker').ticks"]);
    assert_eq!(value["result"]["value"], 15);
}

#[test]
#[ignore = "needs a Godot executable: set GODOT_PATH"]
fn injected_input_is_seen_by_the_game_the_way_device_input_is() {
    let bridge = Bridge::new("input");
    bridge.start();
    bridge.ok(&["step", "1"]);
    let parked = bridge.ticker()["ticks"].as_i64().unwrap();

    // Measured behaviour: a press is seen as just pressed on the second tick after the park point,
    // and a default tap is down for two ticks.
    bridge.ok(&["tap", "--action", "ui_accept"]);
    bridge.ok(&["step", "10"]);
    let tapped = bridge.ticker();
    assert_eq!(
        tapped["pressed_ticks"], 2,
        "a tap of the default two ticks is down for two ticks"
    );
    assert_eq!(tapped["just_pressed_at"].as_array().unwrap().len(), 1);
    assert_eq!(tapped["just_pressed_at"][0].as_i64().unwrap() - parked, 2);

    bridge.ok(&["press", "--action", "ui_accept", "--ticks", "3"]);
    bridge.ok(&["step", "8"]);
    assert_eq!(
        bridge.ticker()["pressed_ticks"],
        5,
        "a press for three ticks is down for three more"
    );

    bridge.ok(&["press", "--action", "ui_accept"]);
    let held = bridge.ok(&["status"])["result"]["held_inputs"].clone();
    assert_eq!(held, serde_json::json!(["action:ui_accept"]));
    assert_eq!(bridge.ok(&["release-all"])["result"]["released"], 1);
    assert_eq!(
        bridge.ok(&["status"])["result"]["held_inputs"],
        serde_json::json!([])
    );

    let (status, refused) = bridge.run(&["press", "--action", "no_such_action"]);
    assert_eq!(status, 1);
    assert_eq!(refused["error"]["code"], "bad_args");

    // Splitting a step must not change what the game sees: a tap is released by the ticks stepped,
    // however they are divided.
    let before = bridge.ticker()["pressed_ticks"].as_i64().unwrap();
    bridge.ok(&["tap", "--action", "ui_accept"]);
    for _ in 0..3 {
        bridge.ok(&["step", "1"]);
    }
    bridge.ok(&["step", "6"]);
    let after = bridge.ticker()["pressed_ticks"].as_i64().unwrap();
    assert_eq!(
        after - before,
        2,
        "a tap stepped tick by tick is still down for two ticks"
    );
    assert_eq!(
        bridge.ok(&["status"])["result"]["held_inputs"],
        serde_json::json!([])
    );
}

#[test]
#[ignore = "needs a Godot executable: set GODOT_PATH"]
fn the_adapter_state_commands_and_errors_reach_the_shell_with_the_right_status() {
    let bridge = Bridge::new("adapter");
    bridge.start();

    let commands = bridge.ok(&["commands"])["result"].clone();
    assert!(commands["core"].get("step").is_some());
    assert!(commands["project"].get("add").is_some());
    assert!(
        commands["project"].get("ping").is_none(),
        "a core name is refused to the project"
    );

    let sum = bridge.ok(&["do", "add", r#"{"a":2,"b":3}"#]);
    assert_eq!(sum["result"]["sum"], 5);

    let (status, unknown) = bridge.run(&["do", "no_such_command"]);
    assert_eq!(
        (status, unknown["error"]["code"].as_str()),
        (1, Some("unknown_command"))
    );
    let (status, bad) = bridge.run(&["eval", "nope("]);
    assert_eq!(
        (status, bad["error"]["code"].as_str()),
        (1, Some("eval_failed"))
    );
    let (status, missing) = bridge.run(&["get", "/root/Nothing"]);
    assert_eq!(
        (status, missing["error"]["code"].as_str()),
        (1, Some("not_found"))
    );
    let (status, headless) = bridge.run(&["shot"]);
    assert_eq!(
        (status, headless["error"]["code"].as_str()),
        (1, Some("no_renderer"))
    );

    let node = bridge.ok(&["get", "/root/Ticker", "ticks"]);
    assert_eq!(node["result"]["properties"]["ticks"], 0);
    assert_eq!(node["result"]["class"], "Node");
    assert_eq!(
        bridge.ok(&["tree", "/root", "--depth", "1"])["result"]["name"],
        "root"
    );
    assert_eq!(
        bridge.ok(&["raw", r#"{"cmd":"ping"}"#])["result"]["pong"],
        true
    );
}

#[test]
#[ignore = "needs a Godot executable: set GODOT_PATH"]
fn stop_ends_the_game_and_a_stale_session_can_be_replaced() {
    let bridge = Bridge::new("lifecycle");
    let started = bridge.start();
    let pid = started["pid"].as_u64().unwrap() as u32;
    assert!(is_alive(pid));

    let (status, again) = bridge.run(&["start", "--headless"]);
    assert_eq!(status, 2, "a live session is not started over");
    assert!(
        again["error"]["message"]
            .as_str()
            .unwrap()
            .contains("already running")
    );

    let stopped = bridge.ok(&["stop"]);
    assert_eq!(
        stopped["killed"], false,
        "a game that was asked to quit leaves by itself"
    );
    assert!(!is_alive(pid));
    let closed = bridge.root.path().join("lifecycle").join("closed.json");
    let reason: Value = serde_json::from_str(&std::fs::read_to_string(closed).unwrap()).unwrap();
    assert_eq!(reason["reason"], "quit");

    let sessions = bridge.ok(&["sessions"])["sessions"].clone();
    assert_eq!(sessions[0]["session"], "lifecycle");
    assert_eq!(sessions[0]["alive"], false);

    let restarted = bridge.start();
    assert_ne!(
        restarted["pid"], started["pid"],
        "a stale session is replaced by a fresh one"
    );
    assert_eq!(bridge.ticker()["ticks"], 0);
}
