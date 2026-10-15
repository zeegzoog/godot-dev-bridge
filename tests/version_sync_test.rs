//! The addon and the client always ship with the same version number.

#[test]
fn addon_version_matches_crate_version() {
    let manifest = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/addons/godot_dev_bridge/plugin.cfg"
    );
    let text = std::fs::read_to_string(manifest).expect("plugin.cfg is readable");
    let line = text
        .lines()
        .find(|line| line.starts_with("version="))
        .expect("plugin.cfg has a version line");
    let version = line.trim_start_matches("version=").trim_matches('"');
    assert_eq!(version, env!("CARGO_PKG_VERSION"));
}
