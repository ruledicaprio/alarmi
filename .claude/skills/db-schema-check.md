# DB Schema — Apply & Verify (Rocky 9)

All SQL is applied on Rocky 9 as user `bht`. Never connect to production DB from WSL.

---

## Single migration file

```bash
scp db/migrate_foo.sql root@192.168.108.88:/tmp/
ssh root@192.168.108.88 \
  'sudo -u bht psql -d alarms -v ON_ERROR_STOP=1 -f /tmp/migrate_foo.sql'
```

---

## Multiple migration files

```bash
ssh root@192.168.108.88 'mkdir -p /tmp/migrations'
scp db/*.sql root@192.168.108.88:/tmp/migrations/
ssh root@192.168.108.88 'for f in /tmp/migrations/*.sql; do
  echo "==> $f"
  sudo -u bht psql -d alarms -v ON_ERROR_STOP=1 -f "$f" || exit 1
done'
```

---

## After schema changes

If the migration touches episode pairing tables (`fact_event`, `dim_alarm_pair`, etc.):
```bash
ssh root@192.168.108.88 \
  'sudo -u bht psql -d alarms -c "SELECT rebuild_episodes();"'
```

---

## Verify schema

```bash
ssh root@192.168.108.88 'sudo -u bht psql -d alarms -c "
\dt public.*
\dt neteco.*
SELECT count(*) AS fact_event_rows   FROM fact_event;
SELECT count(*) AS neteco_alarm_rows FROM neteco.alarms;
SELECT pg_size_pretty(pg_database_size('"'"'alarms'"'"')) AS db_size;
SELECT hypertable_name, num_chunks FROM timescaledb_information.hypertables;
"'
```

---

## Notes

- `-v ON_ERROR_STOP=1` is required — without it psql continues past errors silently
- TimescaleDB hypertable partitioning must be preserved; do not alter partition columns
- `rebuild_episodes()` is idempotent but slow on large datasets — run after hours if possible
- DB schema changes are **raw SQL only** — no migration tooling (Diesel, sqlx-migrate, etc.)
