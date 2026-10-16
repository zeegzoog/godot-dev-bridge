//! Text that may reach a public issue: scrubbing, the shape of a message, the fingerprint of a
//! report and percent-encoding. Pure functions; what they need to know arrives as arguments.

use std::path::Path;

const ZERO_WIDTH_SPACE: char = '\u{200B}';

/// Directories whose names must not reach a report, and the session name, replaced by placeholders.
#[derive(Debug, Default, Clone)]
pub struct Known {
    pub project: Option<String>,
    pub root: Option<String>,
    pub session_dir: Option<String>,
    pub session: Option<String>,
    /// Paths compare without regard to case (Windows).
    pub ignore_case: bool,
}

impl Known {
    pub fn new(
        project: Option<&Path>,
        root: Option<&Path>,
        session: Option<&str>,
        ignore_case: bool,
    ) -> Self {
        let text = |path: &Path| {
            let text = path.to_string_lossy();
            let trimmed = text.trim_end_matches(['/', '\\']);
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        };
        Self {
            project: project.and_then(text),
            root: root.and_then(text),
            session_dir: match (root, session) {
                (Some(root), Some(session)) => text(&root.join(session)),
                _ => None,
            },
            session: session.map(str::to_string),
            ignore_case,
        }
    }
}

/// Everything a report may quote passes through here: control and direction-override characters,
/// known directories, other absolute paths, credential-shaped words and Markdown that would notify
/// someone or fetch content are removed or neutralized. A second pass changes nothing.
pub fn scrub(text: &str, known: &Known) -> String {
    let text = strip_controls(text);
    let text = replace_known(&text, known);
    let text = replace_absolute_paths(&text);
    let text = redact_credentials(&text);
    neutralize_markdown(&text)
}

/// Removes control characters except newline and tab, and the characters that reorder text.
pub fn strip_controls(text: &str) -> String {
    text.chars()
        .filter(|&c| c == '\n' || c == '\t' || !c.is_control())
        .filter(|c| !matches!(c, '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}'))
        .collect()
}

fn normalized(c: char, ignore_case: bool) -> char {
    match c {
        '\\' => '/',
        c if ignore_case => c.to_ascii_lowercase(),
        c => c,
    }
}

fn continues_a_name(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '-' | '.')
}

/// Replaces every occurrence of `needle` that does not continue into a longer name, comparing
/// either separator alike.
fn replace_dir(text: &str, needle: &str, placeholder: &str, ignore_case: bool) -> String {
    let hay: Vec<char> = text.chars().collect();
    let pattern: Vec<char> = needle.chars().map(|c| normalized(c, ignore_case)).collect();
    if pattern.is_empty() {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < hay.len() {
        let end = i + pattern.len();
        let matches = end <= hay.len()
            && hay[i..end]
                .iter()
                .zip(&pattern)
                .all(|(&c, &p)| normalized(c, ignore_case) == p)
            && hay.get(end).is_none_or(|&next| !continues_a_name(next))
            && (i == 0 || !continues_a_name(hay[i - 1]));
        if matches {
            out.push_str(placeholder);
            i = end;
        } else {
            out.push(hay[i]);
            i += 1;
        }
    }
    out
}

/// The project directory, the session directory, the spool root (longest first) and the quoted
/// session name become placeholders.
pub fn replace_known(text: &str, known: &Known) -> String {
    let mut dirs: Vec<(&str, &str)> = [
        (known.project.as_deref(), "{project}"),
        (known.session_dir.as_deref(), "{session}"),
        (known.root.as_deref(), "{root}"),
    ]
    .into_iter()
    .filter_map(|(dir, placeholder)| dir.map(|dir| (dir, placeholder)))
    .collect();
    dirs.sort_by_key(|(dir, _)| std::cmp::Reverse(dir.chars().count()));
    let mut text = text.to_string();
    for (dir, placeholder) in dirs {
        text = replace_dir(&text, dir, placeholder, known.ignore_case);
    }
    if let Some(session) = known.session.as_deref().filter(|name| !name.is_empty()) {
        text = text.replace(&format!("'{session}'"), "'{session}'");
    }
    text
}

fn is_token_delimiter(c: char) -> bool {
    c.is_whitespace()
        || matches!(
            c,
            '"' | '\'' | '`' | '<' | '>' | '[' | ']' | ',' | ';' | '|' | '='
        )
}

/// Splits `text` into delimiter characters and the tokens between them, keeping both.
fn tokens(text: &str) -> Vec<(bool, String)> {
    let mut parts: Vec<(bool, String)> = Vec::new();
    for c in text.chars() {
        let delimiter = is_token_delimiter(c);
        match parts.last_mut() {
            Some((false, token)) if !delimiter => token.push(c),
            _ => parts.push((delimiter, c.to_string())),
        }
    }
    parts
}

const UNIX_ROOTS: [&str; 7] = [
    "/home", "/Users", "/tmp", "/var", "/opt", "/mnt", "/private",
];

/// A filesystem path that names a place on this machine. Node paths below `/root` are the
/// engine's scene tree, not the root user's home, and stay.
fn is_absolute_path(token: &str) -> bool {
    let bytes = token.as_bytes();
    if token.starts_with("file://") {
        return true;
    }
    if bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'\\' | b'/')
    {
        return true;
    }
    if token.starts_with("\\\\") {
        return true;
    }
    if !token.starts_with('/') || token.starts_with("//") {
        return false;
    }
    if token == "/root" || token.starts_with("/root/") {
        return false;
    }
    let known_root = UNIX_ROOTS
        .iter()
        .any(|root| token == *root || token.starts_with(&format!("{root}/")));
    let segments = token
        .split('/')
        .filter(|segment| !segment.is_empty())
        .count();
    known_root || segments >= 2
}

