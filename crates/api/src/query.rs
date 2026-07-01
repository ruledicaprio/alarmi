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
                CASE \
                    WHEN COALESCE(a.open,0) = 0       THEN NULL \
                    WHEN COALESCE(a.sev_critical,false) THEN 'critical' \
                    WHEN COALESCE(a.sev_major,false)    THEN 'major' \
                    WHEN COALESCE(a.sev_minor,false)    THEN 'minor' \
                    WHEN COALESCE(a.sev_warning,false)  THEN 'warning' \
                    ELSE 'info' \
                END worst_severity, \
                COALESCE(le.t::text, '') last_event \
         FROM dim_site s \
         LEFT JOIN (SELECT site_key, count(*) open, \
                           bool_or(severity::text = 'critical') AS sev_critical, \
                           bool_or(severity::text = 'major')    AS sev_major, \
                           bool_or(severity::text = 'minor')    AS sev_minor, \
                           bool_or(severity::text = 'warning')  AS sev_warning \
                    FROM fact_alarm_episode WHERE is_open GROUP BY 1) a \
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
        "site_key":        r.get::<_, String>("site_key"),
        "name":            r.get::<_, String>("name"),
        "region":          r.get::<_, String>("region"),
        "municipality":    r.get::<_, String>("municipality"),
        "open_alarms":     r.get::<_, i64>("open_alarms"),
        "worst_severity":  r.get::<_, Option<String>>("worst_severity"),
        "last_event":      r.get::<_, String>("last_event"),
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

// ---- /api/regions — canonical region list from v7 migrate (sorted) + ad-hoc seen
pub async fn regions(State(st): State<AppState>) -> ApiResult {
    let c = st.pool.get().await?;
    // canonical 7 from dim_region_canonical
    let canonical = c.query(
        "SELECT region, label FROM dim_region_canonical ORDER BY sort_idx", &[]).await?;
    let items: Vec<Value> = canonical.iter().map(|r| json!({
        "region": r.get::<_, String>("region"),
        "label":  r.get::<_, String>("label"),
        "canonical": true,
    })).collect();
    // plus any region that's shown up in dim_site but isn't in canonical (e.g. TUZLAHASE typo)
    let extra = c.query(
        "SELECT DISTINCT region FROM dim_site \
         WHERE region IS NOT NULL AND region <> '' \
           AND region NOT IN (SELECT region FROM dim_region_canonical) \
         ORDER BY region", &[]).await?;
    let mut all = items;
    for r in extra.iter() {
        let region: String = r.get("region");
        all.push(json!({ "region": region.clone(), "label": region, "canonical": false }));
    }
    Ok(Json(json!({ "items": all })))
}

// ============================================================ v7 ENDPOINTS

// ---- /api/admin/users — list all users
pub async fn admin_users(State(st): State<AppState>) -> ApiResult {
    let c = st.pool.get().await?;
    let rows = c.query(
        "SELECT id, username, full_name, role::text role, COALESCE(region,'') region, \
                created_at::text created_at, COALESCE(last_seen::text,'') last_seen, disabled \
         FROM dim_user ORDER BY id", &[]).await?;
    let items: Vec<Value> = rows.iter().map(|r| json!({
        "id":         r.get::<_, i64>("id"),
        "username":   r.get::<_, String>("username"),
        "full_name":  r.get::<_, String>("full_name"),
        "role":       r.get::<_, String>("role"),
        "region":     r.get::<_, String>("region"),
        "created_at": r.get::<_, String>("created_at"),
        "last_seen":  r.get::<_, String>("last_seen"),
        "disabled":   r.get::<_, bool>("disabled"),
    })).collect();
    Ok(Json(json!({ "count": items.len(), "items": items })))
}

