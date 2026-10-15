use godot_dev_bridge::session::{
    SessionRecord, clear_directory, death_note, list, log_path, log_tail, read_record,
    require_live, session_directory, write_record,
};

fn record(pid: u32) -> SessionRecord {
    SessionRecord {
        pid,
        command: vec!["godot".into(), "--path".into(), "p".into()],
        started: 1.5,
    }
}

#[test]
fn a_record_survives_a_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    write_record(dir.path(), &record(42)).unwrap();
    let back = read_record(dir.path()).unwrap();
    assert_eq!(back.pid, 42);
    assert_eq!(back.command[1], "--path");
}

#[test]
fn a_missing_or_broken_record_reads_as_none() {
    let dir = tempfile::tempdir().unwrap();
    assert!(read_record(dir.path()).is_none());
    std::fs::write(dir.path().join("session.json"), "not json").unwrap();
    assert!(read_record(dir.path()).is_none());
}

#[test]
fn require_live_reports_missing_dead_and_live_sessions() {
    let root = tempfile::tempdir().unwrap();

    let error = require_live(root.path(), "nope").unwrap_err().to_string();
    assert!(error.contains("no session named 'nope'"), "{error}");

    let dead = session_directory(root.path(), "dead");
    std::fs::create_dir_all(&dead).unwrap();
    write_record(&dead, &record(u32::MAX - 1)).unwrap();
    std::fs::write(log_path(&dead), "line one\nline two\n").unwrap();
    let error = require_live(root.path(), "dead").unwrap_err().to_string();
    assert!(
        error.contains("is not running") && error.contains("line two"),
        "{error}"
    );

    let live = session_directory(root.path(), "live");
    std::fs::create_dir_all(&live).unwrap();
    write_record(&live, &record(std::process::id())).unwrap();
    let (directory, found) = require_live(root.path(), "live").unwrap();
    assert_eq!(directory, live);
    assert_eq!(found.pid, std::process::id());
}

#[test]
fn list_reports_every_session_with_its_liveness_sorted_by_name() {
    let root = tempfile::tempdir().unwrap();
    for (name, pid) in [("b", std::process::id()), ("a", u32::MAX - 1)] {
        let dir = session_directory(root.path(), name);
        std::fs::create_dir_all(&dir).unwrap();
        write_record(&dir, &record(pid)).unwrap();
    }
    std::fs::create_dir_all(root.path().join("empty")).unwrap();
    let found = list(root.path());
    assert_eq!(found.len(), 2);
    assert_eq!(found[0]["session"], "a");
    assert_eq!(found[0]["alive"], false);
    assert_eq!(found[1]["session"], "b");
    assert_eq!(found[1]["alive"], true);
}

#[test]
fn clear_directory_removes_files_and_the_files_in_subdirectories() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("ready.json"), "{}").unwrap();
    std::fs::create_dir(dir.path().join("shots")).unwrap();
    std::fs::write(dir.path().join("shots").join("a.png"), "x").unwrap();
    clear_directory(dir.path());
    assert!(!dir.path().join("ready.json").exists());
    assert!(!dir.path().join("shots").join("a.png").exists());
}

#[test]
fn the_log_tail_keeps_only_the_last_lines_and_the_note_is_empty_without_a_log() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(death_note(dir.path()), "");
    let text: String = (1..=100).map(|n| format!("line {n}\n")).collect();
    std::fs::write(log_path(dir.path()), text).unwrap();
    let tail = log_tail(&log_path(dir.path()), 3);
    assert_eq!(tail, "line 98\nline 99\nline 100");
    let note = death_note(dir.path());
    assert!(
        note.starts_with("; the log ends:\n") && note.contains("line 100"),
        "{note}"
    );
}
