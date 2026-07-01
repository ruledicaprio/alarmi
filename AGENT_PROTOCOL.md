# AGENT_PROTOCOL.md — Dual-Loop Agent Model

Extends `CLAUDE.md`. Does not repeat its rules — where they conflict, `CLAUDE.md` wins.
Applies to every session working on `alarmi-repo`.

## 0. Model

Two coupled feedback loops run on every task. Both are always active; most turns
only touch Loop 1, but Loop 2 fires whenever a wrong assumption gets corrected or
a new environmental constraint is discovered.

```
        GOALS ──> DECISIONS ──> STATE OF SYSTEM ──> (delay) ──> perceived STATE ──> GOALS
          ^                                                                          |
          └──────────────────────── Loop 2 (agent context) ────────────────────────┘
```

| | Loop 1 — Product | Loop 2 — Meta (Agent) |
|---|---|---|
| Governs | `alarmi-repo` state | `CLAUDE.md` / `CONTEXT_*.md` state |
| Goal | working bht-api / bht-poller / neteco-poller / SPA | fewer repeated mistakes |
| Decision | code, SQL, config | edits to context files |
| Delay | build → tarball → transfer → `rocky_deploy.sh` | time between mistake and correction |
| Side effect | Modbus timeout, hypertable partition miss, poll overlap | context bloat, stale rule, lost constraint |

## 1. Loop 1 — Product Loop

Before writing code:

1. **State the target reality** — which binary/service, which config file, Rocky 9 or WSL/Docker build side.
2. **Mitigate the delay** — the air-gap round-trip is expensive (no scp, manual curl, no `tar` on Rocky). Don't ship something untested locally that only fails after the transfer.
3. **Name the side effect** — if touching `bht-poller`, say what it does to the Modbus thread pool / poll cycle. If touching SQL, say what it does to hypertable partitioning. If touching `bht-api` routes, say what it does to the SPA build assumptions.

This is just the existing "Think Before Coding" + "Goal-Driven Execution" principles in `CLAUDE.md`, restated as a checklist so it's not skipped under time pressure.

## 2. Loop 2 — Meta Loop (context maintenance)

Trigger: a deploy succeeds after a nontrivial fix, or a wrong assumption gets corrected.

On trigger:
1. Stop generating code.
2. State the one fact that would have prevented the mistake (e.g. "Rocky 9 has no `tar`, use `python3 tarfile`" — already captured; new ones look like this).
3. Propose it as a one-line addition to `CLAUDE.md` or the relevant `CONTEXT_*.md`. Do not write it in automatically — surface it, let it get approved.

This only fires on genuine new environmental/architectural facts. Not on routine work — most turns produce nothing for Loop 2.

## 3. Resource governance (context window / tokens / attention)

The project has large flat files at repo root (`events.tsv` 43MB, `NE_sites.json` 3.5MB, `NETECO_*_clean.txt` 1–1.1MB, `master_alarms (2).log` 7.6MB). Treat these as **never read in full**:

- Grep/target a line range or key first. Never `Read` these whole.
- If a task needs to scan one of these, do it in the shell (`grep`, `awk`, `python3` streaming) and bring back only the extracted result, not the raw file.

General discipline:

- **Prefer Edit over Write** for existing files — a diff costs less context than a full-file rewrite and is easier for Rusmir to review.
- **Don't re-read a file just edited** — the harness already confirms the edit succeeded.
- **Don't paste large tool output back into chat** — summarize the result, keep raw logs in the shell/file.
- **Multi-step build/deploy chains** (build → pack → transfer → `rocky_deploy.sh` → health check) span many tool calls. Track them with the task list, not by holding running state in prose — if the session gets long, the task list is the source of truth, not scrollback.
- **Sliding attention**: on a full-stack change (Rust crate + SQL + TS), don't hold all three in working context at once if they're independent. Finish and verify one layer, note the result, move to the next — re-derive from files/task list rather than trusting memory of what was decided 40 tool calls ago.
- If a task is clearly going to exceed a reasonable single-session budget (e.g. a multi-crate refactor), say so up front and propose splitting it, rather than degrading silently mid-task.

## 4. Double-Commit output rule

On completing a milestone, output two blocks:

1. **System Output** — the actual Rust/TS/SQL/bash.
2. **Agent Output** — *only if Loop 2 fired* — the one-line context-file addition from §2. Omit this block entirely on routine turns; don't manufacture one.

## 5. Isolated-inefficiency proposals

Per standing instruction: no unrequested feature work, refactors, or deletions — except a change that is an obvious, meaningful global upgrade.

For itchy isolated inefficiencies (not full features): don't raise them the moment they're noticed. Hold across the current task; if the same inefficiency would recur or block a later step, raise it once, as a single proposal line, after the requested work is done — not as a mid-task detour. If it turns out to be a one-off, drop it without mentioning it.

## 6. Non-goals

Everything under `CLAUDE.md` → "What Claude Should NOT Do" and "Key Constraints" applies unchanged: no Docker/Podman on Rocky, no dynamic linking, no schema/systemd changes without explicit ask, no `Cargo.lock` edits, no touching `_build_pack_*.sh`.
