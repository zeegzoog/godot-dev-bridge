# Changelog

All notable changes to godot-dev-bridge. The client and the Godot addon always ship with the same
version number. The section for a version is used as the text of its GitHub Release, so every
release needs one (the release workflow fails without it).

## [Unreleased]

### Added

- `feedback`: an agent records what it finds missing, broken or misleading in the bridge itself
  (`feedback note`), turns the notes into one scrubbed report per problem for the user to review
  (`feedback draft`, with a prefilled new-issue link), and, only after the user's yes, files it as a
  GitHub issue through the user's own `gh` (`feedback submit --confirmed`), which reports an existing
  issue with the same fingerprint instead of filing it again. `start` and `stop` answers point to it.
  `DEV_BRIDGE_FEEDBACK` turns it down to drafting only, or off; `DEV_BRIDGE_GH` names the `gh` to use.

- `registry.refuse(message)`: what a project command returns to refuse its arguments. It returns
  the dictionary whose only key is `__bad_args__`, so commands that write that dictionary by hand
  keep working; calling `refuse` means a misspelled key cannot turn a refusal into an ordinary result.

## [0.2.0] - 2026-10-15

### Added

- `closed.json` says `game` when the game ended on its own (its code quit, or its window was
  closed), not only when a driver or the idle time ended it. A client that finds the game gone gives
  the reason in its error, and `sessions` shows it as `closed`.
- `sessions --prune` removes the directories of the sessions that have ended, and the leftovers of
  starts that failed before the game ran; it never touches a link or anything that is not a
  session's.
- A project command can refuse its arguments: a dictionary whose only key is `__bad_args__` is
  answered as a `bad_args` error (exit status 1), with the value as the message.

### Changed

- The README says that ticks are not paced by the clock (a headless step runs thousands of them in
  a fraction of a second) and how to give work on other threads the real time it needs.

## [0.1.0] - 2026-10-15

First release: a standalone development bridge that any Godot 4.5+ project can use.

### Added

- The addon `addons/godot_dev_bridge`: an autoload stub that is inert unless a launch argument
  enables it in a debug build, a runtime that parks the main loop inside a physics tick so that the
  game can be stepped by an exact number of ticks, device-equivalent input injection, observation
  (`eval`, `get`, `tree`, `state`, `shot`) and the adapter contract `register(registry)`.
- The plugin registers the `DevBridge` autoload and the `godot_dev_bridge/adapter` setting itself,
  so installing is "copy the folder, enable the plugin".
- An export plugin that keeps the runtime, the plugin scripts and the project adapter out of every
  non-debug export, so a release build cannot contain the bridge.
- The native client `godot-dev-bridge` (Windows, Linux, macOS Intel and Apple Silicon): one command
  per run, one JSON object on standard output, exit status 0, 1 or 2.
- `start` finds the project from the working directory, accepts `--scene`, and refuses a project
  that was never imported, naming the command to run.
- Protocol version 1: the request and response files, the error codes, the verbs and the exit
  statuses are the contract.
- Session names are validated and the spool directory is private to the user (on Unix: mode 0700,
  a per-user location, no symbolic links, no other owner; on Windows: access rules for the user and
  the system alone, no symbolic links or junctions, no other owner), so a session name cannot point
  outside the spool root and another local user cannot drive a game. Starting a session clears its
  directory without ever following a link out of it.
- `start` and `stop` recognise the recorded game by its creation time (Windows, Linux), so a process
  that was handed the pid of a game that died is never mistaken for it or killed.
- `start` uses the physics tick rate the project sets as its frame rate unless `--fps` is given, so a
  project that does not run at 60 ticks per second is still stepped exactly. The executable is run as
  it is on every platform (no console wrapper is needed for `game.log`), and a `shot` to a path that
  cannot be written answers `bad_args`.
- A request file that the game cannot read yet (Windows may hold a fresh file for a moment when many
  clients write at once) is looked at again instead of being answered as malformed.
- `ready.json` and `status` say whether the project adapter was found and registered (`adapter`),
  and why not.
- A usage mistake is answered with the client error object as well as clap's explanation, so every
  failure of the client is one JSON object and status 2. `step --max` needs `--until`, and every
  argument has help text.
- A request that times out says when the game had already taken it (a `step` keeps running), answers
  nobody collected are swept, an answer that is not JSON is reported as such, and a `start` with a
  mistake in its arguments leaves the previous session's log alone.
- `step --until` and `eval` no longer list every project setting on each evaluation (about 50 times
  faster per tick), non-finite pointer positions are refused instead of answered as invalid JSON,
  deeply nested or self-referencing values are cut off, a key has one name however it is spelled,
  and an adapter that extends `Node` is freed when the session ends.

[0.2.0]: https://github.com/zigenzoog/godot-dev-bridge/releases/tag/v0.2.0
[0.1.0]: https://github.com/zigenzoog/godot-dev-bridge/releases/tag/v0.1.0
