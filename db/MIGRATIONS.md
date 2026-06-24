# DB Migration Apply Order

Fresh install (Rocky 9):

```bash
sudo -u bht psql -d alarms -f db/schema.sql
sudo -u bht psql -d alarms -f db/schema_stage2.sql
sudo -u bht psql -d alarms -f db/migrate_v6.sql
sudo -u bht psql -d alarms -f db/migrate_v7.sql
sudo -u bht psql -d alarms -f db/migrate_v8.sql
sudo -u bht psql -d alarms -f db/migrate_neteco_v1.sql
sudo -u bht psql -d alarms -f db/migrate_v9.sql

sudo -u bht psql -d alarms -f db/seed_dimensions.sql
sudo -u bht psql -d alarms -f db/seed_sites.sql
python3 db/seed_devices_v8.py config/devices.toml | sudo -u bht psql -d alarms
```

Upgrade from v8 → v9:

```bash
sudo -u bht psql -d alarms -f db/migrate_v9.sql
```

## File index

| File | Contents |
|---|---|
| `schema.sql` | Core enums, `dim_site`, `dim_source`, `dim_alarm_class`, `fact_event` hypertable, `fact_alarm_episode`, hourly/daily caggs, `rebuild_episodes()`, views |
| `schema_stage2.sql` | `fact_measurement` hypertable + `cagg_measurement_daily` + `v_latest_measurement` |
| `migrate_v6.sql` | `source_t += smartlogger_huawei`, `fact_site_verification`, `v_site_verification_status` |
| `migrate_v7.sql` | `user_role_t`, `dim_user`, `dim_region_canonical` (7 BHT regions), `v_verified_inventory` |
| `migrate_v8.sql` | `dim_device` (DB-backed device registry), `is_stub` on `dim_site`, `v_device_orphans`, `v_device_health` |
| `migrate_neteco_v1.sql` | `neteco.*` schema — 7 metric hypertables, `neteco.alarms`, `source_t += neteco_nbi`, 2 caggs |
| `migrate_v9.sql` | `source_t += datakom`, `neteco.devices.std_type_name`, `v_poller_summary` |
| `seed_dimensions.sql` | `dim_source` and `dim_alarm_class` reference data |
| `seed_sites.sql` | 769 `dim_site` rows from neteco_sites.csv |
| `seed_devices_v8.py` | Generates INSERT for `dim_device` from `config/devices.toml` |
