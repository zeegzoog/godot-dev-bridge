# Agent Guidelines & Engineering Standards

This document sets the architectural principles, development workflow and operational rules for AI
agents and human contributors working on godot-dev-bridge.

## Mission & Scope

A reusable development bridge for any Godot 4.5+ project: a shell (or an agent with a shell) starts
the running game, holds it still, steps it by an exact number of physics ticks, injects input, reads
state and takes screenshots of the window the game drew. It drives the **running game**; it does not
edit scenes in the editor.

## Architecture Guidelines

### 1. Files, not sockets

- Client and game talk through a spool directory (`dev_bridge/<session>/` under a root that only
  the user can use; the session name is validated, because starting a session clears its
  directory): one
  `cmd-<seq>-<id>.json` per request, one `resp-<id>.json` per answer. Writers create `<name>.tmp` and
  rename; readers ignore `*.tmp`. The addon must never open a network endpoint (a test scans for
  the classes).
- The wire protocol (version 1) is a contract for existing adapters and clients: do not change
  message shapes, error codes, verbs or exit statuses without a version bump.

### 2. Parked main loop

- The autoload stub runs at the lowest physics priority and parks the main loop inside its physics
  callback; `step N` lets exactly N ticks run. Exact stepping needs `--fixed-fps` equal to the physics
  rate, declared as `--bridge-fixed-fps`; under it every tick is paired with one process frame, and
  while parked no tick, frame, awaited signal or timer advances.
- An injected press is seen as "just pressed" on the second tick after the park point, so a tap lasts
  two ticks by default. These facts were measured and are pinned by `tests/e2e_test.rs`; do not
  change them without measuring again.

### 3. Development only, impossible to ship by accident

- Inactive unless `--bridge-dir=` is given, never active when `OS.is_debug_build()` is false.
- `export_plugin.gd` keeps `runtime/*`, the plugin scripts and the project adapter out of non-debug
  exports. Godot asks export plugins in the order of their names and the first one to skip or
  replace a file ends the chain, so the plugin's name must sort before the built-in `GDScript`
  plugin (it compiles every script as soon as it is asked, and would hide the scripts from this
  one). The rule that decides what to skip lives in `export_policy.gd`, because an
  `EditorExportPlugin` cannot be created outside the editor and the rule must be testable;
  `tests/export_test.rs` exports a real pack to prove the result. Never weaken either lock.

### 4. Every request gets an answer

- GDScript has no try/catch; a script error before the reply leaves the client waiting for its
  timeout. Read request values untyped and validate them; never assign a wrongly typed value to a
  typed variable; a handler result that is not a dictionary becomes an `internal` error.
- Anything newer than Godot 4.5 is called through `has_method()` + `call()`.

### 5. Reusable core

- Nothing in the addon names a project class or `res://src`. No script of the addon declares a
  `class_name`: a script left out of an export still leaves its class in the exported game's class
  list, and a class could collide with one of the project the addon is installed in.
  Project knowledge enters only through the adapter (`register(registry)`).

## Implementation Standards

### Rust client

- Synchronous, no async runtime. One run: write one request file, poll for the answer, print one JSON
  object on standard output, exit 0, 1 or 2. Diagnostics only on standard error.
- No `unwrap()` / `expect()` in production paths; propagate `Result` with messages that say what to
  do next. Functions that read the environment take a `Context`, so tests never mutate it.
- A client failure prints `{"ok": false, "error": {"code": "client", "message": "..."}}` and exits 2.

### GDScript addon

- Strict typing wherever the language allows. Four-space indentation. English only.
- Zero third-party dependencies; the repository does not use GUT.

## Quality Assurance & Verification Protocols

Before submitting or committing changes, run the complete pipeline:

1. Format and lint:

   ```bash
   cargo fmt --all -- --check
   cargo clippy --all-targets --all-features -- -D warnings
   ```

2. Tests (includes the version sync, changelog, structure and docs guards):

   ```bash
   cargo test --all-targets
   ```

3. GDScript syntax check. The repo root has its own minimal `project.godot`; `--path .` is required:

   ```bash
   godot --headless --path . --import   # first run, or after adding scripts
   godot --headless --path . --check-only --script addons/godot_dev_bridge/plugin.gd
   ```

   Run it for every changed script. New scripts need their `.gd.uid` file committed (the import
   creates it).

4. GDScript unit scripts, end-to-end and installer tests, whenever `addons/` or `src/launch.rs`
   changed (needs Godot 4.5+; set `GODOT_CONSOLE_PATH` as well on Windows):

   ```bash
   GODOT_PATH=/path/to/godot cargo test --test gdscript_test --test e2e_test --test installer_test --test export_test -- --ignored
   ```

The version in `addons/godot_dev_bridge/plugin.cfg` must always equal `Cargo.toml`; bump both
together and add a `CHANGELOG.md` section.

## Using the Bridge as an Agent

1. `godot-dev-bridge start --headless` (or a window when you need `shot`), then `status`: if
   `deterministic` is false, read `reason`.
2. Between commands the game is held. Act with `press` / `tap` / `do`, advance with `step N` or
   `step --until EXPR --max M` (never guess a tick count for work on other threads), observe with
   `state`, `eval`, `get`, `tree` and `shot`.
3. Always `stop` the session when done. Exit status 1 means the game refused the command (read
   `error.code`); status 2 means the client could not reach the game (read `error.message`, it
   contains the end of `game.log` when the game died).

## Completion Protocol (Mandatory Checklist)

Before concluding any development task, every agent MUST verify the following checklist:

- [ ] **Technical Language**: All code, identifiers, comments, and technical docs are in English.
- [ ] **Communication Language**: All conversational responses and planning are in Russian.
