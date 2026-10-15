# Godot Development Bridge

Drive a running Godot 4 game from a shell: hold it still, advance it by an exact number of physics
ticks, press buttons, read its state and save a screenshot of the window the game actually drew. It
is made for developers and AI agents that need to *look at and play* a game while building it.

- **Exact time control.** Between commands the game is held inside a physics tick: no tick, frame,
  timer or awaited signal runs until the next `step`. An answer is the state after exactly the ticks
  you asked for.
- **Input like a device.** Presses reach the game through the same engine entry point as a keyboard
  or mouse, so game code cannot tell the difference.
- **One native binary, one folder.** The client has no runtime dependencies; the addon is one folder
  that installs itself when you enable the plugin. No server, no port: client and game talk through
  files in a temporary directory.
- **Development only.** Inactive unless enabled at launch, never active in a release build, and left
  out of non-debug exports automatically.

It complements [godot-mcp](https://github.com/zigenzoog/godot-mcp): that one edits the scenes of the
*editor*, this one drives the *running game*.

## Installation

Requirements: **Godot 4.5+**. No Rust toolchain needed.

### Option A: let your AI assistant install it

Open your Godot project in the assistant and paste:

```text
Install godot-dev-bridge (https://github.com/zigenzoog/godot-dev-bridge) for this Godot project. Use the GitHub release, not a local clone of that repository.

1. Detect the OS and CPU architecture (Windows, Linux, or macOS; x86_64 or arm64).
2. Ask me for the full path to my Godot executable. Never guess it: Windows builds are portable, version-named files. Run `<path> --version` to confirm it works.
3. Download the client for this platform and godot-dev-bridge-addon.zip from https://github.com/zigenzoog/godot-dev-bridge/releases/latest, e.g.:
   gh release download --repo zigenzoog/godot-dev-bridge --pattern "godot-dev-bridge-<target-triple>.*"
   gh release download --repo zigenzoog/godot-dev-bridge --pattern "godot-dev-bridge-addon.zip"
   Extract the client to a permanent folder that is on PATH or that I can call by full path: %LOCALAPPDATA%\godot-dev-bridge\ (Windows), ~/.local/bin/ (Linux), ~/Library/Application Support/godot-dev-bridge/ (macOS).
4. Copy addons/godot_dev_bridge from the addon archive into <project>/addons/godot_dev_bridge and add it to project.godot:
   [editor_plugins]
   enabled=PackedStringArray("res://addons/godot_dev_bridge/plugin.cfg")
   Do not edit [autoload]: the plugin registers the DevBridge autoload itself when the editor opens the project.
5. Put GODOT_PATH from step 2 in a .env file in the project root (add .env to .gitignore if it is not ignored), or export it in my shell.
6. Import the project once: <godot> --headless --path <project> --import
7. Open the project in the Godot editor once so the plugin registers itself (or run: <godot> --headless --editor --path <project> --quit-after 600), then verify with: godot-dev-bridge start --headless, godot-dev-bridge ping, godot-dev-bridge stop. Report the results.
```

### Option B: install manually

1. Download the client for your platform and the addon from the
   [latest release](https://github.com/zigenzoog/godot-dev-bridge/releases/latest):

   | Platform | Asset |
   | --- | --- |
   | Windows x86_64 | `godot-dev-bridge-x86_64-pc-windows-msvc.zip` |
   | Linux x86_64 | `godot-dev-bridge-x86_64-unknown-linux-gnu.tar.gz` |
   | macOS Apple Silicon | `godot-dev-bridge-aarch64-apple-darwin.tar.gz` |
   | macOS Intel | `godot-dev-bridge-x86_64-apple-darwin.tar.gz` |
   | Godot addon (any platform) | `godot-dev-bridge-addon.zip` |

2. Put the client on your `PATH` (or call it by full path).
3. Copy `addons/godot_dev_bridge` from the addon archive into your project's `addons/` folder, open
   the project in Godot and enable **Godot Development Bridge** in **Project → Project Settings →
   Plugins**. The plugin registers the `DevBridge` autoload for you.
4. Tell the client where Godot is: `GODOT_PATH` in the environment or in a `.env` file (see
   [.env.example](.env.example); use single quotes around Windows paths), or `--godot` on `start`.
5. Import the project once if it was never opened in the editor:
   `godot --headless --path <project> --import`.

If your version control ignores `addons/` as third-party code, carve this folder out of the rule.

## Quick start

From inside the project (the client finds `project.godot` by itself):

```sh
godot-dev-bridge start                 # a window; --headless for none
godot-dev-bridge status                # is stepping exact ("deterministic")?
godot-dev-bridge step 30               # 30 physics ticks, then the game holds still again
godot-dev-bridge tap --action ui_accept
godot-dev-bridge step 10
godot-dev-bridge shot                  # the window's last frame, as a PNG
godot-dev-bridge stop
```

Every run is a short process that keeps nothing between runs and prints **one JSON object**. Exit
status: `0` success, `1` the game answered with an error (`ok` is false, `error` has a `code` and a
`message`), `2` the client could not start, reach or understand the session.

## Commands

`--session NAME` and `--timeout SECONDS` (default 30, and 90 for `start`) are accepted before or
after the verb.

| Verb | What it does |
| --- | --- |
| `start` | Starts the game detached with the bridge enabled and waits until it is ready. Options: `--project DIR`, `--scene res://...`, `--fps N` (60), `--headless`, `--window WxH` (1152x648), `--fast` (vsync off), `--idle-quit SECONDS`, `--keep-mouse`, `--godot PATH`, and more engine arguments after `--`. Refuses to start over a live session of the same name. |
| `stop` | Asks the game to quit, ends it if it stays. The logs are kept. |
| `sessions` | Lists the sessions on this machine and whether each game is alive. |
| `status`, `ping`, `commands` | Information about the session, a liveness check, and the core and project commands with their help. |
| `step N` | Runs N physics ticks, then holds still. `step --until EXPR [--max M]` runs until the expression holds (checked before every tick, default max 600). |
| `press`, `release`, `tap` | Input by `--action`, `--key` or `--button`; `--ticks` releases after that many ticks (a tap defaults to 2). |
| `release-all` | Releases everything held. |
| `pointer X Y [--relative]` | Moves the pointer in viewport pixels. |
| `shot [PATH] [--width W]` | Saves the window's last drawn frame as a PNG (not in a headless session). |
| `eval EXPR` | Evaluates an expression; `root`, `tree`, `bridge` and every autoload are in scope. |
| `get PATH [PROPS...]`, `tree [PATH] [--depth N] [--props]` | Reads node properties and lists nodes. |
| `state [NAMES...]`, `do NAME [JSON]` | Reads the state and runs the commands your adapter registered. |
| `raw JSON` | Sends a request as written. |

Error codes: `bad_request`, `unknown_command`, `bad_args`, `eval_failed`, `not_found`, `no_renderer`,
`project_error`, `internal`.

## Writing an adapter

Without an adapter the bridge is the generic core. To show and do things specific to your game,
write a script with a `register(registry)` method:

```gdscript
extends RefCounted

func register(registry) -> void:
    registry.add_state("player", _player)
    registry.add_command("heal", _heal, "Restores health: {amount?}.")

func _player() -> Dictionary:
    var player := Engine.get_main_loop().root.find_child("Player", true, false)
    return {} if player == null else {"health": player.health}

func _heal(args: Dictionary) -> Dictionary:
    return {"healed": int(args.get("amount", 1))}
```

Then pick the script in **Project Settings → Godot Dev Bridge → Adapter** (the setting
`godot_dev_bridge/adapter`). A state provider takes no arguments and returns a value; a command
takes the JSON arguments as a dictionary and returns a value. Values are converted to JSON like the
result of `eval`. A provider should answer an empty value, not an error, while what it reads does
not exist (a menu has no player). The names of the core commands are reserved. Read it back with
`godot-dev-bridge state player` and `godot-dev-bridge do heal '{"amount": 3}'`.

## Sessions and launch arguments

A session is a name (`--session`, then `DEV_BRIDGE_SESSION`, then `default`) and a directory
`<temporary directory>/dev_bridge/<name>/` (`DEV_BRIDGE_ROOT` moves the root) that holds
`session.json`, `ready.json`, the request and response files, `closed.json`, `game.log` and
`shots/`. A game that receives no request for 30 minutes ends itself.

`start` passes these after `--` on the Godot command line; you can also use them by hand:

| Argument | Meaning |
| --- | --- |
| `--bridge-dir=<directory>` | The session directory. Without it the bridge stays off. |
| `--bridge-fixed-fps=<n>` | Declares the fixed frame rate the engine was started with (`--fixed-fps <n>`); stepping is `deterministic` only when it equals the physics rate. |
| `--bridge-idle-quit=<seconds>` | Ends the game after this long with no request (default 1800, 0 disables). |
| `--bridge-keep-mouse` | Leaves the pointer as the game sets it. By default a session frees it. |

## Release builds

Nothing to configure. The plugin's export plugin leaves `runtime/`, the plugin scripts and your
adapter out of every non-debug export. Only the inert autoload stub ships, and without `runtime/`
there is nothing for a launch flag to switch on. The stub also checks `OS.is_debug_build()` itself.

## Guarantees and limits

- Development only: off unless enabled at launch, never on in a non-debug build.
- No network: only files in the launching user's temporary folder.
- Input parity: injected input goes through `Input.parse_input_event()`.
- Controlled time: exact stepping needs the fixed frame rate that `start` sets (`status` reports
  `deterministic`). Work on other threads and the file system finishes in real time, so wait for
  it with `step --until`.
- Safe failure: a malformed, unknown or failing command gets an error answer and changes nothing.
- No screenshots without a window. One request at a time, one game per session directory.
- `eval` can run anything the game can: the enablement guards are its whole protection.

## Development

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets

# GDScript checks. The repo root is a minimal Godot project; `--path .` is required.
godot --headless --path . --import      # first run, or after adding scripts
godot --headless --path . --check-only --script addons/godot_dev_bridge/plugin.gd

# GDScript unit scripts, end-to-end and installer tests (need Godot 4.5+).
GODOT_PATH=/path/to/godot cargo test --test gdscript_test --test e2e_test --test installer_test -- --ignored
```

**Releasing:** add a section for the version to [CHANGELOG.md](CHANGELOG.md), bump the version in
`Cargo.toml` and `addons/godot_dev_bridge/plugin.cfg` together (`tests/version_sync_test.rs` fails if
they differ), commit, and push a `vX.Y.Z` tag once CI is green.

## License

MIT. See [LICENSE](LICENSE).
