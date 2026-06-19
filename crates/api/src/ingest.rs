//! Ingest endpoints — turn the manual scrape/tar/copy into a service.
//! The work-PC scraper can curl the feed and POST the raw text here; the API
//! normalizes it with bht-normalize (same logic as the loader) and inserts.

use crate::db::{insert_events, insert_measurements, MeasIn};
use crate::{ApiError, AppState};
use axum::extract::{Path, State};
use axum::Json;
use bht_normalize::{normalize_line, parse_smetnje_html, CanonicalEvent};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Debug, Deserialize)]
pub struct VerifyInput {
    #[serde(default)] pub verified_by: String,
    #[serde(default)] pub notes: String,
    /// ISO 8601 timestamp; defaults to now() server-side if absent/empty.
    #[serde(default)] pub events_through: String,
    #[serde(default)] pub ip_inventory: Vec<String>,
    #[serde(default)] pub region_confirmed: String,
}

/// POST /api/sites/:site_key/verify — operator marks events reviewed.
pub async fn site_verify(
    State(st): State<AppState>,
    Path(site_key): Path<String>,
    Json(p): Json<VerifyInput>,
) -> Result<Json<Value>, ApiError> {
    let c = st.pool.get().await?;
    let by   = if p.verified_by.is_empty() { "operator".to_string() } else { p.verified_by };
    let through_opt = if p.events_through.is_empty() { None } else { Some(p.events_through) };
    let row = c.query_one(
        "INSERT INTO fact_site_verification \
           (site_key, verified_by, notes, events_through, ip_inventory, region_confirmed) \
         VALUES ($1, $2, $3, COALESCE($4::timestamptz, now()), $5, $6) \
         RETURNING id, verified_at::text va",
        &[&site_key, &by, &p.notes, &through_opt, &p.ip_inventory, &p.region_confirmed]).await?;
    Ok(Json(json!({
        "id": row.get::<_, i64>("id"),
        "verified_at": row.get::<_, String>("va"),
        "site_key": site_key,
    })))
}

/// POST /ingest/raw/ispadnap  (Content-Type: text/plain) — raw feed lines.
pub async fn ingest_raw_ispadnap(
    State(st): State<AppState>,
    body: String,
) -> Result<Json<Value>, ApiError> {
    let (mut total, mut dropped) = (0u64, 0u64);
    let mut events: Vec<CanonicalEvent> = Vec::new();
    for line in body.lines() {
        if line.trim().is_empty() { continue; }
        total += 1;
        match normalize_line(line) {
            Ok(ev) => events.push(ev),
            Err(_) => dropped += 1,
        }
    }
    let inserted = insert_events(&st.pool, &events).await?;
    Ok(Json(json!({
        "received": total, "normalized": events.len(), "dropped": dropped, "inserted": inserted
    })))
}

/// POST /ingest/raw/smetnje  (Content-Type: text/html or text/plain) — raw
/// HTML body of the 4-column /smetnje.html outage table. The parser does its
/// own HTML scraping so curl can pipe straight in:
///   curl -sS http://192.168.108.77/smetnje.html | \
///     curl -sS --data-binary @- -H "Content-Type: text/html" \
///       http://localhost:8080/ingest/raw/smetnje
pub async fn ingest_raw_smetnje(
    State(st): State<AppState>,
    body: String,
) -> Result<Json<Value>, ApiError> {
    let events = parse_smetnje_html(&body);
    let parsed = events.len();
    let inserted = insert_events(&st.pool, &events).await?;
    Ok(Json(json!({
        "received_bytes": body.len(), "parsed": parsed, "inserted": inserted
    })))
}

/// POST /ingest/events — JSON array of already-normalized CanonicalEvent.
pub async fn ingest_events(
    State(st): State<AppState>,
    Json(events): Json<Vec<CanonicalEvent>>,
) -> Result<Json<Value>, ApiError> {
    let inserted = insert_events(&st.pool, &events).await?;
    Ok(Json(json!({ "received": events.len(), "inserted": inserted })))
}

/// POST /ingest/measurements — JSON array of measurements.
pub async fn ingest_measurements(
    State(st): State<AppState>,
    Json(rows): Json<Vec<MeasIn>>,
) -> Result<Json<Value>, ApiError> {
    let inserted = insert_measurements(&st.pool, &rows).await?;
    Ok(Json(json!({ "received": rows.len(), "inserted": inserted })))
}
