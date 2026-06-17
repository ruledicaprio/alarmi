# BHT Alarm Pipeline — Stage 1: canonical model + normalization + schema

Greenfield rebuild of the alarm data pipeline. **Stage 1 delivers the foundation
only** — the unified data model, the parsers that normalize all sources into it,
and the TimescaleDB schema with retention tiers. No API and no UI yet (next stages).

> Watchdog cycle: this is **one stage**. Nothing in the live/legacy setup was
> altered or deleted — the new system lives in new folders alongside the old files.

## What's here

```
Cargo.toml                     Rust workspace
crates/normalize/              normalization library (the core)
  src/types.rs                   canonical model: Source, AlarmClass, Severity, Transition, CanonicalEvent
  src/parse.rs                   per-source line parsers (ispadnap feed)
  src/classify.rs                alarm-class taxonomy + severity/transition rules
  src/html.rs                    /alarmi/ out-of-service table parser
  tests/real_lines.rs            tests using verbatim real log lines
crates/loader/                 bht-loader: bulk-normalize log -> Postgres COPY/JSONL
db/schema.sql                  TimescaleDB: hypertables, retention, compression, rollups, episode pairing
db/seed_dimensions.sql         dim_source + dim_alarm_class
db/seed_sites.sql              dim_site seeded from 769 real NetEco sites
deploy/docker-compose.yml      TimescaleDB for local dev
scripts_load_sample.sh         end-to-end smoke test (build -> load -> rebuild -> verify)
tools/normalize_ref.py         Python validation ORACLE (rules mirror classify.rs 1:1)
docs/DATA_MODEL.md             the canonical model, taxonomy and retention design
```

## Run it on your home PC (WSL + Docker + cargo)

```bash
cp .env.example .env
docker compose -f deploy/docker-compose.yml up -d        # TimescaleDB (auto-runs schema + seeds)
cargo test -p bht-normalize                              # parser tests on real lines
./scripts_load_sample.sh master_alarms.log              # normalize + COPY + rebuild episodes + verify
```

`cargo build --release -p bht-loader` produces a single static-ish binary you can
also run standalone:

```bash
./target/release/bht-loader master_alarms.log > events.tsv          # COPY-ready TSV
./target/release/bht-loader --format jsonl master_alarms.log        # JSONL
```

## Validation (done, on your real data)

`tools/normalize_ref.py` over the real `master_alarms.log` (170,769 lines):

- **Parse coverage: 99.97%** (58 dropped: 30 unknown-system junk, 14 blank, 14 malformed)
- **Classification: 99.27%** into the taxonomy
- 8 sources, 132 distinct sites
- Transition split: 158,628 `INSTANT` (count-only) · 6,665 `RAISE` · 5,418 `CLEAR`

The Rust crate mirrors this oracle; `cargo test` locks them together. The crate
was **not** compiled in this environment (no toolchain/registry available here) —
compile it on your home PC. It was written against the validated logic and reviewed
for compile-correctness.

## Legacy files: superseded vs. still used

- **Superseded by this stage**: `BHT_Engine.ps1`, `refresh-alarms.ps1`,
  `BHT-Analytics-Engine*`, `stats_*.json` (the PowerShell→JSON→git-push approach).
- **Still used as data / inputs**: `master_alarms.log` (sample), `neteco_sites.csv`
  (site seed), `modbus/` (poller + `modbusmap-sc200300.txt` + `alarmna_lista.json`
  for the Modbus stage), `genset-inventory/`, `gis-map-export/` (inventory enrichment).

## Next stages (not in this cycle)

1. Rust Modbus poller (`source = modbus_eaton`) — staggered polling, circuit
   breaker, register-group batching against SC200/300.
2. Rust/Axum ingest + query API over this schema.
3. ant.design Pro dashboard.
