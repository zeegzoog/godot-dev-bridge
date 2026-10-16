---
phase: 1
name: "Agent Feedback Channel"
status: Done
subsystem: "src/feedback"
requires: [l1-agent-feedback.md, l2-agent-feedback-client.md]
provides:
  - "feedback verb: note, list, draft, submit, discard"
  - "notebook files under <root>/@feedback, per session name, 7-day expiry"
  - "evidence of failing core verbs and of unknown verbs and options, recorded after every run"
  - "scrubbed report with fingerprint and prefilled link; submission through gh with duplicate look-up"
  - "feedback hints on the start and stop answers; DEV_BRIDGE_FEEDBACK and DEV_BRIDGE_GH settings"
key_files:
  created: [src/feedback.rs, src/feedback/scrub.rs, src/feedback/notebook.rs, src/feedback/report.rs, src/feedback/issues.rs, examples/fake_gh.rs, tests/feedback_test.rs, .github/ISSUE_TEMPLATE/agent-feedback.md]
  modified: [src/lib.rs, src/cli.rs, src/config.rs, src/launch.rs, tests/common/mod.rs, tests/config_test.rs, tests/docs_test.rs, tests/e2e_test.rs, README.md, AGENTS.md, CHANGELOG.md, .env.example]
patterns_established:
  - "The command line is built per feedback mode (cli::command) so help text can follow the mode"
  - "Outcome recording after a run reads the final status and object and never changes them"
  - "Tests drive child-process behaviour through the child's environment only (stand-in gh)"
duration_minutes: ~
---

# Stage 1 Tasks — Agent Feedback Channel

**Phase:** 1
**Status:** Done
**Strategic Goal:** An agent can record notes about the bridge itself, draft one scrubbed report per problem, and, after the human's yes, submit it as a GitHub issue; every other verb behaves exactly as before.

## Atomic Checklist

<!-- A line is `- [ ]` (open) or `- [x]` (done). A Blocked task keeps `- [ ]`: it is blocked by its `Status` in Detailed Tracking, and the `[!]` the workflows write after "Blocked" spells that status — it is not a checkbox state. -->

- [x] [T-1A01] Scrubber, shape, fingerprint and percent-encoding
- [x] [T-1A02] Report body, title, environment block and prefilled link
- [x] [T-1B01] Feedback settings in `Context`
- [x] [T-1B02] Notebook files and `feedback note`, `list`, `discard`
- [x] [T-1B03] Evidence recording after every run
- [x] [T-1B04] `feedback draft` and the hints on `start` and `stop`
- [x] [T-1C01] `feedback submit` through `gh`, with the stand-in executable
- [x] [T-1D01] Documentation: README, `.env.example`, AGENTS.md, issue template, CHANGELOG
- [x] [T-1T01] Validation: gate pipeline and spec compliance

## Detailed Tracking

### [T-1A01] Scrubber, shape, fingerprint and percent-encoding

- **Spec:** l2-agent-feedback-client.md §5.6, §5.8, §5.7 (percent-encoding)
- **Status:** Done
- **Assignment:** Agent
- **Verify:** `cargo test --lib feedback::scrub` passes with one test per scrubber step (control and bidirectional characters, project and root paths with both separators and Windows case, other absolute paths, `res://` and `user://` kept, each credential class, mentions, issue references, `<`, `![`), an idempotence test, and fingerprint stability under reworded summaries.
- **Handoff:** T-1A02 and T-1B03 use `scrub` and `shape`.
- **Notes:** Pure functions in `src/feedback/scrub.rs`, no new crate. The project directory and the spool root arrive as parameters, never read from the environment.
- **Changes:** `src/feedback/scrub.rs`: scrub (controls, known dirs, absolute paths, credentials, Markdown), shape, FNV-1a fingerprint, percent-encoding; 14 unit tests (`cargo test --lib feedback::scrub`, exit 0).

### [T-1A02] Report body, title, environment block and prefilled link

