//! bht-api — Axum ingest/query API over the BHT TimescaleDB schema.
//! Plain HTTP + NoTls Postgres (isolated LAN). Build static musl for Rocky.

mod db;
mod ingest;
mod query;

use anyhow::Context as _;
use axum::extract::DefaultBodyLimit;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, patch, post};
use axum::{Json, Router};
use deadpool_postgres::Pool;
use serde::Deserialize;
use serde_json::json;
use std::net::SocketAddr;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};

#[derive(Clone)]
pub struct AppState {
    pub pool: Pool,
    /// Static secret for NetEco push OAuth2 Bearer auth. Empty = auth disabled.
    pub neteco_push_secret: String,
}

/// Handler error -> 500 + JSON. Lets handlers use `?`.
pub struct ApiError(anyhow::Error);
impl<E: Into<anyhow::Error>> From<E> for ApiError {
    fn from(e: E) -> Self { ApiError(e.into()) }
}
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": self.0.to_string() }))).into_response()
    }
}
pub type ApiResult = Result<Json<serde_json::Value>, ApiError>;

#[derive(Debug, Deserialize)]
struct ApiCfg {
    #[serde(default = "default_bind")]
    bind: String,
    #[serde(default = "default_static")]
    static_dir: String,
    #[serde(default)]
    database: DbCfg,
    #[serde(default)]
    neteco_auth: NetecoAuthCfg,
    #[serde(default)]
    tls: TlsCfg,
}
#[derive(Debug, Default, Deserialize)]
struct DbCfg {
    #[serde(default)]
    dsn: String,
}
#[derive(Debug, Default, Deserialize)]
struct TlsCfg {
    /// e.g. "0.0.0.0:8443" — leave empty to disable TLS
    #[serde(default)]
    bind: String,
    #[serde(default)]
    cert: String,
    #[serde(default)]
    key: String,
}

#[derive(Debug, Default, Deserialize)]
struct NetecoAuthCfg {
    /// Secret NetEco sends as client_secret to /api/neteco/token.
    /// Also the Bearer token it must include on push calls.
    /// Leave empty to disable auth (open push endpoint).
    #[serde(default)]
    client_secret: String,
}
fn default_bind() -> String { "0.0.0.0:8080".into() }
fn default_static() -> String { "web/dist".into() }

