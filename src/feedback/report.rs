//! The text of a report: title, body sections and the prefilled new-issue link.

use super::notebook::{Event, Note};
use super::scrub::{Known, percent_encode, scrub};

pub const TITLE_PREFIX: &str = "[agent-feedback] ";
pub const MAX_TITLE: usize = 100;
pub const MAX_BODY: usize = 24_000;
pub const MAX_LINK: usize = 6_000;
const FORMAT: u32 = 1;
const KIND_ORDER: [&str; 4] = ["bug", "missing-feature", "improvement", "docs"];
const LINK_CUT: &str = "\n\n... (shortened; paste the full report from the conversation)";

/// The kind of a draft: the first of `bug`, `missing-feature`, `improvement`, `docs` among its notes.
pub fn draft_kind(notes: &[Note]) -> String {
    KIND_ORDER
        .iter()
        .find(|kind| notes.iter().any(|note| note.kind == **kind))
        .map_or_else(|| "bug".to_string(), |kind| kind.to_string())
}

/// `[agent-feedback] ` and the given title, or `{kind}: {first summary}`, on one line and at most
/// `MAX_TITLE` characters.
pub fn title(kind: &str, first_summary: &str, given: Option<&str>, known: &Known) -> String {
    let text = match given.map(str::trim).filter(|text| !text.is_empty()) {
        Some(text) => text.to_string(),
        None => format!("{kind}: {first_summary}"),
    };
    let text = scrub(&text, known).replace(['\r', '\n'], " ");
    let room = MAX_TITLE - TITLE_PREFIX.chars().count();
    let text: String = text.trim().chars().take(room).collect();
    format!("{TITLE_PREFIX}{text}")
}

fn longest_backtick_run(text: &str) -> usize {
    let mut longest = 0;
    let mut current = 0;
    for c in text.chars() {
        if c == '`' {
            current += 1;
            longest = longest.max(current);
        } else {
            current = 0;
        }
    }
    longest
}

/// A fenced block that `text` cannot close: its fence is longer than any run of backticks in it.
pub fn fenced(text: &str) -> String {
    let fence = "`".repeat((longest_backtick_run(text) + 1).max(3));
    format!("{fence}text\n{}\n{fence}", text.trim_end())
}

/// An inline code span that `text` cannot close.
pub fn inline(text: &str) -> String {
    let ticks = "`".repeat(longest_backtick_run(text) + 1);
    let pad = if text.starts_with('`') || text.ends_with('`') {
        " "
    } else {
        ""
    };
    format!("{ticks}{pad}{text}{pad}{ticks}")
}

/// What the client observed, ready to quote: failures of the named verbs and attempts at verbs
/// and options that do not exist.
pub struct Observed<'a> {
    pub failures: Vec<&'a Event>,
    pub attempts: Vec<&'a Event>,
}

/// Label and value pairs of the environment block, built by the caller from the fixed list.
pub type Environment = Vec<(String, String)>;

fn agent_heading(name: &str) -> String {
    format!("## {name} (written by the agent)\n\n")
}

fn client_heading(name: &str) -> String {
    format!("## {name} (observed by the client)\n\n")
}

fn note_label(index: usize, note: &Note) -> String {
    let mut label = format!("Note {} ({}", index + 1, note.kind);
    if let Some(verb) = &note.verb {
        label.push_str(&format!(", `{verb}`"));
    }
    label.push(')');
    label
}