/// Absolute paths other than `res://` and `user://` become `{path}`; punctuation around a path
/// stays outside the replacement.
pub fn replace_absolute_paths(text: &str) -> String {
    tokens(text)
        .into_iter()
        .map(|(delimiter, token)| {
            if delimiter {
                return token;
            }
            let start = token.len() - token.trim_start_matches('(').len();
            let core_end = token
                .trim_end_matches(['.', ',', ':', ';', '!', '?', ')'])
                .len()
                .max(start);
            let core = &token[start..core_end];
            if is_absolute_path(core) {
                format!("{}{{path}}{}", &token[..start], &token[core_end..])
            } else {
                token
            }
        })
        .collect()
}

const CREDENTIAL_PREFIXES: [&str; 9] = [
    "ghp_",
    "gho_",
    "ghs_",
    "ghu_",
    "github_pat_",
    "sk-",
    "xoxb-",
    "xoxp-",
    "AKIA",
];

fn is_word_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-'
}

/// Splits into runs of word characters and runs of everything else, keeping both.
fn runs(text: &str, belongs: fn(char) -> bool) -> Vec<(bool, String)> {
    let mut parts: Vec<(bool, String)> = Vec::new();
    for c in text.chars() {
        let inside = belongs(c);
        match parts.last_mut() {
            Some((kind, run)) if *kind == inside => run.push(c),
            _ => parts.push((inside, c.to_string())),
        }
    }
    parts
}

fn looks_like_key(run: &str) -> bool {
    run.len() >= 32
        && run.chars().any(|c| c.is_ascii_alphabetic())
        && run.chars().any(|c| c.is_ascii_digit())
}

/// Tokens with a known credential prefix, the value after `Bearer`, and long runs of letters and
/// digits that mix both become `{redacted}`.
pub fn redact_credentials(text: &str) -> String {
    let mut words = runs(text, is_word_char);
    for (word, run) in words.iter_mut() {
        if *word
            && CREDENTIAL_PREFIXES
                .iter()
                .any(|prefix| run.starts_with(prefix) && run.len() >= prefix.len() + 8)
        {
            *run = "{redacted}".to_string();
        }
    }
    let text: String = words.into_iter().map(|(_, run)| run).collect();

    let mut out = String::with_capacity(text.len());
    let mut rest = text.as_str();
    while let Some(at) = rest.find("Bearer") {
        let (before, after) = rest.split_at(at + "Bearer".len());
        out.push_str(before);
        let spaces = after.len() - after.trim_start_matches([' ', '\t']).len();
        let value = &after[spaces..];
        let value_len = value
            .find(|c: char| !(c.is_ascii_alphanumeric() || "._~+/=-".contains(c)))
            .unwrap_or(value.len());
        if spaces > 0 && value_len >= 8 {
            out.push_str(&after[..spaces]);
            out.push_str("{redacted}");
            rest = &value[value_len..];
        } else {
            rest = after;
        }
    }
    out.push_str(rest);

    runs(&out, |c| c.is_ascii_alphanumeric())
        .into_iter()
        .map(|(alnum, run)| {
            if alnum && looks_like_key(&run) {
                "{redacted}".to_string()
            } else {
                run
            }
        })
        .collect()
}

