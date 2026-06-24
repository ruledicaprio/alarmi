//! Configuration and API response types.

use serde::Deserialize;
use std::collections::HashMap;

/// Top-level config loaded from TOML + env overrides.
#[derive(Debug, Deserialize)]
pub struct Config {
    pub neteco: NetEcoConfig,
    #[serde(default)]
    pub database: DbConfig,
    #[serde(default)]
    pub intervals: Intervals,
}

#[derive(Debug, Deserialize)]
pub struct NetEcoConfig {
    /// e.g. "https://10.10.0.3:31943"
    pub url: String,
    /// NBI third-party user
    pub user: String,
    /// NBI password
    pub password: String,
    /// Path to NetEco CA cert PEM (optional — if absent, uses system trust store)
    #[serde(default)]
    pub ca_cert_path: Option<String>,
    /// Accept invalid TLS certs (dev only, never in production)
    #[serde(default)]
    pub danger_accept_invalid_certs: bool,
}

#[derive(Debug, Default, Deserialize)]
pub struct DbConfig {
    #[serde(default)]
    pub dsn: String,
}

#[derive(Debug, Deserialize)]
pub struct Intervals {
    /// Topology refresh (sites + devices) in seconds. Default: 6h.
    #[serde(default = "default_topo")]
    pub topology_secs: u64,
    /// Metric poll interval in seconds. Default: 300 (5 min).
    #[serde(default = "default_metrics")]
    pub metrics_secs: u64,
    /// Alarm poll interval in seconds. Default: 120 (2 min).
    #[serde(default = "default_alarms")]
    pub alarms_secs: u64,
}

impl Default for Intervals {
    fn default() -> Self {
        Self {
            topology_secs: default_topo(),
            metrics_secs: default_metrics(),
            alarms_secs: default_alarms(),
        }
    }
}

fn default_topo() -> u64 { 21600 }
fn default_metrics() -> u64 { 300 }
fn default_alarms() -> u64 { 120 }

/// Cached device record from neteco.devices.
#[derive(Debug, Clone)]
pub struct DeviceRecord {
    pub device_id: i64,
    pub station_code: String,
    pub dev_type_id: i32,
    pub std_type_name: Option<String>,
}

/// NBI login response (token comes from header, failCode from body).
#[derive(Debug, Deserialize)]
pub struct NbiResponse {
    #[serde(default, rename = "failCode")]
    pub fail_code: i64,
    #[serde(default)]
    pub data: serde_json::Value,
    #[serde(default)]
    pub message: Option<String>,
}

/// getStationList → data[]
#[derive(Debug, Deserialize)]
pub struct Station {
    #[serde(rename = "stationCode")]
    pub station_code: String,
    #[serde(rename = "stationName")]
    pub station_name: Option<String>,
}

/// getDevList → data[]
#[derive(Debug, Deserialize)]
pub struct Device {
    pub id: i64,
    #[serde(rename = "devName")]
    pub dev_name: Option<String>,
    #[serde(rename = "devTypeId")]
    pub dev_type_id: i32,
    #[serde(rename = "esnCode")]
    pub esn_code: Option<String>,
    #[serde(rename = "stationCode")]
    pub station_code: String,
    #[serde(default)]
    pub longitude: Option<f64>,
    #[serde(default)]
    pub latitude: Option<f64>,
}

/// getDevRealKpi → data[]
#[derive(Debug, Deserialize)]
pub struct KpiRecord {
    #[serde(rename = "devId")]
    pub dev_id: i64,
    #[serde(default, rename = "dataItemMap")]
    pub data_item_map: HashMap<String, serde_json::Value>,
}

/// getAlarmList → data[]
#[derive(Debug, Deserialize)]
pub struct Alarm {
    #[serde(rename = "alarmId")]
    pub alarm_id: Option<String>,
    #[serde(rename = "stationCode")]
    pub station_code: Option<String>,
    #[serde(rename = "stationName")]
    pub station_name: Option<String>,
    #[serde(rename = "devId")]
    pub dev_id: Option<i64>,
    #[serde(rename = "devName")]
    pub dev_name: Option<String>,
    #[serde(rename = "devTypeId")]
    pub dev_type_id: Option<i32>,
    #[serde(rename = "alarmName")]
    pub alarm_name: Option<String>,
    #[serde(rename = "alarmCause")]
    pub alarm_cause: Option<String>,
    /// Numeric alarm type from the alarm catalog (may be absent in REST response)
    #[serde(default, rename = "alarmType")]
    pub alarm_type: Option<i16>,
    #[serde(default)]
    pub lev: Option<i16>,
    #[serde(default)]
    pub status: Option<i16>,
    #[serde(rename = "raiseTime")]
    pub raise_time: Option<i64>,
    #[serde(rename = "repairTime")]
    pub repair_time: Option<i64>,
}
