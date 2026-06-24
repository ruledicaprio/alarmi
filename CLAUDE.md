# alarmi-repo — Project Context

## Stack
- **Backend**: Rust (`crates/`) — `bht-api` (Axum REST), `bht-poller` (device polling)
- **Frontend**: Vite + TypeScript (`web/`) — static SPA
- **DB**: PostgreSQL 16 + TimescaleDB on target machine
- **Target**: Rocky Linux 9, air-gapped, no internet access

## Compilation
```
cargo build --release --target x86_64-unknown-linux-musl
```
Static MUSL binary. Zero dynamic deps. Do not suggest dynamic linking.

## Build & Deploy (Rocky 9, air-gapped)

### Target
- Rocky Linux 9 LXC (Proxmox container 102), IP `192.168.108.88`
- No internet, no `tar` command — use `python3 tarfile` for extraction
- Services run as user `bht`, install dir `/opt/bht`
- PostgreSQL 16 + TimescaleDB, DB name `alarms`, user `bht`

### 1. Build frontend (WSL or host)
```bash
cd ~/alarmi-repo/web
npm install          # only if deps changed
npm run build        # output → web/dist/
```

### 2. Build backend — two options

**Option A — Native MUSL (if musl-tools installed in WSL):**
```bash
cd ~/alarmi-repo
rustup target add x86_64-unknown-linux-musl   # one-time
cargo build --release --target x86_64-unknown-linux-musl -p bht-api -p bht-poller
# binaries → target/x86_64-unknown-linux-musl/release/{bht-api,bht-poller}
```

**Option B — Docker (no Rust/musl needed on host):**
```bash
bash deploy/build_in_docker.sh
# same output path
```

### 3. Pack tarball
```bash
cd ~/alarmi-repo
tar czf ~/bht-upgrade.tar.gz \
  -C target/x86_64-unknown-linux-musl/release bht-api bht-poller \
  -C "$PWD/web" dist
```
If only frontend changed, pack just `dist`. If only backend, pack just binaries.

### 4. Transfer to Rocky 9
No `scp` — network policy blocks it. Serve via Python HTTP and curl from Rocky:
```bash
# On work PC (WSL terminal):
cd ~ && python3 -m http.server 8000

# On Rocky 9:
curl -O http://192.168.82.205:8000/bht-upgrade.tar.gz
```

### 5. Deploy on Rocky 9
**Automated (recommended):**
```bash
bash rocky_deploy.sh bht-upgrade.tar.gz
```
This script: extracts → stops bht-api → backs up old binary → copies new binary + dist → restarts → health-checks.

**Manual steps (if rocky_deploy.sh not present or deploying poller too):**
```bash
# Extract (Rocky 9 has no tar)
python3 -c "import tarfile, warnings; warnings.filterwarnings('ignore'); tarfile.open('bht-upgrade.tar.gz').extractall()"

# Stop services
sudo systemctl stop bht-api bht-poller

# Deploy binaries
sudo cp ~/bht-api /opt/bht/bht-api && sudo chmod +x /opt/bht/bht-api
sudo cp ~/bht-poller /opt/bht/bht-poller && sudo chmod +x /opt/bht/bht-poller
sudo chown bht:bht /opt/bht/bht-api /opt/bht/bht-poller

# Deploy frontend
sudo rm -rf /opt/bht/web/dist
sudo mv ~/dist /opt/bht/web/dist
sudo chown -R bht:bht /opt/bht/web

# Start services
sudo systemctl start bht-api bht-poller
sleep 2

# Verify
sudo systemctl is-active bht-api bht-poller
curl -sf localhost:8080/api/health && echo "OK"
```

### 6. Run SQL on Rocky 9
```bash
sudo -u bht psql -d alarms -f /path/to/script.sql
# or inline:
sudo -u bht psql -d alarms -c "SELECT count(*) FROM dim_device WHERE fne;"
```

### Systemd services
- `bht-api.service` — Axum REST, serves SPA from `/opt/bht/web/dist`, config at `/opt/bht/config/api.toml`
- `bht-poller.service` — Modbus poller, config at `/opt/bht/config/poller.toml` + `devices.toml` + `eaton_alarms.toml`
- Logs: `sudo journalctl -u bht-api -f` / `sudo journalctl -u bht-poller -f`

