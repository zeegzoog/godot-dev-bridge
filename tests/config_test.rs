use godot_dev_bridge::config::{
    Context, GodotEnv, absolutize, default_root, find_project, project_physics_rate, resolve_godot,
    validate_session_name,
};
use std::path::Path;

fn touch(path: &Path) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, "").unwrap();
}

fn project(dir: &Path) {
    touch(&dir.join("project.godot"));
}

#[test]
fn the_session_name_is_the_flag_then_the_environment_then_default() {
    let mut ctx = Context::default();
    assert_eq!(ctx.session_name(None), "default");
    ctx.session_env = Some("from-env".into());
    assert_eq!(ctx.session_name(None), "from-env");
    assert_eq!(ctx.session_name(Some("flag")), "flag");
    assert_eq!(ctx.session_name(Some("")), "from-env");
}

#[test]
fn godot_is_taken_from_the_flag_then_the_environment_in_order() {
    let dir = tempfile::tempdir().unwrap();
    let flag = dir.path().join("flag-godot");
    let path = dir.path().join("path-godot");
    let plain = dir.path().join("plain-godot");
    for file in [&flag, &path, &plain] {
        touch(file);
    }
    let ctx = Context {
        godot_env: GodotEnv {
            godot_path: Some(path.to_string_lossy().into()),
            godot: Some(plain.to_string_lossy().into()),
        },
        ..Context::default()
    };
    assert_eq!(
        resolve_godot(&ctx, Some(flag.to_str().unwrap())).unwrap(),
        flag
    );
    assert_eq!(resolve_godot(&ctx, None).unwrap(), path);

    let ctx = Context {
        godot_env: GodotEnv {
            godot_path: Some("/missing".into()),
            godot: Some(plain.to_string_lossy().into()),
        },
        ..Context::default()
    };
    assert_eq!(resolve_godot(&ctx, None).unwrap(), plain);
}

#[test]
fn the_physics_rate_comes_from_the_physics_section_of_the_project() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(project_physics_rate(dir.path()), None, "no project.godot");
    let write = |text: &str| std::fs::write(dir.path().join("project.godot"), text).unwrap();

    write("config_version=5\n\n[application]\n\nconfig/name=\"x\"\n");
    assert_eq!(
        project_physics_rate(dir.path()),
        None,
        "a project that sets none"
    );
    write("[physics]\n\ncommon/physics_ticks_per_second=120\n\n[rendering]\n");
    assert_eq!(project_physics_rate(dir.path()), Some(120));
    write("[physics]\n\ncommon/physics_jitter_fix=0.0\ncommon/physics_ticks_per_second = 30\n");
    assert_eq!(
        project_physics_rate(dir.path()),
        None,
        "only the exact `key=value` form is read"
    );
    write("[physics]\n\ncommon/physics_ticks_per_second=30\n");
    assert_eq!(project_physics_rate(dir.path()), Some(30));
    write("[display]\n\ncommon/physics_ticks_per_second=90\n");
    assert_eq!(
        project_physics_rate(dir.path()),
        None,
        "the key of another section is not it"
    );
    for bad in ["0", "-5", "abc", "12.5", "99999999999"] {
        write(&format!(
            "[physics]\n\ncommon/physics_ticks_per_second={bad}\n"
        ));
        assert_eq!(project_physics_rate(dir.path()), None, "{bad}");
    }
}

#[test]
fn godot_is_found_on_path_and_a_failure_lists_what_was_tried() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = Context {
        godot_env: GodotEnv {
            godot_path: Some("/missing/godot".into()),
            ..GodotEnv::default()
        },
        path_var: Some(std::env::join_paths([dir.path()]).unwrap()),
        ..Context::default()
    };
    let error = resolve_godot(&ctx, None).unwrap_err().to_string();
    assert!(
        error.contains("GODOT_PATH=/missing/godot") && error.contains("PATH"),
        "{error}"
    );

    touch(&dir.path().join("godot"));
    assert_eq!(resolve_godot(&ctx, None).unwrap(), dir.path().join("godot"));
}

#[test]
fn an_explicit_godot_that_does_not_exist_is_an_error_not_a_fallback() {
    let error = resolve_godot(&Context::default(), Some("/nope/godot"))
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("--godot") && error.contains("/nope/godot"),
        "{error}"
    );
}

#[test]
fn the_project_is_found_by_walking_up() {
    let dir = tempfile::tempdir().unwrap();
    project(dir.path());
    let deep = dir.path().join("a").join("b");
    std::fs::create_dir_all(&deep).unwrap();
    assert_eq!(find_project(&deep, None).unwrap(), dir.path());
}

