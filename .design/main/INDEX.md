# Workspace Specifications Registry

**Version:** 1.2.0
**Status:** Active

## Overview

Local registry of specifications for the godot-dev-bridge client and addon.

## Domain Specifications

| File | Description | Status | Layer | Version |
| --- | --- | --- | --- | --- |
| [l1-driving-protocol.md](specifications/l1-driving-protocol.md) | The contract between a driver and a running game: sessions, the request/answer exchange, outcome classes and how protocol 1 may evolve. Open it before changing anything a driver or adapter can observe. | Stable | concept | 1.0.1 |
| [l1-controlled-time.md](specifications/l1-controlled-time.md) | What holding the game, stepping it by exact ticks and injecting device-like input mean, including the measured timing facts. Open it before changing stepping or input behaviour. | Stable | concept | 1.0.0 |
| [l1-development-only.md](specifications/l1-development-only.md) | The locks that keep the bridge a development tool: enablement, debug-only, absence from release exports, a private channel, no network. Open it before touching any of those guards. | Stable | concept | 1.0.0 |
| [l1-project-extension.md](specifications/l1-project-extension.md) | The boundary between the generic core and a project's adapter, and what an adapter's returned values mean. Open it before changing the adapter contract. | Stable | concept | 1.0.0 |
| [l1-verification.md](specifications/l1-verification.md) | What the project must prove automatically and the gates between a change and a published release. Open it before changing tests, pipelines or the release rules. | Stable | concept | 1.0.0 |
| [l1-agent-feedback.md](specifications/l1-agent-feedback.md) | How an agent reports what it found missing, broken or misleading in the bridge to the maintainers, as copyable text or as an issue submitted after the human's approval. Open it before changing what is recorded, what a report may contain or how one is published. | Stable | concept | 1.0.0 |
| [l2-wire-format.md](specifications/l2-wire-format.md) | The files of a session directory and their JSON shapes as both the client and the runtime use them. Open it before changing a field, file name or exchange timing. | Stable | implementation | 1.0.0 |
| [l2-game-session.md](specifications/l2-game-session.md) | The GDScript session that answers requests: dispatcher, core commands, observer and lifecycle. Open it before adding or changing a core command. | Stable | implementation | 1.0.0 |
| [l2-rust-client.md](specifications/l2-rust-client.md) | The command-line client: verbs, environment resolution, start, stop and liveness. Open it before adding a verb or argument or changing how a game is launched. | Stable | implementation | 1.0.1 |
| [l2-step-and-input.md](specifications/l2-step-and-input.md) | How the runtime parks the main loop, counts ticks, verifies pairing and injects input. Open it before touching the stub's processing, `step` or an input command. | Stable | implementation | 1.0.0 |
| [l2-access-and-export-locks.md](specifications/l2-access-and-export-locks.md) | The concrete enablement checks, private directories, link-safe clearing, export plugin and editor plugin. Open it before touching any lock. | Stable | implementation | 1.0.0 |
| [l2-adapter-registry.md](specifications/l2-adapter-registry.md) | The registry an adapter registers into and how the session loads it and answers project commands and state. Open it before changing the registry. | Stable | implementation | 1.0.0 |
| [l2-test-suite.md](specifications/l2-test-suite.md) | Which test target proves what, and which need Godot or a display. Open it before adding or changing a test target. | Stable | implementation | 1.0.0 |
| [l2-ci-release.md](specifications/l2-ci-release.md) | The CI, audit and release pipelines and the manual release procedure. Open it before changing a workflow, a release asset or the procedure. | Stable | implementation | 1.0.0 |
| [l2-agent-feedback-client.md](specifications/l2-agent-feedback-client.md) | The client's `feedback` verb: notebook files, evidence recording, scrubber, report, fingerprint, submission through `gh` and the hints on `start` and `stop`. Open it before touching any of them. | Stable | implementation | 1.0.1 |

<!-- Add your specifications here -->
<!-- Description: what the specification is for and when to open it, in two sentences at most; no change history (that goes in the specification's Document History). -->

## Meta Information

- **Maintainer**: Oleg Alexandrov
- **Last Updated**: 2026-10-16
