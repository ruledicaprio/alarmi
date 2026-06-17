#!/usr/bin/env bash
# Provision PostgreSQL 16 + TimescaleDB on Rocky 9 LXC (192.168.108.88).
# Run as root (or with sudo) ON the Rocky box. Native dnf install.
set -euo pipefail

PGBIN=/usr/pgsql-16/bin
DB_NAME="${DB_NAME:-alarms}"
DB_USER="${DB_USER:-bht}"
DB_PASS="${DB_PASS:-bht_dev_pw}"   # change me

echo "==> 0. sanity: can we reach the repos?"
if ! dnf -q repolist >/dev/null 2>&1; then
  echo "!! dnf cannot reach repos. Use the AIR-GAPPED path (see README_DEPLOY_ROCKY.md)."; exit 1
fi

echo "==> 1. PGDG repo + PostgreSQL 16"
dnf -y install "https://download.postgresql.org/pub/repos/yum/reporpms/EL-9-x86_64/pgdg-redhat-repo-latest.noarch.rpm"
dnf -qy module disable postgresql || true
dnf -y install postgresql16 postgresql16-server postgresql16-contrib

echo "==> 2. TimescaleDB repo"
cat > /etc/yum.repos.d/timescale_timescaledb.repo <<'REPO'
[timescale_timescaledb]
name=timescale_timescaledb
baseurl=https://packagecloud.io/timescale/timescaledb/el/9/$basearch
repo_gpgcheck=1
gpgcheck=0
enabled=1
gpgkey=https://packagecloud.io/timescale/timescaledb/gpgkey
sslverify=1
sslcacert=/etc/pki/tls/certs/ca-bundle.crt
metadata_expire=300
REPO
dnf -y install timescaledb-2-postgresql-16

echo "==> 3. initdb + timescaledb tuning (sets shared_preload_libraries)"
if [ ! -s /var/lib/pgsql/16/data/PG_VERSION ]; then
  "$PGBIN/postgresql-16-setup" initdb
fi
timescaledb-tune --quiet --yes --pg-config="$PGBIN/pg_config" || true

echo "==> 4. enable + start"
systemctl enable --now postgresql-16
sleep 3

echo "==> 5. role + database + extension"
sudo -u postgres psql -v ON_ERROR_STOP=1 <<SQL
DO \$\$ BEGIN
  IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname='${DB_USER}') THEN
    CREATE ROLE ${DB_USER} LOGIN PASSWORD '${DB_PASS}';
  END IF;
END \$\$;
SELECT 'CREATE DATABASE ${DB_NAME} OWNER ${DB_USER}'
  WHERE NOT EXISTS (SELECT FROM pg_database WHERE datname='${DB_NAME}')\gexec
SQL
sudo -u postgres psql -d "${DB_NAME}" -c "CREATE EXTENSION IF NOT EXISTS timescaledb;"

echo "==> DONE. PostgreSQL 16 + TimescaleDB ready."
echo "    DSN: host=localhost port=5432 user=${DB_USER} password=${DB_PASS} dbname=${DB_NAME}"
echo "    Next: deploy/rocky_apply_schema.sh"
