# Changelog

All notable changes to godot-dev-bridge. The client and the Godot addon always ship with the same
version number. The section for a version is used as the text of its GitHub Release, so every
release needs one (the release workflow fails without it).

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
  a per-user location, no symbolic links, no other owner), so a session name cannot point outside
  the spool root and another local user cannot drive a game.
- `start` and `stop` recognise the recorded game by its creation time (Windows, Linux), so a process
  that was handed the pid of a game that died is never mistaken for it or killed.

[0.1.0]: https://github.com/zigenzoog/godot-dev-bridge/releases/tag/v0.1.0
