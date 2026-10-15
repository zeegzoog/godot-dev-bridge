//! A real PCK export of a fresh project: with the default export settings the runtime, the
//! editor-only scripts and the project adapter must stay out, and only the inert stub ships.
//! Needs a Godot 4.5+ executable (export templates are not needed for a pack):
//! `GODOT_PATH=/path/to/godot cargo test --test export_test -- --ignored`.

mod common;

use std::path::Path;
use std::time::Duration;

fn contains(haystack: &[u8], needle: &str) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle.as_bytes())
}

/// A project that has the addon installed and enabled, an adapter, and one export preset.
fn exportable_project(dir: &Path, adapter_setting: &str) {
    std::fs::write(
        dir.join("project.godot"),
        format!(
            "config_version=5\n\n[application]\n\nconfig/name=\"Export Test\"\n\
             run/main_scene=\"res://main.tscn\"\nconfig/features=PackedStringArray(\"4.5\")\n\n\
             [autoload]\n\nDevBridge=\"*res://addons/godot_dev_bridge/dev_bridge.gd\"\n\n\
             [editor_plugins]\n\n\
             enabled=PackedStringArray(\"res://addons/godot_dev_bridge/plugin.cfg\")\n\n\
             [godot_dev_bridge]\n\nadapter=\"{adapter_setting}\"\n"
        ),
    )
    .unwrap();
    std::fs::write(
        dir.join("main.tscn"),
        "[gd_scene format=3]\n\n[node name=\"Main\" type=\"Node\"]\n",
    )
    .unwrap();
    std::fs::create_dir_all(dir.join("dev")).unwrap();
    std::fs::copy(
        common::project_root().join("tests/fixtures/adapter.gd"),
        dir.join("dev/BridgeAdapter.gd"),
    )
    .unwrap();
    std::fs::write(
        dir.join("export_presets.cfg"),
        "[preset.0]\n\nname=\"pack\"\nplatform=\"Windows Desktop\"\nrunnable=true\n\
         export_filter=\"all_resources\"\ninclude_filter=\"\"\nexclude_filter=\"\"\n\
         export_path=\"\"\nscript_export_mode=2\n\n\
         [preset.0.options]\n\ncustom_template/debug=\"\"\n",
    )
    .unwrap();
    common::copy_dir(
        &common::project_root().join("addons/godot_dev_bridge"),
        &dir.join("addons/godot_dev_bridge"),
    );
}

fn import(godot: &Path, dir: &Path) {
    let (imported, log) = common::run_godot(
        godot,
        &["--headless", "--path", dir.to_str().unwrap(), "--import"],
        Duration::from_secs(180),
    );
    assert!(
        imported,
        "importing the export project failed:
{log}"
    );
}

/// Exports the project's pack with the default script export mode and returns its bytes.
fn export_pack(godot: &Path, dir: &Path) -> Vec<u8> {
    import(godot, dir);
    let pack = dir.join("game.pck");
    let (exported, log) = common::run_godot(
        godot,
        &[
            "--headless",
            "--path",
            dir.to_str().unwrap(),
            "--export-pack",
            "pack",
            pack.to_str().unwrap(),
        ],
        Duration::from_secs(240),
    );
    assert!(
        exported,
        "exporting the pack failed:
{log}"
    );
    std::fs::read(&pack).expect("the pack was written")
}

#[test]
#[ignore = "needs a Godot executable: set GODOT_PATH"]
fn a_default_export_leaves_the_runtime_the_editor_scripts_and_the_adapter_out() {
    let godot = common::godot_path();
    let project = tempfile::tempdir().unwrap();
    exportable_project(project.path(), "res://dev/BridgeAdapter.gd");

    let pack = export_pack(&godot, project.path());

    // The default script export mode compiles every shipped script to `<name>.gdc` and writes a
    // `<name>.gd.remap` next to it. (The bare `.gd` paths that remain in the pack are metadata such
    // as the UID cache, not files.)
    assert!(
        contains(&pack, "godot_dev_bridge/dev_bridge.gdc"),
        "the inert autoload stub must ship: the project's autoload list names it"
    );
    for name in [
        "godot_dev_bridge/runtime/session",
        "godot_dev_bridge/runtime/observer",
        "godot_dev_bridge/runtime/registry",
        "godot_dev_bridge/runtime/input_injector",
        "godot_dev_bridge/runtime/spool_transport",
        "godot_dev_bridge/runtime/step_controller",
        "godot_dev_bridge/plugin",
        "godot_dev_bridge/export_plugin",
        "godot_dev_bridge/export_policy",
        "dev/BridgeAdapter",
    ] {
        for suffix in [".gdc", ".gd.remap"] {
            let entry = format!("{name}{suffix}");
            assert!(
                !contains(&pack, &entry),
                "a non-debug export shipped '{entry}'"
            );
        }
    }
}

#[test]
#[ignore = "needs a Godot executable: set GODOT_PATH"]
fn an_adapter_stored_as_a_uid_is_left_out_of_the_export_too() {
    let godot = common::godot_path();
    let project = tempfile::tempdir().unwrap();
    let dir = project.path();
    exportable_project(dir, "");
    import(&godot, dir);

    // The file picker of Project Settings stores the adapter as the uid the import wrote.
    let uid = std::fs::read_to_string(dir.join("dev/BridgeAdapter.gd.uid"))
        .expect("the import wrote a .uid file")
        .trim()
        .to_string();
    assert!(uid.starts_with("uid://"), "{uid}");
    let settings = std::fs::read_to_string(dir.join("project.godot")).unwrap();
    std::fs::write(
        dir.join("project.godot"),
        settings.replace("adapter=\"\"", &format!("adapter=\"{uid}\"")),
    )
    .unwrap();

    let pack = export_pack(&godot, dir);

    for entry in ["dev/BridgeAdapter.gdc", "dev/BridgeAdapter.gd.remap"] {
        assert!(
            !contains(&pack, entry),
            "the adapter, named by a uid, shipped as '{entry}'"
        );
    }
}