/// Mentions and issue references stop linking, raw markup shows as text, and images are not
/// fetched.
pub fn neutralize_markdown(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    for (i, &c) in chars.iter().enumerate() {
        let next = chars.get(i + 1).copied();
        match c {
            '@' if next.is_some_and(|n| n.is_alphanumeric() || n == '_') => {
                out.push('@');
                out.push(ZERO_WIDTH_SPACE);
            }
            '#' if next.is_some_and(|n| n.is_ascii_digit()) => {
                out.push('#');
                out.push(ZERO_WIDTH_SPACE);
            }
            '<' => out.push_str("&lt;"),
            '!' if next == Some('[') => out.push_str("!\\"),
            c => out.push(c),
        }
    }
    out
}

/// Placeholders, engine resource paths, node paths and filesystem paths: the parts of a message
/// that name the project or the machine rather than the problem.
fn is_placeholder_or_path(token: &str) -> bool {
    let trimmed =
        token.trim_matches(|c: char| !c.is_alphanumeric() && !matches!(c, '/' | '\\' | '{' | '}'));
    trimmed.contains('{') && trimmed.contains('}')
        || trimmed.contains("res://")
        || trimmed.contains("user://")
        || trimmed.starts_with('/')
        || is_absolute_path(trimmed)
}

/// What stays of a message when the details that differ between runs and machines are gone: a
/// stable basis for the fingerprint.
pub fn shape(message: &str) -> String {
    let words: Vec<String> = message
        .split_whitespace()
        .map(|word| {
            if is_placeholder_or_path(word) {
                "<path>".to_string()
            } else {
                word.to_lowercase()
                    .chars()
                    .map(|c| if c.is_ascii_digit() { '#' } else { c })
                    .collect()
            }
        })
        .collect();
    words.join(" ").chars().take(120).collect()
}

/// FNV-1a, 64 bit.
pub fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// What identifies the problem a draft reports, independent of its wording.
pub struct FingerprintBasis<'a> {
    pub kind: &'a str,
    pub verbs: Vec<String>,
    pub shapes: Vec<String>,
    pub attempts: Vec<String>,
    pub first_summary: &'a str,
}

/// One alphanumeric token, so the tracker's search treats it as a single term.
pub fn fingerprint(basis: FingerprintBasis) -> String {
    let FingerprintBasis {
        kind,
        mut verbs,
        mut shapes,
        mut attempts,
        first_summary,
    } = basis;
    verbs.sort();
    verbs.dedup();
    shapes.sort();
    shapes.dedup();
    attempts.sort();
    attempts.dedup();
    let mut key = format!(
        "kind:{kind}\nverbs:{}\nshapes:{}\nattempts:{}",
        verbs.join(","),
        shapes.join("\u{1F}"),
        attempts.join(",")
    );
    if verbs.is_empty() && shapes.is_empty() && attempts.is_empty() {
        let words: Vec<String> = first_summary
            .split(|c: char| !c.is_alphanumeric())
            .filter(|word| !word.is_empty())
            .map(str::to_lowercase)
            .collect();
        key.push_str("\nsummary:");
        key.push_str(&words.join(" "));
    }
    let hex = format!("{:016x}", fnv1a64(key.as_bytes()));
    format!("gdbfb{}", &hex[..12])
}

