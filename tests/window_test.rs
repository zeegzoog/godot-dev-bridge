//! The screenshot of a real window. It needs a display, so it is not part of the headless runs:
//! `GODOT_PATH=/path/to/godot cargo test --test window_test -- --ignored` on a machine with one.

mod common;

use common::Bridge;
use serde_json::json;

fn assert_png(path: &std::path::Path) -> Vec<u8> {
    let bytes = std::fs::read(path).unwrap();
    assert_eq!(
        &bytes[..8],
        b"\x89PNG\r\n\x1a\n",
        "{} is not a PNG",
        path.display()
    );
    bytes
}

#[test]
#[ignore = "needs a Godot executable and a display: set GODOT_PATH"]
fn a_windowed_session_saves_what_the_window_drew() {
    let bridge = Bridge::new("window");
    bridge.ok(&["start", "--window", "320x200"]);
    bridge.ok(&["step", "3"]);

    let first = bridge.root.path().join("first.png");
    let shot = bridge.ok(&["shot", first.to_str().unwrap()]);
    assert_eq!(shot["result"]["size"], json!([320, 200]));
    let first_bytes = assert_png(&first);

    let scaled = bridge.root.path().join("scaled.png");
    let shot = bridge.ok(&["shot", scaled.to_str().unwrap(), "--width", "160"]);
    assert_eq!(shot["result"]["size"], json!([160, 100]));
    assert_png(&scaled);

    // A path that cannot be written is the caller's mistake, not a fault of the bridge.
    let under_a_file = first.join("inside.png");
    let (status, refused) = bridge.run(&["shot", under_a_file.to_str().unwrap()]);
    assert_eq!(status, 1, "{refused}");
    assert_eq!(refused["error"]["code"], "bad_args");

    // The fixture paints the whole window in a colour that follows its tick count, so a frame that
    // did not come from the game (an empty one) would be the same after any number of ticks.
    bridge.ok(&["step", "20"]);
    let second = bridge.root.path().join("second.png");
    bridge.ok(&["shot", second.to_str().unwrap()]);
    let second_bytes = assert_png(&second);
    assert_ne!(
        first_bytes, second_bytes,
        "the window looked the same after 20 more ticks"
    );
}
