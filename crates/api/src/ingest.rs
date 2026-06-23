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

// ============================================================ v8 INVENTORY MANAGEMENT

#[derive(Debug, Deserialize)]
pub struct DeviceInput {
    pub ip:       String,
    #[serde(default = "inv_port")]    pub port:     i32,
    #[serde(default = "inv_unit")]    pub unit_id:  i16,
    pub site_key: String,
    #[serde(default = "inv_dtype")]   pub dev_type: String,
    #[serde(default)]                 pub base0:    bool,
    #[serde(default)]                 pub fne:      bool,
    #[serde(default = "inv_enabled")] pub enabled:  bool,
    #[serde(default)]                 pub name:     String,
    #[serde(default)]                 pub notes:    String,
    #[serde(default)]                 pub added_by: String,
}
fn inv_port()    -> i32    { 502 }
fn inv_unit()    -> i16    { 1 }
fn inv_dtype()   -> String { "eaton".into() }
fn inv_enabled() -> bool   { true }

/// POST /api/inventory/devices — add or update (upsert by ip + unit_id).
/// Auto-stubs dim_site if site_key is new.
pub async fn device_upsert(
    State(st): State<AppState>,
    Json(p): Json<DeviceInput>,
) -> Result<Json<Value>, ApiError> {
    if p.ip.is_empty()       { return Err(anyhow::anyhow!("ip is required").into()); }
    if p.site_key.is_empty() { return Err(anyhow::anyhow!("site_key is required").into()); }
    let added_by = if p.added_by.is_empty() { "operator".to_string() } else { p.added_by };
    let c = st.pool.get().await?;
    c.execute(
        "INSERT INTO dim_site (site_key, display_name, is_stub) VALUES ($1, $1, true) \
         ON CONFLICT (site_key) DO NOTHING",
        &[&p.site_key]).await?;
    let row = c.query_one(
        "INSERT INTO dim_device \
           (ip, port, unit_id, site_key, dev_type, base0, fne, enabled, name, notes, added_by) \
         VALUES ($1::inet, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11) \
         ON CONFLICT (ip, unit_id) DO UPDATE SET \
           site_key   = EXCLUDED.site_key, \
           dev_type   = EXCLUDED.dev_type, \
           base0      = EXCLUDED.base0, \
           fne        = EXCLUDED.fne, \
           enabled    = EXCLUDED.enabled, \
           name       = EXCLUDED.name, \
           notes      = EXCLUDED.notes, \
           updated_at = now() \
         RETURNING id, host(ip)::text ip, unit_id, site_key, enabled, updated_at::text ua",
        &[&p.ip, &p.port, &p.unit_id, &p.site_key, &p.dev_type,
          &p.base0, &p.fne, &p.enabled, &p.name, &p.notes, &added_by]).await?;
    Ok(Json(json!({
        "id":         row.get::<_, i64>("id"),
        "ip":         row.get::<_, String>("ip"),
        "unit_id":    row.get::<_, i16>("unit_id"),
        "site_key":   row.get::<_, String>("site_key"),
        "enabled":    row.get::<_, bool>("enabled"),
        "updated_at": row.get::<_, String>("ua"),
    })))
}

#[derive(Debug, Deserialize)]
pub struct DevicePatch {
    pub enabled:  Option<bool>,
    pub site_key: Option<String>,
    pub name:     Option<String>,
    pub notes:    Option<String>,
    pub dev_type: Option<String>,
}

/// PATCH /api/inventory/devices/:id — partial update. Only provided fields change.
/// If site_key changes to a new value, auto-stubs the new dim_site row.
pub async fn device_patch(
    State(st): State<AppState>,
    Path(id): Path<i64>,
    Json(p): Json<DevicePatch>,
) -> Result<Json<Value>, ApiError> {
    let c = st.pool.get().await?;
    if let Some(ref sk) = p.site_key {
        c.execute(
            "INSERT INTO dim_site (site_key, display_name, is_stub) VALUES ($1, $1, true) \
             ON CONFLICT (site_key) DO NOTHING", &[sk]).await?;
    }
    let row = c.query_one(
        "UPDATE dim_device SET \
           enabled    = COALESCE($2, enabled), \
           site_key   = COALESCE($3, site_key), \
           name       = COALESCE($4, name), \
           notes      = COALESCE($5, notes), \
           dev_type   = COALESCE($6, dev_type), \
           updated_at = now() \
         WHERE id = $1 \
         RETURNING id, host(ip)::text ip, unit_id, site_key, enabled, dev_type, name, updated_at::text ua",
        &[&id, &p.enabled, &p.site_key, &p.name, &p.notes, &p.dev_type]).await?;
    Ok(Json(json!({
        "id":         row.get::<_, i64>("id"),
        "ip":         row.get::<_, String>("ip"),
        "unit_id":    row.get::<_, i16>("unit_id"),
        "site_key":   row.get::<_, String>("site_key"),
        "enabled":    row.get::<_, bool>("enabled"),
        "dev_type":   row.get::<_, String>("dev_type"),
        "name":       row.get::<_, String>("name"),
        "updated_at": row.get::<_, String>("ua"),
    })))
}

