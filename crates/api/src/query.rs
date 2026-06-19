//! Read endpoints for the ant.design dashboard. Timestamps are cast to ::text
//! and numerics to ::float8 in SQL so no extra tokio-postgres type features are
//! needed. All views/tables come from Stage-1 + Stage-2 schema.

use crate::{ApiResult, AppState};
use axum::extract::{Path, Query, State};
use axum::Json;
use serde_json::{json, Value};
use std::collections::HashMap;

fn hours_of(q: &HashMap<String, String>, default: i32) -> i32 {
    q.get("hours").and_then(|s| s.parse().ok()).unwrap_or(default)
}
fn i64_of(q: &HashMap<String, String>, k: &str, default: i64) -> i64 {
    q.get(k).and_then(|s| s.parse().ok()).unwrap_or(default)
}
fn str_of(q: &HashMap<String, String>, k: &str) -> String {
    q.get(k).cloned().unwrap_or_default()
}

pub async fn health(State(st): State<AppState>) -> ApiResult {
    let c = st.pool.get().await?;
    let row = c.query_one("SELECT now()::text AS ts", &[]).await?;
    Ok(Json(json!({ "status": "ok", "db_time": row.get::<_, String>("ts") })))
}

// ---------------------------------------------------------------- sites
// Supports server-side filter/page so ProTable can paginate over 3k sites.
pub async fn sites(State(st): State<AppState>, Query(q): Query<HashMap<String, String>>) -> ApiResult {
    let query   = str_of(&q, "q");
    let region  = str_of(&q, "region");
    let min_open: i64 = i64_of(&q, "min_open", 0);
    let limit:    i64 = i64_of(&q, "limit",   100).clamp(1, 5000);
    let offset:   i64 = i64_of(&q, "offset",    0).max(0);
    let c = st.pool.get().await?;

    let total_row = c.query_one(
        "SELECT count(*)::int8 n FROM dim_site s \
         LEFT JOIN (SELECT site_key, count(*) open FROM fact_alarm_episode WHERE is_open GROUP BY 1) a \
           USING (site_key) \
         WHERE ($1 = '' OR site_key ILIKE '%'||$1||'%' OR COALESCE(display_name,'') ILIKE '%'||$1||'%') \
           AND ($2 = '' OR region = $2) \
           AND COALESCE(a.open,0) >= $3",
        &[&query, &region, &min_open]).await?;
    let total: i64 = total_row.get("n");

    let rows = c.query(
        "SELECT s.site_key, COALESCE(s.display_name,'') name, COALESCE(s.region,'') region, \
                COALESCE(s.municipality,'') municipality, \
                COALESCE(a.open,0)::int8 open_alarms, \
                COALESCE(le.t::text, '') last_event \
         FROM dim_site s \
         LEFT JOIN (SELECT site_key, count(*) open FROM fact_alarm_episode WHERE is_open GROUP BY 1) a \
           USING (site_key) \
         LEFT JOIN (SELECT site_key, MAX(event_time) t FROM fact_event GROUP BY 1) le \
           USING (site_key) \
         WHERE ($1 = '' OR site_key ILIKE '%'||$1||'%' OR COALESCE(display_name,'') ILIKE '%'||$1||'%') \
           AND ($2 = '' OR region = $2) \
           AND COALESCE(a.open,0) >= $3 \
         ORDER BY open_alarms DESC, site_key \
         LIMIT $4 OFFSET $5",
        &[&query, &region, &min_open, &limit, &offset]).await?;

    let items: Vec<Value> = rows.iter().map(|r| json!({
        "site_key":     r.get::<_, String>("site_key"),
        "name":         r.get::<_, String>("name"),
        "region":       r.get::<_, String>("region"),
        "municipality": r.get::<_, String>("municipality"),
        "open_alarms":  r.get::<_, i64>("open_alarms"),
        "last_event":   r.get::<_, String>("last_event"),
    })).collect();
    Ok(Json(json!({ "count": items.len(), "total": total, "items": items })))
}

