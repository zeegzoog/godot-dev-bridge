//! The headline promise: copy the folder, enable the plugin, and the bridge works in a fresh
//! project with no manual edits to `project.godot`.
//! Needs a Godot 4.5+ executable: `GODOT_PATH=/path/to/godot cargo test --test installer_test -- --ignored`.

mod common;

use std::process::Command;
use std::time::Duration;

fn open_editor(godot: &std::path::Path, project: &std::path::Path) -> String {
    let (_, log) = common::run_godot(
        godot,
        &[
            "--headless",
            "--editor",
            "--path",
            project.to_str().unwrap(),
            "--quit-after",
            "600",
        ],
        Duration::from_secs(240),
    );
    log
}

#[test]
#[ignore = "needs a Godot executable: set GODOT_PATH"]
fn the_plugin_installs_itself_into_a_fresh_project_and_the_game_can_be_driven() {
    let godot = common::godot_path();
    let project = tempfile::tempdir().unwrap();
    let dir = project.path();
    std::fs::write(
        dir.join("project.godot"),
        concat!(
            "config_version=5\n\n[application]\n\nconfig/name=\"Installer Test\"\n",
            "run/main_scene=\"res://main.tscn\"\nconfig/features=PackedStringArray(\"4.5\")\n\n",
            "[editor_plugins]\n\n",
            "enabled=PackedStringArray(\"res://addons/godot_dev_bridge/plugin.cfg\")\n",
        ),
    )
    .unwrap();
    std::fs::write(
        dir.join("main.tscn"),
        "[gd_scene format=3]\n\n[node name=\"Main\" type=\"Node\"]\n",
    )
    .unwrap();
    common::copy_dir(
        &common::project_root().join("addons/godot_dev_bridge"),
        &dir.join("addons/godot_dev_bridge"),
    );

    let log = open_editor(&godot, dir);
    let settings = std::fs::read_to_string(dir.join("project.godot")).unwrap();
    assert!(
        settings.contains("DevBridge=\"*"),
        "the plugin did not register the autoload:\n{settings}\n--- editor log ---\n{log}"
    );

    open_editor(&godot, dir);
    let reopened = std::fs::read_to_string(dir.join("project.godot")).unwrap();
    assert_eq!(
        reopened.matches("DevBridge=").count(),
        1,
        "opening twice must not duplicate it:\n{reopened}"
    );

    let root = tempfile::tempdir().unwrap();
    let bridge = |args: &[&str]| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_godot-dev-bridge"));
        command
            .args(["--session", "installer"])
            .args(args)
            .current_dir(dir)
            .env("DEV_BRIDGE_ROOT", root.path())
            .env("GODOT_PATH", &godot)
            .env_remove("GODOT_CONSOLE_PATH");
        let (status, stdout, _) = common::run_client(command, Duration::from_secs(120));
        (status, stdout)
    };
    let (status, started) = bridge(&["start", "--headless"]);
    assert_eq!(status, 0, "{started}");
    let (status, pong) = bridge(&["ping"]);
    assert_eq!(status, 0, "{pong}");
    assert!(pong.contains("\"pong\":true"), "{pong}");
    let (status, stopped) = bridge(&["stop"]);
    assert_eq!(status, 0, "{stopped}");
}