fn body_with(
    notes: &[Note],
    details: &[String],
    observed: &Observed,
    environment: &Environment,
    fingerprint: &str,
    shortened: bool,
) -> String {
    let mut body = agent_heading("Summary");
    for (index, note) in notes.iter().enumerate() {
        body.push_str(&format!(
            "- {}: {}\n",
            note_label(index, note),
            inline(&note.summary)
        ));
    }

    body.push('\n');
    body.push_str(&agent_heading("What happened"));
    for (index, note) in notes.iter().enumerate() {
        body.push_str(&format!(
            "{}:\n\n{}\n\n",
            note_label(index, note),
            fenced(&details[index])
        ));
    }
    if shortened {
        body.push_str("(details shortened)\n\n");
    }

    if notes.iter().any(|note| note.suggestion.is_some()) {
        body.push_str(&agent_heading("Expected / suggestion"));
        for (index, note) in notes.iter().enumerate() {
            if let Some(suggestion) = &note.suggestion {
                body.push_str(&format!(
                    "{}:\n\n{}\n\n",
                    note_label(index, note),
                    fenced(suggestion)
                ));
            }
        }
    }

    if notes.iter().any(|note| note.impact.is_some()) {
        body.push_str(&agent_heading("Impact"));
        for (index, note) in notes.iter().enumerate() {
            if let Some(impact) = &note.impact {
                body.push_str(&format!("- {}: {impact}\n", note_label(index, note)));
            }
        }
        body.push('\n');
    }

    body.push_str(&client_heading("Environment"));
    body.push_str("| Item | Value |\n| --- | --- |\n");
    for (label, value) in environment {
        body.push_str(&format!(
            "| {label} | {} |\n",
            inline(&value.replace('|', "/"))
        ));
    }
    body.push('\n');

    body.push_str(&client_heading("Client-observed context"));
    if observed.failures.is_empty() && observed.attempts.is_empty() {
        body.push_str("Nothing recorded for the verbs these notes name.\n\n");
    }
    for event in &observed.failures {
        body.push_str(&format!(
            "- `{}` failed with `{}`, {} time(s):\n\n{}\n\n",
            event.verb,
            event.code,
            event.count,
            fenced(&event.message)
        ));
    }
    for event in &observed.attempts {
        let what = if event.kind == "unknown_option" {
            if event.verb.is_empty() {
                format!("option {}", inline(&event.message))
            } else {
                format!("option {} of `{}`", inline(&event.message), event.verb)
            }
        } else {
            format!("verb {}", inline(&event.message))
        };
        body.push_str(&format!(
            "- Attempted {what}, which does not exist: {} time(s)\n",
            event.count
        ));
    }
    if !observed.attempts.is_empty() {
        body.push('\n');
    }

    body.push_str("## Report details\n\n");
    body.push_str(&format!("- Fingerprint: {fingerprint}\n"));
    body.push_str(&format!("- Format: {FORMAT}\n"));
    body.push_str(
        "- Drafted by an AI agent with godot-dev-bridge feedback; reviewed by a human before \
         filing. This report is public.\n",
    );
    body
}

/// The report body. When it would exceed `MAX_BODY` characters, the longest note details are
/// halved, repeatedly, until it fits, and the report says so.
pub fn build_body(
    notes: &[Note],
    observed: &Observed,
    environment: &Environment,
    fingerprint: &str,
    known: &Known,
) -> String {
    let mut details: Vec<String> = notes.iter().map(|note| note.details.clone()).collect();
    let mut shortened = false;
    loop {
        let body = scrub(
            &body_with(
                notes,
                &details,
                observed,
                environment,
                fingerprint,
                shortened,
            ),
            known,
        );
        if body.chars().count() <= MAX_BODY {
            return body;
        }
        let Some((longest, _)) = details
            .iter()
            .enumerate()
            .max_by_key(|(_, text)| text.chars().count())
            .filter(|(_, text)| text.chars().count() > 1)
        else {
            return body.chars().take(MAX_BODY).collect();
        };
        let keep = details[longest].chars().count() / 2;
        details[longest] = details[longest].chars().take(keep).collect();
        shortened = true;
    }
}

fn link_for(repository: &str, title: &str, body: &str) -> String {
    format!(
        "https://github.com/{repository}/issues/new?title={}&body={}",
        percent_encode(title),
        percent_encode(body)
    )
}