pub async fn active_alarms(State(st): State<AppState>) -> ApiResult {
    let c = st.pool.get().await?;
    let rows = c.query(
        "SELECT site_key, source::text source, alarm_class::text alarm_class, severity::text severity, \
                raised_at::text raised_at, open_minutes::float8 open_minutes \
         FROM v_active_alarms ORDER BY raised_at LIMIT 500", &[]).await?;
    let items: Vec<Value> = rows.iter().map(|r| json!({
        "site_key":     r.get::<_, String>("site_key"),
        "source":       r.get::<_, String>("source"),
        "alarm_class":  r.get::<_, String>("alarm_class"),
        "severity":     r.get::<_, String>("severity"),
        "raised_at":    r.get::<_, String>("raised_at"),
        "open_minutes": r.get::<_, f64>("open_minutes"),
    })).collect();
    Ok(Json(json!({ "count": items.len(), "items": items })))
}

// ---------------------------------------------------------------- recent_alarms
// Server-side pagination + extended filters. Returns {items, total} for ProTable.
pub async fn recent_alarms(State(st): State<AppState>, Query(q): Query<HashMap<String, String>>) -> ApiResult {
    let hours    = hours_of(&q, 24);
    let site     = str_of(&q, "site");
    let class    = str_of(&q, "class");
    let source   = str_of(&q, "source");
    let severity = str_of(&q, "severity");
    let trans    = str_of(&q, "transition");
    let raw_like = str_of(&q, "raw_alarm_like");
    let limit:  i64 = i64_of(&q, "limit",  50).clamp(1, 1000);
    let offset: i64 = i64_of(&q, "offset",  0).max(0);
    let c = st.pool.get().await?;

    let where_sql = "WHERE event_time >= now() - make_interval(hours => $1) \
                       AND ($2 = '' OR site_key = $2) \
                       AND ($3 = '' OR alarm_class::text = $3) \
                       AND ($4 = '' OR source::text = $4) \
                       AND ($5 = '' OR severity::text = $5) \
                       AND ($6 = '' OR transition::text = $6) \
                       AND ($7 = '' OR raw_alarm ILIKE '%'||$7||'%')";

    let total_row = c.query_one(
        &format!("SELECT count(*)::int8 n FROM fact_event {where_sql}"),
        &[&hours, &site, &class, &source, &severity, &trans, &raw_like]).await?;
    let total: i64 = total_row.get("n");

    let sql = format!(
        "SELECT event_time::text et, source::text src, site_key, alarm_class::text ac, \
                severity::text sv, transition::text tr, COALESCE(raw_alarm,'') ra, \
                COALESCE(host(device_ip),'') ip, \
                COALESCE((SELECT region FROM dim_site d WHERE d.site_key = fact_event.site_key),'') region \
         FROM fact_event {where_sql} \
         ORDER BY event_time DESC LIMIT $8 OFFSET $9");
    let rows = c.query(&sql,
        &[&hours, &site, &class, &source, &severity, &trans, &raw_like, &limit, &offset]).await?;
    let items: Vec<Value> = rows.iter().map(|r| json!({
        "event_time":  r.get::<_, String>("et"),
        "source":      r.get::<_, String>("src"),
        "site_key":    r.get::<_, String>("site_key"),
        "alarm_class": r.get::<_, String>("ac"),
        "severity":    r.get::<_, String>("sv"),
        "transition":  r.get::<_, String>("tr"),
        "raw_alarm":   r.get::<_, String>("ra"),
        "device_ip":   r.get::<_, String>("ip"),
        "region":      r.get::<_, String>("region"),
    })).collect();
    Ok(Json(json!({ "count": items.len(), "total": total, "items": items })))
}

