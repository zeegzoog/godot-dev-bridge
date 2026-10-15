---
name: release
description: Cut a new godot-dev-bridge release - bump the version, run QA gates, tag, and push to trigger the cross-platform GitHub Actions release build. Use when the user asks to "release", "cut a release", "publish a new version", or "ship vX.Y.Z" of godot-dev-bridge.
---

# Release godot-dev-bridge

## Overview

Pushing a tag matching `v*.*.*` triggers `.github/workflows/release.yml`, which verifies the tag, builds `godot-dev-bridge` for Windows x86_64, Linux x86_64, macOS x86_64, and macOS aarch64, packages the addon, and then one `publish` job creates the GitHub Release with every archive attached. This skill walks through the steps in order — skipping the QA gate or getting the tag format wrong produces a broken or missing release.

## Procedure

1. **Determine the new version.** Read the current version from `Cargo.toml` (`[package] version`). If the user didn't specify a target version, ask them, or propose a semver bump (patch/minor/major) based on what changed since the last tag (`git log <last-tag>..HEAD --oneline`). Don't guess silently.

2. **Preflight — all must pass before continuing.** This is the pipeline of the "Quality Assurance & Verification Protocols" section of `AGENTS.md`; run it as written there, in this order:

   ```bash
   git status --porcelain          # must be empty; stop and ask if it isn't
   cargo fmt --all -- --check
   cargo clippy --all-targets --all-features -- -D warnings
   cargo test --all-targets
   ```

   A Godot 4.5+ binary is needed for the rest (`GODOT_PATH`, in `.env` or the environment). If none is reachable, stop and say so: a release must not go out without the Godot checks. The repo root is itself a minimal Godot project (`--path .` is required, and `--import` must run once for a fresh `.godot/` cache):

   ```bash
   godot --headless --path . --import
   for script in $(find addons tests -name '*.gd' | sort); do godot --headless --path . --check-only --script "$script" || exit 1; done
   GODOT_PATH=/path/to/godot cargo test --test gdscript_test --test e2e_test --test installer_test --test export_test -- --ignored
   ```

   Stop and report if any check fails. Do not tag a red build.

3. **Bump the version** in `Cargo.toml` (`[package] version = "X.Y.Z"`), then run `cargo build` once so `Cargo.lock`'s matching entry updates. Set the same version in `addons/godot_dev_bridge/plugin.cfg` (`version="X.Y.Z"`): the client and the addon always ship together and `tests/version_sync_test.rs` fails if they differ. Add a `## [X.Y.Z] - YYYY-MM-DD` section to `CHANGELOG.md` (it becomes the release text; the release workflow fails without it).

4. **Commit the bump:**

   ```bash
   git add Cargo.toml Cargo.lock addons/godot_dev_bridge/plugin.cfg CHANGELOG.md
   git commit -m "chore(release): bump version to vX.Y.Z"
   ```

5. **Stop and confirm with the user before step 6.** Tagging and pushing is the point of no return: it publishes a public GitHub Release and can't be casually undone. Show them the version and the commit, and get an explicit go-ahead — this is not a step to run automatically even inside this skill.

6. **Tag and push** (only after confirmation):

   ```bash
   git tag -a vX.Y.Z -m "vX.Y.Z"
   git push origin <branch>
   git push origin vX.Y.Z
   ```

7. **Watch the build:**

   ```bash
   gh run watch $(gh run list --workflow=release.yml -L 1 --json databaseId -q '.[0].databaseId')
   ```

   All 7 jobs (`verify`, `addon`, the 4 binary matrix targets `x86_64-pc-windows-msvc`, `x86_64-unknown-linux-gnu`, `x86_64-apple-darwin`, `aarch64-apple-darwin`, and `publish`, which creates the release) must succeed. If `publish` fails after the builds passed, re-run that job only.

8. **Verify the release:**

   ```bash
   gh release view vX.Y.Z
   ```

   Confirm all 5 assets are attached: 4 binary archives (`godot-dev-bridge-<target>.{zip,tar.gz}`) plus `godot-dev-bridge-addon.zip` (the Godot Editor plugin, platform-independent) — see the asset table in `README.md`.

## Notes

- Tag format must be exactly `v*.*.*` (e.g. `v0.2.0`) — the workflow only triggers on that glob.
- If the build fails after the tag is already pushed, prefer fixing forward with a new patch tag over deleting and re-pushing the same tag.
- This procedure never force-pushes, deletes tags, or deletes releases. If the user wants any of that, treat it as a separate, explicitly-confirmed action, not part of a normal release.