### Quick shortcuts (common ops on Rocky)
```bash
# Tail API logs
sudo journalctl -u bht-api -f --no-pager

# Restart just API (frontend-only or query changes)
sudo systemctl restart bht-api

# Restart just poller (polling config/metric changes)
sudo systemctl restart bht-poller

# DB shell
sudo -u bht psql -d alarms

# Check binary is static
ldd /opt/bht/bht-api    # should say "not a dynamic executable"
```

## Key Constraints
- **No std networking changes** on target — air-gapped, static IPs only
- **No migration tooling** — DB schema changes are raw SQL applied manually
- **No tokio runtime changes** without explicit request — tuned for embedded poller workload
- **TimescaleDB hypertable partitioning** must be respected in all query changes

## Directory Map
```
crates/
  bht-api/        ← Axum HTTP server, routes, handlers
  bht-poller/     ← Device polling loop, alarm ingestion
  normalize/      ← Alarm normalization logic
  loader/         ← Data loading utilities
web/
  src/            ← All TS/React source (Vite + React 18 + Ant Design 5 + Recharts)
  index.html
  vite.config.ts
deploy/
  build_in_docker.sh       ← Docker-based MUSL build (no local Rust needed)
  rocky_deploy.sh          ← Automated deploy on Rocky 9
  rocky_setup_timescaledb.sh ← One-time PG16+TimescaleDB setup
  bht-api.service          ← systemd unit
  bht-poller.service       ← systemd unit
Cargo.toml        ← workspace root (members: normalize, loader, poller, api)
```

## Coding Rules
- Rust by default; match existing async patterns (tokio + sqlx)
- No `unwrap()` in production paths — use `?` or explicit error handling
- Frontend: match existing component style before introducing new patterns
- Do not touch `_build_pack_*.sh` unless explicitly asked
- Do not alter systemd unit files or DB schema without explicit instruction

## What Claude Should NOT Do
- Propose refactors outside the immediate request scope
- Add dependencies without confirming compatibility with MUSL target
- Suggest Docker, Podman, or container-based deployment — target is bare metal
- Modify Cargo.lock (let cargo manage it)

# Claude Code Guidelines

Four principles in one file that directly address these issues:

| Principle | Addresses |
|-----------|-----------|
| **Think Before Coding** | Wrong assumptions, hidden confusion, missing tradeoffs |
| **Simplicity First** | Overcomplication, bloated abstractions |
| **Surgical Changes** | Orthogonal edits, touching code you shouldn't |
| **Goal-Driven Execution** | Leverage through tests-first, verifiable success criteria |

## The Four Principles in Detail

### 1. Think Before Coding

**Don't assume. Don't hide confusion. Surface tradeoffs.**

LLMs often pick an interpretation silently and run with it. This principle forces explicit reasoning:

- **State assumptions explicitly** — If uncertain, ask rather than guess
- **Present multiple interpretations** — Don't pick silently when ambiguity exists
- **Push back when warranted** — If a simpler approach exists, say so
- **Stop when confused** — Name what's unclear and ask for clarification

### 2. Simplicity First

**Minimum code that solves the problem. Nothing speculative.**

Combat the tendency toward overengineering:

- No features beyond what was asked
- No abstractions for single-use code
- No "flexibility" or "configurability" that wasn't requested
- No error handling for impossible scenarios
- If 200 lines could be 50, rewrite it

**The test:** Would a senior engineer say this is overcomplicated? If yes, simplify.

### 3. Surgical Changes

**Touch only what you must. Clean up only your own mess.**

When editing existing code:

- Don't "improve" adjacent code, comments, or formatting
- Don't refactor things that aren't broken
- Match existing style, even if you'd do it differently
- If you notice unrelated dead code, mention it — don't delete it

When your changes create orphans:

- Remove imports/variables/functions that YOUR changes made unused
- Don't remove pre-existing dead code unless asked

**The test:** Every changed line should trace directly to the user's request.

### 4. Goal-Driven Execution

**Define success criteria. Loop until verified.**

Transform imperative tasks into verifiable goals:

