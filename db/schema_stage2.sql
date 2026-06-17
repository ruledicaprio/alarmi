-- =====================================================================
-- BHT Alarm Pipeline — Stage 2 schema additions (Modbus telemetry)
-- ADDITIVE: does not modify any Stage-1 object. Run after schema.sql.
-- Modbus alarm EVENTS reuse fact_event (source='modbus_eaton'); this file
-- adds the time-series MEASUREMENT tier (voltage, power, battery, energy...).
-- =====================================================================

-- Narrow (long) measurement model: one row per (device, metric, time).
-- Flexible — new metrics need no DDL. site_key joins dim_site.
CREATE TABLE IF NOT EXISTS fact_measurement (
    ts         TIMESTAMPTZ      NOT NULL,
    site_key   TEXT             NOT NULL,
    device_ip  INET,
    metric     TEXT             NOT NULL,   -- e.g. u_battery_v, p_load_kw, ac_voltage_v
    value      DOUBLE PRECISION,
    ingest_time TIMESTAMPTZ     NOT NULL DEFAULT now()
);
SELECT create_hypertable('fact_measurement','ts',
       chunk_time_interval => INTERVAL '1 day', if_not_exists => TRUE);

CREATE INDEX IF NOT EXISTS ix_meas_site_metric_ts ON fact_measurement (site_key, metric, ts DESC);
CREATE INDEX IF NOT EXISTS ix_meas_metric_ts      ON fact_measurement (metric, ts DESC);

ALTER TABLE fact_measurement SET (
    timescaledb.compress,
    timescaledb.compress_segmentby = 'site_key, metric',
    timescaledb.compress_orderby   = 'ts DESC'
);
SELECT add_compression_policy('fact_measurement', INTERVAL '7 days',  if_not_exists => TRUE);
SELECT add_retention_policy  ('fact_measurement', INTERVAL '90 days', if_not_exists => TRUE);

-- Daily min/avg/max rollup = the multi-year performance-history tier.
CREATE MATERIALIZED VIEW IF NOT EXISTS cagg_measurement_daily
WITH (timescaledb.continuous) AS
SELECT time_bucket('1 day', ts) AS bucket,
       site_key, metric,
       avg(value) AS avg_value,
       min(value) AS min_value,
       max(value) AS max_value,
       count(*)   AS samples
FROM fact_measurement
GROUP BY bucket, site_key, metric
WITH NO DATA;

SELECT add_continuous_aggregate_policy('cagg_measurement_daily',
       start_offset => INTERVAL '10 days', end_offset => INTERVAL '1 hour',
       schedule_interval => INTERVAL '1 hour', if_not_exists => TRUE);
SELECT add_retention_policy('cagg_measurement_daily', INTERVAL '1825 days', if_not_exists => TRUE);

-- Latest value per device/metric (dashboard tiles).
CREATE OR REPLACE VIEW v_latest_measurement AS
SELECT DISTINCT ON (site_key, metric)
       site_key, device_ip, metric, value, ts
FROM fact_measurement
ORDER BY site_key, metric, ts DESC;
