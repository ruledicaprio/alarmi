//! NetEco NBI REST client — auth, token lifecycle, typed API calls.
//!
//! Token valid for 30 min (hardcoded by NetEco). We refresh at the 25-min mark.
//! Login rate limit: 5 calls / 10 min per user — serialized, never concurrent.

use crate::types::{Alarm, Device, KpiRecord, NbiResponse, NetEcoConfig, Station};
use anyhow::{bail, Context, Result};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Thread-safe NetEco NBI client with lazy token refresh.
pub struct NetEcoClient {
    http: reqwest::Client,
    base_url: String,
    user: String,
    password: String,
    token: Arc<Mutex<TokenState>>,
}

struct TokenState {
    value: Option<String>,
    acquired_at: std::time::Instant,
}

impl TokenState {
    fn new() -> Self {
        Self {
            value: None,
            acquired_at: std::time::Instant::now(),
        }
    }

    fn is_expired(&self) -> bool {
        // Refresh at 25 min (1500s) — token valid 30 min
        self.value.is_none() || self.acquired_at.elapsed().as_secs() > 1500
    }
}

impl NetEcoClient {
    pub fn new(cfg: &NetEcoConfig) -> Result<Self> {
        let mut builder = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .connect_timeout(std::time::Duration::from_secs(10));

        if let Some(ref ca_path) = cfg.ca_cert_path {
            let pem = std::fs::read(ca_path)
                .with_context(|| format!("read CA cert: {ca_path}"))?;
            let cert = reqwest::Certificate::from_pem(&pem)?;
            builder = builder.add_root_certificate(cert);
        }

        if cfg.danger_accept_invalid_certs {
            eprintln!("[neteco] WARNING: accepting invalid TLS certs (dev mode)");
            builder = builder.danger_accept_invalid_certs(true);
        }

        let http = builder.build()?;
        Ok(Self {
            http,
            base_url: cfg.url.trim_end_matches('/').to_string(),
            user: cfg.user.clone(),
            password: cfg.password.clone(),
            token: Arc::new(Mutex::new(TokenState::new())),
        })
    }

    /// Ensure we have a valid token, re-logging-in if needed.
    async fn ensure_token(&self) -> Result<String> {
        let mut ts = self.token.lock().await;
        if !ts.is_expired() {
            return Ok(ts.value.clone().unwrap());
        }

        eprintln!("[neteco] logging in as '{}'", self.user);
        let url = format!("{}/thirdData/login", self.base_url);
        let resp = self.http.post(&url)
            .json(&serde_json::json!({
                "userName": self.user,
                "systemCode": self.password
            }))
            .send()
            .await
            .context("login request")?;

        // Token comes from the XSRF-TOKEN response header
        let token = resp.headers()
            .get("xsrf-token")
            .or_else(|| resp.headers().get("XSRF-TOKEN"))
            .map(|v| v.to_str().unwrap_or("").to_string())
            .filter(|t| !t.is_empty());

        let body: NbiResponse = resp.json().await.context("login response body")?;
        if body.fail_code != 0 {
            bail!("login failCode={}: {:?}", body.fail_code, body.message);
        }

        let token = token.context("no XSRF-TOKEN in login response")?;
        ts.value = Some(token.clone());
        ts.acquired_at = std::time::Instant::now();
        eprintln!("[neteco] login OK, token acquired");
        Ok(token)
    }

    /// Force token re-acquisition on next call.
    pub async fn invalidate_token(&self) {
        let mut ts = self.token.lock().await;
        ts.value = None;
    }

    /// POST to an NBI endpoint with auto-auth. Returns parsed NbiResponse.
    async fn post(&self, path: &str, body: &serde_json::Value) -> Result<NbiResponse> {
        let token = self.ensure_token().await?;
        let url = format!("{}{}", self.base_url, path);

        let resp = self.http.post(&url)
            .header("XSRF-TOKEN", &token)
            .json(body)
            .send()
            .await
            .with_context(|| format!("POST {path}"))?;

        let nbi: NbiResponse = resp.json().await
            .with_context(|| format!("parse response from {path}"))?;

        // Handle known error codes
        match nbi.fail_code {
            0 => Ok(nbi),
            401 => {
                eprintln!("[neteco] token expired (401), invalidating");
                self.invalidate_token().await;
                bail!("token expired (401)")
            }
            407 => {
                eprintln!("[neteco] rate limit (407)");
                bail!("rate limit hit (407)")
            }
            code => {
                bail!("NBI failCode={}: {:?}", code, nbi.message)
            }
        }
    }

    // ------------------------------------------------------------------ typed API

    /// Fetch all sites.
    pub async fn get_stations(&self) -> Result<Vec<Station>> {
        let resp = self.post("/thirdData/getStationList", &serde_json::json!({})).await?;
        let stations: Vec<Station> = serde_json::from_value(resp.data)
            .context("parse station list")?;
        Ok(stations)
    }

    /// Fetch devices for given station codes (comma-separated).
    pub async fn get_devices(&self, station_codes: &str) -> Result<Vec<Device>> {
        let resp = self.post("/thirdData/getDevList", &serde_json::json!({
            "stationCodes": station_codes
        })).await?;
        let devices: Vec<Device> = serde_json::from_value(resp.data)
            .context("parse device list")?;
        Ok(devices)
    }

    /// Fetch real-time KPIs for a device type + list of device IDs.
    pub async fn get_dev_kpi(&self, dev_type_id: i32, dev_ids: &str) -> Result<Vec<KpiRecord>> {
        let resp = self.post("/thirdData/getDevRealKpi", &serde_json::json!({
            "devTypeId": dev_type_id,
            "devIds": dev_ids
        })).await?;
        let records: Vec<KpiRecord> = serde_json::from_value(resp.data)
            .context("parse KPI data")?;
        Ok(records)
    }

    /// Fetch active alarms for given station codes.
    pub async fn get_alarms(&self, station_codes: &str, status: i32) -> Result<Vec<Alarm>> {
        let resp = self.post("/thirdData/getAlarmList", &serde_json::json!({
            "stationCodes": station_codes,
            "status": status
        })).await?;
        let alarms: Vec<Alarm> = serde_json::from_value(resp.data)
            .context("parse alarm list")?;
        Ok(alarms)
    }
}
