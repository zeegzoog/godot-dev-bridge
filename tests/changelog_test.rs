//! A release can never go out without notes: the changelog documents the current version.

#[test]
fn the_changelog_has_notes_for_the_current_version() {
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/CHANGELOG.md"))
        .expect("CHANGELOG.md exists");
    let header = format!("## [{}]", env!("CARGO_PKG_VERSION"));
    let start = text
        .find(&header)
        .unwrap_or_else(|| panic!("CHANGELOG.md has no '{header}' section"));
    let notes: Vec<&str> = text[start..]
        .lines()
        .skip(1)
        .take_while(|line| {
            !line.starts_with("## [") && !(line.starts_with('[') && line.contains("]: "))
        })
        .filter(|line| !line.trim().is_empty())
        .collect();
    assert!(!notes.is_empty(), "the '{header}' section is empty");
}
