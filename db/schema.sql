-- =====================================================================
-- BHT Alarm Dashboard v0.9.0 — Base TimescaleDB schema
-- Apply order: schema.sql → schema_stage2.sql → migrate_v6 → v7 → v8
--              → migrate_neteco_v1 → migrate_v9
-- Target: TimescaleDB (PostgreSQL 16) on Rocky 9 LXC 102 (192.168.108.88)
-- Enum LABELS match the Rust serde output of bht-normalize EXACTLY, so the
-- bht-loader TSV streams straight into COPY without translation.
-- =====================================================================
CREATE EXTENSION IF NOT EXISTS timescaledb;

-- ------------------------------------------------------------------ enums
DO $$ BEGIN
  CREATE TYPE source_t AS ENUM
    ('ignition','net_eco','u2020','rps_sc200','rps_sc300',
     'dse74xx','benning','baran','modbus_eaton','html_oos');
EXCEPTION WHEN duplicate_object THEN NULL; END $$;

DO $$ BEGIN
  CREATE TYPE alarm_class_t AS ENUM
    ('NE_DISCONNECTED','COMMS_LOST','MAINS_FAILURE','RECTIFIER_FAILURE',
     'RECTIFIER_COMMS','SOLAR_FAULT','UPS_MODULE','BATTERY_LOW','BATTERY_FAULT',
     'HIGH_VOLTAGE','GENSET_EVENT','COOLING_FAULT','DOOR_OPEN','FUSE_LOAD',
     'GENERIC_ERROR','SERVICE_OUTAGE','UNCLASSIFIED');
EXCEPTION WHEN duplicate_object THEN NULL; END $$;

DO $$ BEGIN
  CREATE TYPE severity_t AS ENUM ('critical','major','minor','warning','info');
EXCEPTION WHEN duplicate_object THEN NULL; END $$;

DO $$ BEGIN
  CREATE TYPE transition_t AS ENUM ('raise','clear','instant');
EXCEPTION WHEN duplicate_object THEN NULL; END $$;

-- ------------------------------------------------------- dimension tables
-- Site inventory: the single key every source resolves to. Enriched later
-- from neteco_sites.csv, genset inventory and the GIS exports.
CREATE TABLE IF NOT EXISTS dim_site (
    site_key      TEXT PRIMARY KEY,
    display_name  TEXT,
    region        TEXT,
    municipality  TEXT,
    technologies  TEXT[],                 -- PRISTUP/BTS/MPLS/DC/DWDM/RR/SDH...
    latitude      DOUBLE PRECISION,
    longitude     DOUBLE PRECISION,
    has_genset    BOOLEAN DEFAULT FALSE,
    has_battery   BOOLEAN DEFAULT FALSE,
    has_solar     BOOLEAN DEFAULT FALSE,
    is_important  BOOLEAN DEFAULT FALSE,
    updated_at    TIMESTAMPTZ DEFAULT now()
);

CREATE TABLE IF NOT EXISTS dim_source (
    source        source_t PRIMARY KEY,
    label         TEXT NOT NULL,
    is_stateful   BOOLEAN NOT NULL,       -- emits raise+clear -> durations pair
    description   TEXT
);

CREATE TABLE IF NOT EXISTS dim_alarm_class (
    alarm_class       alarm_class_t PRIMARY KEY,
    label             TEXT NOT NULL,
    is_power_critical BOOLEAN NOT NULL DEFAULT FALSE,
    default_severity  severity_t,
    description       TEXT
);

-- ---------------------------------------------------- fact: raw events (HOT)
-- The detailed, queryable event stream. Retention 90d keeps well over the
-- "30 days fast view" requirement; compression after 7d shrinks the cold tail.
CREATE TABLE IF NOT EXISTS fact_event (
    event_time   TIMESTAMPTZ      NOT NULL,
    source       source_t         NOT NULL,
    site_key     TEXT             NOT NULL,
    region       TEXT,
    alarm_class  alarm_class_t    NOT NULL,
    severity     severity_t       NOT NULL,
    transition   transition_t     NOT NULL,
    raw_site     TEXT,
    raw_alarm    TEXT,
    device_ip    INET,
    ingest_time  TIMESTAMPTZ      NOT NULL DEFAULT now()
);
SELECT create_hypertable('fact_event','event_time',
       chunk_time_interval => INTERVAL '1 day', if_not_exists => TRUE);

