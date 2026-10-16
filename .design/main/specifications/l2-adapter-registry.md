# Adapter Registry

**Version:** 1.0.0
**Status:** Stable
**Layer:** implementation
**Implements:** l1-project-extension.md

## Overview

The GDScript registry a project adapter registers into, how the session loads the adapter, and how project commands and state are answered. Open it before changing the adapter contract or the registry's outcome rules.

## Related Specifications

- [l1-project-extension.md](l1-project-extension.md) - The invariants.
- [l2-game-session.md](l2-game-session.md) - Dispatch, `state`, `commands` and `status`.
- [l2-access-and-export-locks.md](l2-access-and-export-locks.md) - The adapter is left out of release exports.

## 1. Motivation

The adapter contract is public and used by projects that cannot be changed in lockstep; it must be unambiguous and stable under protocol 1.

## 2. Constraints & Assumptions

- The adapter extends `RefCounted` (a `Node` adapter outside the tree is freed on close) and declares no `class_name`.
- The adapter keeps the registry handed to `register(registry)` to call `refuse`.

## 4. Invariant Compliance

| L1 Invariant | Implementation |
| --- | --- |
| E1 | The registry, like every addon script, names no project class or path (structure scan). |
| E2 | The session loads the script named by `godot_dev_bridge/adapter` with `load()` after `ResourceLoader.exists`, instantiates it and calls `register(registry)` if present. |
| E3 | `add_state(name, provider)` requires a valid callable with 0 arguments; `add_command(name, handler, help)` a valid callable with exactly 1; otherwise `push_warning` and `false`. |
| E4 | `registry.reserved` is set to the core command names; `add_command` refuses them. |
| E5 | `refuse(message)` returns `{"__bad_args__": str(message)}` (`BAD_ARGS_KEY`); `run_command` maps a dictionary whose only key is that one to `{bad_args}`, everything else to `{value}`. |
| E6 | `_adapter_state`: `{configured: false}`, or `{configured: true, path, loaded, error?}` with `error` "the script cannot be loaded (see the game's log)" or "the script has no register(registry) method"; reported in `ready.json` and `status`. What `register` added before a script error stays registered. |
| E7 | `read_state(names)` answers `{error}` per missing or invalid provider; others are unaffected. |
| E8 | The session passes command values and the state map through `observer.to_json`. |
| E9 | Documented in the README for adapter authors; not enforceable by the core. |

## 5. Detailed Design

### 5.1 Answers

The mapping from what a command returns to the answer is fixed in [l1-project-extension.md §4.2](l1-project-extension.md): the registry's `{value}`, `{bad_args}` and `{error}` become a result, `bad_args` and `project_error`. `state` with `names` not a list is `bad_args`; with no names it reads every provider.

### 5.2 Adapter Shape (pseudo-code)

```plaintext
adapter extends RefCounted
    register(registry):
        keep registry
        registry.add_state("player", provider with no arguments)
        registry.add_command("heal", handler(args), "Restores health: {amount?}.")
    handler(args):
        if args.amount is present and not a number: return registry.refuse("amount must be a number")
        return {healed: amount}
```

The user-facing example lives in the README; the fixture adapter is a working one.

## Canonical References

| Alias | Path | Purpose |
| --- | --- | --- |
| `[REGISTRY]` | `addons/godot_dev_bridge/runtime/registry.gd` | Registration and outcome rules. |
| `[LOADER]` | `addons/godot_dev_bridge/runtime/session.gd` | `_load_adapter`, `_run`, `_state`. |
| `[TESTS]` | `tests/gd/registry_test.gd` | Unit tests. |
| `[FIXTURE]` | `tests/fixtures/adapter.gd` | End-to-end adapter. |

## Document History

| Version | Date | Author | Description |
| --- | --- | --- | --- |
| 1.0.0 | 2026-10-16 | Agent | Initial specification reverse-engineered from the existing code. |
