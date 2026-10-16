# Project State

<!-- STATE.md — live project memory. Read FIRST in every workflow session. -->
<!-- Maximum 100 lines. Agent updates AFTER each completed action. -->

**Workspace:** {workspace-name}
**Updated:** 2026-10-16 07:01
**Phase:** {N} — {Phase Name}
**Status:** {Active | Blocked}

## Current Position

- **Task:** [{T-ID}] {Task Title}
- **Spec:** {spec-file.md} §{section}
- **Next Action:** Run /magic.task main to update the plan

## Progress

```
Phase {N}: [{filled}/{total}] ████░░░░ {pct}%
Overall:   [{done}/{all}]     ██░░░░░░ {pct}%
```

## Recent Decisions

<!-- Last 3-5 locked decisions. Older entries are dropped (not archived) — see PLAN.md / CHANGELOG.md for phase history. -->

- {YYYY-MM-DD} **Decision:** {What was decided and why}
- {YYYY-MM-DD} **Pattern:** {Established pattern name} — {brief description}

## Blockers

<!-- Empty if none. Format: [severity] description -->

## Blocking Constraints

<!-- Anti-patterns discovered through real failures. MANDATORY reading. -->
<!-- Agent MUST explicitly acknowledge each constraint before working. -->
<!-- Format: - [C-NNN] **Title**: what must not be done and why -->

## Session Continuity

**Bootstrap Mode:** {true | false}
