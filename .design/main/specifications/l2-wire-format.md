# Wire Format (Protocol 1)

**Version:** 1.0.0
**Status:** Stable
**Layer:** implementation
**Implements:** l1-driving-protocol.md

## Overview

The concrete files of a session directory and the JSON shapes in them, as the Rust client and the GDScript runtime both read and write them. Open it before touching a field, a file name or a timing constant of the exchange.

## Related Specifications

- [l1-driving-protocol.md](l1-driving-protocol.md) - The contract this format realizes.
- [l2-game-session.md](l2-game-session.md) - Who produces answers, readiness and closing records.
- [l2-rust-client.md](l2-rust-client.md) - Who produces requests and session records.

## 1. Motivation

Two implementations in two languages must agree byte for byte on names and shapes; this is the single place both are described.

## 2. Constraints & Assumptions

- JSON text, UTF-8. The game serializes with the engine's JSON stringifier, the client with serde.
- Rename over an existing file is atomic on every supported platform (the game removes the target first, then renames).

## 4. Invariant Compliance

| L1 Invariant | Implementation |
| --- | --- |
| P1 Files, not sockets | Only the files of §5.1; no transport class opens an endpoint (proved by a structure scan). |
| P2 Finished or absent | Both writers write `<name>.tmp` then rename; the game lists only `cmd-*.json`, the client opens only the exact `resp-<id>.json`. |
| P3 Every request gets an answer | A request file that cannot be read yet is retried (game: up to 500 looks, 2 ms apart) and then answered `bad_request`; see l2-game-session for dispatch. |
| P4 Valid JSON | Answers pass through the runtime's JSON conversion (non-finite floats as text, depth bounded). |
| P5 Failing request changes nothing | Shapes are validated before dispatch (`cmd` a non-empty string, `args` an object). |
| P6 / P7 Outcome classes and codes | `ok` and `error.code` of §5.3; the client maps `ok` to exit 0/1, its own failures to 2 with code `client`. |
| P8 Taken request runs | On timeout the client removes the request only if still present, and reports whether the game took it; answers older than 60 s are swept before every new request. |
| P9 Readiness and closing | `ready.json` and `closed.json` of §5.4. |
| P10 Versioned | `ready.json.protocol` is `1`; the client refuses any other value. |
| P11 Order | The sequence number is microseconds since the epoch; ties are broken by file name. |

## 5. Detailed Design

### 5.1 Session Directory

```plaintext
<spool root>/
└── <session name>/
    ├── session.json        client: the game it started
    ├── ready.json          game: listening
    ├── cmd-<seq>-<id>.json client: one request
    ├── resp-<id>.json      game: one answer
    ├── closed.json         game: why it ended
    ├── game.log            engine output (appended)
    └── shots/              default screenshot target
```

`<id>` is 12 lowercase hexadecimal digits from the client's clock, process id and a counter. A request may carry its own `id`; the game uses it to name the answer only when it is a non-empty valid file name, else the id from the file name.

### 5.2 Request `[REFERENCE]`

```json
{"id": "0123456789ab", "cmd": "step", "args": {"ticks": 30}}
```

### 5.3 Answer `[REFERENCE]`

```json
{"id": "0123456789ab", "ok": true, "result": {"ticks_run": 30, "physics_frames": 31, "process_frames": 31},
 "error": null, "physics_frames": 31, "process_frames": 31}
```

On refusal `ok` is false, `result` null, `error` is `{"code": "<code>", "message": "<text>"}`.

### 5.4 Session Files `[REFERENCE]`

| File | Fields |
| --- | --- |
| `session.json` | `pid`, `command` (the full argument list), `started` (seconds since the epoch, float) |
| `ready.json` | `protocol`, `pid` (the engine's), `project`, `godot_version`, `debug_build`, `headless`, `deterministic`, `reason` (only when not deterministic), `physics_ticks_per_second`, `window_size` `[w, h]`, `adapter` (`configured`, and when configured `path`, `loaded`, `error`) |
| `closed.json` | `reason`: `quit`, `idle` or `game` |

Optional fields added to `ready.json` after protocol 1 (`reason`, `adapter`) are compatible additions and are documented in the README.

### 5.5 Timing Constants

| Constant | Value | Side |
| --- | --- | --- |
| Poll interval | 10 ms client, 2 ms game | both |
| Unreadable answer retries | 100 reads, 5 parse attempts | client |
| Unreadable request looks | 500 | game |
| Stale answer age | 60 s | client |

## 6. Implementation Notes

The game reads requests with a `JSON` instance, not the static parse helper, so a malformed request does not log an engine error.

## Canonical References

| Alias | Path | Purpose |
| --- | --- | --- |
| `[CLIENT-SPOOL]` | `src/spool.rs` | Atomic writes, request ids, send and wait, sweeping. |
| `[GAME-SPOOL]` | `addons/godot_dev_bridge/runtime/spool_transport.gd` | Listing, reading and answering requests. |
| `[RECORD]` | `src/session.rs` | `session.json` and `closed.json` reading. |
| `[READY]` | `addons/godot_dev_bridge/runtime/session.gd` | `ready.json` and answer shapes. |
| `[TESTS]` | `tests/spool_test.rs` | Client-side exchange tests. |

## Document History

| Version | Date | Author | Description |
| --- | --- | --- | --- |
| 1.0.0 | 2026-10-16 | Agent | Initial specification reverse-engineered from the existing code. |