// ---- /api/admin/regions — canonical regions + per-region site counts
pub async fn admin_regions(State(st): State<AppState>) -> ApiResult {
    let c = st.pool.get().await?;
    let rows = c.query(
        "SELECT r.region, r.label, r.sort_idx, \
                (SELECT count(*)::int8 FROM dim_site s WHERE s.region = r.region) sites, \
                (SELECT count(*)::int8 FROM dim_user u WHERE u.region = r.region) users \
         FROM dim_region_canonical r ORDER BY r.sort_idx", &[]).await?;
    let items: Vec<Value> = rows.iter().map(|r| json!({
        "region":   r.get::<_, String>("region"),
        "label":    r.get::<_, String>("label"),
        "sort_idx": r.get::<_, i32>("sort_idx"),
        "sites":    r.get::<_, i64>("sites"),
        "users":    r.get::<_, i64>("users"),
    })).collect();
    Ok(Json(json!({ "items": items })))
}

// ---- /api/inventory/verified — verified-inventory view
pub async fn inventory_verified(State(st): State<AppState>, Query(q): Query<HashMap<String, String>>) -> ApiResult {
    let only_verified = q.get("only").map(|v| v == "verified").unwrap_or(false);
    let only_unverified = q.get("only").map(|v| v == "unverified").unwrap_or(false);
    let region = str_of(&q, "region");
    let c = st.pool.get().await?;
    let rows = c.query(
        "SELECT site_key, display_name, region, municipality, \
                COALESCE(last_verified::text,'') last_verified, \
                COALESCE(last_verified_by,'')    last_verified_by, \
                COALESCE(events_through::text,'') events_through, \
                is_verified, has_unverified_events \
         FROM v_verified_inventory \
         WHERE ($1 = '' OR region = $1) \
           AND ($2 = false OR is_verified) \
           AND ($3 = false OR NOT is_verified) \
         ORDER BY region, site_key LIMIT 5000",
        &[&region, &only_verified, &only_unverified]).await?;
    let items: Vec<Value> = rows.iter().map(|r| json!({
        "site_key":             r.get::<_, String>("site_key"),
        "display_name":         r.get::<_, String>("display_name"),
        "region":               r.get::<_, String>("region"),
        "municipality":         r.get::<_, String>("municipality"),
        "last_verified":        r.get::<_, String>("last_verified"),
        "last_verified_by":     r.get::<_, String>("last_verified_by"),
        "events_through":       r.get::<_, String>("events_through"),
        "is_verified":          r.get::<_, bool>("is_verified"),
        "has_unverified_events":r.get::<_, bool>("has_unverified_events"),
    })).collect();
    Ok(Json(json!({ "count": items.len(), "items": items })))
}

