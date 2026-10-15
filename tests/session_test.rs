use godot_dev_bridge::session::{
    SessionRecord, clear_directory, closed_reason, death_note, list, log_path, log_tail, prune,
    read_record, require_live, session_directory, write_record,
};
use std::time::Duration;

fn now() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs_f64()
}

fn record(pid: u32) -> SessionRecord {
    SessionRecord {
        pid,
        command: vec!["godot".into(), "--path".into(), "p".into()],
        started: now(),
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

/// The game writes why it ended in `closed.json`; a game that crashed or was killed writes nothing.
#[test]
fn the_reason_a_game_ended_is_read_from_closed_json() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(closed_reason(dir.path()), None);
    std::fs::write(dir.path().join("closed.json"), "not json").unwrap();
    assert_eq!(closed_reason(dir.path()), None);
    std::fs::write(dir.path().join("closed.json"), r#"{"reason": "game"}"#).unwrap();
    assert_eq!(closed_reason(dir.path()).as_deref(), Some("game"));
}

#[test]
fn the_note_about_a_dead_game_says_why_it_ended_before_the_end_of_its_log() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("closed.json"), r#"{"reason": "game"}"#).unwrap();
    let note = death_note(dir.path());
    assert!(note.contains("ended on its own"), "{note}");
    std::fs::write(log_path(dir.path()), "last words\n").unwrap();
    let note = death_note(dir.path());
    let reason = note.find("ended on its own").expect("the reason");
    let log = note.find("last words").expect("the log");
    assert!(reason < log, "{note}");
    for (reason, expected) in [
        ("quit", "asked to quit"),
        ("idle", "idle"),
        ("odd", "'odd'"),
    ] {
        std::fs::write(
            dir.path().join("closed.json"),
            format!(r#"{{"reason": "{reason}"}}"#),
        )
        .unwrap();
        let note = death_note(dir.path());
        assert!(note.contains(expected), "{reason}: {note}");
    }
}

#[test]
fn a_command_to_a_game_that_ended_on_its_own_says_so() {
    let root = tempfile::tempdir().unwrap();
    let dead = session_directory(root.path(), "dead");
    std::fs::create_dir_all(&dead).unwrap();
    write_record(&dead, &record(u32::MAX - 1)).unwrap();
    std::fs::write(dead.join("closed.json"), r#"{"reason": "game"}"#).unwrap();
    let error = require_live(root.path(), "dead").unwrap_err().to_string();
    assert!(
        error.contains("is not running") && error.contains("ended on its own"),
        "{error}"
    );
}

#[test]
fn list_says_why_a_game_ended_when_it_left_a_reason() {
    let root = tempfile::tempdir().unwrap();
    for name in ["closed", "crashed"] {
        let dir = session_directory(root.path(), name);
        std::fs::create_dir_all(&dir).unwrap();
        write_record(&dir, &record(u32::MAX - 1)).unwrap();
    }
    std::fs::write(
        session_directory(root.path(), "closed").join("closed.json"),
        r#"{"reason": "game"}"#,
    )
    .unwrap();
    let found = list(root.path());
    assert_eq!(found[0]["session"], "closed");
    assert_eq!(found[0]["closed"], "game");
    assert_eq!(found[1]["session"], "crashed");
    assert!(
        found[1].get("closed").is_none(),
        "a game that left no reason has none: {}",
        found[1]
    );
}

/// A session directory as a game leaves it: its record, its log, an answer and a screenshot.
fn ended_session(root: &std::path::Path, name: &str, pid: u32) -> std::path::PathBuf {
    let dir = session_directory(root, name);
    std::fs::create_dir_all(dir.join("shots")).unwrap();
    write_record(&dir, &record(pid)).unwrap();
    std::fs::write(log_path(&dir), "log\n").unwrap();
    std::fs::write(dir.join("resp-abc.json"), "{}").unwrap();
    std::fs::write(dir.join("shots").join("shot-1.png"), "png").unwrap();
    dir
}

#[test]
fn prune_removes_the_sessions_whose_game_has_ended_and_keeps_live_ones() {
    let root = tempfile::tempdir().unwrap();
    let dead = ended_session(root.path(), "dead", u32::MAX - 1);
    let live = ended_session(root.path(), "live", std::process::id());
    let pruned = prune(root.path(), Duration::from_secs(60)).unwrap();
    assert_eq!(pruned, vec!["dead".to_string()]);
    assert!(!dead.exists(), "the ended session's directory is gone");
    assert!(
        live.join("session.json").exists(),
        "the live one is untouched"
    );
    assert_eq!(list(root.path()).len(), 1);
}

/// A start that failed before the game ran leaves a directory with a log and no record.
#[test]
fn prune_removes_a_leftover_without_a_record_only_once_it_is_old_enough() {
    let root = tempfile::tempdir().unwrap();
    let leftover = session_directory(root.path(), "leftover");
    std::fs::create_dir_all(leftover.join("shots")).unwrap();
    std::fs::write(log_path(&leftover), "").unwrap();

    let pruned = prune(root.path(), Duration::from_secs(3600)).unwrap();
    assert!(
        pruned.is_empty(),
        "a start may be writing its record right now"
    );
    assert!(leftover.exists());

    let pruned = prune(root.path(), Duration::ZERO).unwrap();
    assert_eq!(pruned, vec!["leftover".to_string()]);
    assert!(!leftover.exists());
}

/// Whatever else is in the root (by mistake, `DEV_BRIDGE_ROOT` may name a directory that holds
/// other things) is not a session's to remove.
#[test]
fn prune_leaves_alone_what_is_not_a_session() {
    let root = tempfile::tempdir().unwrap();
    let foreign = root.path().join("foreign");
    std::fs::create_dir_all(&foreign).unwrap();
    std::fs::write(log_path(&foreign), "").unwrap();
    std::fs::write(foreign.join("notes.txt"), "mine").unwrap();
    let empty = root.path().join("empty");
    std::fs::create_dir_all(&empty).unwrap();
    std::fs::write(root.path().join("file.json"), "{}").unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    let target = elsewhere.path().join("target");
    ended_session(elsewhere.path(), "target", u32::MAX - 1);
    link_directory(&root.path().join("linked"), &target);

    let pruned = prune(root.path(), Duration::ZERO).unwrap();
    assert!(pruned.is_empty(), "{pruned:?}");
    assert!(foreign.join("notes.txt").exists());
    assert!(empty.exists());
    assert!(root.path().join("file.json").exists());
    assert!(
        std::fs::symlink_metadata(root.path().join("linked")).is_ok(),
        "a link in the root stays"
    );
    assert!(
        target.join("session.json").exists(),
        "and what it points to too"
    );
}

#[test]
fn prune_removes_a_link_inside_an_ended_session_without_touching_what_it_points_to() {
    let root = tempfile::tempdir().unwrap();
    let dead = ended_session(root.path(), "dead", u32::MAX - 1);
    let elsewhere = tempfile::tempdir().unwrap();
    std::fs::write(elsewhere.path().join("important.txt"), "keep me").unwrap();
    link_directory(&dead.join("linked"), elsewhere.path());

    assert_eq!(
        prune(root.path(), Duration::ZERO).unwrap(),
        vec!["dead".to_string()]
    );
    assert!(!dead.exists());
    assert!(elsewhere.path().join("important.txt").exists());
}

#[test]
fn prune_has_nothing_to_do_without_a_root_and_refuses_a_root_that_is_a_link() {
    let base = tempfile::tempdir().unwrap();
    assert!(
        prune(&base.path().join("missing"), Duration::ZERO)
            .unwrap()
            .is_empty()
    );
    let real = base.path().join("real");
    ended_session(&real, "dead", u32::MAX - 1);
    let link = base.path().join("link");
    link_directory(&link, &real);
    let error = prune(&link, Duration::ZERO).unwrap_err().to_string();
    assert!(error.contains("link"), "{error}");
    assert!(real.join("dead").join("session.json").exists());
}

#[cfg(unix)]
mod private_directories {
    use godot_dev_bridge::session::ensure_private_dir;
    use std::os::unix::fs::PermissionsExt;

    fn mode(path: &std::path::Path) -> u32 {
        std::fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    #[test]
    fn a_new_directory_is_private_to_the_user() {
        let base = tempfile::tempdir().unwrap();
        let target = base.path().join("a").join("dev_bridge");
        ensure_private_dir(&target).unwrap();
        assert_eq!(mode(&target), 0o700);
    }

    #[test]
    fn an_existing_directory_of_ours_is_tightened() {
        let base = tempfile::tempdir().unwrap();
        let target = base.path().join("shared");
        std::fs::create_dir(&target).unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o777)).unwrap();
        ensure_private_dir(&target).unwrap();
        assert_eq!(mode(&target), 0o700);
    }

    #[test]
    fn a_symbolic_link_is_refused() {
        let base = tempfile::tempdir().unwrap();
        let real = base.path().join("real");
        std::fs::create_dir(&real).unwrap();
        let link = base.path().join("link");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        let error = ensure_private_dir(&link).unwrap_err().to_string();
        assert!(
            error.contains("symbolic link") || error.contains("not a directory"),
            "{error}"
        );
    }
}

/// Makes `link` a directory link to `target` (a symbolic link on Unix, a junction on Windows, which
/// needs no privilege).
fn link_directory(link: &std::path::Path, target: &std::path::Path) {
    #[cfg(unix)]
    std::os::unix::fs::symlink(target, link).unwrap();
    #[cfg(windows)]
    {
        let status = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(link)
            .arg(target)
            .stdout(std::process::Stdio::null())
            .status()
            .unwrap();
        assert!(status.success(), "mklink /J failed");
    }
}

/// A link inside a session directory is removed, never followed: what it points to is not ours.
#[test]
fn clear_directory_removes_a_link_without_touching_what_it_points_to() {
    let base = tempfile::tempdir().unwrap();
    let victim = base.path().join("victim");
    std::fs::create_dir(&victim).unwrap();
    std::fs::write(victim.join("important.txt"), "keep me").unwrap();

    let session = base.path().join("session");
    std::fs::create_dir(&session).unwrap();
    link_directory(&session.join("shots"), &victim);
    std::fs::write(session.join("ready.json"), "{}").unwrap();

    clear_directory(&session);

    assert!(
        victim.join("important.txt").exists(),
        "a file behind a link was deleted"
    );
    assert!(!session.join("ready.json").exists());
    assert!(
        std::fs::symlink_metadata(session.join("shots")).is_err(),
        "the link itself is gone"
    );
}

#[test]
fn a_session_record_that_exists_but_cannot_be_read_is_named_as_such() {
    let root = tempfile::tempdir().unwrap();
    let directory = session_directory(root.path(), "broken");
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(directory.join("session.json"), "{\"pid\": \"x\"}").unwrap();
    let error = require_live(root.path(), "broken").unwrap_err().to_string();
    assert!(
        error.contains("session.json") && error.contains("unreadable"),
        "{error}"
    );
}

#[cfg(windows)]
mod private_directories_windows {
    use godot_dev_bridge::session::ensure_private_dir;

    #[test]
    fn a_junction_is_refused() {
        let base = tempfile::tempdir().unwrap();
        let real = base.path().join("real");
        std::fs::create_dir(&real).unwrap();
        let link = base.path().join("link");
        let status = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&link)
            .arg(&real)
            .stdout(std::process::Stdio::null())
            .status()
            .unwrap();
        assert!(status.success());
        let error = ensure_private_dir(&link).unwrap_err().to_string();
        assert!(error.contains("link"), "{error}");
    }

    #[test]
    fn a_new_directory_can_still_be_used_and_removed_by_its_owner() {
        let base = tempfile::tempdir().unwrap();
        let target = base.path().join("a").join("dev_bridge");
        ensure_private_dir(&target).unwrap();
        ensure_private_dir(&target).unwrap();
        std::fs::write(target.join("session.json"), "{}").unwrap();
        assert_eq!(
            std::fs::read_to_string(target.join("session.json")).unwrap(),
            "{}"
        );
        std::fs::remove_dir_all(&target).unwrap();
    }
}
