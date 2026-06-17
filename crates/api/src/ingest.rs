//! Ingest endpoints — turn the manual scrape/tar/copy into a service.
//! The work-PC scraper can curl the feed and POST the raw text here; the API
//! normalizes it with bht-normalize (same logic as the loader) and inserts.

use crate::db::{insert_events, insert_measurements, MeasIn};
use crate::{ApiError, AppState};
use axum::extract::State;
use axum::Json;
use bht_normalize::{normalize_line, CanonicalEvent};
use serde_json::{json, Value};

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