pub async fn site_reliability(State(st): State<AppState>, Path(site_key): Path<String>) -> ApiResult {
    let c = st.pool.get().await?;
    let rows = c.query(
        "SELECT episodes, open_now, outage_hours::float8 oh, COALESCE(avg_minutes,0)::float8 am \
         FROM v_site_reliability_30d WHERE site_key = $1", &[&site_key]).await?;
    let v = match rows.first() {
        Some(r) => json!({
            "site_key":     site_key,
            "episodes":     r.get::<_, i64>("episodes"),
            "open_now":     r.get::<_, i64>("open_now"),
            "outage_hours": r.get::<_, f64>("oh"),
            "avg_minutes":  r.get::<_, f64>("am"),
        }),
        None => json!({ "site_key": site_key, "episodes": 0, "open_now": 0, "outage_hours": 0.0, "avg_minutes": 0.0 }),
    };
    Ok(Json(v))
}

pub async fn site_measurements(State(st): State<AppState>, Path(site_key): Path<String>, Query(q): Query<HashMap<String, String>>) -> ApiResult {
    let metric = str_of(&q, "metric");
    let hours  = hours_of(&q, 24);
    let c = st.pool.get().await?;
    let rows = c.query(
        "SELECT ts::text ts, value::float8 v FROM fact_measurement \
         WHERE site_key = $1 AND metric = $2 AND ts >= now() - make_interval(hours => $3) \
         ORDER BY ts LIMIT 5000",
        &[&site_key, &metric, &hours]).await?;
    let series: Vec<Value> = rows.iter().map(|r| json!({
        "ts": r.get::<_, String>("ts"), "value": r.get::<_, f64>("v"),
    })).collect();
    Ok(Json(json!({ "site_key": site_key, "metric": metric, "points": series.len(), "series": series })))
}

pub async fn latest_measurements(State(st): State<AppState>, Query(q): Query<HashMap<String, String>>) -> ApiResult {
    let site = str_of(&q, "site");
    let c = st.pool.get().await?;
    let rows = c.query(
        "SELECT site_key, COALESCE(host(device_ip),'') ip, metric, value::float8 v, ts::text ts \
         FROM v_latest_measurement WHERE ($1 = '' OR site_key = $1) ORDER BY site_key, metric",
        &[&site]).await?;
    let items: Vec<Value> = rows.iter().map(|r| json!({
        "site_key":  r.get::<_, String>("site_key"),
        "device_ip": r.get::<_, String>("ip"),
        "metric":    r.get::<_, String>("metric"),
        "value":     r.get::<_, f64>("v"),
        "ts":        r.get::<_, String>("ts"),
    })).collect();
    Ok(Json(json!({ "count": items.len(), "items": items })))
}

pub async fn stats_by_class(State(st): State<AppState>, Query(q): Query<HashMap<String, String>>) -> ApiResult {
    let hours = hours_of(&q, 24);
    let c = st.pool.get().await?;
    let rows = c.query(
        "SELECT alarm_class::text ac, count(*)::int8 n FROM fact_event \
         WHERE event_time >= now() - make_interval(hours => $1) GROUP BY 1 ORDER BY 2 DESC",
        &[&hours]).await?;
    let items: Vec<Value> = rows.iter().map(|r| json!({
        "alarm_class": r.get::<_, String>("ac"), "count": r.get::<_, i64>("n"),
    })).collect();
    Ok(Json(json!({ "hours": hours, "items": items })))
}

pub async fn stats_by_region(State(st): State<AppState>, Query(q): Query<HashMap<String, String>>) -> ApiResult {
    let hours = hours_of(&q, 24);
    let c = st.pool.get().await?;
    let rows = c.query(
        "SELECT COALESCE(s.region,'?') region, count(*)::int8 n \
         FROM fact_event e LEFT JOIN dim_site s USING (site_key) \
         WHERE e.event_time >= now() - make_interval(hours => $1) GROUP BY 1 ORDER BY 2 DESC",
        &[&hours]).await?;
    let items: Vec<Value> = rows.iter().map(|r| json!({
        "region": r.get::<_, String>("region"), "count": r.get::<_, i64>("n"),
    })).collect();
    Ok(Json(json!({ "hours": hours, "items": items })))
}

// ============================================================ NEW ENDPOINTS

