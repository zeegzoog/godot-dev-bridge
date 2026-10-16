//! A stand-in for the GitHub command-line client, for the feedback tests; not shipped.
//!
//! `FAKE_GH_DIR`, else the directory this executable is in, holds `behaviour.json`: for `list` and
//! `create` (the second argument), an object with `exit`, `stdout`, `stderr` and `sleep_ms`. Every
//! call is recorded there as `call-<n>.json` with its arguments and its standard input.

use serde_json::{Value, json};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::time::Duration;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let own_dir = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(PathBuf::from));
    let Some(dir) = std::env::var_os("FAKE_GH_DIR")
        .map(PathBuf::from)
        .or(own_dir)
    else {
        eprintln!("fake gh: no directory for its behaviour");
        std::process::exit(3);
    };
    let mut stdin = String::new();
    let _ = std::io::stdin().read_to_string(&mut stdin);

    let calls = std::fs::read_dir(&dir)
        .map(|entries| {
            entries
                .filter_map(|entry| entry.ok())
                .filter(|entry| entry.file_name().to_string_lossy().starts_with("call-"))
                .count()
        })
        .unwrap_or(0);
    let record = json!({"args": args, "stdin": stdin});
    let _ = std::fs::write(
        dir.join(format!("call-{}.json", calls + 1)),
        record.to_string(),
    );

    let behaviour: Value = std::fs::read_to_string(dir.join("behaviour.json"))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or(Value::Null);
    let which = args.get(1).map(String::as_str).unwrap_or("");
    let chosen = &behaviour[which];
    if chosen.is_null() {
        eprintln!("fake gh: no behaviour for '{which}'");
        std::process::exit(4);
    }
    if let Some(ms) = chosen["sleep_ms"].as_u64() {
        std::thread::sleep(Duration::from_millis(ms));
    }
    let _ = std::io::stdout().write_all(chosen["stdout"].as_str().unwrap_or("").as_bytes());
    let _ = std::io::stderr().write_all(chosen["stderr"].as_str().unwrap_or("").as_bytes());
    std::process::exit(chosen["exit"].as_i64().unwrap_or(0) as i32);
}