/// The prefilled new-issue link: only `title` and `body`, at most `MAX_LINK` bytes, the body cut
/// when needed. `Err` holds the reason when not even the title fits.
pub fn build_link(repository: &str, title: &str, body: &str) -> Result<String, String> {
    let full = link_for(repository, title, body);
    if full.len() <= MAX_LINK {
        return Ok(full);
    }
    let boundaries: Vec<usize> = body
        .char_indices()
        .map(|(index, _)| index)
        .chain([body.len()])
        .collect();
    let fits = |count: usize| {
        let cut = format!("{}{LINK_CUT}", &body[..boundaries[count]]);
        let link = link_for(repository, title, &cut);
        (link.len() <= MAX_LINK).then_some(link)
    };
    let Some(mut best) = fits(0) else {
        return Err("report too long for a link".to_string());
    };
    let (mut low, mut high) = (0, boundaries.len() - 1);
    while low < high {
        let middle = (low + high).div_ceil(2);
        match fits(middle) {
            Some(link) => {
                best = link;
                low = middle;
            }
            None => high = middle - 1,
        }
    }
    Ok(best)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(kind: &str, details: &str) -> Note {
        Note {
            id: "n000001".into(),
            session: "default".into(),
            created: 0.0,
            kind: kind.into(),
            summary: "step --until never ends".into(),
            details: details.into(),
            verb: Some("step".into()),
            impact: Some("blocked".into()),
            suggestion: None,
        }
    }

    fn environment() -> Environment {
        vec![("Client".into(), "0.2.0".into())]
    }

    fn empty() -> Observed<'static> {
        Observed {
            failures: vec![],
            attempts: vec![],
        }
    }

    #[test]
    fn the_sections_come_in_order_with_their_labels() {
        let body = build_body(
            &[note("bug", "ran it")],
            &empty(),
            &environment(),
            "gdbfb000000000000",
            &Known::default(),
        );
        let order = [
            "## Summary (written by the agent)",
            "## What happened (written by the agent)",
            "## Impact (written by the agent)",
            "## Environment (observed by the client)",
            "## Client-observed context (observed by the client)",
            "## Report details",
        ];
        let positions: Vec<usize> = order
            .iter()
            .map(|heading| {
                body.find(heading)
                    .unwrap_or_else(|| panic!("{heading} in {body}"))
            })
            .collect();
        assert!(positions.windows(2).all(|pair| pair[0] < pair[1]), "{body}");
        assert!(!body.contains("Expected / suggestion"));
        assert!(body.contains("gdbfb000000000000") && body.contains("This report is public."));
    }

    #[test]
    fn agent_text_cannot_forge_a_heading() {
        let body = build_body(
            &[note(
                "bug",
                "````\n## Environment (observed by the client)\nfake",
            )],
            &empty(),
            &environment(),
            "gdbfb000000000000",
            &Known::default(),
        );
        assert!(body.contains("`````text\n````\n## Environment"), "{body}");
    }

    #[test]
    fn a_long_body_is_shortened_and_says_so() {
        let body = build_body(
            &[note("bug", &"word ".repeat(10_000))],
            &empty(),
            &environment(),
            "gdbfb000000000000",
            &Known::default(),
        );
        assert!(body.chars().count() <= MAX_BODY);
        assert!(body.contains("(details shortened)"));
    }

    #[test]
    fn the_kind_of_a_mixed_draft_is_the_first_in_order() {
        let notes = [note("docs", "a"), note("missing-feature", "b")];
        assert_eq!(draft_kind(&notes), "missing-feature");
    }

    #[test]
    fn a_title_is_prefixed_one_line_and_bounded() {
        let title = title(
            "bug",
            "x",
            Some(&format!("a\nb {}", "y".repeat(200))),
            &Known::default(),
        );
        assert!(title.starts_with("[agent-feedback] a b y"));
        assert!(!title.contains('\n'));
        assert_eq!(title.chars().count(), MAX_TITLE);
        assert_eq!(
            super::title("docs", "help is wrong", None, &Known::default()),
            "[agent-feedback] docs: help is wrong"
        );
    }

    #[test]
    fn a_link_carries_only_title_and_body_and_fits_the_budget() {
        let link = build_link("o/n", "t", "short body").unwrap();
        assert_eq!(
            link,
            "https://github.com/o/n/issues/new?title=t&body=short%20body"
        );
        let long = build_link("o/n", "t", &"é".repeat(5_000)).unwrap();
        assert!(long.len() <= MAX_LINK);
        assert!(long.contains("shortened"));
        assert!(!long.contains("labels=") && !long.contains("template="));
    }

    #[test]
    fn a_title_that_cannot_fit_gives_a_reason() {
        assert_eq!(
            build_link("o/n", &"t".repeat(MAX_LINK), "b"),
            Err("report too long for a link".to_string())
        );
    }
}