CREATE INDEX IF NOT EXISTS ix_event_site_time  ON fact_event (site_key, event_time DESC);
CREATE INDEX IF NOT EXISTS ix_event_class_time ON fact_event (alarm_class, event_time DESC);
CREATE INDEX IF NOT EXISTS ix_event_src_time   ON fact_event (source, event_time DESC);

-- Dedup constraint: same (instant, system, site, raw_alarm, transition) is
-- the same event. raw_alarm is required in the key because the alarm_class
-- bucket is lossy (e.g. DO-Alarm-1 and DO-Alarm-2 both classify as
-- GENERIC_ERROR but are distinct events). Lets us re-POST whole log feeds
-- without growing the table. Hypertable requires the partition column
-- (event_time) in any unique index.
CREATE UNIQUE INDEX IF NOT EXISTS ux_event_dedup
    ON fact_event (event_time, source, site_key, raw_alarm, transition);

ALTER TABLE fact_event SET (
    timescaledb.compress,
    timescaledb.compress_segmentby = 'site_key, source, alarm_class',
    timescaledb.compress_orderby   = 'event_time DESC'
);
SELECT add_compression_policy('fact_event', INTERVAL '7 days',  if_not_exists => TRUE);
SELECT add_retention_policy  ('fact_event', INTERVAL '90 days', if_not_exists => TRUE);

-- Lines that failed normalization, kept for parser-coverage triage.
CREATE TABLE IF NOT EXISTS fact_event_quarantine (
    ingest_time TIMESTAMPTZ NOT NULL DEFAULT now(),
    reason      TEXT,
    raw_line    TEXT
);

-- ------------------------------------------- fact: alarm episodes (DURATIONS)
-- Paired RAISE->CLEAR intervals = the "performance / duration" data. Compact,
-- so retained 2y. Built by rebuild_episodes() from stateful events.
CREATE TABLE IF NOT EXISTS fact_alarm_episode (
    raised_at        TIMESTAMPTZ   NOT NULL,
    cleared_at       TIMESTAMPTZ,
    duration_seconds DOUBLE PRECISION,
    is_open          BOOLEAN       NOT NULL,
    source           source_t      NOT NULL,
    site_key         TEXT          NOT NULL,
    alarm_class      alarm_class_t NOT NULL,
    severity         severity_t    NOT NULL
);
SELECT create_hypertable('fact_alarm_episode','raised_at',
       chunk_time_interval => INTERVAL '7 days', if_not_exists => TRUE);
CREATE INDEX IF NOT EXISTS ix_epi_site ON fact_alarm_episode (site_key, raised_at DESC);
CREATE INDEX IF NOT EXISTS ix_epi_open ON fact_alarm_episode (is_open) WHERE is_open;
SELECT add_retention_policy('fact_alarm_episode', INTERVAL '730 days', if_not_exists => TRUE);

-- ---------------------------------------------- continuous aggregates (ROLLUPS)
-- Hourly counts feed the 30-day fast-view dashboards. Retained 90d.
CREATE MATERIALIZED VIEW IF NOT EXISTS cagg_event_hourly
WITH (timescaledb.continuous) AS
SELECT time_bucket('1 hour', event_time) AS bucket,
       site_key, source, alarm_class, severity,
       count(*)                                          AS event_count,
       count(*) FILTER (WHERE transition = 'raise')      AS raise_count,
       count(*) FILTER (WHERE transition = 'clear')      AS clear_count
FROM fact_event
GROUP BY bucket, site_key, source, alarm_class, severity
WITH NO DATA;

-- Daily rollups are the multi-year history tier (basic per-site/device queries).
CREATE MATERIALIZED VIEW IF NOT EXISTS cagg_event_daily
WITH (timescaledb.continuous) AS
SELECT time_bucket('1 day', event_time) AS bucket,
       site_key, source, alarm_class,
       count(*)                                     AS event_count,
       count(*) FILTER (WHERE transition='raise')   AS raise_count,
       count(*) FILTER (WHERE transition='clear')   AS clear_count