// ---- /api/solar/summary — v7: per-source stacked timeseries
pub async fn solar_summary_v7(State(st): State<AppState>, Query(q): Query<HashMap<String, String>>) -> ApiResult {
    let hours = hours_of(&q, 24);
    let c = st.pool.get().await?;

    // current-power head, per source so dashboard can show split
    // Uses 24h window so sites stay visible at night (value may be 0)
    // Classifies by dim_device.dev_type (set via Inventory), not event source
    let head = c.query(
        "WITH latest AS ( \
           SELECT DISTINCT ON (m.site_key) m.site_key, m.value, \
                  COALESCE(d.dev_type, 'unknown') src \
           FROM fact_measurement m \
           LEFT JOIN dim_device d ON d.site_key = m.site_key \
           WHERE m.metric = 'p_solar_kw' AND m.ts >= now() - INTERVAL '24 hours' \
           ORDER BY m.site_key, m.ts DESC) \
         SELECT src, count(*)::int8 sites, COALESCE(SUM(value),0)::float8 kw_now \
         FROM latest GROUP BY src", &[]).await?;
    let mut sites_active = 0i64;
    let mut total_kw = 0.0f64;
    let mut by_source: Vec<Value> = Vec::new();
    for r in head.iter() {
        let n: i64 = r.get("sites");
        let kw: f64 = r.get("kw_now");
        sites_active += n;
        total_kw += kw;
        by_source.push(json!({ "source": r.get::<_, String>("src"), "sites": n, "kw_now": kw }));
    }

    // hourly stacked timeseries: per-bucket sum split by device family (dim_device.dev_type)
    let series = c.query(
        "SELECT time_bucket(INTERVAL '1 hour', m.ts)::text bucket, \
                COALESCE(d.dev_type, 'eaton') family, \
                AVG(m.value)::float8 avg_kw, \
                MAX(m.value)::float8 max_kw, \
                count(DISTINCT m.site_key)::int8 sites \
         FROM fact_measurement m \
         LEFT JOIN dim_device d ON d.site_key = m.site_key \
         WHERE m.metric = 'p_solar_kw' AND m.ts >= now() - make_interval(hours => $1) \
         GROUP BY bucket, family ORDER BY bucket",
        &[&hours]).await?;
    let series_items: Vec<Value> = series.iter().map(|r| json!({
        "bucket": r.get::<_, String>("bucket"),
        "family": r.get::<_, String>("family"),
        "avg_kw": r.get::<_, f64>("avg_kw"),
        "max_kw": r.get::<_, f64>("max_kw"),
        "sites":  r.get::<_, i64>("sites"),
    })).collect();

    // consumption (p_load_kw) timeseries — same buckets, aggregated across all FNE sites
    let load_series = c.query(
        "SELECT time_bucket(INTERVAL '1 hour', m.ts)::text bucket, \
                AVG(m.value)::float8 avg_kw, \
                MAX(m.value)::float8 max_kw, \
                count(DISTINCT m.site_key)::int8 sites \
         FROM fact_measurement m \
         JOIN dim_device d ON d.site_key = m.site_key AND d.fne = true \
         WHERE m.metric = 'p_load_kw' AND m.ts >= now() - make_interval(hours => $1) \
         GROUP BY bucket ORDER BY bucket",
        &[&hours]).await?;
    let load_items: Vec<Value> = load_series.iter().map(|r| json!({
        "bucket": r.get::<_, String>("bucket"),
        "avg_kw": r.get::<_, f64>("avg_kw"),
        "max_kw": r.get::<_, f64>("max_kw"),
        "sites":  r.get::<_, i64>("sites"),
    })).collect();

    // top 10 producers right now (includes 0 kW so list is always populated)
    let top = c.query(
        "WITH latest AS ( \
           SELECT DISTINCT ON (site_key) site_key, value FROM fact_measurement \
           WHERE metric = 'p_solar_kw' AND ts >= now() - INTERVAL '24 hours' \
           ORDER BY site_key, ts DESC) \
         SELECT l.site_key, l.value::float8 power_kw, \
                COALESCE(s.region,'') region, COALESCE(s.display_name,'') name, \
                COALESCE(d.dev_type, 'eaton') family \
         FROM latest l \
         LEFT JOIN dim_site s USING (site_key) \
         LEFT JOIN dim_device d ON d.site_key = l.site_key \
         ORDER BY l.value DESC LIMIT 10", &[]).await?;
    let top_items: Vec<Value> = top.iter().map(|r| json!({
        "site_key": r.get::<_, String>("site_key"),
        "name":     r.get::<_, String>("name"),
        "region":   r.get::<_, String>("region"),
        "power_kw": r.get::<_, f64>("power_kw"),
        "family":   r.get::<_, String>("family"),
    })).collect();

    Ok(Json(json!({
        "hours":              hours,
        "sites_active_now":   sites_active,
        "total_power_kw_now": total_kw,
        "by_source":          by_source,
        "timeseries":         series_items,
        "load_timeseries":    load_items,
        "top_producers":      top_items,
    })))
}

