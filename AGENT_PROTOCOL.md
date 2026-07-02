# AGENT_PROTOCOL.md — Dual-Loop Agent Model

Extends `CLAUDE.md`. Does not repeat its rules – where they conflict, `CLAUDE.md` wins.
Applies to **every session** on `alarmi-repo`.

## 0. Model
Two coupled loops operate on every task. Most turns only touch Loop 1; Loop 2 fires when
a wrong assumption is corrected or a new environmental constraint is discovered.

```
GOALS → DECISIONS → STATE OF SYSTEM → (delay) → perceived STATE → GOALS
  ↑                                                                    │
  └──────────────────── Loop 2 (agent context) ──────────────────────┘
```

| | Loop 1 — Product | Loop 2 — Meta (Agent) |
| :--- | :--- | :--- |
| Governs | Code, SQL, config | `CLAUDE.md` / context files |
| Goal | Working binaries & SPA | Fewer repeated mistakes |
| Decision | Edits to source tree | Proposed additions to context files |
| Delay | Build → pack → transfer → deploy | Time between mistake and correction |
| Side effect | Modbus timeouts, partition misses, etc. | Context bloat, stale rules |

## 1. Loop 1 — Product Loop (checklist)
Before any code change:
- **State the target reality** — which binary/service, config file, build environment.
- **Mitigate the air-gap delay** — test locally when possible; don't ship untested assumptions.
- **Name the side effect** — what touching this code does to the poll cycle, hypertable, or SPA assumptions.

## 2. Loop 2 — Meta Loop (concrete triggers)
**Fire when**:
- A deployment succeeds after a non-obvious workaround (e.g., missing `tar`, special curl flags).
- A command fails because of a wrong environment assumption (missing binary, wrong path, permissions).
- A new invariant is discovered that, if undocumented, would cause future failures.

**Action**:
1. Stop generating code.
2. State the **one fact** that would have prevented the mistake.
3. **Propose** a one-line addition to `CLAUDE.md` or a new guard. Do **not** write it automatically – surface it for approval.

**Examples of past captures**:
- "Rocky 9 has no `tar` (at least for now ;-)); use `python3 tarfile`."
- "`bht-neteco-poller` crate is actually named `neteco-poller`."
- "Config files in `/opt/bht/config/` are never in the tarball."

## 3. Resource governance (context window / tokens / attention)
**Large files — never read whole**:
- `events.tsv` (43 MB), `NE_sites.json`, `NETECO_*_clean.txt`, `master_alarms (2).log` – use `grep`, `awk`, or Python streaming.
- `snmp/*.log` files – same rule; stream with `grep` or `Select-String`, never `cat`.
- SQL migration files – never `cat` entire files; use `head -n` or `grep` to inspect schemas.

**General discipline**:
- **Prefer `Edit` over `Write`** – minimise diff size.
- **Don't re-read just-edited files.**
- **Summarise large output** – keep raw logs in the shell/file.
- **Use the task list** for multi-step chains; don't rely on scrollback memory.
- **Sliding attention**: finish one layer (Rust, SQL, TS) before moving to the next.
- If a task will clearly require **more than 10 file reads**, say so upfront and propose a split point before writing any code.

## 4. Double-Commit Output Rule
After completing a milestone:
- **System Output** – the actual Rust/TS/SQL/bash changes (always shown).
- **Agent Output** – *only if Loop 2 fired*: the proposed one-line context addition.
  **Omit this block on routine turns.**

## 5. Isolated-Inefficiency Proposals
Hold small inefficiencies across the current task. If the same inefficiency would recur or block later steps, raise it **once** as a single proposal line after the requested work is done. One-off annoyances – drop them.

## 6. Non-Goals
Everything under `CLAUDE.md` → "What Claude Should NOT Do" and "Safety Rails" applies unchanged:
no Docker on target, no dynamic linking, no schema/systemd changes without explicit ask, no `Cargo.lock` edits, no touching `_build_pack_*.sh`.
