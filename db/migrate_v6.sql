-- v6 schema additions:
--   1. add 'smartlogger_huawei' to source_t enum
--   2. fact_site_verification table + view
--   3. (idempotent — re-run safe)

\set ON_ERROR_STOP on

-- 1. Enum value (idempotent via NOT EXISTS check)
DO $$ BEGIN
  IF NOT EXISTS (
    SELECT 1 FROM pg_enum
    WHERE enumlabel = 'smartlogger_huawei'
      AND enumtypid = (SELECT oid FROM pg_type WHERE typname = 'source_t')
  ) THEN
    ALTER TYPE source_t ADD VALUE 'smartlogger_huawei';
  END IF;
END $$;

-- 2. Site verification log (operator marks "I reviewed these events")
CREATE TABLE IF NOT EXISTS fact_site_verification (
    id              BIGSERIAL PRIMARY KEY,
    site_key        TEXT        NOT NULL REFERENCES dim_site(site_key),
    verified_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    verified_by     TEXT        NOT NULL DEFAULT 'operator',
    notes           TEXT        NOT NULL DEFAULT '',
    events_through  TIMESTAMPTZ NOT NULL,
    ip_inventory    TEXT[]      NOT NULL DEFAULT '{}',
    region_confirmed TEXT       NOT NULL DEFAULT ''
);
CREATE INDEX IF NOT EXISTS ix_verif_site ON fact_site_verification (site_key, verified_at DESC);

-- 3. last-verification summary per site
CREATE OR REPLACE VIEW v_site_verification_status AS
SELECT site_key,
       MAX(verified_at)                                                      AS last_verified,
       MAX(verified_by)        FILTER (WHERE verified_at = (
         SELECT MAX(verified_at) FROM fact_site_verification v2
         WHERE v2.site_key = fact_site_verification.site_key))                AS last_verified_by,
       MAX(events_through)     FILTER (WHERE verified_at = (
         SELECT MAX(verified_at) FROM fact_site_verification v2
         WHERE v2.site_key = fact_site_verification.site_key))                AS events_through
FROM fact_site_verification
GROUP BY site_key;

-- 4. seed dim_site rows for the 4 Huawei SmartLogger PV sites
INSERT INTO dim_site (site_key, display_name, region, municipality, technologies)
VALUES
  ('FNE_BIHAC_TKC',     'FNE Bihać TKC',     'BIHAC',    'Bihać',     ARRAY['SOLAR_PV']),
  ('FNE_FRANCA_LEHARA', 'FNE Franca Lehara', 'SARAJEVO', 'Sarajevo',  ARRAY['SOLAR_PV']),
  ('FNE_DMALTA',        'FNE D. Malta',      'SARAJEVO', 'Sarajevo',  ARRAY['SOLAR_PV']),
  ('FNE_ILIDZA',        'FNE Ilidža',        'SARAJEVO', 'Ilidža',    ARRAY['SOLAR_PV'])
ON CONFLICT (site_key) DO UPDATE
   SET display_name = EXCLUDED.display_name,
       region       = EXCLUDED.region,
       municipality = EXCLUDED.municipality;

-- 5. on first install (and every install), force a full episode rebuild so
--    /api/alarms/active stops being empty
SELECT rebuild_episodes('-infinity');

SELECT
  (SELECT count(*) FROM pg_enum WHERE enumlabel='smartlogger_huawei') AS smartlogger_enum_present,
  (SELECT count(*) FROM fact_alarm_episode)                            AS episode_count,
  (SELECT count(*) FROM fact_alarm_episode WHERE is_open)              AS open_episodes;