// ---- /api/sites/:site_key/timeline?hours= — combined events at one site
pub async fn site_timeline(State(st): State<AppState>, Path(site_key): Path<String>, Query(q): Query<HashMap<String, String>>) -> ApiResult {
    let hours = hours_of(&q, 168);
    let limit: i64 = i64_of(&q, "limit", 500).clamp(1, 5000);
    let c = st.pool.get().await?;
    let rows = c.query(
        "SELECT event_time::text et, source::text src, alarm_class::text ac, \
                severity::text sv, transition::text tr, COALESCE(raw_alarm,'') ra, \
                COALESCE(host(device_ip),'') ip \
         FROM fact_event \
         WHERE site_key = $1 AND event_time >= now() - make_interval(hours => $2) \
         ORDER BY event_time DESC LIMIT $3",
        &[&site_key, &hours, &limit]).await?;
    let items: Vec<Value> = rows.iter().map(|r| json!({
        "event_time":  r.get::<_, String>("et"),
        "source":      r.get::<_, String>("src"),
        "alarm_class": r.get::<_, String>("ac"),
        "severity":    r.get::<_, String>("sv"),
        "transition":  r.get::<_, String>("tr"),
        "raw_alarm":   r.get::<_, String>("ra"),
        "device_ip":   r.get::<_, String>("ip"),
    })).collect();
    Ok(Json(json!({ "site_key": site_key, "count": items.len(), "items": items })))
}

// ---- /api/sites/:site_key/episodes — paired raise->clear durations
pub async fn site_episodes(State(st): State<AppState>, Path(site_key): Path<String>) -> ApiResult {
    let c = st.pool.get().await?;
    let rows = c.query(
        "SELECT raised_at::text ra, COALESCE(cleared_at::text,'') cl, \
                COALESCE(duration_seconds,0)::float8 dur, is_open, \
                source::text src, alarm_class::text ac, severity::text sv \
         FROM fact_alarm_episode \
         WHERE site_key = $1 ORDER BY raised_at DESC LIMIT 200",
        &[&site_key]).await?;
    let items: Vec<Value> = rows.iter().map(|r| json!({
        "raised_at":  r.get::<_, String>("ra"),
        "cleared_at": r.get::<_, String>("cl"),
        "duration_seconds": r.get::<_, f64>("dur"),
        "is_open":    r.get::<_, bool>("is_open"),
        "source":     r.get::<_, String>("src"),
        "alarm_class":r.get::<_, String>("ac"),
        "severity":   r.get::<_, String>("sv"),
    })).collect();
    Ok(Json(json!({ "site_key": site_key, "count": items.len(), "items": items })))
}

// ---- /api/inventory/orphans — site_keys in events that aren't in dim_site
pub async fn inventory_orphans(State(st): State<AppState>) -> ApiResult {
    let c = st.pool.get().await?;
    let rows = c.query(
        "SELECT e.site_key, count(*)::int8 events, MAX(event_time)::text last_seen \
         FROM fact_event e \
         LEFT JOIN dim_site s USING (site_key) \
         WHERE s.site_key IS NULL \
         GROUP BY e.site_key \
         ORDER BY events DESC LIMIT 500", &[]).await?;
    let items: Vec<Value> = rows.iter().map(|r| json!({
        "site_key":  r.get::<_, String>("site_key"),
        "events":    r.get::<_, i64>("events"),
        "last_seen": r.get::<_, String>("last_seen"),
    })).collect();
    Ok(Json(json!({ "count": items.len(), "items": items })))
}