fn load_cfg() -> ApiCfg {
    let mut path = "config/api.toml".to_string();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        if a == "--config" { if let Some(p) = args.next() { path = p; } }
    }
    let mut cfg: ApiCfg = std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| toml::from_str(&s).ok())
        .unwrap_or_else(|| ApiCfg { bind: default_bind(), static_dir: default_static(), database: DbCfg::default(), neteco_auth: NetecoAuthCfg::default(), tls: TlsCfg::default() });
    // env wins (containers inject DATABASE_URL=host=timescaledb ...)
    if let Ok(url) = std::env::var("DATABASE_URL") { cfg.database.dsn = url; }
    if cfg.database.dsn.is_empty() {
        cfg.database.dsn = "host=localhost port=5432 user=bht password=bht_dev_pw dbname=alarms".into();
    }
    cfg
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // rustls 0.23 requires an explicit crypto provider before any TLS operation.
    rustls::crypto::ring::default_provider().install_default().ok();
    let cfg = load_cfg();
    let pool = db::make_pool(&cfg.database.dsn)?;
    // fail fast if the DB is unreachable
    let _ = pool.get().await.map_err(|e| anyhow::anyhow!("DB connect failed: {e}"))?;
    let state = AppState { pool, neteco_push_secret: cfg.neteco_auth.client_secret.clone() };

    // Background: keep fact_alarm_episode current. On startup do a full rebuild
    // (catches up any events that landed before this version of the API), then
    // re-window every 60s to capture recent transitions.
    {
        let pool = state.pool.clone();
        tokio::spawn(async move {
            match pool.get().await {
                Ok(c) => {
                    if let Err(e) = c.execute("SELECT rebuild_episodes('-infinity')", &[]).await {
                        eprintln!("[episode-rebuild] startup full rebuild: {e}");
                    } else {
                        eprintln!("[episode-rebuild] startup full rebuild OK");
                    }
                }
                Err(e) => eprintln!("[episode-rebuild] startup db pool: {e}"),
            }
            let mut tick = tokio::time::interval(std::time::Duration::from_secs(60));
            tick.tick().await; // skip immediate fire (we just rebuilt)
            loop {
                tick.tick().await;
                match pool.get().await {
                    Ok(c) => {
                        if let Err(e) = c.execute(
                            "SELECT rebuild_episodes(now() - INTERVAL '15 minutes')", &[]).await {
                            eprintln!("[episode-rebuild] {e}");
                        }
                    }
                    Err(e) => eprintln!("[episode-rebuild] db pool: {e}"),
                }
            }
        });
    }

    let cors = CorsLayer::new().allow_origin(Any).allow_methods(Any).allow_headers(Any);
    let app = Router::new()
        .route("/api/health", get(query::health))
        .route("/api/sites", get(query::sites))
        .route("/api/regions", get(query::regions))
        .route("/api/alarms/active", get(query::active_alarms))
        .route("/api/alarms/recent", get(query::recent_alarms))
        .route("/api/sites/:site_key/reliability",  get(query::site_reliability))
        .route("/api/sites/:site_key/measurements", get(query::site_measurements))
        .route("/api/sites/:site_key/timeline",     get(query::site_timeline))
        .route("/api/sites/:site_key/episodes",     get(query::site_episodes))
        .route("/api/measurements/latest",  get(query::latest_measurements))
        .route("/api/stats/by-class",       get(query::stats_by_class))
        .route("/api/stats/by-region",      get(query::stats_by_region))
        .route("/api/stats/sources",        get(query::stats_sources))
        .route("/api/stats/timeseries",     get(query::stats_timeseries))
        .route("/api/map/sites",            get(query::map_sites))
        .route("/api/inventory/orphans",    get(query::inventory_orphans))
        .route("/api/inventory/stale",      get(query::inventory_stale))
        .route("/api/inventory/coverage",   get(query::inventory_coverage))
        // v8 inventory management
        .route("/api/inventory/devices",
               get(query::inventory_devices).post(ingest::device_upsert))
        .route("/api/inventory/devices/:id",
               patch(ingest::device_patch).delete(ingest::device_delete))
        .route("/api/inventory/device-orphans",
               get(query::inventory_device_orphans))
        .route("/api/inventory/device-orphans/claim",
               post(ingest::claim_orphan))
        .route("/api/inventory/stubs",      get(query::inventory_stubs))
        .route("/api/sites/:site_key",      patch(ingest::site_patch))
        .route("/api/measurements/metrics", get(query::measurement_metrics))
        .route("/api/sites/:site_key/ips",     get(query::site_ips))
        .route("/api/sites/:site_key/related", get(query::site_related))
        .route("/api/sites/:site_key/verification",         get(query::site_verification))
        .route("/api/sites/:site_key/verification/summary", get(query::site_verification_summary))
        .route("/api/sites/:site_key/verify",              post(ingest::site_verify))
        // v7 admin / system / inventory
        .route("/api/admin/users",          get(query::admin_users).post(ingest::admin_user_upsert))
        .route("/api/admin/users/:id",      delete(ingest::admin_user_delete))
        .route("/api/admin/regions",        get(query::admin_regions))
        .route("/api/inventory/verified",   get(query::inventory_verified))
        .route("/api/system/status",        get(query::system_status))
        .route("/api/system/journal",       get(query::system_journal))
        // v7 solar (per-source stacked + family-tagged sites)
        .route("/api/solar/summary",        get(query::solar_summary_v7))
        .route("/api/solar/sites",          get(query::solar_sites_v7))
        // neteco NBI
        .route("/api/neteco/alarms",         get(query::neteco_alarms))
        .route("/api/neteco/alarms/summary", get(query::neteco_alarm_summary))
        .route("/api/neteco/token",          post(ingest::neteco_oauth_token))
        .route("/ingest/neteco/push",        post(ingest::ingest_neteco_push))
        .route("/ingest/raw/ispadnap", post(ingest::ingest_raw_ispadnap))
        .route("/ingest/raw/smetnje", post(ingest::ingest_raw_smetnje))
        .route("/ingest/events", post(ingest::ingest_events))
        .route("/ingest/measurements", post(ingest::ingest_measurements))
        // raw log feeds (napajanjeranW.log etc.) can exceed Axum's 2 MB default.
        // 32 MB headroom covers years of append-only growth.
        .layer(DefaultBodyLimit::max(32 * 1024 * 1024))
        .with_state(state);

    let app = if std::path::Path::new(&cfg.static_dir).exists() {
        let index = format!("{}/index.html", cfg.static_dir);
        eprintln!("[api] serving UI from {}", cfg.static_dir);
        app.fallback_service(ServeDir::new(&cfg.static_dir).fallback(ServeFile::new(index)))
    } else {
        app
    };
    let app = app.layer(cors);

    // Optional TLS listener (shares same router, spawned as background task)
    if !cfg.tls.bind.is_empty() && !cfg.tls.cert.is_empty() && !cfg.tls.key.is_empty() {
        let tls_cfg = axum_server::tls_rustls::RustlsConfig::from_pem_file(&cfg.tls.cert, &cfg.tls.key)
            .await
            .with_context(|| format!("load TLS cert={} key={}", cfg.tls.cert, cfg.tls.key))?;
        let tls_addr: SocketAddr = cfg.tls.bind.parse()?;
        eprintln!("[api] TLS listening on https://{tls_addr}");
        let app_tls = app.clone();
        tokio::spawn(async move {
            if let Err(e) = axum_server::bind_rustls(tls_addr, tls_cfg)
                .serve(app_tls.into_make_service())
                .await
            {
                eprintln!("[api] TLS server error: {e}");
            }
        });
    }

    let addr: SocketAddr = cfg.bind.parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    eprintln!("[api] listening on http://{addr}");
    axum::serve(listener, app).await?;
    Ok(())
}
