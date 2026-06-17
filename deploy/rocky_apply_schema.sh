#!/usr/bin/env bash
# Apply the BHT schema + seeds (Stage 1 + Stage 2) to the local DB on Rocky.
# Run from the repo root on the Rocky box (after rocky_setup_timescaledb.sh).
set -euo pipefail
DB_NAME="${DB_NAME:-alarms}"; DB_USER="${DB_USER:-bht}"; export PGPASSWORD="${DB_PASS:-bht_dev_pw}"
PSQL="psql -h localhost -U $DB_USER -d $DB_NAME -v ON_ERROR_STOP=1"
$PSQL -f db/schema.sql
$PSQL -f db/seed_dimensions.sql
$PSQL -f db/seed_sites.sql
$PSQL -f db/schema_stage2.sql
echo "==> schema applied. Tables:"
$PSQL -c "\dt"
$PSQL -c "SELECT count(*) AS sites FROM dim_site;"
