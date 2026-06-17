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

pub async fn health(State(st): State<AppState>) -> ApiResult {
    let c = st.pool.get().await?;
    let row = c.query_one("SELECT now()::text AS ts", &[]).await?;
    Ok(Json(json!({ "status": "ok", "db_time": row.get::<_, String>("ts") })))
}

pub async fn sites(State(st): State<AppState>) -> ApiResult {
    let c = st.pool.get().await?;
    let rows = c.query(
        "SELECT s.site_key, COALESCE(s.display_name,'') name, COALESCE(s.region,'') region, \
                COALESCE(a.open,0)::int8 open_alarms \
         FROM dim_site s \
         LEFT JOIN (SELECT site_key, count(*) open FROM fact_alarm_episode WHERE is_open GROUP BY 1) a \
           USING (site_key) \
         ORDER BY s.site_key", &[]).await?;
    let items: Vec<Value> = rows.iter().map(|r| json!({
        "site_key": r.get::<_, String>("site_key"),
        "name": r.get::<_, String>("name"),
        "region": r.get::<_, String>("region"),
        "open_alarms": r.get::<_, i64>("open_alarms"),
    })).collect();
    Ok(Json(json!({ "count": items.len(), "items": items })))
}

pub async fn active_alarms(State(st): State<AppState>) -> ApiResult {
    let c = st.pool.get().await?;
    let rows = c.query(
        "SELECT site_key, source::text source, alarm_class::text alarm_class, severity::text severity, \
                raised_at::text raised_at, open_minutes::float8 open_minutes \
         FROM v_active_alarms ORDER BY raised_at LIMIT 500", &[]).await?;
    let items: Vec<Value> = rows.iter().map(|r| json!({
        "site_key": r.get::<_, String>("site_key"),
        "source": r.get::<_, String>("source"),
        "alarm_class": r.get::<_, String>("alarm_class"),
        "severity": r.get::<_, String>("severity"),
        "raised_at": r.get::<_, String>("raised_at"),
        "open_minutes": r.get::<_, f64>("open_minutes"),
    })).collect();
    Ok(Json(json!({ "count": items.len(), "items": items })))
}

pub async fn recent_alarms(State(st): State<AppState>, Query(q): Query<HashMap<String, String>>) -> ApiResult {
    let hours = hours_of(&q, 24);
    let site = q.get("site").cloned().unwrap_or_default();
    let class = q.get("class").cloned().unwrap_or_default();
    let source = q.get("source").cloned().unwrap_or_default();
    let limit: i64 = q.get("limit").and_then(|s| s.parse().ok()).unwrap_or(200);
    let c = st.pool.get().await?;
    let rows = c.query(
        "SELECT event_time::text et, source::text src, site_key, alarm_class::text ac, \
                severity::text sv, transition::text tr, COALESCE(raw_alarm,'') ra, \
                COALESCE(host(device_ip),'') ip \
         FROM fact_event \
         WHERE event_time >= now() - make_interval(hours => $1) \
           AND ($2 = '' OR site_key = $2) \
           AND ($3 = '' OR alarm_class::text = $3) \
           AND ($4 = '' OR source::text = $4) \
         ORDER BY event_time DESC LIMIT $5",
        &[&hours, &site, &class, &source, &limit]).await?;
    let items: Vec<Value> = rows.iter().map(|r| json!({
        "event_time": r.get::<_, String>("et"),
        "source": r.get::<_, String>("src"),
        "site_key": r.get::<_, String>("site_key"),
        "alarm_class": r.get::<_, String>("ac"),
        "severity": r.get::<_, String>("sv"),
        "transition": r.get::<_, String>("tr"),
        "raw_alarm": r.get::<_, String>("ra"),
        "device_ip": r.get::<_, String>("ip"),
    })).collect();
    Ok(Json(json!({ "count": items.len(), "items": items })))
}

pub async fn site_reliability(State(st): State<AppState>, Path(site_key): Path<String>) -> ApiResult {
    let c = st.pool.get().await?;
    let rows = c.query(
        "SELECT episodes, open_now, outage_hours::float8 oh, COALESCE(avg_minutes,0)::float8 am \
         FROM v_site_reliability_30d WHERE site_key = $1", &[&site_key]).await?;
    let v = match rows.first() {
        Some(r) => json!({
            "site_key": site_key,
            "episodes": r.get::<_, i64>("episodes"),
            "open_now": r.get::<_, i64>("open_now"),
            "outage_hours": r.get::<_, f64>("oh"),
            "avg_minutes": r.get::<_, f64>("am"),
        }),
        None => json!({ "site_key": site_key, "episodes": 0, "open_now": 0, "outage_hours": 0.0, "avg_minutes": 0.0 }),
    };
    Ok(Json(v))
}

pub async fn site_measurements(State(st): State<AppState>, Path(site_key): Path<String>, Query(q): Query<HashMap<String, String>>) -> ApiResult {
    let metric = q.get("metric").cloned().unwrap_or_default();
    let hours = hours_of(&q, 24);
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
    let site = q.get("site").cloned().unwrap_or_default();
    let c = st.pool.get().await?;
    let rows = c.query(
        "SELECT site_key, COALESCE(host(device_ip),'') ip, metric, value::float8 v, ts::text ts \
         FROM v_latest_measurement WHERE ($1 = '' OR site_key = $1) ORDER BY site_key, metric",
        &[&site]).await?;
    let items: Vec<Value> = rows.iter().map(|r| json!({
        "site_key": r.get::<_, String>("site_key"),
        "device_ip": r.get::<_, String>("ip"),
        "metric": r.get::<_, String>("metric"),
        "value": r.get::<_, f64>("v"),
        "ts": r.get::<_, String>("ts"),
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