#[test]
fn a_single_child_project_is_found_below_the_working_directory() {
    let dir = tempfile::tempdir().unwrap();
    project(&dir.path().join("app"));
    std::fs::create_dir(dir.path().join("docs")).unwrap();
    assert_eq!(
        find_project(dir.path(), None).unwrap(),
        dir.path().join("app")
    );
}

#[test]
fn zero_or_several_projects_are_reported_with_the_way_out() {
    let dir = tempfile::tempdir().unwrap();
    let error = find_project(dir.path(), None).unwrap_err().to_string();
    assert!(error.contains("--project"), "{error}");

    project(&dir.path().join("one"));
    project(&dir.path().join("two"));
    let error = find_project(dir.path(), None).unwrap_err().to_string();
    assert!(
        error.contains("one") && error.contains("two") && error.contains("--project"),
        "{error}"
    );
}

#[test]
fn an_explicit_project_must_contain_project_godot() {
    let dir = tempfile::tempdir().unwrap();
    assert!(find_project(dir.path(), Some(dir.path())).is_err());
    project(dir.path());
    assert_eq!(
        find_project(Path::new("/elsewhere"), Some(dir.path())).unwrap(),
        dir.path()
    );
    let relative = find_project(dir.path(), Some(Path::new("."))).unwrap();
    assert!(relative.join("project.godot").is_file());
}

#[test]
fn plain_session_names_are_accepted() {
    for name in [
        "default",
        "a",
        "my-game-2",
        "my_game.v2",
        "A1",
        &"x".repeat(64),
    ] {
        assert!(validate_session_name(name).is_ok(), "{name}");
    }
}

#[test]
fn a_session_name_cannot_leave_the_spool_root() {
    for name in [
        "",
        ".",
        "..",
        "../x",
        "a/b",
        r"a\b",
        "/abs",
        r"C:\x",
        "C:x",
        "a b",
        "tab\t",
        "ä",
        &"x".repeat(65),
    ] {
        let error = validate_session_name(name).expect_err(name).to_string();
        assert!(error.contains("session name"), "{name}: {error}");
    }
}

#[test]
fn a_relative_path_is_made_absolute_against_the_working_directory() {
    let cwd = Path::new("/work/project");
    assert_eq!(absolutize(cwd, Path::new(".bridge")), cwd.join(".bridge"));
    let absolute = std::env::temp_dir();
    assert_eq!(absolutize(cwd, &absolute), absolute);
}

#[test]
fn the_default_spool_root_is_private_to_the_user_on_every_platform() {
    let temp = Path::new("/tmp");
    // Unix with a per-user runtime directory: nobody else can enter it.
    assert_eq!(
        default_root(temp, Some(Path::new("/run/user/1000")), Some(1000)),
        Path::new("/run/user/1000/dev_bridge")
    );
    // Unix without one: the shared temporary directory, so the name carries the user id.
    assert_eq!(
        default_root(temp, None, Some(1000)),
        Path::new("/tmp/dev_bridge-1000")
    );
    // Windows and macOS give every user a temporary directory of their own.
    assert_eq!(default_root(temp, None, None), Path::new("/tmp/dev_bridge"));
}

#[test]
fn the_feedback_mode_is_off_draft_or_full_in_any_case_and_full_by_default() {
    use godot_dev_bridge::config::{FeedbackMode, parse_feedback_mode};
    assert_eq!(parse_feedback_mode(None), (FeedbackMode::Full, None));
    assert_eq!(parse_feedback_mode(Some("  ")), (FeedbackMode::Full, None));
    assert_eq!(parse_feedback_mode(Some("OFF")), (FeedbackMode::Off, None));
    assert_eq!(
        parse_feedback_mode(Some("Draft")),
        (FeedbackMode::Draft, None)
    );
    assert_eq!(
        parse_feedback_mode(Some("full")),
        (FeedbackMode::Full, None)
    );
    let (mode, warning) = parse_feedback_mode(Some("loud"));
    assert_eq!(mode, FeedbackMode::Full);
    let warning = warning.expect("a warning for an unknown value");
    assert!(warning.contains("DEV_BRIDGE_FEEDBACK=loud") && warning.contains("off, draft or full"));
}

#[test]
fn the_default_context_records_feedback_and_names_no_gh() {
    let ctx = Context::default();
    assert_eq!(ctx.feedback, godot_dev_bridge::config::FeedbackMode::Full);
    assert_eq!(ctx.gh, None);
}