- **Spec:** l2-agent-feedback-client.md §5.7, §5.2 (title rules); l1-agent-feedback.md §4.3, §4.4
- **Status:** Done
- **Assignment:** Agent
- **Verify:** `cargo test --lib feedback::report` passes: the sections and their labels appear in the order of the parent's 4.3; agent text with a run of backticks stays inside a longer fence; a body over 24,000 characters is shortened with "(details shortened)"; a link never exceeds 6,000 bytes and carries only `title` and `body`; a title that cannot fit yields the reason instead of a link; the environment block never contains `adapter.path`, `adapter.error` or the session name.
- **Handoff:** T-1B04 assembles drafts with these builders.
- **Notes:** `src/feedback/report.rs`; inputs are plain values (notes, evidence, environment facts), so the tests need no files.
- **Changes:** `src/feedback/report.rs`: title, fenced/inline quoting, sectioned body with labels and shortening, link within 6,000 bytes; 7 unit tests (`cargo test --lib feedback::report`, exit 0).

### [T-1B01] Feedback settings in `Context`

- **Spec:** l2-agent-feedback-client.md §2 (settings table)
- **Status:** Done
- **Assignment:** Agent
- **Verify:** `cargo test --test config_test` passes with cases for `off`, `draft`, `full` in any case, the empty value, an unknown value (default selected, warning text produced for standard error only) and `DEV_BRIDGE_GH`.
- **Handoff:** Every later task reads the mode and the `gh` path from `Context`.
- **Notes:** Only `Context::from_env` reads the variables; the parsing of a mode value is a separate pure function so tests do not mutate the environment.
- **Changes:** `FeedbackMode`, `parse_feedback_mode`, `Context.feedback`/`Context.gh`, `find_program`; 2 tests in `tests/config_test.rs` (exit 0).

### [T-1B02] Notebook files and `feedback note`, `list`, `discard`

- **Spec:** l2-agent-feedback-client.md §4 (F1, F5, F12), §5.2, §5.3
- **Status:** Done
- **Assignment:** Agent
- **Verify:** `cargo test --test feedback_test --test docs_test --test session_test` passes: limits refused with messages naming them; two session names keep separate notebooks; entries older than 7 days are expired; `discard` removes only its own files and follows no link; `<root>/@feedback` that is a link is refused; `sessions` and `sessions --prune` leave `@feedback` alone; `--session @feedback` is refused; every nested subcommand argument has help text.
- **Handoff:** T-1B03 writes evidence into the same notebook; T-1B04 builds drafts from its notes.
- **Notes:** `src/feedback/notebook.rs`, `src/feedback.rs`, the `Feedback` verb in `src/cli.rs`; `docs_test.rs` walks nested subcommands. Feedback verbs send no request and exit 0 or 2 only.
- **Changes:** `src/feedback/notebook.rs` and the `feedback` verb in `src/cli.rs`; note/list/discard; nested-help check in `tests/docs_test.rs`; limits, separate notebooks, expiry, link refusal, sessions/prune pass-over in `tests/feedback_test.rs` (exit 0).

### [T-1B03] Evidence recording after every run

- **Spec:** l2-agent-feedback-client.md §5.4, §4 (F6, F11)
- **Status:** Done
- **Assignment:** Agent
- **Verify:** `cargo test --test feedback_test` passes: a failing core verb records one `failure` entry whose count grows on repetition; an unknown verb and an unknown option record `unknown_verb` and `unknown_option`; `do`, `state`, `raw`, successes, `--help` and `--version` record nothing; a message with a log tail keeps only its first line; the same failing command gives byte-identical output and the same exit status with the mode `full` and `off`; an unwritable root changes nothing in the output.
- **Handoff:** T-1B04 quotes this evidence in drafts.
- **Notes:** `observe` is called once in `run` after the outcome is final; all its errors are discarded. The session name of a usage error is read from the argument list.
- **Changes:** `observe_usage`/`observe_run` called from `run` after the outcome is final; tests for counting, unknown verbs/options, excluded verbs, first-line-only, full/off parity, unwritable root (exit 0).

### [T-1B04] `feedback draft` and the hints on `start` and `stop`

