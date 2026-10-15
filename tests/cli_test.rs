use clap::Parser;
use godot_dev_bridge::cli::{Cli, Verb, command_for};
use godot_dev_bridge::config::Context;
use godot_dev_bridge::msys::strip_root;
use serde_json::json;

fn parse(args: &[&str]) -> Cli {
    let mut argv = vec!["godot-dev-bridge"];
    argv.extend_from_slice(args);
    Cli::try_parse_from(argv).unwrap_or_else(|error| panic!("{error}"))
}

fn ctx() -> Context {
    Context {
        cwd: std::env::temp_dir(),
        ..Context::default()
    }
}

fn cmd(name: &str, args: serde_json::Value) -> (String, serde_json::Value) {
    (name.to_string(), args)
}

fn mapped(args: &[&str]) -> (String, serde_json::Value) {
    command_for(&parse(args).verb, &ctx()).unwrap()
}

#[test]
fn session_and_timeout_are_accepted_before_or_after_the_verb() {
    let before = parse(&["--session", "a", "--timeout", "5", "ping"]);
    let after = parse(&["ping", "--session", "a", "--timeout", "5"]);
    assert_eq!(before.session.as_deref(), Some("a"));
    assert_eq!(after.session.as_deref(), Some("a"));
    assert_eq!(after.timeout, Some(5.0));
}

#[test]
fn simple_verbs_map_onto_the_command_of_the_same_name() {
    assert_eq!(mapped(&["status"]), cmd("status", json!({})));
    assert_eq!(mapped(&["ping"]), cmd("ping", json!({})));
    assert_eq!(mapped(&["commands"]), cmd("commands", json!({})));
    assert_eq!(mapped(&["release-all"]), cmd("release_all", json!({})));
}

#[test]
fn step_takes_ticks_or_an_expression_with_a_limit() {
    assert_eq!(mapped(&["step", "30"]), cmd("step", json!({"ticks": 30})));
    assert_eq!(
        mapped(&["step", "--until", "a > 1", "--max", "5"]),
        cmd("step", json!({"until": "a > 1", "max_ticks": 5}))
    );
    assert_eq!(
        mapped(&["step", "--until", "a"]),
        cmd("step", json!({"until": "a"}))
    );
    let error = command_for(&parse(&["step"]).verb, &ctx())
        .unwrap_err()
        .to_string();
    assert!(error.contains("number of ticks"), "{error}");
}

#[test]
fn input_verbs_carry_the_named_input_and_ticks() {
    assert_eq!(
        mapped(&["tap", "--action", "jump", "--ticks", "2"]),
        cmd("tap", json!({"action": "jump", "ticks": 2}))
    );
    assert_eq!(
        mapped(&["press", "--key", "A"]),
        cmd("press", json!({"key": "A"}))
    );
    assert_eq!(
        mapped(&["release", "--button", "left"]),
        cmd("release", json!({"button": "left"}))
    );
}

#[test]
fn a_pointer_accepts_negative_numbers_and_the_relative_flag() {
    assert_eq!(
        mapped(&["pointer", "-5", "10.5", "--relative"]),
        cmd("pointer", json!({"x": -5.0, "y": 10.5, "relative": true}))
    );
    assert_eq!(
        mapped(&["pointer", "1", "2"]),
        cmd("pointer", json!({"x": 1.0, "y": 2.0}))
    );
}

#[test]
fn eval_joins_its_words_and_keeps_a_leading_minus() {
    assert_eq!(
        mapped(&["eval", "root.x", "-", "1"]),
        cmd("eval", json!({"expr": "root.x - 1"}))
    );
}

#[test]
fn a_relative_shot_path_is_made_absolute_against_the_working_directory() {
    let (_, args) = mapped(&["shot", "out.png", "--width", "640"]);
    let path = std::path::PathBuf::from(args["path"].as_str().unwrap());
    assert!(path.is_absolute(), "{path:?}");
    assert!(path.ends_with("out.png"));
    assert_eq!(args["width"], 640);
    assert_eq!(mapped(&["shot"]), cmd("shot", json!({})));
}

