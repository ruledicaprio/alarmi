# Stage 3 — Axum ingest/query API

A single static binary (`bht-api`) over the Stage-1/2 TimescaleDB. Plain HTTP +
`NoTls` Postgres (isolated LAN), CORS-open for the future ant.design dashboard.
Turns the manual scrape→tar→copy into a service: the work-PC scraper can POST
raw feed text and the API normalizes + inserts it with the same `bht-normalize`
logic as the loader.

## Endpoints

Ingest:
- `POST /ingest/raw/ispadnap`  — body = raw feed lines → normalized → inserted
- `POST /ingest/events`        — JSON `CanonicalEvent[]`
- `POST /ingest/measurements`  — JSON `[{ts?,site_key,device_ip?,metric,value}]`

Query (for the dashboard):
- `GET /api/health`
- `GET /api/sites`                         — sites + open-alarm counts
- `GET /api/alarms/active`                 — currently open (paired) alarms
- `GET /api/alarms/recent?hours=&site=&class=&source=&limit=`
- `GET /api/sites/:site_key/reliability`   — 30-day episodes / outage hours
- `GET /api/sites/:site_key/measurements?metric=&hours=`   — telemetry series
- `GET /api/measurements/latest?site=`
- `GET /api/stats/by-class?hours=`
- `GET /api/stats/by-region?hours=`

## Run (home PC, against the Stage-1 Docker DB)

The Docker DB only loaded Stage-1 schema at init; add Stage-2 objects (needed by
the measurement endpoints) once:
```bash
docker exec -i bht_tsdb psql -U bht -d alarms < db/schema_stage2.sql
```
Then:
```bash
cargo run -p bht-api                 # listens on 0.0.0.0:8080
```
Smoke test:
```bash
curl -s localhost:8080/api/health
curl -s 'localhost:8080/api/stats/by-class?hours=100000'
curl -s 'localhost:8080/api/alarms/recent?limit=5&hours=100000'
# ingest the sample feed straight in:
curl -s -X POST --data-binary @master_alarms.log localhost:8080/ingest/raw/ispadnap
```
(`hours=100000` widens the window past the April sample data.)

## Deploy to Rocky

Build static and ship (see **README_DEPLOY_ROCKY.md**):
```bash
cargo build --release --target x86_64-unknown-linux-musl -p bht-api
scp target/x86_64-unknown-linux-musl/release/bht-api config/api.toml user@192.168.108.88:/opt/bht/
sudo systemctl enable --now bht-api    # deploy/bht-api.service
```

## Notes / seams

- Auth: none yet (isolated LAN). Add a token middleware before any non-isolated exposure.
- Pagination: list endpoints cap at sensible limits; cursor paging is a later add.
- The `Ignition/NetEco clear` model fix (Stage-1 patch) will change what
  `/api/alarms/active` shows for those sources once applied.