// ---- /api/solar/sites — v7: tag each row with family
pub async fn solar_sites_v7(State(st): State<AppState>, Query(q): Query<HashMap<String, String>>) -> ApiResult {
    let hours = hours_of(&q, 24);
    let c = st.pool.get().await?;
    let rows = c.query(
        "WITH latest_p AS ( \
           SELECT DISTINCT ON (site_key) site_key, value p, ts \
           FROM fact_measurement \
           WHERE metric = 'p_solar_kw' AND ts >= now() - INTERVAL '24 hours' \
           ORDER BY site_key, ts DESC), \
         latest_load AS ( \
           SELECT DISTINCT ON (m.site_key) m.site_key, m.value load_kw \
           FROM fact_measurement m \
           JOIN dim_device d ON d.site_key = m.site_key AND d.fne = true \
           WHERE m.metric = 'p_load_kw' AND m.ts >= now() - INTERVAL '24 hours' \
           ORDER BY m.site_key, m.ts DESC), \
         energy AS ( \
           SELECT site_key, GREATEST((MAX(value) - MIN(value))::float8, 0) dkwh \
           FROM fact_measurement \
           WHERE metric = 'e_total_kwh' AND ts >= now() - make_interval(hours => $1) \
           GROUP BY site_key) \
         SELECT lp.site_key, lp.p::float8 power_kw, lp.ts::text last_ts, \
                COALESCE(ll.load_kw, 0)::float8 load_kw, \
                COALESCE(e.dkwh, 0)::float8 energy_kwh, \
                COALESCE(s.region,'') region, COALESCE(s.display_name,'') name, \
                COALESCE(d.dev_type, 'eaton') family \
         FROM latest_p lp \
         LEFT JOIN latest_load ll USING (site_key) \
         LEFT JOIN energy   e USING (site_key) \
         LEFT JOIN dim_site s USING (site_key) \
         LEFT JOIN dim_device d ON d.site_key = lp.site_key \
         ORDER BY lp.p DESC",
        &[&hours]).await?;
    let items: Vec<Value> = rows.iter().map(|r| json!({
        "site_key":  r.get::<_, String>("site_key"),
        "name":      r.get::<_, String>("name"),
        "region":    r.get::<_, String>("region"),
        "power_kw":  r.get::<_, f64>("power_kw"),
        "load_kw":   r.get::<_, f64>("load_kw"),
        "energy_kwh":r.get::<_, f64>("energy_kwh"),
        "last_ts":   r.get::<_, String>("last_ts"),
        "family":    r.get::<_, String>("family"),
    })).collect();
    Ok(Json(json!({ "hours": hours, "count": items.len(), "items": items })))
}

// ---- /api/system/status — read-only box health for the System page
pub async fn system_status(State(st): State<AppState>) -> ApiResult {
    use std::process::Command;
    let c = st.pool.get().await?;

    // DB stats
    let db_row = c.query_one(
        "SELECT pg_database_size(current_database())::int8 db_bytes, \
                (SELECT count(*)::int8 FROM fact_event) events, \
                (SELECT count(*)::int8 FROM fact_measurement) meas, \
                (SELECT count(*)::int8 FROM fact_alarm_episode WHERE is_open) open_episodes, \
                version() pg_version", &[]).await?;

    // services
    fn unit_status(name: &str) -> Value {
        let out = Command::new("systemctl")
            .args(["show", "--no-page", "--property=ActiveState,SubState,ActiveEnterTimestamp,MemoryCurrent", name])
            .output();
        let mut active = "unknown".to_string();
        let mut sub = "unknown".to_string();
        let mut since = "".to_string();
        let mut memory_mb: i64 = -1;
        if let Ok(o) = out {
            for line in String::from_utf8_lossy(&o.stdout).lines() {
                if let Some(v) = line.strip_prefix("ActiveState=") { active = v.into() }
                else if let Some(v) = line.strip_prefix("SubState=") { sub = v.into() }
                else if let Some(v) = line.strip_prefix("ActiveEnterTimestamp=") { since = v.into() }
                else if let Some(v) = line.strip_prefix("MemoryCurrent=") {
                    memory_mb = v.parse::<i64>().unwrap_or(0) / (1024 * 1024);
                }
            }
        }
        json!({ "active": active, "sub": sub, "active_since": since, "memory_mb": memory_mb })
    }

    // disk usage of /opt/bht
    let df_out = Command::new("df").args(["-BM", "--output=size,used,avail,pcent", "/opt"]).output();
    let disk = if let Ok(o) = df_out {
        let s = String::from_utf8_lossy(&o.stdout);
        let lines: Vec<&str> = s.lines().collect();
        if lines.len() >= 2 {
            let parts: Vec<&str> = lines[1].split_whitespace().collect();
            if parts.len() >= 4 {
                json!({ "size": parts[0], "used": parts[1], "avail": parts[2], "use_pct": parts[3] })
            } else { json!(null) }
        } else { json!(null) }
    } else { json!(null) };

    Ok(Json(json!({
        "db": {
            "size_bytes":      db_row.get::<_, i64>("db_bytes"),
            "events":          db_row.get::<_, i64>("events"),
            "measurements":    db_row.get::<_, i64>("meas"),
            "open_episodes":   db_row.get::<_, i64>("open_episodes"),
            "pg_version":      db_row.get::<_, String>("pg_version"),
        },
        "services": {
            "bht-api":        unit_status("bht-api"),
            "bht-poller":     unit_status("bht-poller"),
            "neteco-poller":  unit_status("neteco-poller"),
            "postgresql-16":  unit_status("postgresql-16"),
        },
        "disk_opt": disk,
        "api_version": env!("CARGO_PKG_VERSION"),
    })))
}

