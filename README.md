# Alarm Dashboard

A full-stack monitoring dashboard for industrial alarm data, built for **air-gapped
deployment** on Rocky Linux. It ingests alarms from Modbus and SNMP devices, stores
them in a time-series database, and serves a reactive SPA for real-time supervision.

## Table of Contents
- [Overview](#overview)
- [Tech Stack](#tech-stack)
- [Architecture](#architecture)
- [Prerequisites](#prerequisites)
- [Development Setup](#development-setup)
- [Building](#building)
- [Deployment](#deployment)
- [Configuration](#configuration)
- [Database](#database)
- [Project Structure](#project-structure)
- [Contributing](#contributing)

## Overview
- **Backend** – Rust micro-services that poll devices over Modbus/SNMP, normalise
  alarm payloads, and expose a REST API.
- **Frontend** – Single-page application with dashboards, charts, and alarm tables.
- **Database** – PostgreSQL + TimescaleDB for partitioned time-series storage and
  efficient retention management.

The system is designed for **offline operation**: all dependencies are vendored,
binaries are statically linked, and no runtime internet access is required.

## Tech Stack
| Layer | Technology |
| :---- | :--------- |
| API server | Rust, Axum, Tokio |
| Device pollers | Rust, Tokio-Modbus, custom SNMP |
| Frontend | TypeScript, React 18, Vite, Ant Design 5, Recharts |
| Database | PostgreSQL 16, TimescaleDB (hypertables) |
| Target OS | Rocky Linux 9 (air-gapped) |
| Build | Docker (MUSL cross-compilation) or native Rust toolchain |

## Architecture
```
[Devices] ──(Modbus/SNMP)──> [bht-poller / neteco-poller]
                                         │
                                (normalise & insert)
                                         │
                              [PostgreSQL + TimescaleDB]
                                         │
                    [bht-api] ── REST ──> [SPA (Vite + React)]
```

- **`bht-poller`** – continuously polls configured Modbus devices, ingests alarm
  events, and writes them to the database.
- **`neteco-poller`** – SNMP-based poller for NetEco devices.
- **`bht-api`** – Axum HTTP server that queries the database and serves both the
  JSON API and the static frontend assets.
- **Database** – hypertables partition alarm data by time; custom SQL functions
  (`rebuild_episodes()`) maintain materialised state for dashboards.

The frontend is a Vite-built SPA that communicates exclusively with `bht-api`
through `/api/v1` endpoints.

## Prerequisites

### Development (on a workstation with internet)
- Rust toolchain (stable) with `x86_64-unknown-linux-musl` target
- Node.js 18+ and npm
- PostgreSQL 16 + TimescaleDB (for local testing)
- Docker (optional, used for reproducible production builds)

### Target Environment
- Rocky Linux 9 machine (bare-metal or LXC)
- PostgreSQL 16 with TimescaleDB extension
- Static IP address, no internet access
- Systemd for service management

## Development Setup

```bash
# Clone the repository
git clone <repo-url>
cd alarmi-repo
```

**Backend:**
```bash
cargo build
# Optionally run locally with a local Postgres
DATABASE_URL=postgres://bht:password@localhost/alarms cargo run -p bht-api
```

**Frontend:**
```bash
cd web
npm install
npm run dev       # Vite dev server at http://localhost:5173
```

The dev server proxies API calls to the backend; configure the proxy in
`web/vite.config.ts` if needed.

## Building

### Production (MUSL, static binaries + frontend)
```bash
bash deploy/build_in_docker.sh
```

This uses Docker to produce three statically-linked binaries
(`bht-api`, `bht-poller`, `neteco-poller`) and the frontend `dist/`.
Everything is placed under `target/x86_64-unknown-linux-musl/release/` and
`web/dist/`.

**Verification:**
```bash
file target/x86_64-unknown-linux-musl/release/bht-api
# must print: ... statically linked, stripped
```

A tarball for deployment is created manually after the build (see [Deployment](#deployment)).

## Deployment
Deployment targets an air-gapped Rocky Linux machine. The typical workflow:

1. Build the tarball:
   ```bash
   tar czf bht-upgrade.tar.gz \
     -C target/x86_64-unknown-linux-musl/release bht-api bht-poller neteco-poller \
     -C "$PWD/web" dist
   ```
2. Transfer the tarball to the target (e.g., via HTTP server + `curl`, as `scp` is
   usually blocked in air-gapped environments).
3. On the target, extract and install (a helper script `rocky_deploy.sh` automates
   the API + frontend part).

**Important**: Configuration files (`/opt/bht/config/*.toml`) are **not** part of
the tarball and must be maintained separately on the target.

For full manual steps, see [`docs/MANUAL_DEPLOY.md`](docs/MANUAL_DEPLOY.md).

## Configuration
All runtime configuration lives in TOML files on the target machine under
`/opt/bht/config/`:

- `api.toml` – listen address, database connection, CORS
- `poller.toml` – Modbus timeouts, poll intervals
- `devices.toml` – device list and register maps
- `eaton_alarms.toml`, `datakom_alarms.toml`, … – alarm definition files
- `.neteco.env` – environment file for the SNMP poller

These files are never checked into the repository (except templates). Adjust them
directly on the target.

## Database
Migrations are plain SQL files stored in `db/migrations/`. Apply them in order:
```bash
psql -U bht -d alarms -f db/migrations/NNN_description.sql
```

After schema changes, run the maintenance function:
```sql
SELECT rebuild_episodes();
```

Hypertables and retention policies are configured once during setup (see
`deploy/rocky_setup_timescaledb.sh`). All queries must respect the partitioning
key (`time`) to avoid performance degradation.

## Project Structure
```
alarmi-repo/
├── crates/
│   ├── bht-api/          # REST server
│   ├── bht-poller/       # Modbus poller
│   ├── neteco-poller/    # SNMP poller
│   ├── normalize/        # Alarm normalization
│   └── loader/           # Data loading utilities
├── web/                  # React SPA (Vite)
│   ├── src/
│   │   ├── pages/        # Route components
│   │   ├── components/   # Reusable UI
│   │   ├── services/     # API client
│   │   └── types/        # TypeScript interfaces
│   └── vite.config.ts
├── db/
│   └── migrations/       # SQL migration files
├── deploy/               # Build, deploy, and systemd scripts
├── snmp/                 # SNMP trap log files (stream only — never cat whole)
├── Cargo.toml            # Workspace definition
└── README.md
```

## Contributing
This repository follows a **surgical-change** policy:

- Match existing code style and patterns.
- Do not refactor unrelated code or add unrequested dependencies.
- All new dependencies must compile for `x86_64-unknown-linux-musl`.
- Database changes must be supplied as plain SQL scripts.
- Frontend changes must respect the Ant Design component library.
- After changing the database schema, always call `rebuild_episodes()`.

For detailed development and automation rules, see [`CLAUDE.md`](CLAUDE.md) and
[`AGENT_PROTOCOL.md`](AGENT_PROTOCOL.md).

**License:** BH Telecom proprietary – internal use only.