// ---- /api/inventory/stale?days= — dim_site rows with no events in window
pub async fn inventory_stale(State(st): State<AppState>, Query(q): Query<HashMap<String, String>>) -> ApiResult {
    let days: i64 = i64_of(&q, "days", 30).clamp(1, 365);
    let c = st.pool.get().await?;
    let rows = c.query(
        "WITH active AS (SELECT DISTINCT site_key FROM fact_event \
                          WHERE event_time >= now() - make_interval(days => $1)) \
         SELECT s.site_key, COALESCE(s.display_name,'') name, COALESCE(s.region,'') region, \
                COALESCE((SELECT MAX(event_time)::text FROM fact_event WHERE site_key = s.site_key),'') last_event \
         FROM dim_site s \
         LEFT JOIN active a USING (site_key) \
         WHERE a.site_key IS NULL \
         ORDER BY s.region, s.site_key LIMIT 1000",
        &[&days]).await?;
    let items: Vec<Value> = rows.iter().map(|r| json!({
        "site_key":   r.get::<_, String>("site_key"),
        "name":       r.get::<_, String>("name"),
        "region":     r.get::<_, String>("region"),
        "last_event": r.get::<_, String>("last_event"),
    })).collect();
    Ok(Json(json!({ "days": days, "count": items.len(), "items": items })))
}

// ---- /api/inventory/coverage — per-region: sites total / with events / open
pub async fn inventory_coverage(State(st): State<AppState>) -> ApiResult {
    let c = st.pool.get().await?;
    let rows = c.query(
        "WITH active AS (SELECT DISTINCT site_key FROM fact_event), \
              open AS   (SELECT DISTINCT site_key FROM fact_alarm_episode WHERE is_open) \
         SELECT COALESCE(s.region,'(no region)') region, \
                count(*)::int8 sites, \
                count(*) FILTER (WHERE a.site_key IS NOT NULL)::int8 sites_with_events, \
                count(*) FILTER (WHERE o.site_key IS NOT NULL)::int8 sites_with_open_alarms \
         FROM dim_site s \
         LEFT JOIN active a USING (site_key) \
         LEFT JOIN open o   USING (site_key) \
         GROUP BY 1 ORDER BY 1", &[]).await?;
    let items: Vec<Value> = rows.iter().map(|r| json!({
        "region":                 r.get::<_, String>("region"),
        "sites":                  r.get::<_, i64>("sites"),
        "sites_with_events":      r.get::<_, i64>("sites_with_events"),
        "sites_with_open_alarms": r.get::<_, i64>("sites_with_open_alarms"),
    })).collect();
    Ok(Json(json!({ "items": items })))
}

// ---- /api/stats/sources — per-source last-ingest + 24h volume
pub async fn stats_sources(State(st): State<AppState>) -> ApiResult {
    let c = st.pool.get().await?;
    let rows = c.query(
        "SELECT source::text src, count(*)::int8 events_24h, \
                MAX(event_time)::text last_event, \
                MAX(ingest_time)::text last_ingest \
         FROM fact_event \
         WHERE event_time >= now() - INTERVAL '24 hours' \
         GROUP BY source ORDER BY events_24h DESC", &[]).await?;
    let items: Vec<Value> = rows.iter().map(|r| json!({
        "source":      r.get::<_, String>("src"),
        "events_24h":  r.get::<_, i64>("events_24h"),
        "last_event":  r.get::<_, String>("last_event"),
        "last_ingest": r.get::<_, String>("last_ingest"),
    })).collect();
    Ok(Json(json!({ "count": items.len(), "items": items })))
}

// ---- /api/stats/timeseries?bucket=hour|day&hours= — for spark/area charts
pub async fn stats_timeseries(State(st): State<AppState>, Query(q): Query<HashMap<String, String>>) -> ApiResult {
    let hours = hours_of(&q, 168);
    let bucket = match str_of(&q, "bucket").as_str() {
        "day"  => "1 day",
        _      => "1 hour",
    };
    let c = st.pool.get().await?;
    let sql = format!(
        "SELECT time_bucket(INTERVAL '{bucket}', event_time)::text bucket, \
                count(*)::int8 n, \
                count(*) FILTER (WHERE severity::text='critical')::int8 critical, \
                count(*) FILTER (WHERE severity::text='major')::int8    major, \
                count(*) FILTER (WHERE severity::text IN ('minor','warning','info'))::int8 other \
         FROM fact_event \
         WHERE event_time >= now() - make_interval(hours => $1) \
         GROUP BY bucket ORDER BY bucket");
    let rows = c.query(&sql, &[&hours]).await?;
    let items: Vec<Value> = rows.iter().map(|r| json!({
        "bucket":   r.get::<_, String>("bucket"),
        "n":        r.get::<_, i64>("n"),
        "critical": r.get::<_, i64>("critical"),
        "major":    r.get::<_, i64>("major"),
        "other":    r.get::<_, i64>("other"),
    })).collect();
    Ok(Json(json!({ "bucket": bucket, "hours": hours, "items": items })))
}