| Instead of... | Transform to... |
|--------------|-----------------|
| "Add validation" | "Write tests for invalid inputs, then make them pass" |
| "Fix the bug" | "Write a test that reproduces it, then make it pass" |
| "Refactor X" | "Ensure tests pass before and after" |

For multi-step tasks, state a brief plan:

```
1. [Step] → verify: [check]
2. [Step] → verify: [check]
3. [Step] → verify: [check]
```

## Claude Code Automation Rules

### Project Binaries
| Binary        | Crate                  | Deployed as service                          |
|---------------|------------------------|----------------------------------------------|
| bht-api       | crates/api             | bht-api.service + bht-api-staging.service    |
| bht-poller    | crates/poller          | bht-poller.service                           |
| neteco-poller | crates/neteco-poller   | neteco-poller.service                        |
| bht-loader    | crates/loader          | utility only (no service)                    |

### When asked to build
1. Determine scope: frontend only, backend only (which binaries), or both.
2. Full build (frontend + backend) — **always use Docker**:
   ```bash
   bash deploy/build_in_docker.sh
   ```
   `build_in_docker.sh` builds Rust MUSL binaries (rust:slim) AND frontend via `npm ci && npm run build` (node:lts-slim) in separate containers. No local Rust or Node needed.
   (Backend-only fallback if Docker unavailable: `cargo build --release --target x86_64-unknown-linux-musl -p bht-api -p bht-poller -p bht-neteco-poller`)
4. Verify every binary is statically linked:
   ```bash
   for bin in bht-api bht-poller neteco-poller; do
     file target/x86_64-unknown-linux-musl/release/$bin | grep -q "statically linked" \
       && echo "$bin OK" || { echo "$bin FAIL"; exit 1; }
   done
   ```
5. Pack:
   ```bash
   tar czf ~/bht-upgrade.tar.gz \
     -C target/x86_64-unknown-linux-musl/release bht-api bht-poller neteco-poller \
     -C "$PWD/web" dist
   ```
6. Print: tarball location, `du -sh` size, `sha256sum`, and scp command.

### When asked to deploy
1. Verify tarball exists: `ls -lh ~/bht-upgrade.tar.gz`
2. Transfer — **no scp**, serve via Python 3.6.8 Anaconda HTTP server from **work PC Windows 10 client** with IP address: **[192.168.82.205]**, curl from Rocky:
   ```bash
   # Work PC (Windows 10):    cd ~ && python3 -m http.server 8000
   # Rocky Linux (LXC 102):   curl -O http://[IP_ADDRESS]/bht-upgrade.tar.gz
   ```
3. Print: `ssh root@192.168.108.88` then on Rocky: `bash ~/rocky_deploy.sh bht-upgrade.tar.gz`
   - rocky_deploy.sh handles bht-api + dist only; use manual steps for bht-poller / neteco-poller.
4. Remind: config files live at `/opt/bht/config/` on Rocky and are NOT in the tarball.
5. Print post-deploy health checks (see `.claude/skills/deploy-to-rocky.md`).

### When SQL is involved
1. Lint locally: `sqlfluff lint db/`
2. Print exact `scp` + `psql -v ON_ERROR_STOP=1` commands for Rocky.
3. After schema changes: run `SELECT rebuild_episodes();` on Rocky.
4. Never connect to production/staging DB from WSL.

### Target invariants
- Rocky 9 LXC 102, IP `192.168.108.88`
- User `bht`, install dir `/opt/bht` (staging: `/opt/bht-staging`)
- DB: `alarms` (prod), `alarms_staging` (staging), DB user: `bht`
- Rocky has **no `tar`** — use `python3 tarfile` or `rocky_deploy.sh`
- Config files: `/opt/bht/config/{api,poller,devices,eaton_alarms,datakom_alarms,smartlogger_alarms,neteco}.toml` + `.neteco.env`
- SELinux disabled on Rocky

### What NOT to do
- Don't modify systemd unit files or DB schema without explicit instruction
- Don't alter `Cargo.lock` manually
- Don't suggest dynamic linking, container-based targets, or runtime Docker on Rocky
- Don't touch `_build_pack_*.sh` unless explicitly asked
