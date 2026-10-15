# Changelog

All notable changes to godot-dev-bridge. The client and the Godot addon always ship with the same
version number. The section for a version is used as the text of its GitHub Release, so every
release needs one (the release workflow fails without it).

## [0.1.0] - 2026-10-15

First release: the successor of the `dev_bridge` addon of the treaker project, rebuilt as a
standalone product that any Godot 4.5+ project can use.

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
- `start` finds the project from the working directory, accepts `--scene`, and explains a project
  that was never imported.
- Protocol version 1, unchanged from `dev_bridge`: existing adapters work as they are.

[0.1.0]: https://github.com/zigenzoog/godot-dev-bridge/releases/tag/v0.1.0