#[test]
fn reading_verbs_pass_their_selectors() {
    assert_eq!(
        mapped(&["get", "/root/Player", "position:x", "modulate"]),
        cmd(
            "get",
            json!({"path": "/root/Player", "properties": ["position:x", "modulate"]})
        )
    );
    assert_eq!(
        mapped(&["tree", "/root", "--depth", "3", "--props"]),
        cmd(
            "tree",
            json!({"path": "/root", "depth": 3, "properties": true})
        )
    );
    assert_eq!(mapped(&["tree"]), cmd("tree", json!({})));
    assert_eq!(
        mapped(&["state", "run", "level"]),
        cmd("state", json!({"names": ["run", "level"]}))
    );
    assert_eq!(mapped(&["state"]), cmd("state", json!({})));
}

#[test]
fn do_and_raw_forward_json_and_reject_bad_json() {
    assert_eq!(
        mapped(&["do", "add", r#"{"n":3}"#]),
        cmd("add", json!({"n": 3}))
    );
    assert_eq!(mapped(&["do", "heal"]), cmd("heal", json!({})));
    assert_eq!(
        mapped(&["raw", r#"{"cmd":"ping","args":{"a":1}}"#]),
        cmd("ping", json!({"a": 1}))
    );
    for bad in [
        ["do", "add", "{bad"],
        ["raw", "{bad", ""],
        ["raw", r#"{"x":1}"#, ""],
    ] {
        let words: Vec<&str> = bad
            .iter()
            .copied()
            .filter(|word| !word.is_empty())
            .collect();
        assert!(
            command_for(&parse(&words).verb, &ctx()).is_err(),
            "{words:?}"
        );
    }
}

#[test]
fn start_has_defaults_and_collects_engine_arguments_after_the_separator() {
    let Verb::Start(start) = parse(&["start"]).verb else {
        panic!("not start")
    };
    assert_eq!(start.fps, None, "the default comes from the project");
    assert_eq!(start.window, "1152x648");
    assert!(!start.headless && !start.fast && !start.keep_mouse);
    assert!(start.engine_args.is_empty());

    let Verb::Start(start) = parse(&[
        "start",
        "--headless",
        "--scene",
        "res://a.tscn",
        "--idle-quit",
        "120",
        "--",
        "--verbose",
    ])
    .verb
    else {
        panic!("not start")
    };
    assert!(start.headless);
    assert_eq!(start.scene.as_deref(), Some("res://a.tscn"));
    assert_eq!(start.idle_quit, Some(120));
    assert_eq!(start.engine_args, vec!["--verbose".to_string()]);
}

#[test]
fn a_step_limit_needs_an_expression_to_limit() {
    assert!(Cli::try_parse_from(["godot-dev-bridge", "step", "3", "--max", "1"]).is_err());
    assert!(
        Cli::try_parse_from(["godot-dev-bridge", "step", "--until", "x", "--max", "1"]).is_ok()
    );
}

#[test]
fn lifecycle_verbs_are_not_protocol_commands() {
    for verb in [&["start"][..], &["stop"], &["sessions"]] {
        assert!(command_for(&parse(verb).verb, &ctx()).is_err());
    }
}

#[test]
fn the_git_bash_prefix_is_removed_from_a_rewritten_node_path() {
    let root = "C:/Program Files/Git";
    assert_eq!(
        strip_root("C:/Program Files/Git/root/App", root),
        "/root/App"
    );
    assert_eq!(
        strip_root("C:\\Program Files\\Git\\root\\App", root),
        "/root/App"
    );
    assert_eq!(strip_root("C:/Elsewhere/root", root), "C:/Elsewhere/root");
    assert_eq!(strip_root("/root/App", root), "/root/App");
}