// ---- /api/regions — distinct region list for filter UI
pub async fn regions(State(st): State<AppState>) -> ApiResult {
    let c = st.pool.get().await?;
    let rows = c.query(
        "SELECT DISTINCT region FROM dim_site WHERE region IS NOT NULL AND region <> '' ORDER BY region",
        &[]).await?;
    let items: Vec<String> = rows.iter().map(|r| r.get::<_, String>("region")).collect();
    Ok(Json(json!({ "items": items })))
}

// ---- /api/measurements/metrics — distinct metrics with coverage info
pub async fn measurement_metrics(State(st): State<AppState>) -> ApiResult {
    let c = st.pool.get().await?;
    let rows = c.query(
        "SELECT metric, count(*)::int8 n, count(DISTINCT site_key)::int8 sites, \
                COALESCE(MAX(ts)::text,'') last_seen \
         FROM fact_measurement GROUP BY metric ORDER BY n DESC", &[]).await?;
    let items: Vec<Value> = rows.iter().map(|r| json!({
        "metric": r.get::<_, String>("metric"),
        "n":      r.get::<_, i64>("n"),
        "sites":  r.get::<_, i64>("sites"),
        "last_seen": r.get::<_, String>("last_seen"),
    })).collect();
    Ok(Json(json!({ "items": items })))
}

// ---- /api/solar/summary?hours=  — aggregated solar PV stats
pub async fn solar_summary(State(st): State<AppState>, Query(q): Query<HashMap<String, String>>) -> ApiResult {
    let hours = hours_of(&q, 24);
    let c = st.pool.get().await?;

    let head = c.query_one(
        "WITH latest AS ( \
           SELECT DISTINCT ON (site_key) site_key, value \
           FROM fact_measurement \
           WHERE metric = 'p_solar_kw' AND ts >= now() - INTERVAL '1 hour' \
           ORDER BY site_key, ts DESC) \
         SELECT count(*)::int8 sites_active, \
                COALESCE(SUM(value),0)::float8 total_power_kw_now \
         FROM latest WHERE value > 0", &[]).await?;
    let sites_active:  i64 = head.get("sites_active");
    let total_power:   f64 = head.get("total_power_kw_now");

    let top = c.query(
        "WITH latest AS ( \
           SELECT DISTINCT ON (site_key) site_key, value, ts \
           FROM fact_measurement \
           WHERE metric = 'p_solar_kw' AND ts >= now() - INTERVAL '1 hour' \
           ORDER BY site_key, ts DESC) \
         SELECT latest.site_key, latest.value::float8 power_kw, \
                COALESCE(s.region,'') region, COALESCE(s.display_name,'') name \
         FROM latest LEFT JOIN dim_site s USING (site_key) \
         WHERE latest.value > 0 ORDER BY latest.value DESC LIMIT 10", &[]).await?;
    let top_items: Vec<Value> = top.iter().map(|r| json!({
        "site_key": r.get::<_, String>("site_key"),
        "name":     r.get::<_, String>("name"),
        "region":   r.get::<_, String>("region"),
        "power_kw": r.get::<_, f64>("power_kw"),
    })).collect();

    let series = c.query(
        "SELECT time_bucket(INTERVAL '1 hour', ts)::text bucket, \
                AVG(value)::float8 avg_kw, MAX(value)::float8 max_kw, \
                count(DISTINCT site_key)::int8 sites \
         FROM fact_measurement \
         WHERE metric = 'p_solar_kw' AND ts >= now() - make_interval(hours => $1) \
         GROUP BY bucket ORDER BY bucket",
        &[&hours]).await?;
    let series_items: Vec<Value> = series.iter().map(|r| json!({
        "bucket": r.get::<_, String>("bucket"),
        "avg_kw": r.get::<_, f64>("avg_kw"),
        "max_kw": r.get::<_, f64>("max_kw"),
        "sites":  r.get::<_, i64>("sites"),
    })).collect();

    Ok(Json(json!({
        "hours": hours,
        "sites_active_now":     sites_active,
        "total_power_kw_now":   total_power,
        "top_producers":        top_items,
        "timeseries":           series_items,
    })))
}

