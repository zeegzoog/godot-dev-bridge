//! Source scans that keep the addon honest about its own invariants.

use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn addon() -> PathBuf {
    root().join("addons/godot_dev_bridge")
}

fn gd_files(directory: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut pending = vec![directory.to_path_buf()];
    while let Some(current) = pending.pop() {
        for entry in std::fs::read_dir(&current)
            .unwrap()
            .filter_map(|entry| entry.ok())
        {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|extension| extension == "gd") {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

fn text(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap()
}

#[test]
fn the_addon_has_scripts_to_scan() {
    assert!(gd_files(&addon()).len() >= 10, "{:?}", gd_files(&addon()));
}

#[test]
fn runtime_scripts_declare_no_class_name() {
    for path in gd_files(&addon().join("runtime")) {
        let offending = text(&path)
            .lines()
            .any(|line| line.trim_start().starts_with("class_name"));
        assert!(!offending, "{} declares a class_name", path.display());
    }
}

#[test]
fn the_addon_opens_no_network_endpoint() {
    const FORBIDDEN: [&str; 9] = [
        "TCPServer",
        "UDPServer",
        "WebSocketPeer",
        "HTTPRequest",
        "HTTPClient",
        "ENetConnection",
        "StreamPeerTCP",
        "PacketPeerUDP",
        "WebRTC",
    ];
    for path in gd_files(&addon()) {
        let source = text(&path);
        for name in FORBIDDEN {
            assert!(!source.contains(name), "{} mentions {name}", path.display());
        }
    }
}

#[test]
fn the_addon_names_no_project_class_or_path() {
    for path in gd_files(&addon()) {
        let source = text(&path);
        for forbidden in ["res://src", "res://app", "res://game"] {
            assert!(
                !source.contains(forbidden),
                "{} mentions {forbidden}",
                path.display()
            );
        }
    }
}

#[test]
fn every_addon_script_has_its_uid_file_committed() {
    for path in gd_files(&addon()) {
        let mut uid = path.clone().into_os_string();
        uid.push(".uid");
        assert!(
            PathBuf::from(&uid).is_file(),
            "{} has no .uid file: run `godot --headless --path . --import` and commit it",
            path.display()
        );
    }
}

#[test]
fn plugin_cfg_names_a_script_that_exists() {
    let cfg = text(&addon().join("plugin.cfg"));
    let script = cfg
        .lines()
        .find_map(|line| line.strip_prefix("script="))
        .expect("plugin.cfg has a script line")
        .trim_matches('"');
    assert!(addon().join(script).is_file(), "missing {script}");
}
