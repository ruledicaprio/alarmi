#!/usr/bin/env bash
# End-to-end Stage-1 smoke test on the home PC:
#   1. build bht-loader
#   2. normalize master_alarms.log -> TSV
#   3. COPY into fact_event, quarantine the rest
#   4. rebuild duration episodes + refresh rollups
# Requires: cargo, psql, a running TimescaleDB (deploy/docker-compose.yml).
set -euo pipefail
cd "$(dirname "$0")"
: "${PGHOST:=localhost}" "${PGPORT:=5432}" "${PGUSER:=bht}" "${PGDATABASE:=alarms}"
export PGPASSWORD="${PGPASSWORD:-bht_dev_pw}"
PSQL="psql -h $PGHOST -p $PGPORT -U $PGUSER -d $PGDATABASE -v ON_ERROR_STOP=1"
LOG="${1:-master_alarms.log}"

echo "==> building loader"
cargo build --release -p bht-loader

echo "==> normalizing $LOG"
./target/release/bht-loader --quarantine /tmp/bht_quarantine.tsv "$LOG" > /tmp/bht_events.tsv

echo "==> COPY into fact_event"
$PSQL -c "\copy fact_event(event_time,source,site_key,region,alarm_class,severity,transition,raw_site,raw_alarm,device_ip) FROM '/tmp/bht_events.tsv' WITH (FORMAT text, NULL '\\N')"
$PSQL -c "\copy fact_event_quarantine(reason,raw_line) FROM '/tmp/bht_quarantine.tsv' WITH (FORMAT text)" || true

echo "==> rebuild episodes + refresh rollups"
$PSQL -c "SELECT rebuild_episodes();"
$PSQL -c "CALL refresh_continuous_aggregate('cagg_event_daily', NULL, NULL);"
$PSQL -c "CALL refresh_continuous_aggregate('cagg_event_hourly', NULL, NULL);"

echo "==> sanity"
$PSQL -c "SELECT source, count(*) FROM fact_event GROUP BY source ORDER BY 2 DESC;"
$PSQL -c "SELECT alarm_class, count(*) FROM fact_event GROUP BY alarm_class ORDER BY 2 DESC LIMIT 10;"
$PSQL -c "SELECT count(*) AS episodes, count(*) FILTER (WHERE is_open) AS open FROM fact_alarm_episode;"
