//! bht-api — Axum ingest/query API over the BHT TimescaleDB schema.
//! Plain HTTP + NoTls Postgres (isolated LAN). Build static musl for Rocky.

mod db;
mod ingest;
mod query;

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use deadpool_postgres::Pool;
use serde::Deserialize;
use serde_json::json;
use std::net::SocketAddr;
use tower_http::cors::{Any, CorsLayer};

#[derive(Clone)]
pub struct AppState {
    pub pool: Pool,
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
    #[serde(default)]
    database: DbCfg,
}
#[derive(Debug, Default, Deserialize)]
struct DbCfg {
    #[serde(default)]
    dsn: String,
}
fn default_bind() -> String { "0.0.0.0:8080".into() }

fn load_cfg() -> ApiCfg {
    let mut path = "config/api.toml".to_string();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        if a == "--config" { if let Some(p) = args.next() { path = p; } }
    }
    let mut cfg: ApiCfg = std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| toml::from_str(&s).ok())
        .unwrap_or_else(|| ApiCfg { bind: default_bind(), database: DbCfg::default() });
    // env wins (containers inject DATABASE_URL=host=timescaledb ...)
    if let Ok(url) = std::env::var("DATABASE_URL") { cfg.database.dsn = url; }
    if cfg.database.dsn.is_empty() {
        cfg.database.dsn = "host=localhost port=5432 user=bht password=bht_dev_pw dbname=alarms".into();
    }
    cfg
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cfg = load_cfg();
    let pool = db::make_pool(&cfg.database.dsn)?;
    // fail fast if the DB is unreachable
    let _ = pool.get().await.map_err(|e| anyhow::anyhow!("DB connect failed: {e}"))?;
    let state = AppState { pool };

    let cors = CorsLayer::new().allow_origin(Any).allow_methods(Any).allow_headers(Any);
    let app = Router::new()
        .route("/api/health", get(query::health))
        .route("/api/sites", get(query::sites))
        .route("/api/alarms/active", get(query::active_alarms))
        .route("/api/alarms/recent", get(query::recent_alarms))
        .route("/api/sites/:site_key/reliability", get(query::site_reliability))
        .route("/api/sites/:site_key/measurements", get(query::site_measurements))
        .route("/api/measurements/latest", get(query::latest_measurements))
        .route("/api/stats/by-class", get(query::stats_by_class))
        .route("/api/stats/by-region", get(query::stats_by_region))
        .route("/ingest/raw/ispadnap", post(ingest::ingest_raw_ispadnap))
        .route("/ingest/events", post(ingest::ingest_events))
        .route("/ingest/measurements", post(ingest::ingest_measurements))
        .with_state(state)
        .layer(cors);

    let addr: SocketAddr = cfg.bind.parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    eprintln!("[api] listening on http://{addr}");
    axum::serve(listener, app).await?;
    Ok(())
}
