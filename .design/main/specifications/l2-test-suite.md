# Test Suite

**Version:** 1.0.0
**Status:** Stable
**Layer:** implementation
**Implements:** l1-verification.md

## Overview

The project's tests: what each test target proves, which need a Godot binary or a display, and the fixtures they drive. Open it before adding a test target or changing what an existing one pins.

## Related Specifications

- [l1-verification.md](l1-verification.md) - What must be proven.
- [l2-ci-release.md](l2-ci-release.md) - Where the tests run.

## 1. Motivation

The suite mixes fast unit tests, source scans and slow engine-backed tests; knowing which proves what keeps a change from skipping the one that matters.

## 2. Constraints & Assumptions

- No third-party GDScript test framework: GDScript unit scripts run under a small runner of the repository's own.
- Engine-backed targets are `#[ignore]` and need `GODOT_PATH` (Godot 4.5+); the window target also needs a display.
- The repository root is a minimal Godot project; `--path .` is required and `--import` runs once for a fresh cache.

## 4. Invariant Compliance

| L1 Invariant | Implementation |
| --- | --- |
| V1 | `e2e_test.rs` drives `tests/fixtures/ticker.tscn` and asserts exact ticks, the hold, the two-tick press, adapter answers with exit statuses, stop and replacement, and closing reasons. |
| V2 | `structure_test.rs`: no `class_name`, export plugin name sorts before `GDScript`, no network classes, no project names, `.gd.uid` committed, `plugin.cfg` names an existing script. `export_test.rs`: a real export leaves runtime, editor scripts and adapter (path or `uid://`) out. |
| V3 | Every target is listed in the gate pipeline of `AGENTS.md`. |
| V4 | Client tests build a `Context` explicitly (`config_test.rs`, `launch_test.rs`, `run_test.rs`). |
| V5 | `docs_test.rs`: every argument of every verb has help text; the README documents every verb. |
| V6 | `version_sync_test.rs` (Cargo.toml equals `plugin.cfg`), `changelog_test.rs` (a non-empty section for the current version). |
| V9 | New behaviour lands with a test that failed before it. |

## 5. Detailed Design

### 5.1 Targets

| Target | Needs | Proves |
| --- | --- | --- |
| `cli_test`, `config_test`, `launch_test`, `run_test`, `session_test`, `spool_test` | nothing | client parsing, resolution, launch command, exit statuses, sessions, exchange |
| `structure_test`, `docs_test`, `version_sync_test`, `changelog_test` | nothing | structural and documentation guards |
| `gdscript_test` | Godot | runs every `tests/gd/*_test.gd` through `tests/gd/check.gd` |
| `e2e_test`, `installer_test`, `export_test`, `load_test` | Godot | real sessions, plugin self-install into a fresh project, real export, concurrency and large answers |
| `window_test` | Godot and a display | `shot` of a real window |

### 5.2 Layout

```plaintext
tests/
├── *_test.rs        Rust targets
├── common/          shared helpers
├── gd/              GDScript unit scripts and their runner
└── fixtures/        ticker scene and script, adapter
```

## Canonical References

| Alias | Path | Purpose |
| --- | --- | --- |
| `[E2E]` | `tests/e2e_test.rs` | Measured facts. |
| `[STRUCTURE]` | `tests/structure_test.rs` | Structural locks. |
| `[EXPORT]` | `tests/export_test.rs` | Export proof. |
| `[GD-RUNNER]` | `tests/gd/check.gd` | GDScript runner. |
| `[GD-TARGET]` | `tests/gdscript_test.rs` | Runs the GDScript unit scripts. |
| `[HELPERS]` | `tests/common/mod.rs` | Shared helpers of the engine-backed targets. |
| `[FIXTURE]` | `tests/fixtures/ticker.gd` | The game under test. |

## Document History

| Version | Date | Author | Description |
| --- | --- | --- | --- |
| 1.0.0 | 2026-10-16 | Agent | Initial specification reverse-engineered from the existing tests. |
