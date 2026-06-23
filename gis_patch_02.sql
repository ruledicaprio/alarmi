-- v_active_alarms_geo: active alarms + site GIS data
-- Wraps existing v_active_alarms, no changes to underlying schema
CREATE OR REPLACE VIEW v_active_alarms_geo AS
SELECT
    a.site_key,
    a.source,
    a.alarm_class,
    a.severity,
    a.raised_at,
    a.open_minutes,
    ds.display_name,
    ds.region,
    ds.municipality,
    ds.latitude,
    ds.longitude,
    ds.technologies,
    ds.has_genset,
    ds.has_battery,
    ds.has_solar,
    ds.is_important
FROM v_active_alarms a
LEFT JOIN dim_site ds ON ds.site_key = a.site_key;