// ---- /api/solar/sites?hours= — per-site solar latest + delta energy
pub async fn solar_sites(State(st): State<AppState>, Query(q): Query<HashMap<String, String>>) -> ApiResult {
    let hours = hours_of(&q, 24);
    let c = st.pool.get().await?;
    let rows = c.query(
        "WITH latest_p AS ( \
           SELECT DISTINCT ON (site_key) site_key, value p, ts \
           FROM fact_measurement \
           WHERE metric = 'p_solar_kw' AND ts >= now() - INTERVAL '24 hours' \
           ORDER BY site_key, ts DESC), \
         energy AS ( \
           SELECT site_key, GREATEST((MAX(value) - MIN(value))::float8, 0) dkwh \
           FROM fact_measurement \
           WHERE metric = 'e_total_kwh' AND ts >= now() - make_interval(hours => $1) \
           GROUP BY site_key) \
         SELECT lp.site_key, lp.p::float8 power_kw, lp.ts::text last_ts, \
                COALESCE(e.dkwh, 0)::float8 energy_kwh, \
                COALESCE(s.region,'') region, COALESCE(s.display_name,'') name \
         FROM latest_p lp \
         LEFT JOIN energy   e USING (site_key) \
         LEFT JOIN dim_site s USING (site_key) \
         WHERE lp.p > 0 \
         ORDER BY lp.p DESC",
        &[&hours]).await?;
    let items: Vec<Value> = rows.iter().map(|r| json!({
        "site_key":  r.get::<_, String>("site_key"),
        "name":      r.get::<_, String>("name"),
        "region":    r.get::<_, String>("region"),
        "power_kw":  r.get::<_, f64>("power_kw"),
        "energy_kwh":r.get::<_, f64>("energy_kwh"),
        "last_ts":   r.get::<_, String>("last_ts"),
    })).collect();
    Ok(Json(json!({ "hours": hours, "count": items.len(), "items": items })))
}

// ---- /api/sites/:site_key/ips — distinct device_ips ever seen at this site
pub async fn site_ips(State(st): State<AppState>, Path(site_key): Path<String>) -> ApiResult {
    let c = st.pool.get().await?;
    let rows = c.query(
        "SELECT host(device_ip) ip, count(*)::int8 n, MAX(event_time)::text last_seen \
         FROM fact_event \
         WHERE site_key = $1 AND device_ip IS NOT NULL \
         GROUP BY device_ip ORDER BY n DESC LIMIT 50",
        &[&site_key]).await?;
    let items: Vec<Value> = rows.iter().map(|r| json!({
        "ip":        r.get::<_, String>("ip"),
        "events":    r.get::<_, i64>("n"),
        "last_seen": r.get::<_, String>("last_seen"),
    })).collect();
    Ok(Json(json!({ "site_key": site_key, "items": items })))
}

// ---- /api/sites/:site_key/verification — recent verification history
pub async fn site_verification(State(st): State<AppState>, Path(site_key): Path<String>) -> ApiResult {
    let c = st.pool.get().await?;
    let rows = c.query(
        "SELECT id, verified_at::text va, verified_by, notes, \
                events_through::text et, ip_inventory, region_confirmed \
         FROM fact_site_verification WHERE site_key = $1 \
         ORDER BY verified_at DESC LIMIT 50",
        &[&site_key]).await?;
    let items: Vec<Value> = rows.iter().map(|r| json!({
        "id":               r.get::<_, i64>("id"),
        "verified_at":      r.get::<_, String>("va"),
        "verified_by":      r.get::<_, String>("verified_by"),
        "notes":            r.get::<_, String>("notes"),
        "events_through":   r.get::<_, String>("et"),
        "ip_inventory":     r.get::<_, Vec<String>>("ip_inventory"),
        "region_confirmed": r.get::<_, String>("region_confirmed"),
    })).collect();
    Ok(Json(json!({ "site_key": site_key, "items": items })))
}

