//! Runs every `tests/gd/*_test.gd` script in a real headless Godot. Each script prints
//! `<name>: N checks, M failed` and exits with status 1 when a check failed.

mod common;

use std::time::Duration;

#[test]
#[ignore = "needs a Godot executable: set GODOT_PATH"]
fn gdscript_unit_scripts_pass() {
    let godot = common::godot_path();
    common::ensure_imported(&godot);

    let mut scripts: Vec<String> = std::fs::read_dir(common::project_root().join("tests/gd"))
        .expect("tests/gd exists")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with("_test.gd"))
        .collect();
    scripts.sort();
    assert!(!scripts.is_empty(), "no GDScript test scripts found");

    let mut failures = Vec::new();
    for script in &scripts {
        let path = format!("tests/gd/{script}");
        let (ok, log) = common::run_godot(
            &godot,
            &["--headless", "--path", ".", "--script", &path],
            Duration::from_secs(120),
        );
        if !ok || !log.contains(" checks, 0 failed") {
            failures.push(format!("{script}:\n{log}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}
