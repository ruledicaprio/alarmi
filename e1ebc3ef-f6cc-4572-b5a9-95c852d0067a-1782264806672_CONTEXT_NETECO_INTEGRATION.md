# NetEco / SitePower OSS Integration — Session Context

## Project: alarmi-repo (BHT Alarm Dashboard)

### Stack recap
- **Backend**: Rust (`crates/`) — `bht-api` (Axum 0.7 REST), `bht-poller` (Modbus collector)
- **Frontend**: Vite + TypeScript + React 18 + Ant Design 5 (`web/`)
- **DB**: PostgreSQL 16 + TimescaleDB on Rocky Linux 9 LXC (192.168.108.88)
- **Target**: Air-gapped, static MUSL binary, no Docker on target
- **Build**: `cargo build --release --target x86_64-unknown-linux-musl` via Docker on WSL

### Current ingest sources
| Source | Protocol | Crate/Handler |
|--------|----------|---------------|
| Eaton SC200/SC300 | Modbus TCP | `bht-poller` |
| Huawei SmartLogger 3000 | Modbus TCP | `bht-poller` (smartlogger module) |
| HTML scraper (legacy PSU alarms) | HTTP scrape | external, posts to `/ingest/events` |
| SNMP log files (u2020/ljutoc) | File parse | **new task** |

### v8 inventory (just deployed)
- `dim_device` table — 301 devices, live health via poller writeback
- `v_device_health` view — ok/degraded/dead/stale/never per device
- 8 new API endpoints for device CRUD, orphan claim, site enrichment
- Poller hot-reloads from DB every 10 cycles, no restart needed for new devices

---

## New Task: NetEco / SitePower OSS Integration

### What is NetEco SitePower OSS?
Huawei NetEco (formerly U2020) network management + SitePower OSS — manages PSU (Power Supply Units), SMU (Site Monitoring Units), DPDU (Digital Power Distribution Units), OPMS, and other telecom site power equipment via SNMP.

### Integration goal
Ingest alarms from NetEco-managed devices into `fact_event` (same pipeline as Eaton/SmartLogger), making them visible in the dashboard alongside existing alarms.

### Scale
~800 NetEco devices: SMU, DPDU, PSU, OPMS — multiple device types, all reporting via SNMP to the NetEco server.

### Current PSU alarm source (to be replaced/supplemented)
Currently scraped from HTML — fragile. Target: parse structured log files instead.

### Log file source
SFTP server: `192.168.132.117` (separate machine from Rocky 9 target)
```
sftp://rusmir:Sarajevo2025%21@192.168.132.117/root/snmplogovi/
Fingerprint: ssh-rsa-4c-7a-11-7e-e2-17-ee-6a-a4-fc-33-4f-d4-01-f4-8c

Files:
  /root/snmplogovi/ljutoc.log        ← ALL systems: Eaton RPSS C200/C300, Baran, Benning, DSE-74xx genset controllers
  /root/snmplogovi/sve_napajanjeran.log  ← u2020 (NetEco/SitePower) alarms ONLY
```

### Note on SNMP/Ignition
Ignition SCADA SNMP logging is temporarily disabled — logs may be incomplete during this period.

---

## Integration design (to be worked out in session)

### Option A — Log file poller (recommended starting point)
New crate or module that:
1. SFTPs to 192.168.132.117, reads `/root/snmplogovi/sve_napajanjeran.log` (and optionally `ljutoc.log`)
2. Parses log lines → `CanonicalEvent`
3. POSTs to `/ingest/events` or writes directly to DB via same UNNEST pattern

### Option B — NetEco northbound API
If NetEco exposes a REST/SOAP northbound interface, poll it directly.
(Need credentials/docs — check with Rusmir.)

### Device types to handle
| Type | Description |
|------|-------------|
| PSU  | Power Supply Unit — currently HTML-scraped |
| SMU  | Site Monitoring Unit |
| DPDU | Digital Power Distribution Unit |
| OPMS | (clarify with Rusmir) |

### Source enum
Current `source_t` DB enum: `modbus_eaton`, `smartlogger_huawei`, (others).
Will need a new value: `neteco_huawei` or `sitepower_huawei` (requires DB migration).

### Alarm normalization
The `crates/normalize/` crate contains `CanonicalEvent` and existing normalization logic.
New NetEco alarm definitions will need a mapping file (similar to `config/eaton_alarms.toml`).

---

## Files to read at session start
```
crates/normalize/src/lib.rs          ← CanonicalEvent, Source enum, AlarmClass, Severity
crates/poller/src/sink.rs            ← write_events() UNNEST pattern (reuse for new poller)
crates/poller/src/types.rs           ← DeviceCfg, PollerConfig patterns
crates/api/src/ingest.rs             ← /ingest/events handler (POST target if not direct DB)
db/schema.sql                        ← source_t enum, fact_event columns
db/migrate_v8.sql                    ← latest migration (v8, applied)
config/eaton_alarms.toml             ← alarm definition pattern to replicate for NetEco
```

---

## Key constraints (always apply)
- Static MUSL binary — no dynamic deps, no std networking changes
- No migration tooling — DB changes are raw SQL applied manually via psql
- No tokio runtime changes without explicit request
- TimescaleDB hypertable partitioning respected in all query changes
- No unwrap() in production paths
- Air-gapped target — binaries transferred via HTTP served from WSL dev machine

## Build & Deploy
```bash
# WSL
cd ~/alarmi-repo
sudo rm target/x86_64-unknown-linux-musl/release/bht-api   # if stale
touch crates/api/src/main.rs   # force recompile if needed
bash deploy/build_in_docker.sh

tar czf ~/bht-upgrade-neteco1.tar.gz \
  -C target/x86_64-unknown-linux-musl/release bht-api bht-poller \
  -C ~/alarmi-repo/web dist \
  -C ~/alarmi-repo deploy/rocky_deploy.sh

cd ~ && python3 -m http.server 8000

# Rocky 9
curl -O http://192.168.82.205:8000/bht-upgrade-neteco1.tar.gz
python3 -c "import tarfile,warnings; warnings.filterwarnings('ignore'); tarfile.open('bht-upgrade-neteco1.tar.gz').extractall()"
bash ~/deploy/rocky_deploy.sh bht-upgrade-neteco1.tar.gz
sudo systemctl stop bht-poller
sudo cp ~/bht-poller /opt/bht/bht-poller && sudo chmod +x /opt/bht/bht-poller && sudo chown bht:bht /opt/bht/bht-poller
sudo systemctl start bht-poller
sudo journalctl -u bht-poller -n 30 --no-pager
```
