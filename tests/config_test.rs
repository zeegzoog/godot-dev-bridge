use godot_dev_bridge::config::{Context, GodotEnv, find_project, resolve_godot};
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
            console_path: None,
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
            console_path: None,
        },
        ..Context::default()
    };
    assert_eq!(resolve_godot(&ctx, None).unwrap(), plain);
}

#[test]
fn the_console_wrapper_is_preferred_only_on_windows() {
    let dir = tempfile::tempdir().unwrap();
    let console = dir.path().join("console-godot");
    let gui = dir.path().join("gui-godot");
    touch(&console);
    touch(&gui);
    let env = GodotEnv {
        godot_path: Some(gui.to_string_lossy().into()),
        godot: None,
        console_path: Some(console.to_string_lossy().into()),
    };
    let windows = Context {
        godot_env: env.clone(),
        windows: true,
        ..Context::default()
    };
    let other = Context {
        godot_env: env,
        windows: false,
        ..Context::default()
    };
    assert_eq!(resolve_godot(&windows, None).unwrap(), console);
    assert_eq!(resolve_godot(&other, None).unwrap(), gui);
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