/// Every byte except the unreserved characters `A-Z a-z 0-9 - . _ ~` as `%XX`.
pub fn percent_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 3);
    for &byte in text.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn known() -> Known {
        Known {
            project: Some("C:\\Users\\Ann\\Games\\Rocket".into()),
            root: Some("C:\\Users\\Ann\\AppData\\Local\\Temp\\dev_bridge".into()),
            session_dir: Some(
                "C:\\Users\\Ann\\AppData\\Local\\Temp\\dev_bridge\\rocket-run".into(),
            ),
            session: Some("rocket-run".into()),
            ignore_case: true,
        }
    }

    #[test]
    fn control_and_direction_characters_are_removed() {
        assert_eq!(
            strip_controls("a\u{0007}b\u{202E}c\u{2067}d\n\te"),
            "abcd\n\te"
        );
    }

    #[test]
    fn the_project_session_and_root_become_placeholders_with_either_separator_and_any_case() {
        let text = "at c:/users/ann/games/rocket/main.gd and C:\\Users\\Ann\\AppData\\Local\\Temp\\dev_bridge\\rocket-run\\game.log in C:\\Users\\Ann\\AppData\\Local\\Temp\\dev_bridge";
        assert_eq!(
            replace_known(text, &known()),
            "at {project}/main.gd and {session}\\game.log in {root}"
        );
    }

    #[test]
    fn a_known_directory_inside_a_longer_name_is_left_alone() {
        assert_eq!(
            replace_known("C:\\Users\\Ann\\Games\\Rocket2\\x", &known()),
            "C:\\Users\\Ann\\Games\\Rocket2\\x"
        );
    }

    #[test]
    fn the_quoted_session_name_becomes_a_placeholder() {
        assert_eq!(
            replace_known("no session named 'rocket-run'", &known()),
            "no session named '{session}'"
        );
    }

    #[test]
    fn other_absolute_paths_become_placeholders_and_engine_paths_stay() {
        assert_eq!(
            replace_absolute_paths(
                "read /home/ann/x.txt, D:\\Work\\a.gd; (/var/log) \\\\srv\\share file:///Users/ann res://a.tscn user://save /root/Main/Player /etc"
            ),
            "read {path}, {path}; ({path}) {path} {path} res://a.tscn user://save /root/Main/Player /etc"
        );
    }

    #[test]
    fn placeholders_with_a_path_after_them_stay_one_token() {
        assert_eq!(
            replace_absolute_paths("{project}/a/b.gd"),
            "{project}/a/b.gd"
        );
    }

    #[test]
    fn credential_shaped_words_are_redacted() {
        assert_eq!(
            redact_credentials("token ghp_abcdefgh1234 and github_pat_11ABCDEFGH0123"),
            "token {redacted} and {redacted}"
        );
        assert_eq!(
            redact_credentials("Authorization: Bearer abc.def-123456"),
            "Authorization: Bearer {redacted}"
        );
        assert_eq!(redact_credentials("key AKIAABCDEFGH12"), "key {redacted}");
        assert_eq!(
            redact_credentials("hash 3f1e9a7c2b4d6e8f0a1b3c5d7e9f1a2b3c4d"),
            "hash {redacted}"
        );
    }

    #[test]
    fn identifiers_and_short_words_are_not_credentials() {
        let text = "sk-short release_all ui_accept 0123456789ab res://scenes/main_menu.tscn";
        assert_eq!(redact_credentials(text), text);
        let uuid = "123e4567-e89b-12d3-a456-426614174000";
        assert_eq!(redact_credentials(uuid), uuid);
    }

    #[test]
    fn markdown_that_notifies_or_fetches_is_neutralized() {
        assert_eq!(
            neutralize_markdown("@ann see #12 <b> ![x](http://e/i.png) a@ b# c"),
            "@\u{200B}ann see #\u{200B}12 &lt;b> !\\[x](http://e/i.png) a@ b# c"
        );
    }

    #[test]
    fn scrubbing_twice_changes_nothing() {
        let text = "C:\\Users\\Ann\\Games\\Rocket\\a.gd: @ann #3 <x> ![i] ghp_abcdefgh12345 /home/ann/y Bearer abcdefgh123";
        let once = scrub(text, &known());
        assert_eq!(scrub(&once, &known()), once);
        assert!(!once.contains("Ann") && !once.contains("ann/"));
    }

    #[test]
    fn a_shape_ignores_numbers_paths_and_case() {
        assert_eq!(
            shape("Node  /root/Main not found at {path} after 30 ticks in res://a.tscn"),
            "node <path> not found at <path> after ## ticks in <path>"
        );
        assert_eq!(shape(&"x".repeat(300)).len(), 120);
    }

    #[test]
    fn a_fingerprint_ignores_wording_when_facts_exist_and_order() {
        let first = fingerprint(FingerprintBasis {
            kind: "bug",
            verbs: vec!["step".into(), "tap".into()],
            shapes: vec!["a".into()],
            attempts: vec![],
            first_summary: "Step hangs",
        });
        let second = fingerprint(FingerprintBasis {
            kind: "bug",
            verbs: vec!["tap".into(), "step".into(), "step".into()],
            shapes: vec!["a".into()],
            attempts: vec![],
            first_summary: "Stepping never returns",
        });
        assert_eq!(first, second);
        assert_eq!(first.len(), 17);
        assert!(first.starts_with("gdbfb") && first.chars().all(|c| c.is_ascii_alphanumeric()));
    }

    #[test]
    fn without_facts_the_summary_words_decide() {
        let basis = |summary| FingerprintBasis {
            kind: "docs",
            verbs: vec![],
            shapes: vec![],
            attempts: vec![],
            first_summary: summary,
        };
        assert_eq!(
            fingerprint(basis("README: unclear!")),
            fingerprint(basis("readme unclear"))
        );
        assert_ne!(
            fingerprint(basis("readme unclear")),
            fingerprint(basis("help unclear"))
        );
    }

    #[test]
    fn percent_encoding_keeps_only_unreserved_characters() {
        assert_eq!(percent_encode("a b/é~_.-"), "a%20b%2F%C3%A9~_.-");
    }
}