/// DELETE /api/inventory/devices/:id — remove device from registry.
pub async fn device_delete(
    State(st): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, ApiError> {
    let c = st.pool.get().await?;
    let n = c.execute("DELETE FROM dim_device WHERE id = $1", &[&id]).await?;
    if n == 0 { return Err(anyhow::anyhow!("device id {id} not found").into()); }
    Ok(Json(json!({ "deleted": n, "id": id })))
}

#[derive(Debug, Deserialize)]
pub struct ClaimInput {
    pub ip:       String,
    pub site_key: String,
    #[serde(default = "inv_port")]    pub port:     i32,
    #[serde(default = "inv_unit")]    pub unit_id:  i16,
    #[serde(default = "inv_dtype")]   pub dev_type: String,
    #[serde(default)]                 pub base0:    bool,
    #[serde(default)]                 pub fne:      bool,
    #[serde(default)]                 pub name:     String,
    #[serde(default)]                 pub notes:    String,
    #[serde(default)]                 pub added_by: String,
}

/// POST /api/inventory/device-orphans/claim
/// Promotes an orphan IP (seen in events but not in dim_device) to a registered device.
/// Validates that the IP actually appears in event history before inserting.
/// Creates a dim_site stub if the given site_key is new.
pub async fn claim_orphan(
    State(st): State<AppState>,
    Json(p): Json<ClaimInput>,
) -> Result<Json<Value>, ApiError> {
    if p.ip.is_empty()       { return Err(anyhow::anyhow!("ip is required").into()); }
    if p.site_key.is_empty() { return Err(anyhow::anyhow!("site_key is required").into()); }
    let added_by = if p.added_by.is_empty() { "operator".to_string() } else { p.added_by };
    let c = st.pool.get().await?;
    // Confirm the IP has event history (avoids registering phantom devices)
    let ev_count: i64 = c.query_one(
        "SELECT count(*)::int8 n FROM fact_event WHERE host(device_ip)::text = $1",
        &[&p.ip]).await?.get("n");
    if ev_count == 0 {
        return Err(anyhow::anyhow!("ip {} has no event history — is it the right address?", p.ip).into());
    }
    c.execute(
        "INSERT INTO dim_site (site_key, display_name, is_stub) VALUES ($1, $1, true) \
         ON CONFLICT (site_key) DO NOTHING",
        &[&p.site_key]).await?;
    let row = c.query_one(
        "INSERT INTO dim_device \
           (ip, port, unit_id, site_key, dev_type, base0, fne, enabled, name, notes, added_by) \
         VALUES ($1::inet, $2, $3, $4, $5, $6, $7, true, $8, $9, $10) \
         ON CONFLICT (ip, unit_id) DO UPDATE SET \
           site_key   = EXCLUDED.site_key, \
           dev_type   = EXCLUDED.dev_type, \
           name       = EXCLUDED.name, \
           notes      = EXCLUDED.notes, \
           updated_at = now() \
         RETURNING id, host(ip)::text ip, unit_id, site_key",
        &[&p.ip, &p.port, &p.unit_id, &p.site_key, &p.dev_type,
          &p.base0, &p.fne, &p.name, &p.notes, &added_by]).await?;
    Ok(Json(json!({
        "id":            row.get::<_, i64>("id"),
        "ip":            row.get::<_, String>("ip"),
        "unit_id":       row.get::<_, i16>("unit_id"),
        "site_key":      row.get::<_, String>("site_key"),
        "event_history": ev_count,
        "claimed":       true,
    })))
}

#[derive(Debug, Deserialize)]
pub struct SitePatch {
    pub display_name: Option<String>,
    pub region:       Option<String>,
    pub municipality: Option<String>,
    pub technologies: Option<Vec<String>>,
    pub latitude:     Option<f64>,
    pub longitude:    Option<f64>,
    pub has_genset:   Option<bool>,
    pub has_battery:  Option<bool>,
    pub has_solar:    Option<bool>,
    pub is_important: Option<bool>,
}

/// PATCH /api/sites/:site_key — enrich / update a site's dimension data.
/// Uses COALESCE so only provided fields are changed.
/// Automatically clears is_stub when region is supplied (operator has classified the site).
/// Creates the site row first if it doesn't exist yet (safe to call on any site_key).
pub async fn site_patch(
    State(st): State<AppState>,
    Path(site_key): Path<String>,
    Json(p): Json<SitePatch>,
) -> Result<Json<Value>, ApiError> {
    let c = st.pool.get().await?;
    // ensure row exists (idempotent bootstrap for any site_key)
    c.execute(
        "INSERT INTO dim_site (site_key, display_name, is_stub) VALUES ($1, $1, true) \
         ON CONFLICT (site_key) DO NOTHING",
        &[&site_key]).await?;
    // providing region = operator has reviewed this site → clear stub flag
    let clears_stub = p.region.is_some();
    let row = c.query_one(
        "UPDATE dim_site SET \
           display_name = COALESCE($2,  display_name), \
           region       = COALESCE($3,  region), \
           municipality = COALESCE($4,  municipality), \
           technologies = COALESCE($5,  technologies), \
           latitude     = COALESCE($6,  latitude), \
           longitude    = COALESCE($7,  longitude), \
           has_genset   = COALESCE($8,  has_genset), \
           has_battery  = COALESCE($9,  has_battery), \
           has_solar    = COALESCE($10, has_solar), \
           is_important = COALESCE($11, is_important), \
           is_stub      = CASE WHEN $12 THEN false ELSE is_stub END, \
           updated_at   = now() \
         WHERE site_key = $1 \
         RETURNING site_key, \
                   COALESCE(display_name,'') dn, \
                   COALESCE(region,'') rg, \
                   COALESCE(municipality,'') mu, \
                   is_stub, \
                   updated_at::text ua",
        &[&site_key, &p.display_name, &p.region, &p.municipality,
          &p.technologies, &p.latitude, &p.longitude,
          &p.has_genset, &p.has_battery, &p.has_solar, &p.is_important,
          &clears_stub]).await?;
    Ok(Json(json!({
        "site_key":     row.get::<_, String>("site_key"),
        "display_name": row.get::<_, String>("dn"),
        "region":       row.get::<_, String>("rg"),
        "municipality": row.get::<_, String>("mu"),
        "is_stub":      row.get::<_, bool>("is_stub"),
        "updated_at":   row.get::<_, String>("ua"),
    })))
}

// ============================================================ v7 ADMIN

#[derive(Debug, Deserialize)]
pub struct UserInput {
    pub username:  String,
    #[serde(default)] pub full_name: String,
    #[serde(default = "default_role")] pub role: String,
    #[serde(default)] pub region: String,
    #[serde(default)] pub disabled: bool,
}
fn default_role() -> String { "user".into() }

/// POST /api/admin/users  — create (upsert by username)
pub async fn admin_user_upsert(
    State(st): State<AppState>,
    Json(p): Json<UserInput>,
) -> Result<Json<Value>, ApiError> {
    // role validation (avoid SQL enum cast failing with a 500)
    if !matches!(p.role.as_str(), "superadmin" | "admin" | "user") {
        return Err(ApiError::from(anyhow::anyhow!("invalid role; must be superadmin|admin|user")));
    }
    if p.username.is_empty() {
        return Err(ApiError::from(anyhow::anyhow!("username is required")));
    }
    let c = st.pool.get().await?;
    let row = c.query_one(
        "INSERT INTO dim_user (username, full_name, role, region, disabled) \
         VALUES ($1, $2, $3::user_role_t, NULLIF($4,''), $5) \
         ON CONFLICT (username) DO UPDATE SET \
           full_name = EXCLUDED.full_name, \
           role      = EXCLUDED.role, \
           region    = EXCLUDED.region, \
           disabled  = EXCLUDED.disabled \
         RETURNING id, created_at::text ca",
        &[&p.username, &p.full_name, &p.role, &p.region, &p.disabled]).await?;
    Ok(Json(json!({
        "id":         row.get::<_, i64>("id"),
        "username":   p.username,
        "created_at": row.get::<_, String>("ca"),
    })))
}

/// DELETE /api/admin/users/:id — remove user (audit trail kept via PG row history if extension enabled; here just plain DELETE)
pub async fn admin_user_delete(
    State(st): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, ApiError> {
    let c = st.pool.get().await?;
    let n = c.execute("DELETE FROM dim_user WHERE id = $1", &[&id]).await?;
    Ok(Json(json!({ "deleted": n, "id": id })))
}
