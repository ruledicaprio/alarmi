# alarmi-repo — Project Context (Agent Playbook)

## Agent Protocol
Read `AGENT_PROTOCOL.md` at the start of every session. It defines the dual-loop
operating model and resource-governance rules that **layer on top of this file**.

## Stack
- **Backend**: Rust workspace — `bht-api` (Axum REST), `bht-poller` (device polling), `neteco-poller`
- **Frontend**: Vite + TypeScript (`web/`) — React 18, Ant Design 5, Recharts
- **DB**: PostgreSQL 16 + TimescaleDB (hypertables for time-series data)
- **Target**: Rocky Linux 9 LXC, air-gapped, static IP `192.168.108.88`

## Compilation
```bash
cargo build --release --target x86_64-unknown-linux-musl
```

**Zero dynamic dependencies.** All binaries must be statically linked.
Dynamic linking must never be proposed.

## Directory Map (surgical navigation)
```
crates/
  bht-api/            ← Axum server: routes, handlers, models, DB queries
  bht-poller/         ← Modbus polling loop, alarm ingestion, device config
  neteco-poller/      ← NetEco SNMP poller
  normalize/          ← Alarm normalisation logic
  loader/             ← Data loading utilities
web/
  src/
    pages/            ← Top-level route components
    components/       ← Reusable UI components
    services/         ← API client functions
    types/            ← TypeScript interfaces
  index.html
  vite.config.ts
deploy/
  build_in_docker.sh         ← Docker-based MUSL + frontend build (preferred)
  rocky_deploy.sh            ← Automated deploy on Rocky (bht-api + dist)
  rocky_setup_timescaledb.sh ← One-time PG16+TimescaleDB setup
  bht-api.service
  bht-poller.service
snmp/
  *.log                      ← SNMP trap log files (large; stream with grep/awk only — never cat whole)
Cargo.toml            ← workspace root (members: normalize, loader, poller, api)
```

## Coding Rules
- **Rust by default** – match existing `tokio` + `sqlx` async patterns.
- **No `unwrap()`** in production paths; use `?` or explicit error handling.
- **Frontend** – match existing Ant Design component style; do not introduce new layout paradigms without approval.
- **Do not touch** `_build_pack_*.sh` unless explicitly asked.
- **Do not alter** systemd unit files or DB schema without explicit instruction.

### Extension Patterns (follow these exactly)
| Task | Rust touch points | SQL touch points | Frontend touch points |
| :--- | :--- | :--- | :--- |
| **Add REST endpoint** | `crates/bht-api/src/routes.rs`, `handlers/`, `models/` | — | `web/src/services/`, `types` |
| **Add device metric** | `crates/bht-poller/src/metrics.rs`, `devices.toml` example | — | — |
| **DB schema change** | — | `db/migrations/NNN_description.sql` → apply via psql → run `SELECT rebuild_episodes();` | — |
| **New dashboard page** | — | — | `web/src/pages/`, `components/`, route in `App.tsx` |

## Safety Rails (hard stops)
Stop and ask if any of the following would be violated:
- A binary is not statically linked after build.
- A new query does not respect TimescaleDB hypertable partitioning.
- A config file (`/opt/bht/config/*.toml`) would be modified.
- A new crate/dependency cannot be compiled for `x86_64-unknown-linux-musl`.

## What Claude Should NOT Do
- Propose refactors outside the immediate request scope.
- Add dependencies without verifying MUSL compatibility.
- Suggest Docker/Podman on the target (bare-metal only).
- Modify `Cargo.lock` manually.
- Change the API version prefix (`/api/v1`), Axum `AppState` shape, or Vite proxy config.
- Touch `_build_pack_*.sh` or systemd unit files without explicit request.

---

## Automation Rules (primary instruction set)

### When asked to build

1. **Determine scope**: frontend only, backend only (which binaries), or both.

2. **Full build (frontend + any backend) → always use Docker**:
   ```bash
   bash deploy/build_in_docker.sh
   ```
   This builds all MUSL binaries (`bht-api`, `bht-poller`, `neteco-poller`) and the frontend in separate containers.

3. **Backend-only fallback** (only if Docker unavailable):
   ```bash
   cargo build --release --target x86_64-unknown-linux-musl -p bht-api -p bht-poller -p neteco-poller
   ```

4. **Verify static linking** (non-skippable):
   ```bash
   for bin in bht-api bht-poller neteco-poller; do
     file target/x86_64-unknown-linux-musl/release/$bin | grep -q "statically linked" \
       && echo "$bin OK" || { echo "$bin FAIL"; exit 1; }
   done
   ```

5. **Pack tarball**:
   ```bash
   tar czf ~/bht-upgrade.tar.gz \
     -C target/x86_64-unknown-linux-musl/release bht-api bht-poller neteco-poller \
     -C "$PWD/web" dist
   ```

6. **Print**: tarball location, `du -sh`, `sha256sum`, and the deploy command hint.

### When asked to deploy

1. Verify tarball exists.

2. Transfer via Python HTTP server (no scp):
   ```bash
   # Work PC rp021.telecom.ba (Windows):
   cd ~ && python3 -m http.server 8000
   # Rocky (LXC 102):
   curl -O http://192.168.82.205:8000/bht-upgrade.tar.gz
   ```

3. Deploy using `rocky_deploy.sh` for `bht-api` + frontend. Use manual steps for `bht-poller` / `neteco-poller` if needed.

4. Remind: config files at `/opt/bht/config/` are **not** in the tarball.

5. Print health-check commands.

### When SQL is involved

1. Lint locally: `sqlfluff lint db/`

2. Print the exact `psql -v ON_ERROR_STOP=1` command for Rocky.

3. After schema changes: `SELECT rebuild_episodes();` on Rocky.

4. **Never** connect to production/staging DB from WSL.

### Target invariants

- Rocky 9 LXC 102, user `bht`, install dir `/opt/bht`
- DB: `alarms`, user `bht`
- Rocky has **no `tar`** → use `python3 tarfile` or `rocky_deploy.sh`
- Config files: `/opt/bht/config/*.toml` + `.neteco.env`
- SELinux disabled on Rocky
- **docker-host (Ubuntu 24.04, user `rusmir`)**: same air-gap pattern as Rocky — `docker build` on workstation → `docker save | gzip` → python http.server + curl transfer → `docker load` on docker-host. Never `docker pull` or `npm install` on docker-host directly. Use `--network host` so containers reach llama-server on `localhost:8080-8082`.

> **Manual deployment steps** have been moved to `docs/MANUAL_DEPLOY.md`.
> The Automation Rules above supersede them for all agent-driven tasks.