FROM fact_event
GROUP BY bucket, site_key, source, alarm_class
WITH NO DATA;

SELECT add_continuous_aggregate_policy('cagg_event_hourly',
       start_offset => INTERVAL '3 days', end_offset => INTERVAL '1 hour',
       schedule_interval => INTERVAL '30 minutes', if_not_exists => TRUE);
SELECT add_continuous_aggregate_policy('cagg_event_daily',
       start_offset => INTERVAL '10 days', end_offset => INTERVAL '1 hour',
       schedule_interval => INTERVAL '1 hour', if_not_exists => TRUE);

SELECT add_retention_policy('cagg_event_hourly', INTERVAL '90 days',   if_not_exists => TRUE);
SELECT add_retention_policy('cagg_event_daily',  INTERVAL '1825 days', if_not_exists => TRUE);

-- ---------------------------------------------------------- episode pairing
-- Gaps-and-islands pairing: per (site_key, source, alarm_class), collapse
-- repeated same-state events, then pair each RAISE with the next CLEAR. Mirrors
-- the prior PowerShell engine's first-raise-to-first-clear duration logic.
CREATE OR REPLACE FUNCTION rebuild_episodes(p_since TIMESTAMPTZ DEFAULT '-infinity')
RETURNS BIGINT LANGUAGE plpgsql AS $$
DECLARE n BIGINT;
BEGIN
  DELETE FROM fact_alarm_episode WHERE raised_at >= p_since;

  WITH ordered AS (
    SELECT event_time, site_key, source, alarm_class, severity, transition,
           LAG(transition) OVER w AS prev_t
    FROM fact_event
    WHERE transition IN ('raise','clear') AND event_time >= p_since
    WINDOW w AS (PARTITION BY site_key, source, alarm_class ORDER BY event_time)
  ),
  changes AS (                          -- collapse consecutive same transitions
    SELECT * FROM ordered WHERE prev_t IS DISTINCT FROM transition
  ),
  paired AS (
    SELECT site_key, source, alarm_class, severity, transition,
           event_time AS raised_at,
           LEAD(event_time)  OVER w AS next_time,
           LEAD(transition)  OVER w AS next_t
    FROM changes
    WINDOW w AS (PARTITION BY site_key, source, alarm_class ORDER BY event_time)
  )
  INSERT INTO fact_alarm_episode
        (raised_at, cleared_at, duration_seconds, is_open,
         source, site_key, alarm_class, severity)
  SELECT raised_at,
         CASE WHEN next_t = 'clear' THEN next_time END,
         CASE WHEN next_t = 'clear'
              THEN EXTRACT(EPOCH FROM (next_time - raised_at)) END,
         (next_t IS DISTINCT FROM 'clear'),
         source, site_key, alarm_class, severity
  FROM paired
  WHERE transition = 'raise';

  GET DIAGNOSTICS n = ROW_COUNT;
  RETURN n;
END $$;

-- --------------------------------------------------------- fast-view helpers
-- 30-day site reliability snapshot used by the dashboard landing page.
CREATE OR REPLACE VIEW v_site_reliability_30d AS
SELECT site_key,
       count(*)                                   AS episodes,
       count(*) FILTER (WHERE is_open)            AS open_now,
       round((sum(coalesce(duration_seconds,0))/3600.0)::numeric, 2) AS outage_hours,
       round((avg(duration_seconds)/60.0)::numeric, 1)       AS avg_minutes
FROM fact_alarm_episode
WHERE raised_at >= now() - INTERVAL '30 days'
GROUP BY site_key;

-- Currently-active (unresolved) alarms across stateful sources.
CREATE OR REPLACE VIEW v_active_alarms AS
SELECT site_key, source, alarm_class, severity, raised_at,
       round((EXTRACT(EPOCH FROM (now() - raised_at))/60.0)::numeric, 1) AS open_minutes
FROM fact_alarm_episode
WHERE is_open
ORDER BY raised_at;
