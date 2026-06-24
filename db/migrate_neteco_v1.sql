-- =====================================================================
-- BHT Alarm Pipeline — NetEco NBI Integration schema (v1)
-- Adds neteco.* schema for Huawei iSitePower NBI REST data.
-- Tables: sites, devices, 7 metric hypertables, alarms, 2 continuous aggs.
-- Idempotent. Run after migrate_v8.sql on Rocky 9:
--   sudo -u bht psql -d alarms -f migrate_neteco_v1.sql
-- =====================================================================

\set ON_ERROR_STOP on

-- ------------------------------------------------------------------ schema
CREATE SCHEMA IF NOT EXISTS neteco;

-- ------------------------------------------------------------------ enum update
-- Add 'neteco_nbi' to source_t if not present (for fact_event integration)
DO $$ BEGIN
  ALTER TYPE source_t ADD VALUE IF NOT EXISTS 'neteco_nbi';
EXCEPTION WHEN duplicate_object THEN NULL; END $$;

-- ------------------------------------------------------------------ sites
CREATE TABLE IF NOT EXISTS neteco.sites (
    station_code   TEXT PRIMARY KEY,
    station_name   TEXT,
    updated_at     TIMESTAMPTZ DEFAULT now()
);

-- ------------------------------------------------------------------ devices
CREATE TABLE IF NOT EXISTS neteco.devices (
    device_id        BIGINT PRIMARY KEY,          -- devId from getDevList
    station_code     TEXT NOT NULL REFERENCES neteco.sites(station_code),
    dev_name         TEXT,
    esn_code         TEXT,
    dev_type_id      INT,                         -- internal REST devTypeId
    std_type_id      INT,                         -- Standard Type ID (60026,60067,69999…)
    std_type_name    TEXT,                         -- 'Controller','Site Unit','Power System'…
    controller_model INT,                         -- 0=SMU02B, 1=SMU02C (KPI signal 10010)
    hw_version       TEXT,
    sw_version       TEXT,
    longitude        DOUBLE PRECISION,
    latitude         DOUBLE PRECISION,
    updated_at       TIMESTAMPTZ DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_neteco_dev_station ON neteco.devices(station_code);
CREATE INDEX IF NOT EXISTS idx_neteco_dev_type    ON neteco.devices(dev_type_id);

-- ================================================================== METRICS
-- All metric tables are TimescaleDB hypertables with 7-day chunks,
-- compression after 14 days, segmented by device_id.

-- ------------------------------------------------------------------ site_unit (60067)
CREATE TABLE IF NOT EXISTS neteco.site_unit_metrics (
    ts                          TIMESTAMPTZ NOT NULL,
    device_id                   BIGINT NOT NULL,
    station_code                TEXT NOT NULL,
    indoor_temp_c               DOUBLE PRECISION,   -- 10002
    outdoor_temp_c              DOUBLE PRECISION,   -- 10003
    indoor_humidity_pct         DOUBLE PRECISION,   -- 10004
    ac_input_power_kw           DOUBLE PRECISION,   -- 10007
    dc_output_power_kw          DOUBLE PRECISION,   -- 10008
    ac_output_power_kw          DOUBLE PRECISION,   -- 10011
    max_bbu_temp_c              DOUBLE PRECISION,   -- 10009
    total_ac_input_energy_kwh   DOUBLE PRECISION,   -- 10005
    total_dc_output_energy_kwh  DOUBLE PRECISION,   -- 10006
    staggering_exception_cause  INT,                -- 10010
    source                      TEXT NOT NULL DEFAULT 'nbi_rest',
    PRIMARY KEY (device_id, ts)
);
SELECT create_hypertable('neteco.site_unit_metrics', 'ts',
    chunk_time_interval => INTERVAL '7 days',
    if_not_exists => TRUE);
ALTER TABLE neteco.site_unit_metrics SET (
    timescaledb.compress,
    timescaledb.compress_segmentby = 'device_id',
    timescaledb.compress_orderby = 'ts DESC');
SELECT add_compression_policy('neteco.site_unit_metrics', INTERVAL '14 days',
    if_not_exists => TRUE);

-- ------------------------------------------------------------------ power_system (69999)
CREATE TABLE IF NOT EXISTS neteco.power_system_metrics (
    ts                              TIMESTAMPTZ NOT NULL,
    device_id                       BIGINT NOT NULL,
    station_code                    TEXT NOT NULL,
    current_power_supply_type       INT,            -- 10013 (enum 0-9)
    dc_output_voltage_v             DOUBLE PRECISION, -- 10016
    total_dc_load_current_a         DOUBLE PRECISION, -- 10017
    total_dc_load_power_kw          DOUBLE PRECISION, -- 10018
    system_load_ratio_pct           DOUBLE PRECISION, -- 10012
    total_ac_input_energy_kwh       DOUBLE PRECISION, -- 10020
    total_dc_load_energy_kwh        DOUBLE PRECISION, -- 10019
    total_temp_control_energy_kwh   DOUBLE PRECISION, -- 10009
    port_48v_current_a              DOUBLE PRECISION, -- 10014
    dc_load_48v_current_a           DOUBLE PRECISION, -- 10015
    source                          TEXT NOT NULL DEFAULT 'nbi_rest',
    PRIMARY KEY (device_id, ts)
);
SELECT create_hypertable('neteco.power_system_metrics', 'ts',
    chunk_time_interval => INTERVAL '7 days',
    if_not_exists => TRUE);
ALTER TABLE neteco.power_system_metrics SET (
    timescaledb.compress,
    timescaledb.compress_segmentby = 'device_id',
    timescaledb.compress_orderby = 'ts DESC');
SELECT add_compression_policy('neteco.power_system_metrics', INTERVAL '14 days',
    if_not_exists => TRUE);

-- ------------------------------------------------------------------ dpdu (60009)
CREATE TABLE IF NOT EXISTS neteco.dpdu_metrics (
    ts                          TIMESTAMPTZ NOT NULL,
    device_id                   BIGINT NOT NULL,
    station_code                TEXT NOT NULL,
    dc_output_voltage_v         DOUBLE PRECISION,   -- 10001
    total_dc_load_current_a     DOUBLE PRECISION,   -- 10002
    total_dc_load_power_kw      DOUBLE PRECISION,   -- 10003
    total_dc_load_energy_kwh    DOUBLE PRECISION,   -- 10004
    other_power_input_current_a DOUBLE PRECISION,   -- 10005
    num_llvd                    SMALLINT,            -- 10006
    source                      TEXT NOT NULL DEFAULT 'nbi_rest',
    PRIMARY KEY (device_id, ts)
);
SELECT create_hypertable('neteco.dpdu_metrics', 'ts',
    chunk_time_interval => INTERVAL '7 days',
    if_not_exists => TRUE);
ALTER TABLE neteco.dpdu_metrics SET (
    timescaledb.compress,
    timescaledb.compress_segmentby = 'device_id',
    timescaledb.compress_orderby = 'ts DESC');
SELECT add_compression_policy('neteco.dpdu_metrics', INTERVAL '14 days',
    if_not_exists => TRUE);

-- ------------------------------------------------------------------ battery_group (60016)
CREATE TABLE IF NOT EXISTS neteco.battery_group_metrics (
    ts                          TIMESTAMPTZ NOT NULL,
    device_id                   BIGINT NOT NULL,
    station_code                TEXT NOT NULL,
    battery_state               INT,                -- 10001 (0=Float,1=Boost,2=Discharge…)
    voltage_v                   DOUBLE PRECISION,   -- 10002
    current_a                   DOUBLE PRECISION,   -- 10003
    soc_pct                     DOUBLE PRECISION,   -- 10004
    soh_pct                     INT,                -- 10016
    temp_c                      DOUBLE PRECISION,   -- 10005
    backup_time_h               DOUBLE PRECISION,   -- 10007
    backup_time_ai_h            DOUBLE PRECISION,   -- 10028
    charge_discharge_power_kw   DOUBLE PRECISION,   -- 10026
    rated_capacity_ah           INT,                -- 10013
    remaining_capacity_ah       INT,                -- 10014
    total_cycle_times           INT,                -- 10008
    current_limiting_state      INT,                -- 10009
    on_off_state                INT,                -- 10011
    source                      TEXT NOT NULL DEFAULT 'nbi_rest',
    PRIMARY KEY (device_id, ts)
);
SELECT create_hypertable('neteco.battery_group_metrics', 'ts',
    chunk_time_interval => INTERVAL '7 days',
    if_not_exists => TRUE);
ALTER TABLE neteco.battery_group_metrics SET (
    timescaledb.compress,
    timescaledb.compress_segmentby = 'device_id',
    timescaledb.compress_orderby = 'ts DESC');
SELECT add_compression_policy('neteco.battery_group_metrics', INTERVAL '14 days',
    if_not_exists => TRUE);

-- ------------------------------------------------------------------ mains (60001)
CREATE TABLE IF NOT EXISTS neteco.mains_metrics (
    ts                      TIMESTAMPTZ NOT NULL,
    device_id               BIGINT NOT NULL,
    station_code            TEXT NOT NULL,
    mains_state             INT,                    -- 10001 (0=Off,1=On)
    ac_voltage_v            DOUBLE PRECISION,       -- 10002
    phase_l1_v              DOUBLE PRECISION,       -- 10003
    phase_l2_v              DOUBLE PRECISION,       -- 10004
    phase_l3_v              DOUBLE PRECISION,       -- 10005
    ac_current_a            DOUBLE PRECISION,       -- 10006
    phase_l1_a              DOUBLE PRECISION,       -- 10007
    phase_l2_a              DOUBLE PRECISION,       -- 10008
    phase_l3_a              DOUBLE PRECISION,       -- 10009
    active_power_kw         DOUBLE PRECISION,       -- 10010
    ac_freq_hz              DOUBLE PRECISION,       -- 10011
    power_factor            DOUBLE PRECISION,       -- 10021
    total_energy_kwh        DOUBLE PRECISION,       -- 10016
    grid_quality_grade      INT,                    -- 10020
    source                  TEXT NOT NULL DEFAULT 'nbi_rest',
    PRIMARY KEY (device_id, ts)
);
SELECT create_hypertable('neteco.mains_metrics', 'ts',
    chunk_time_interval => INTERVAL '7 days',
    if_not_exists => TRUE);
ALTER TABLE neteco.mains_metrics SET (
    timescaledb.compress,
    timescaledb.compress_segmentby = 'device_id',
    timescaledb.compress_orderby = 'ts DESC');
SELECT add_compression_policy('neteco.mains_metrics', INTERVAL '14 days',
    if_not_exists => TRUE);

-- ------------------------------------------------------------------ ac_input (60013)
CREATE TABLE IF NOT EXISTS neteco.ac_input_metrics (
    ts                      TIMESTAMPTZ NOT NULL,
    device_id               BIGINT NOT NULL,
    station_code            TEXT NOT NULL,
    ac_input_state          INT,                    -- 10013 (0=Failure,1=Normal)
    phase_l1_v              DOUBLE PRECISION,
    phase_l2_v              DOUBLE PRECISION,
    phase_l3_v              DOUBLE PRECISION,
    phase_l1_a              DOUBLE PRECISION,
    phase_l2_a              DOUBLE PRECISION,
    phase_l3_a              DOUBLE PRECISION,
    ac_freq_hz              DOUBLE PRECISION,
    active_power_kw         DOUBLE PRECISION,
    apparent_power_kva      DOUBLE PRECISION,
    power_factor            DOUBLE PRECISION,
    total_energy_kwh        DOUBLE PRECISION,
    agregat_u_radu          INT,                    -- custom DI: generator running (0/1)
    source                  TEXT NOT NULL DEFAULT 'nbi_rest',
    PRIMARY KEY (device_id, ts)
);
SELECT create_hypertable('neteco.ac_input_metrics', 'ts',
    chunk_time_interval => INTERVAL '7 days',
    if_not_exists => TRUE);
ALTER TABLE neteco.ac_input_metrics SET (
    timescaledb.compress,
    timescaledb.compress_segmentby = 'device_id',
    timescaledb.compress_orderby = 'ts DESC');
SELECT add_compression_policy('neteco.ac_input_metrics', INTERVAL '14 days',
    if_not_exists => TRUE);

-- ------------------------------------------------------------------ genset (60003)
CREATE TABLE IF NOT EXISTS neteco.genset_metrics (
    ts                          TIMESTAMPTZ NOT NULL,
    device_id                   BIGINT NOT NULL,
    station_code                TEXT NOT NULL,
    running_state               INT,                -- 10003 (0=Unknown,1=Stopped,2=Running)
    load_rate_pct               DOUBLE PRECISION,   -- 10007
    cabin_temp_c                DOUBLE PRECISION,   -- 10008
    coolant_temp_c              DOUBLE PRECISION,   -- 10017
    oil_pressure_bar            DOUBLE PRECISION,   -- 10014
    oil_level_pct               INT,                -- 10015
    rotation_speed_rpm          INT,                -- 10011
    output_power_kw             DOUBLE PRECISION,   -- 10018
    ac_freq_hz                  DOUBLE PRECISION,   -- 10019
    phase_l1_v                  DOUBLE PRECISION,
    phase_l2_v                  DOUBLE PRECISION,
    phase_l3_v                  DOUBLE PRECISION,
    phase_l1_a                  DOUBLE PRECISION,
    phase_l2_a                  DOUBLE PRECISION,
    phase_l3_a                  DOUBLE PRECISION,
    total_runtime_h             DOUBLE PRECISION,   -- 10004
    total_fuel_l                DOUBLE PRECISION,   -- 10005
    estimated_runtime_h         DOUBLE PRECISION,   -- 10040
    total_energy_yield_kwh      DOUBLE PRECISION,   -- 10010
    genset_battery_voltage_v    DOUBLE PRECISION,   -- 10013
    source                      TEXT NOT NULL DEFAULT 'nbi_rest',
    PRIMARY KEY (device_id, ts)
);
SELECT create_hypertable('neteco.genset_metrics', 'ts',
    chunk_time_interval => INTERVAL '7 days',
    if_not_exists => TRUE);
ALTER TABLE neteco.genset_metrics SET (
    timescaledb.compress,
    timescaledb.compress_segmentby = 'device_id',
    timescaledb.compress_orderby = 'ts DESC');
SELECT add_compression_policy('neteco.genset_metrics', INTERVAL '14 days',
    if_not_exists => TRUE);

-- ------------------------------------------------------------------ rectifier_group (60039)
CREATE TABLE IF NOT EXISTS neteco.rectifier_group_metrics (
    ts                          TIMESTAMPTZ NOT NULL,
    device_id                   BIGINT NOT NULL,
    station_code                TEXT NOT NULL,
    qty_rectifiers              INT,                -- 10001
    total_dc_output_current_a   DOUBLE PRECISION,   -- 10002
    total_dc_output_power_kw    DOUBLE PRECISION,   -- 10009
    load_usage_rate_pct         DOUBLE PRECISION,   -- 10010
    output_voltage_v            DOUBLE PRECISION,   -- 10011
    total_input_power_kw        DOUBLE PRECISION,   -- 10012
    total_input_energy_kwh      DOUBLE PRECISION,   -- 10003
    source                      TEXT NOT NULL DEFAULT 'nbi_rest',
    PRIMARY KEY (device_id, ts)
);
SELECT create_hypertable('neteco.rectifier_group_metrics', 'ts',
    chunk_time_interval => INTERVAL '7 days',
    if_not_exists => TRUE);
ALTER TABLE neteco.rectifier_group_metrics SET (
    timescaledb.compress,
    timescaledb.compress_segmentby = 'device_id',
    timescaledb.compress_orderby = 'ts DESC');
SELECT add_compression_policy('neteco.rectifier_group_metrics', INTERVAL '14 days',
    if_not_exists => TRUE);

-- ================================================================== ALARMS
CREATE TABLE IF NOT EXISTS neteco.alarms (
    alarm_id            TEXT PRIMARY KEY,
    station_code        TEXT,
    station_name        TEXT,
    device_id           BIGINT,
    dev_name            TEXT,
    dev_type_id         INT,
    std_type_id         INT,
    std_type_name       TEXT,
    alarm_name          TEXT,
    alarm_cause         TEXT,
    alarm_id_number     INT,
    alarm_type          SMALLINT,       -- 1=signal 2=exception 3=protection
    severity            SMALLINT,       -- 1=critical 2=major 3=minor 4=warning
    status              SMALLINT,       -- 1=active 2=acked 4=handled 5=user-clear 6=auto-clear
    raise_time          TIMESTAMPTZ,
    repair_time         TIMESTAMPTZ,
    source              TEXT NOT NULL,  -- 'snmp' | 'nbi_rest'
    first_seen          TIMESTAMPTZ DEFAULT now(),
    last_seen           TIMESTAMPTZ DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_neteco_alarms_station ON neteco.alarms(station_code);
CREATE INDEX IF NOT EXISTS idx_neteco_alarms_active  ON neteco.alarms(severity) WHERE status = 1;
CREATE INDEX IF NOT EXISTS idx_neteco_alarms_raise   ON neteco.alarms(raise_time DESC);

-- ================================================================== CONTINUOUS AGGREGATES

-- 5-min rollup: site_unit dashboard charts
CREATE MATERIALIZED VIEW IF NOT EXISTS neteco.site_unit_5m
WITH (timescaledb.continuous) AS
SELECT
    time_bucket('5 minutes', ts) AS bucket,
    device_id, station_code,
    AVG(indoor_temp_c)          AS avg_indoor_temp_c,
    MAX(indoor_temp_c)          AS max_indoor_temp_c,
    AVG(outdoor_temp_c)         AS avg_outdoor_temp_c,
    AVG(indoor_humidity_pct)    AS avg_humidity_pct,
    AVG(ac_input_power_kw)      AS avg_ac_power_kw,
    AVG(dc_output_power_kw)     AS avg_dc_power_kw
FROM neteco.site_unit_metrics
GROUP BY 1, 2, 3
WITH NO DATA;

SELECT add_continuous_aggregate_policy('neteco.site_unit_5m',
    start_offset => INTERVAL '1 day',
    end_offset   => INTERVAL '5 minutes',
    schedule_interval => INTERVAL '5 minutes',
    if_not_exists => TRUE);

-- 5-min rollup: battery SOC trend
CREATE MATERIALIZED VIEW IF NOT EXISTS neteco.battery_5m
WITH (timescaledb.continuous) AS
SELECT
    time_bucket('5 minutes', ts) AS bucket,
    device_id, station_code,
    AVG(soc_pct)                    AS avg_soc_pct,
    MIN(soc_pct)                    AS min_soc_pct,
    LAST(soc_pct, ts)               AS last_soc_pct,
    LAST(battery_state, ts)         AS last_state,
    AVG(backup_time_h)              AS avg_backup_time_h,
    AVG(charge_discharge_power_kw)  AS avg_cd_power_kw
FROM neteco.battery_group_metrics
GROUP BY 1, 2, 3
WITH NO DATA;

SELECT add_continuous_aggregate_policy('neteco.battery_5m',
    start_offset => INTERVAL '1 day',
    end_offset   => INTERVAL '5 minutes',
    schedule_interval => INTERVAL '5 minutes',
    if_not_exists => TRUE);

-- ================================================================== DONE
-- Apply: sudo -u bht psql -d alarms -f migrate_neteco_v1.sql
-- Verify: \dt neteco.*
--         SELECT hypertable_name FROM timescaledb_information.hypertables
--           WHERE hypertable_schema = 'neteco';
