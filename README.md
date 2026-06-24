# BHT Alarm Dashboard v0.9.0

Multi-source alarm monitoring and telemetry platform for BHT Power & Cooling infrastructure.

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    Sources (10 types)                       │
│  Eaton SC200/300 · Smartlogger · Datakom · NetEco · SCADA  │
└────────────┬──────────────────────────┬─────────────────────┘
             │ Modbus/SNMP              │ REST (NBI)
    ┌────────▼────────┐        ┌────────▼────────┐
    │   bht-poller    │        │ bht-neteco-poller│
    │  (async Modbus) │        │  (NetEco iSite) │
    └────────┬────────┘        └────────┬────────┘
             │                          │
             └──────────┬───────────────┘
                        │ canonical events → TimescaleDB
               ┌────────▼────────┐
               │   bht-normalize │  (shared library)
               └────────┬────────┘
                        │
               ┌────────▼────────────────────────┐
               │  PostgreSQL 16 + TimescaleDB     │
               │  hypertable: fact_event (90d)    │
               │  episodes:   fact_alarm_episode  │
               │  aggregates: cagg_event_daily    │
               └────────┬────────────────────────┘
                        │
               ┌────────▼────────┐
               │    bht-api      │  Axum REST + SPA host
               └────────┬────────┘
                        │
               ┌────────▼────────┐
               │  React SPA      │  Vite · TypeScript · Ant Design 5
               │  (bht-dashboard)│  9 pages · Recharts
               └─────────────────┘
```

## Crates

| Crate | Binary | Role |
|---|---|---|
| `bht-normalize` | — | Cross-source canonicalization library |
| `bht-poller` | `bht-poller` | Async Modbus device polling (Eaton SC200/300, Smartlogger, Datakom) |
| `bht-neteco-poller` | `neteco-poller` | NetEco iSitePower NBI REST poller |
| `bht-api` | `bht-api` | Axum REST API + SPA static file server |
| `bht-loader` | `bht-loader` | Bulk ispadnap log normalization (offline import) |

## Quick Start (dev)

```bash
# 1. Start local DB
docker compose -f deploy/docker-compose.yml up -d

# 2. Apply schema
bash deploy/rocky_apply_schema.sh

# 3. Build frontend
cd web && npm install && npm run build && cd ..

# 4. Run API
cargo run -p bht-api

# 5. Open http://localhost:8080
```

## Build for Production (Rocky 9, MUSL)

```bash
# Backend
cargo build --release --target x86_64-unknown-linux-musl -p bht-api -p bht-poller

# Frontend
cd web && npm run build

# Pack
tar czf bht-upgrade.tar.gz \
  -C target/x86_64-unknown-linux-musl/release bht-api bht-poller \
  -C web dist

# Deploy
scp bht-upgrade.tar.gz root@192.168.108.88:~
ssh root@192.168.108.88 bash rocky_deploy.sh bht-upgrade.tar.gz
```

## Configuration

| File | Purpose |
|---|---|
| `config/api.toml` | API bind address, DB DSN, static dir |
| `config/poller.toml` | Poll interval, timeouts, circuit breaker |
| `config/devices.toml` | Master device inventory |
| `config/eaton_alarms.toml` | Eaton SC200/300 alarm class mapping |
| `config/smartlogger_alarms.toml` | Smartlogger alarm mapping |
| `config/datakom_alarms.toml` | Datakom alarm mapping |
| `config/neteco.toml` | NetEco NBI credentials + polling config |
| `config/staging/` | Staging environment overrides |

## Target Environment

- **Host**: Rocky Linux 9 LXC (Proxmox), `192.168.108.88`
- **Runtime user**: `bht`, install dir `/opt/bht`
- **DB**: PostgreSQL 16 + TimescaleDB, database `alarms`
- **Network**: Air-gapped, static IPs only
- **Binary**: Static MUSL — zero dynamic dependencies

## Data Model

See [`docs/DATA_MODEL.md`](docs/DATA_MODEL.md) for the canonical event schema, alarm taxonomy, and retention tiers.
