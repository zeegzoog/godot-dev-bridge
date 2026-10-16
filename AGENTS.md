# Agent Guidelines & Engineering Standards

This document sets the architectural principles, development workflow and operational rules for AI agents and human contributors working on godot-dev-bridge.

## Mission & Scope

A reusable development bridge for any Godot 4.5+ project: a shell (or an agent with a shell) starts the running game, holds it still, steps it by an exact number of physics ticks, injects input, reads state and takes screenshots of the window the game drew. It drives the **running game**; it does not edit scenes in the editor.

## Architecture Guidelines

### 1. Files, not sockets

- Client and game talk through a spool directory (`dev_bridge/<session>/` under a root that only the user can use): one `cmd-<seq>-<id>.json` per request, one `resp-<id>.json` per answer. Writers create `<name>.tmp` and rename; readers ignore `*.tmp`. The addon must never open a network endpoint (a test scans for the classes).
- "Only the user can use it" is enforced, not assumed. On Unix the directories have mode 0700, belong to the user and are not symbolic links. On Windows (`src/winsec.rs`) their access rules are replaced with ones for the user and the system alone, and a link (symbolic link or junction) or a directory owned by another account is refused. The session name is validated, because starting a session clears its directory, and clearing never follows a link (`DirEntry::file_type`, never `Path::is_dir`).
- The wire protocol (version 1) is a contract for existing adapters and clients: do not change or remove message fields, error codes, verbs or exit statuses, and do not change what they mean, without a version bump. Adding an optional field to an answer (as `reason` and `adapter` were added to `ready.json`) is compatible and needs no bump, but it must be documented in the README.

### 2. Parked main loop

- The autoload stub runs at the lowest physics priority and parks the main loop inside its physics callback; `step N` lets exactly N ticks run. Exact stepping needs `--fixed-fps` equal to the physics rate, declared as `--bridge-fixed-fps`; under it every tick is paired with one process frame, and while parked no tick, frame, awaited signal or timer advances.
- An injected press is seen as "just pressed" on the second tick after the park point, so a tap lasts two ticks by default. These facts were measured and are pinned by `tests/e2e_test.rs`; do not change them without measuring again.
- A request that the game has taken keeps running when its client gives up: a `step` finishes. The client says so in its timeout error, and the answer nobody collected is swept later. Never make a client retry a request on its own.

### 3. Development only, impossible to ship by accident

- Inactive unless `--bridge-dir=` is given, never active when `OS.is_debug_build()` is false.
- `export_plugin.gd` keeps `runtime/*`, the plugin scripts and the project adapter out of non-debug exports. Godot asks export plugins in the order of their names and the first one to skip or replace a file ends the chain, so the plugin's name must sort before the built-in `GDScript` plugin (it compiles every script as soon as it is asked, and would hide the scripts from this one). The rule that decides what to skip lives in `export_policy.gd`, because an `EditorExportPlugin` cannot be created outside the editor and the rule must be testable; `tests/export_test.rs` exports a real pack to prove the result. Never weaken either lock.

### 4. Every request gets an answer