// ---- /api/sites/:site_key/verification/summary — last verify + delta since
pub async fn site_verification_summary(State(st): State<AppState>, Path(site_key): Path<String>) -> ApiResult {
    let c = st.pool.get().await?;
    let last = c.query_opt(
        "SELECT verified_at::text va, verified_by, events_through::text et, \
                ip_inventory, region_confirmed \
         FROM fact_site_verification WHERE site_key = $1 \
         ORDER BY verified_at DESC LIMIT 1", &[&site_key]).await?;
    let (verified_at, by, through, ips, region) = match last {
        Some(r) => (
            r.get::<_, String>("va"),
            r.get::<_, String>("verified_by"),
            r.get::<_, String>("et"),
            r.get::<_, Vec<String>>("ip_inventory"),
            r.get::<_, String>("region_confirmed"),
        ),
        None => (String::new(), String::new(), String::new(), Vec::new(), String::new()),
    };
    // Pass the timestamp as TEXT and cast in SQL — avoids tokio-postgres'
    // Option<String> -> timestamptz serialization quirk (was 500ing in v6).
    let events_since = c.query_one(
        "SELECT count(*)::int8 n FROM fact_event \
         WHERE site_key = $1 \
           AND ($2 = '' OR event_time > $2::timestamptz)",
        &[&site_key, &through]).await?;
    let n_events: i64 = events_since.get("n");
    let cur_ips = c.query(
        "SELECT DISTINCT host(device_ip) ip FROM fact_event \
         WHERE site_key = $1 AND device_ip IS NOT NULL", &[&site_key]).await?;
    let cur_ip_list: Vec<String> = cur_ips.iter().map(|r| r.get::<_, String>("ip")).collect();
    let new_ips: Vec<String> = cur_ip_list.iter()
        .filter(|i| !ips.contains(i)).cloned().collect();

    Ok(Json(json!({
        "site_key":         site_key,
        "last_verified_at": verified_at,
        "last_verified_by": by,
        "events_through":   through,
        "events_since":     n_events,
        "confirmed_ips":    ips,
        "current_ips":      cur_ip_list,
        "new_ips":          new_ips,
        "region_confirmed": region,
    })))
}

// ---- /api/sites/:site_key/related — sites sharing a /24 subnet (via IP overlap)
pub async fn site_related(State(st): State<AppState>, Path(site_key): Path<String>) -> ApiResult {
    let c = st.pool.get().await?;
    let rows = c.query(
        "WITH my_subnets AS ( \
           SELECT DISTINCT regexp_replace(host(device_ip), '\\.\\d+$', '') sub \
           FROM fact_event WHERE site_key = $1 AND device_ip IS NOT NULL), \
         neighbour AS ( \
           SELECT site_key, regexp_replace(host(device_ip), '\\.\\d+$', '') sub \
           FROM fact_event WHERE device_ip IS NOT NULL AND site_key <> $1) \
         SELECT n.site_key, count(*)::int8 ip_overlap, \
                COALESCE(MAX(s.region),'') region \
         FROM neighbour n \
         JOIN my_subnets m USING (sub) \
         LEFT JOIN dim_site s ON s.site_key = n.site_key \
         GROUP BY n.site_key \
         ORDER BY ip_overlap DESC LIMIT 30",
        &[&site_key]).await?;
    let items: Vec<Value> = rows.iter().map(|r| json!({
        "site_key":   r.get::<_, String>("site_key"),
        "ip_overlap": r.get::<_, i64>("ip_overlap"),
        "region":     r.get::<_, String>("region"),
    })).collect();
    Ok(Json(json!({ "site_key": site_key, "items": items })))
}
