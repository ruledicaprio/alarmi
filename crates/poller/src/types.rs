//! Config model (deserialized from config/*.toml). Alarm class/severity strings
//! deserialize straight into the Stage-1 canonical enums, so a bad label fails
//! fast at load time.

use bht_normalize::{AlarmClass, Severity};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct PollerConfig {
    #[serde(default = "d_interval")]   pub poll_interval_secs: u64,
    #[serde(default = "d_concurrent")] pub max_concurrent: usize,
    #[serde(default = "d_conn_to")]    pub connect_timeout_ms: u64,
    #[serde(default = "d_read_to")]    pub read_timeout_ms: u64,
    #[serde(default = "d_retries")]    pub retries: u32,
    #[serde(default = "d_backoff")]    pub retry_backoff_ms: u64,
    #[serde(default = "d_block")]      pub discrete_block_size: u16,
    #[serde(default)]                  pub circuit_breaker: BreakerCfg,
    #[serde(default)]                  pub database: DbCfg,
}
fn d_interval()->u64{120} fn d_concurrent()->usize{8} fn d_conn_to()->u64{3500}
fn d_read_to()->u64{3500} fn d_retries()->u32{2} fn d_backoff()->u64{200} fn d_block()->u16{8}

#[derive(Debug, Clone, Deserialize)]
pub struct BreakerCfg {
    #[serde(default = "d_fail_thr")]  pub failure_threshold: u32,
    #[serde(default = "d_cooldown")]  pub open_cooldown_secs: u64,
}
fn d_fail_thr()->u32{3} fn d_cooldown()->u64{300}
impl Default for BreakerCfg { fn default()->Self{ Self{failure_threshold:d_fail_thr(), open_cooldown_secs:d_cooldown()} } }

#[derive(Debug, Clone, Default, Deserialize)]
pub struct DbCfg {
    #[serde(default)] pub dsn: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DevicesFile { pub device: Vec<DeviceCfg> }

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct DeviceCfg {
    pub ip: String,
    #[serde(default = "d_port")] pub port: u16,
    #[serde(default = "d_unit")] pub unit: u8,
    pub site_key: String,
    #[serde(default)] pub name: String,
    #[serde(rename = "type", default = "d_dtype")] pub dev_type: String,
    #[serde(default)] pub base0: bool,
    #[serde(default)] pub fne: bool,
    #[serde(default = "d_enabled")] pub enabled: bool,
}
fn d_port()->u16{502} fn d_unit()->u8{1} fn d_dtype()->String{"eaton".into()} fn d_enabled()->bool{true}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct AlarmFile {
    #[serde(default)] pub alarm: Vec<AlarmDef>,
    #[serde(default)] pub coil: Vec<AlarmDef>,
    #[serde(default)] pub summary_bit: Vec<SummaryBit>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AlarmDef {
    pub addr: u16,
    pub name: String,
    pub class: AlarmClass,
    pub severity: Severity,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct SummaryBit { pub addr: u16, pub label: String }

// Huawei SmartLogger 3000 alarm bitmap entry (register, bit).
#[derive(Debug, Clone, Deserialize)]
pub struct SlAlarmDef {
    pub addr: u16,
    pub bit:  u8,
    pub name: String,
    pub class: AlarmClass,
    pub severity: Severity,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SlAlarmFile {
    #[serde(default)] pub alarm: Vec<SlAlarmDef>,
}
