-- =====================================================================
-- BHT Alarm Pipeline — v8 schema additions
--   1. dim_device   — DB-backed device registry (replaces config/devices.toml
--                     as the poller's source of truth; enables hot-reload
--                     and per-device health tracking without log-scraping)
--   2. is_stub      — column on dim_site; auto-set when a new site_key
--                     appears in ingested events with no prior dim_site row
--   3. v_device_orphans — device IPs seen in events not yet in dim_device
-- Idempotent. Run after migrate_v7.sql.
--
-- Seed dim_device from existing devices.toml:
--   python3 db/seed_devices_v8.py [path/to/config/devices.toml] | \
--     psql "host=localhost port=5432 dbname=alarms user=bht password=bht_dev_pw"
-- =====================================================================

\set ON_ERROR_STOP on

-- ------------------------------------------------------------------ dim_device
-- The physical device registry. Maps each Modbus endpoint (ip, unit_id) to a
-- site_key. Replaces the static devices.toml as the poller's device source.
-- Health columns (last_polled, last_ok, fail_streak) are written by the poller
-- every cycle, turning this into a live device health dashboard.
CREATE TABLE IF NOT EXISTS dim_device (
    id          BIGSERIAL     PRIMARY KEY,
    ip          INET          NOT NULL,
    port        INT           NOT NULL DEFAULT 502,
    unit_id     SMALLINT      NOT NULL DEFAULT 1,
    site_key    TEXT          NOT NULL,
    dev_type    TEXT          NOT NULL DEFAULT 'eaton',    -- 'eaton' | 'smartlogger'
    base0       BOOLEAN       NOT NULL DEFAULT false,
    fne         BOOLEAN       NOT NULL DEFAULT false,
    enabled     BOOLEAN       NOT NULL DEFAULT true,
    name        TEXT          NOT NULL DEFAULT '',
    notes       TEXT          NOT NULL DEFAULT '',
    -- health state written by poller each cycle
    first_seen  TIMESTAMPTZ   NOT NULL DEFAULT now(),
    last_polled TIMESTAMPTZ,                               -- last attempt (ok or fail)
    last_ok     TIMESTAMPTZ,                               -- last successful poll
    fail_streak INT           NOT NULL DEFAULT 0,          -- consecutive failures
    added_by    TEXT          NOT NULL DEFAULT 'operator',
    updated_at  TIMESTAMPTZ   NOT NULL DEFAULT now()
);

-- Natural key: (ip, unit_id).
-- The same physical IP can expose multiple Modbus unit IDs mapping to different
-- logical sites — e.g. Huawei SmartLogger with parallel inverter strings, or
-- two Eaton SC-series controllers on the same TCP gateway. Seen in practice:
--   10.10.1.126 unit=0 → UPRAVNA_ZGRADA_23_KWP
--   10.10.1.126 unit=1 → FNE_FRANCA_LEHARA
CREATE UNIQUE INDEX IF NOT EXISTS ux_device_ip_unit
    ON dim_device (ip, unit_id);

CREATE INDEX IF NOT EXISTS ix_device_site_key  ON dim_device (site_key);
CREATE INDEX IF NOT EXISTS ix_device_enabled   ON dim_device (enabled) WHERE enabled;
CREATE INDEX IF NOT EXISTS ix_device_fail      ON dim_device (fail_streak) WHERE fail_streak > 0;

-- ------------------------------------------------------------------ dim_site stub flag
-- Marks rows that were auto-inserted when a new site_key appeared in an ingested
-- event but had no prior dim_site entry. Stubs need operator enrichment
-- (region, geo-coordinates, technologies[]).
ALTER TABLE dim_site
    ADD COLUMN IF NOT EXISTS is_stub BOOLEAN NOT NULL DEFAULT false;

-- Existing rows are real entries — leave them as is_stub=false.

-- ------------------------------------------------------------------ v_device_orphans
-- Device IPs that have sent events but are not yet registered in dim_device.
-- These are candidates for operator review and registration.
-- Replaces the previous site-key-level orphan view for the device dimension.
CREATE OR REPLACE VIEW v_device_orphans AS
SELECT host(e.device_ip)::text  AS ip,
       e.site_key,
       count(*)                  AS event_count,
       max(e.event_time)         AS last_seen,
       max(e.source::text)       AS source
FROM fact_event e
WHERE e.device_ip IS NOT NULL
  AND NOT EXISTS (
      SELECT 1 FROM dim_device d WHERE d.ip = e.device_ip
  )
GROUP BY e.device_ip, e.site_key
ORDER BY last_seen DESC;

-- ------------------------------------------------------------------ v_device_health
-- Convenience view for the dashboard: all devices with their live health state.
CREATE OR REPLACE VIEW v_device_health AS
SELECT d.id,
       host(d.ip)::text          AS ip,
       d.port,
       d.unit_id,
       d.site_key,
       COALESCE(s.display_name, d.site_key) AS site_name,
       COALESCE(s.region, '')    AS region,
       d.dev_type,
       d.fne,
       d.enabled,
       d.name,
       d.fail_streak,
       d.last_polled::text       AS last_polled,
       d.last_ok::text           AS last_ok,
       CASE
           WHEN d.last_ok IS NULL                              THEN 'never'
           WHEN d.fail_streak >= 3                             THEN 'dead'
           WHEN d.fail_streak > 0                             THEN 'degraded'
           WHEN d.last_ok < now() - INTERVAL '10 minutes'     THEN 'stale'
           ELSE 'ok'
       END                       AS health,
       d.added_by,
       d.updated_at::text        AS updated_at
FROM dim_device d
LEFT JOIN dim_site s USING (site_key);

-- ------------------------------------------------------------------ report
SELECT
  (SELECT count(*)                FROM dim_device)                   AS devices_total,
  (SELECT count(*) FILTER (WHERE enabled) FROM dim_device)           AS devices_enabled,
  (SELECT count(*) FILTER (WHERE is_stub) FROM dim_site)             AS stub_sites,
  (SELECT count(*)                FROM v_device_orphans)             AS orphan_ips_in_events;