- **Spec:** l2-agent-feedback-client.md §5.2 (`draft`), §5.5, §5.10; l1-agent-feedback.md F8, F10
- **Status:** Done
- **Assignment:** Agent
- **Verify:** `cargo test --test feedback_test` passes: a draft defaults to undrafted notes, accepts `--notes` with drafted ones, refuses an empty selection and unknown ids; the answer has `report`, `link` or `link_reason`, `delivery`, `left_out` and `next`; `draft` mode never mentions submission; the stop hint function returns `pending` only when undrafted notes exist. `GODOT_PATH=<godot> cargo test --test e2e_test -- --ignored` passes with the `start` hint and the `stop` `pending` field asserted.
- **Handoff:** T-1C01 submits these drafts.
- **Notes:** Environment facts come from the notebook session's `ready.json` and the addon's `plugin.cfg`; a fact that cannot be read is `unknown`.
- **Changes:** `feedback draft` (selection, environment block, evidence, fingerprint, link, delivery, next) and the start/stop hints; e2e `start_mentions_the_feedback_channel_and_stop_reports_pending_notes` passes with Godot (exit 0).

### [T-1C01] `feedback submit` through `gh`, with the stand-in executable

- **Spec:** l2-agent-feedback-client.md §5.9, §4 (F2, F3, F7, F9), §5.12 (stand-in)
- **Status:** Done
- **Assignment:** Agent
- **Verify:** `cargo test --test feedback_test` passes against `examples/fake_gh.rs` (selected with `DEV_BRIDGE_GH`): argument vectors exactly as in 5.9; body on stdin equal to the draft body; created (recorded, a repeat starts no `gh`), duplicate with and without `--allow-duplicate`, look-up failure reported in `lookup`, not signed in, `gh` missing, exit 0 without an address (`unknown`), deadline; a fresh lock refuses, a stale lock is replaced; not created and unknown exit 2 with `code` `client` and `outcome`; the client's standard output holds one JSON object and nothing from `gh`; no subcommand argument names a repository, host, address or credential.
- **Handoff:** T-1D01 documents the outcomes.
- **Notes:** `src/feedback/issues.rs`; synchronous polling with threads serving the pipes; process tree killed at the deadline; Windows handle privacy shared from `src/launch.rs`. The stand-in is built by the test helper before use.
- **Changes:** `src/feedback/issues.rs` (gh runner with deadline and pipe threads, look-up, create, outcome mapping) and `feedback submit` with lock; `examples/fake_gh.rs`; 8 submission tests plus the deadline test (exit 0).

### [T-1D01] Documentation: README, `.env.example`, AGENTS.md, issue template, CHANGELOG

- **Spec:** l2-agent-feedback-client.md §5.11
- **Status:** Done
- **Assignment:** Agent
- **Verify:** `cargo test --test docs_test --test changelog_test` passes; the README has the `feedback` row, the feedback section, the optional `feedback` fields of `start` and `stop`, the amended "No network" and "keeps nothing between runs" sentences; `.github/ISSUE_TEMPLATE/agent-feedback.md` exists; `grep -rnE "\.design/|l[12]-[a-z-]+\.md|T-[0-9]+[A-Z][0-9]+" README.md AGENTS.md CHANGELOG.md src tests .github .env.example` prints nothing.
- **Handoff:** T-1T01.
- **Notes:** Product files carry no reference to specifications, tasks or the design workspace.
- **Changes:** README (commands row, start/stop fields, feedback section, settings, No-network and keeps-nothing sentences), `.env.example`, AGENTS.md step 6, issue template, CHANGELOG Unreleased entry; `docs_test` and `changelog_test` pass; SDD-reference scan empty.

### [T-1T01] Validation: gate pipeline and spec compliance

- **Spec:** l1-agent-feedback.md §3 (F1–F12); l2-agent-feedback-client.md §4
- **Status:** Done
- **Assignment:** Agent
- **Verify:** `cargo fmt --all -- --check`, `cargo clippy --all-targets --all-features -- -D warnings` and `cargo test --all-targets` are clean; with Godot, `cargo test --test gdscript_test --test e2e_test --test installer_test --test export_test --test load_test -- --ignored` passes; each row of the L2 Invariant Compliance table is traced to code and a named test; `godot-dev-bridge sessions` shows no game left running.
- **Handoff:** Phase complete.
- **Notes:** The addon is not touched by this phase; if a change reaches `addons/`, the GDScript checks of the gate ladder apply as well.
- **Changes:** fmt and clippy clean; `cargo test --all-targets` all pass; Godot-backed gdscript/e2e/installer/export/load tests pass; no session left running.