- GDScript has no try/catch; a script error before the reply leaves the client waiting for its timeout. Read request values untyped and validate them; never assign a wrongly typed value to a typed variable; a handler result that is not a dictionary becomes an `internal` error.
- Every answer must be valid JSON. Values pass through `observer.to_json` (a non-finite float becomes text, nesting is bounded), and a number that arrives in a request is checked to be finite and in range before it is used.
- A project handler that fails inside its own script cannot be told from one that returns nothing: it answers `null`, or the default of its declared return type (`{}` for `-> Dictionary`), and the engine logs the error. Whether the adapter loaded is reported in `ready.json` and `status` (`adapter`).
- A project handler refuses its arguments by returning `registry.refuse(message)`, a dictionary whose only key is `__bad_args__` (`registry.BAD_ARGS_KEY`); the answer is `bad_args`. Adapters call `refuse` and do not write the key. Every other value, a dictionary with an `error` entry included, is a result: that is what such a value meant in protocol 1 from the start.
- A session that ends says why in `closed.json` (`quit`, `idle`, or `game` when the game ended on its own: the stub's `_exit_tree` tells the session). The first reason written stands.
- Anything newer than Godot 4.5 is called through `has_method()` + `call()`.

### 5. Reusable core

- Nothing in the addon names a project class or `res://src`. No script of the addon declares a `class_name`: a script left out of an export still leaves its class in the exported game's class list, and a class could collide with one of the project the addon is installed in. Project knowledge enters only through the adapter (`register(registry)`).

## Implementation Standards

### Rust client

- Synchronous, no async runtime. One run: write one request file, poll for the answer, print one JSON object on standard output, exit 0, 1 or 2. Diagnostics only on standard error. `--help` and `--version` print their text and no object.
- No `unwrap()` / `expect()` in production paths; propagate `Result` with messages that say what to do next. Bound untrusted numbers before a call that panics on them (`Duration::from_secs_f64`, `Instant + Duration`).
- The environment is read in one place, `Context::from_env`; every other function that needs it takes a `Context`, so tests never mutate the environment.
- A client failure, a usage error included, prints `{"ok": false, "error": {"code": "client", "message": "..."}}` and exits 2.
- A new verb or argument gets help text and a README row; `tests/docs_test.rs` fails without them.
- `start` runs the executable it was given directly, so `pid` is the engine's; killing a game still ends its whole process tree, in case `--godot` names a wrapper.

### GDScript addon

- Strict typing wherever the language allows. Four-space indentation. English only.
- Zero third-party dependencies; the repository does not use GUT.
- A new script needs its `.gd.uid` file committed (the import creates it).

### Language and documentation

- Code, identifiers, comments, documentation, commit messages, issue titles, error messages and logs are English only.
- Documentation, comments, tests and release notes are neutral and self-contained: they do not name other projects or tools.
- Markdown: no horizontal rules (`---`) except in a footer where unavoidable. Personal working notes (`scratch/` and the other paths `.gitignore` lists) are git-ignored and are not part of the repository; nothing in the repository may depend on them.

## Specification-Driven Development

The project is developed specification-first. The SDD layer is scaffolding: it is erected around the product and never carries its weight.

- **Engine** (`.magic/` and the workflow wrappers under `.agents/`): installed on each machine from the engine's release archive and never committed (`.gitignore` excludes it). Its rules and workflows take precedence over this file wherever the two disagree on how work is specified, planned, tracked or recorded; this file stays authoritative for the product itself (architecture, protocol 1, invariants, quality gates).
- **Workspace** (`.design/`): the specifications, registry, plans, tasks, state and generated wiki. It is committed and pushed so that the whole team works from one source; only what the engine regenerates on demand (caches, graph renderings and the wiki) is git-ignored. Change it through the engine's workflows, and commit it together with the code it describes.
- **Scaffold-removal test**: with `.design/`, `.magic/` and `.agents/` deleted, the product must still build, test, run and package. No build step, CI job, test, export or release artifact may read them, and product files (code, comments, README, CHANGELOG, release notes) carry no SDD references: no `.design/` paths, task or phase identifiers, or specification file names.
- The engine installs a git pre-commit hook that checks its own integrity; it needs Node.js and lets the commit through when `.magic/` is absent.
- Releases stay outside the engine: they follow `.agents/skills/release/SKILL.md`, and tagging and pushing still need an explicit go-ahead, because no engine workflow tags or pushes.

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

3. GDScript syntax check. The repo root has its own minimal `project.godot`; `--path .` is required. Run it for every changed script, or for all of them as CI does:

   ```bash
   godot --headless --path . --import   # first run, or after adding scripts
   for script in $(find addons tests -name '*.gd' | sort); do godot --headless --path . --check-only --script "$script" || exit 1; done
   ```

4. GDScript unit scripts, end-to-end, installer and export tests, whenever `addons/`, `tests/fixtures/` or anything the client does with a session (`src/launch.rs`, `src/session.rs`, `src/spool.rs`) changed (needs Godot 4.5+):

   ```bash
   GODOT_PATH=/path/to/godot cargo test --test gdscript_test --test e2e_test --test installer_test --test export_test --test load_test -- --ignored
   ```

5. The window screenshot needs a display, so it is not part of step 4. Run it on a machine with one whenever `shot`, the observer or the fixture changed (on a headless Linux machine, under `xvfb-run`, as CI does):

   ```bash
   GODOT_PATH=/path/to/godot cargo test --test window_test -- --ignored
   ```

CI runs steps 1 to 4 (Rust on Linux, Windows and macOS; the Godot checks on Linux and Windows) and step 5 on Linux under a virtual display. A separate workflow audits the locked dependencies against the RustSec advisories when they change and weekly (`cargo audit`); run it locally after changing `Cargo.toml` or `Cargo.lock`.

The version in `addons/godot_dev_bridge/plugin.cfg` must always equal `Cargo.toml`; bump both together and add a `CHANGELOG.md` section. Releases follow `.agents/skills/release/SKILL.md`; tagging and pushing need an explicit go-ahead.

## Using the Bridge as an Agent

1. `godot-dev-bridge start --headless` (or a window when you need `shot`), then `status`: if `deterministic` is false, read `reason`; if `adapter.loaded` is false, read `adapter.error`.
2. Between commands the game is held. Act with `press` / `tap` / `do`, advance with `step N` or `step --until EXPR --max M` (never guess a tick count for work on other threads: ticks are not paced by the clock, so give such work real time between short `--until` steps), observe with `state`, `eval`, `get`, `tree` and `shot`.
3. Long work: raise `--timeout` for a long step. If a step times out (exit status 2, "the game took the request"), it is still running; look at `status` (`physics_frames`) before sending anything again.
4. Node paths may start with `/` in Git Bash; the client repairs what the shell rewrites.
5. `sessions` lists every session, whether its game is alive and why an ended one closed (`closed`); `sessions --prune` removes the ended ones. Always `stop` the session when done.
6. When the bridge itself gets in your way (a missing verb, a misleading message or help text, a defect), record it with `feedback note` as you go. At the end, `feedback draft`, show the user the whole report and offer to copy it, open its link, submit it or discard it. Never run `feedback submit` without the user's explicit yes. Exit status 1 means the game refused the command (read `error.code`); status 2 means the client could not reach the game or the command line was wrong (read `error.message`, it contains the end of `game.log` when the game died).

## Completion Protocol (Mandatory Checklist)

Before concluding any task, the agent MUST work through this list and report any item it could not tick, with the reason. Items about a part of the project the task did not touch are marked not applicable, never skipped silently.

### Language and style

- [ ] All code, identifiers, comments, docstrings, documentation, commit messages and error messages are in English.

### Contract and invariants

- [ ] Protocol 1 is unchanged: no field, error code, verb or exit status was changed, removed or given a new meaning; an optional field that was added is documented in the README.
- [ ] The development-only locks are intact: inactive without `--bridge-dir=`, never active in a non-debug build, the export plugin's name still sorts before `GDScript`, and `runtime/*`, the plugin scripts and the adapter are still left out of non-debug exports.
- [ ] The addon opens no network endpoint, declares no `class_name`, and names no project class or path.
- [ ] Every request path still ends in an answer, every answer is valid JSON, and request values are validated before use.
- [ ] The spool stays private and link-safe on every platform, and nothing that deletes files follows a link.
- [ ] The measured stepping and input timing was not changed, or it was measured again and `tests/e2e_test.rs` updated.

### Code

- [ ] Rust: no `unwrap()` / `expect()` or panicking conversion in production paths; the environment is read only through `Context`.
- [ ] GDScript: strictly typed, four-space indentation, a `.gd.uid` file committed for every new script.
- [ ] `cargo fmt --all -- --check` and `cargo clippy --all-targets --all-features -- -D warnings` are clean.
- [ ] `cargo test --all-targets` passes.
- [ ] `--check-only` passes for every changed GDScript file.
- [ ] When `addons/`, the fixtures or the session code changed, the Godot-backed tests of step 4 pass; when `shot`, the observer or the fixture changed, the window test of step 5 passed on a machine with a display.
- [ ] When `Cargo.toml` or `Cargo.lock` changed, `cargo audit` reports nothing.
- [ ] A new behaviour has a test that failed before the change.

### Documentation and release

- [ ] The README matches the product: the commands table, error codes, adapter contract, guarantees and limits, development commands.
- [ ] Every new verb or argument has help text.
- [ ] This file is updated when a rule, a command or a standard changed.
- [ ] `Cargo.toml` and `addons/godot_dev_bridge/plugin.cfg` carry the same version, `Cargo.lock` is current, and `CHANGELOG.md` has a section for it.
- [ ] A release, tag or push was made only after an explicit go-ahead, and only through the release skill.

### Hygiene and reporting

- [ ] `git status` shows only the intended files: no `.env`, build output, scratch or generated files (the `.design/` workspace and its wiki are intended and committed; the engine is not).
- [ ] Product files carry no SDD references, and nothing outside `.design/` depends on the SDD layer (scaffold-removal test).
- [ ] No game was left running: every session started during the task was stopped (`godot-dev-bridge sessions`).
- [ ] Failed or skipped checks are reported with their output, and what was verified is stated plainly.
