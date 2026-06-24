-- =====================================================================
-- BHT Alarm Dashboard v0.9.0 — v9 schema additions
--   1. source_t += 'datakom'  (Datakom SNMP now a distinct source)
--   2. neteco.devices.std_type_name (KPI routing for neteco-poller)
--   3. v_poller_summary — live device health snapshot for dashboard
-- Idempotent. Run after migrate_neteco_v1.sql.
--   sudo -u bht psql -d alarms -f db/migrate_v9.sql
-- =====================================================================

\set ON_ERROR_STOP on

-- ------------------------------------------------------------------ source_t
DO $$ BEGIN
  IF NOT EXISTS (
    SELECT 1 FROM pg_enum
    WHERE enumlabel = 'datakom'
      AND enumtypid = (SELECT oid FROM pg_type WHERE typname = 'source_t')
  ) THEN
    ALTER TYPE source_t ADD VALUE 'datakom';
  END IF;
END $$;

-- Update seed for the new source (idempotent)
INSERT INTO dim_source (source, label, is_stateful, description) VALUES
  ('datakom', 'Datakom SNMP', TRUE, 'Datakom genset controller SNMP traps (raise/clear)')
ON CONFLICT (source) DO UPDATE
  SET label=EXCLUDED.label, is_stateful=EXCLUDED.is_stateful, description=EXCLUDED.description;

-- ------------------------------------------------------------------ neteco.devices
-- std_type_name: human-readable device type from getDevList (devTypeName field).
-- Used by neteco-poller to route KPI inserts to the correct hypertable.
ALTER TABLE neteco.devices ADD COLUMN IF NOT EXISTS std_type_name TEXT;

-- ------------------------------------------------------------------ v_poller_summary
-- Per-region device health roll-up for the dashboard header tiles.
CREATE OR REPLACE VIEW v_poller_summary AS
SELECT
    COALESCE(s.region, 'UNKNOWN')                              AS region,
    count(*)                                                   AS devices_total,
    count(*) FILTER (WHERE d.enabled)                         AS devices_enabled,
    count(*) FILTER (WHERE d.enabled AND d.fail_streak = 0
                      AND d.last_ok >= now() - INTERVAL '5 minutes') AS devices_ok,
    count(*) FILTER (WHERE d.enabled AND d.fail_streak > 0)   AS devices_degraded,
    count(*) FILTER (WHERE d.enabled AND d.last_ok IS NULL)   AS devices_never_polled,
    max(d.last_polled)                                         AS last_poll_time
FROM dim_device d
LEFT JOIN dim_site s USING (site_key)
WHERE d.enabled
GROUP BY s.region
ORDER BY s.region;

-- ------------------------------------------------------------------ report
SELECT
  (SELECT count(*) FROM pg_enum WHERE enumlabel='datakom')                          AS datakom_enum_present,
  (SELECT count(*)::int FROM information_schema.columns
   WHERE table_schema='neteco' AND table_name='devices'
     AND column_name='std_type_name')                                               AS std_type_name_column_present,
  (SELECT count(*) FROM dim_region_canonical)                                       AS canonical_regions;