// ---- /api/system/journal?service=bht-api&lines=100 — read-only journalctl tail
pub async fn system_journal(Query(q): Query<HashMap<String, String>>) -> ApiResult {
    use std::process::Command;
    let svc = str_of(&q, "service");
    // strict whitelist — never pass arbitrary strings to a subprocess
    if !matches!(svc.as_str(), "bht-api" | "bht-poller" | "neteco-poller" | "postgresql-16" | "crond") {
        return Ok(Json(json!({ "error": "unknown service",
            "allowed": ["bht-api","bht-poller","neteco-poller","postgresql-16","crond"] })));
    }
    let n: i64 = i64_of(&q, "lines", 100).clamp(10, 1000);
    let out = Command::new("journalctl")
        .args(["-u", &svc, "-n", &n.to_string(), "--no-pager", "-o", "short-iso"])
        .output()
        .map_err(|e| crate::ApiError::from(anyhow::anyhow!("journalctl failed: {e}")))?;
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    Ok(Json(json!({ "service": svc, "lines": n, "text": text })))
}

// ---- /api/map/sites — all GIS-enriched sites + live alarm overlay
pub async fn map_sites(State(st): State<AppState>) -> ApiResult {
    let c = st.pool.get().await?;
    let rows = c.query(
        "SELECT \
             s.site_key, \
             COALESCE(s.display_name,'') display_name, \
             s.latitude::float8  lat, \
             s.longitude::float8 lon, \
             COALESCE(s.region,'')       region, \
             COALESCE(s.municipality,'') municipality, \
             COALESCE(s.technologies,'{}') technologies, \
             COALESCE(s.has_genset,  false) has_genset, \
             COALESCE(s.has_battery, false) has_battery, \
             COALESCE(s.has_solar,   false) has_solar, \
             COUNT(a.site_key)::int8 open_alarms, \
             CASE \
                 WHEN COUNT(a.site_key) = 0                   THEN NULL \
                 WHEN bool_or(a.severity::text = 'critical')  THEN 'critical' \
                 WHEN bool_or(a.severity::text = 'major')     THEN 'major' \
                 WHEN bool_or(a.severity::text = 'minor')     THEN 'minor' \
                 WHEN bool_or(a.severity::text = 'warning')   THEN 'warning' \
                 ELSE 'info' \
             END worst_severity \
         FROM v_site_gis s \
         LEFT JOIN v_active_alarms a ON a.site_key = s.site_key \
         WHERE s.latitude IS NOT NULL \
         GROUP BY s.site_key, s.display_name, s.latitude, s.longitude, \
                  s.region, s.municipality, s.technologies, \
                  s.has_genset, s.has_battery, s.has_solar \
         ORDER BY open_alarms DESC, s.site_key",
        &[]).await?;
    let items: Vec<Value> = rows.iter().map(|r| json!({
        "site_key":       r.get::<_, String>("site_key"),
        "display_name":   r.get::<_, String>("display_name"),
        "lat":            r.get::<_, f64>("lat"),
        "lon":            r.get::<_, f64>("lon"),
        "region":         r.get::<_, String>("region"),
        "municipality":   r.get::<_, String>("municipality"),
        "technologies":   r.get::<_, Vec<String>>("technologies"),
        "has_genset":     r.get::<_, bool>("has_genset"),
        "has_battery":    r.get::<_, bool>("has_battery"),
        "has_solar":      r.get::<_, bool>("has_solar"),
        "open_alarms":    r.get::<_, i64>("open_alarms"),
        "worst_severity": r.get::<_, Option<String>>("worst_severity"),
    })).collect();
    Ok(Json(json!(items)))
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

// ============================================================ v8 INVENTORY

// ---- /api/inventory/devices — device registry with live health
// Filters: region, health (ok|degraded|dead|stale|never), dev_type, enabled (true|false),
//          fne (true|false), q (search site_key/name). Paginated. Includes fleet summary.
pub async fn inventory_devices(
    State(st): State<AppState>,
    Query(q): Query<HashMap<String, String>>,
) -> ApiResult {
    let region   = str_of(&q, "region");
    let health   = str_of(&q, "health");
    let dev_type = str_of(&q, "dev_type");
    let enabled  = str_of(&q, "enabled");   // "true" | "false" | "" (all)
    let fne      = str_of(&q, "fne");       // "true" | "false" | ""
    let search   = str_of(&q, "q");
    let limit:  i64 = i64_of(&q, "limit",  100).clamp(1, 2000);
    let offset: i64 = i64_of(&q, "offset",   0).max(0);
    let c = st.pool.get().await?;

    // Fleet-wide health summary (cheap: 301 rows, no JOIN needed for counts)
    let sum = c.query_one(
        "SELECT count(*)::int8                                               total, \
                count(*) FILTER (WHERE health = 'ok')::int8                 ok, \
                count(*) FILTER (WHERE health = 'degraded')::int8           degraded, \
                count(*) FILTER (WHERE health = 'dead')::int8               dead, \
                count(*) FILTER (WHERE health = 'stale')::int8              stale, \
                count(*) FILTER (WHERE health = 'never')::int8              never, \
                count(*) FILTER (WHERE NOT enabled)::int8                   disabled \
         FROM v_device_health", &[]).await?;

    // Filtered count + page
    let whr = "WHERE ($1 = '' OR region   = $1) \
                 AND ($2 = '' OR health   = $2) \
                 AND ($3 = '' OR dev_type = $3) \
                 AND ($4 = '' OR enabled  = ($4 = 'true')) \
                 AND ($5 = '' OR fne      = ($5 = 'true')) \
                 AND ($6 = '' OR site_key ILIKE '%'||$6||'%' OR name ILIKE '%'||$6||'%')";

    let total: i64 = c.query_one(
        &format!("SELECT count(*)::int8 n FROM v_device_health {whr}"),
        &[&region, &health, &dev_type, &enabled, &fne, &search]).await?.get("n");

    let rows = c.query(
        &format!("SELECT id, ip, port, unit_id, site_key, site_name, region, dev_type, \
                         fne, enabled, name, fail_streak, health, added_by, \
                         COALESCE(last_polled, '') last_polled, \
                         COALESCE(last_ok,     '') last_ok, \
                         updated_at \
                  FROM v_device_health {whr} \
                  ORDER BY region, site_key, ip, unit_id \
                  LIMIT $7 OFFSET $8"),
        &[&region, &health, &dev_type, &enabled, &fne, &search, &limit, &offset]).await?;

    let items: Vec<Value> = rows.iter().map(|r| json!({
        "id":          r.get::<_, i64>("id"),
        "ip":          r.get::<_, String>("ip"),
        "port":        r.get::<_, i32>("port"),
        "unit_id":     r.get::<_, i16>("unit_id"),
        "site_key":    r.get::<_, String>("site_key"),
        "site_name":   r.get::<_, String>("site_name"),
        "region":      r.get::<_, String>("region"),
        "dev_type":    r.get::<_, String>("dev_type"),
        "fne":         r.get::<_, bool>("fne"),
        "enabled":     r.get::<_, bool>("enabled"),
        "name":        r.get::<_, String>("name"),
        "fail_streak": r.get::<_, i32>("fail_streak"),
        "health":      r.get::<_, String>("health"),
        "last_polled": r.get::<_, String>("last_polled"),
        "last_ok":     r.get::<_, String>("last_ok"),
        "added_by":    r.get::<_, String>("added_by"),
        "updated_at":  r.get::<_, String>("updated_at"),
    })).collect();

    Ok(Json(json!({
        "summary": {
            "total":    sum.get::<_, i64>("total"),
            "ok":       sum.get::<_, i64>("ok"),
            "degraded": sum.get::<_, i64>("degraded"),
            "dead":     sum.get::<_, i64>("dead"),
            "stale":    sum.get::<_, i64>("stale"),
            "never":    sum.get::<_, i64>("never"),
            "disabled": sum.get::<_, i64>("disabled"),
        },
        "total":  total,
        "count":  items.len(),
        "items":  items,
    })))
}

// ---- /api/inventory/device-orphans — IPs in events with no dim_device row
// These are candidates to claim via POST /api/inventory/device-orphans/claim.
pub async fn inventory_device_orphans(State(st): State<AppState>) -> ApiResult {
    let c = st.pool.get().await?;
    let rows = c.query(
        "SELECT ip, site_key, event_count::int8, last_seen::text, source \
         FROM v_device_orphans LIMIT 500", &[]).await?;
    let items: Vec<Value> = rows.iter().map(|r| json!({
        "ip":          r.get::<_, String>("ip"),
        "site_key":    r.get::<_, String>("site_key"),
        "event_count": r.get::<_, i64>("event_count"),
        "last_seen":   r.get::<_, String>("last_seen"),
        "source":      r.get::<_, String>("source"),
    })).collect();
    Ok(Json(json!({ "count": items.len(), "items": items })))
}

// ---- /api/inventory/stubs?region=&limit=&offset=
// dim_site rows created automatically by event ingestion, sorted by event activity
// (busiest stubs first so operators prioritise the most impactful enrichments).
pub async fn inventory_stubs(
    State(st): State<AppState>,
    Query(q): Query<HashMap<String, String>>,
) -> ApiResult {
    let region = str_of(&q, "region");
    let limit:  i64 = i64_of(&q, "limit",  100).clamp(1, 1000);
    let offset: i64 = i64_of(&q, "offset",   0).max(0);
    let c = st.pool.get().await?;

    let total: i64 = c.query_one(
        "SELECT count(*)::int8 n FROM dim_site \
         WHERE is_stub AND ($1 = '' OR region = $1)",
        &[&region]).await?.get("n");

    let rows = c.query(
        "SELECT s.site_key, \
                COALESCE(s.display_name,'') display_name, \
                COALESCE(s.region,'')       region, \
                COALESCE((SELECT count(*)::int8    FROM fact_event   e WHERE e.site_key = s.site_key), 0) event_count, \
                COALESCE((SELECT max(event_time)::text FROM fact_event e WHERE e.site_key = s.site_key), '') last_event, \
                (SELECT count(*)::int8 FROM dim_device d WHERE d.site_key = s.site_key)                    device_count \
         FROM dim_site s \
         WHERE s.is_stub AND ($1 = '' OR s.region = $1) \
         ORDER BY event_count DESC, s.site_key \
         LIMIT $2 OFFSET $3",
        &[&region, &limit, &offset]).await?;

    let items: Vec<Value> = rows.iter().map(|r| json!({
        "site_key":     r.get::<_, String>("site_key"),
        "display_name": r.get::<_, String>("display_name"),
        "region":       r.get::<_, String>("region"),
        "event_count":  r.get::<_, i64>("event_count"),
        "last_event":   r.get::<_, String>("last_event"),
        "device_count": r.get::<_, i64>("device_count"),
    })).collect();

    Ok(Json(json!({ "total": total, "count": items.len(), "items": items })))
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

// ============================================================ NETECO

// ---- /api/neteco/alarms — NetEco NBI alarm list with server-side pagination + filters
pub async fn neteco_alarms(
    State(st): State<AppState>,
    Query(q): Query<HashMap<String, String>>,
) -> ApiResult {
    let station:  String      = str_of(&q, "station");
    let severity: Option<i16> = str_of(&q, "severity").parse().ok();
    let status:   Option<i16> = str_of(&q, "status").parse().ok();
    let limit:  i64 = i64_of(&q, "limit",  200).clamp(1, 1000);
    let offset: i64 = i64_of(&q, "offset",   0).max(0);
    let c = st.pool.get().await?;

    let total: i64 = c.query_one(
        "SELECT count(*)::int8 n FROM neteco.alarms \
         WHERE ($1 = '' OR station_code = $1) \
           AND ($2::smallint IS NULL OR severity = $2) \
           AND ($3::smallint IS NULL OR status   = $3)",
        &[&station, &severity, &status]).await?.get("n");

    let rows = c.query(
        "SELECT alarm_id, \
                COALESCE(station_code,'')  station_code, \
                COALESCE(station_name,'')  station_name, \
                COALESCE(dev_name,'')      dev_name, \
                COALESCE(std_type_name,'') std_type_name, \
                COALESCE(alarm_name,'')    alarm_name, \
                COALESCE(alarm_cause,'')   alarm_cause, \
                alarm_type, severity, status, \
                raise_time::text  raise_time, \
                repair_time::text repair_time, \
                source, last_seen::text last_seen \
         FROM neteco.alarms \
         WHERE ($1 = '' OR station_code = $1) \
           AND ($2::smallint IS NULL OR severity = $2) \
           AND ($3::smallint IS NULL OR status   = $3) \
         ORDER BY raise_time DESC NULLS LAST \
         LIMIT $4 OFFSET $5",
        &[&station, &severity, &status, &limit, &offset]).await?;

    let items: Vec<Value> = rows.iter().map(|r| json!({
        "alarm_id":      r.get::<_, String>("alarm_id"),
        "station_code":  r.get::<_, String>("station_code"),
        "station_name":  r.get::<_, String>("station_name"),
        "dev_name":      r.get::<_, String>("dev_name"),
        "std_type_name": r.get::<_, String>("std_type_name"),
        "alarm_name":    r.get::<_, String>("alarm_name"),
        "alarm_cause":   r.get::<_, String>("alarm_cause"),
        "alarm_type":    r.get::<_, Option<i16>>("alarm_type"),
        "severity":      r.get::<_, Option<i16>>("severity"),
        "status":        r.get::<_, Option<i16>>("status"),
        "raise_time":    r.get::<_, Option<String>>("raise_time"),
        "repair_time":   r.get::<_, Option<String>>("repair_time"),
        "source":        r.get::<_, String>("source"),
        "last_seen":     r.get::<_, Option<String>>("last_seen"),
    })).collect();
    Ok(Json(json!({ "total": total, "count": items.len(), "items": items })))
}

// ---- /api/neteco/alarms/summary — active counts by severity for the dashboard badge
pub async fn neteco_alarm_summary(State(st): State<AppState>) -> ApiResult {
    let c = st.pool.get().await?;
    let row = c.query_one(
        "SELECT \
           count(*) FILTER (WHERE status = 1)                          AS active, \
           count(*) FILTER (WHERE status = 1 AND severity = 1)         AS critical, \
           count(*) FILTER (WHERE status = 1 AND severity = 2)         AS major, \
           count(*) FILTER (WHERE status = 1 AND severity IN (3,4))    AS minor_warn, \
           count(DISTINCT station_code) FILTER (WHERE status = 1)      AS affected_stations \
         FROM neteco.alarms", &[]).await?;
    Ok(Json(json!({
        "active":            row.get::<_, i64>("active"),
        "critical":          row.get::<_, i64>("critical"),
        "major":             row.get::<_, i64>("major"),
        "minor_warn":        row.get::<_, i64>("minor_warn"),
        "affected_stations": row.get::<_, i64>("affected_stations"),
    })))
}
