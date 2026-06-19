//! Canonical data model shared by every source parser.
//!
//! One physical condition (e.g. loss of mains) arrives under many raw names
//! across U2020 / NetEco / IgnitionSCADA / Eaton-Modbus / DSE / Benning / BARAN
//! and the HTML out-of-service table. Normalization collapses all of them onto
//! these enums so the dashboard and history queries speak one language.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Originating collector / upstream system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Ignition,   // IgnitionSCADA  — severity feed, count-only
    NetEco,     // Huawei NetEco  — raises only, count-only
    U2020,      // Huawei U2020   — stateful (major/cleared)
    RpsSc200,   // Eaton SC200 via RPS-SC200-MIB — stateful
    RpsSc300,   // Eaton SC300 via RpsSc300Mib   — stateful
    Dse74xx,    // DSE 7410/7420 genset SNMP     — stateful/events
    Benning,    // Benning rectifier DCMCUMIB    — stateful (added/removed)
    Baran,      // BARAN FCS cooling controller  — stateful
    ModbusEaton,// Direct Modbus poll of SC200/300 (future stage) — stateful
    HtmlOos,    // /alarmi/ out-of-service table  — stateful service outage
    SmartloggerHuawei, // Huawei SmartLogger 3000 PV inverter poll — stateful
}

impl Source {
    /// Sources that emit BOTH raise and clear, so RAISE→CLEAR durations pair.
    pub fn is_stateful(self) -> bool {
        matches!(
            self,
            Source::U2020 | Source::RpsSc200 | Source::RpsSc300
                | Source::Dse74xx | Source::Benning | Source::Baran
                | Source::ModbusEaton | Source::HtmlOos | Source::SmartloggerHuawei
        )
    }
}

/// Normalized alarm taxonomy — the cross-source classification target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AlarmClass {
    NeDisconnected,
    CommsLost,
    MainsFailure,
    RectifierFailure,
    RectifierComms,
    SolarFault,
    UpsModule,
    BatteryLow,
    BatteryFault,
    HighVoltage,
    GensetEvent,
    CoolingFault,
    DoorOpen,
    FuseLoad,
    GenericError,
    ServiceOutage,
    Unclassified,
}

/// Normalized severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Critical,
    Major,
    Minor,
    Warning,
    Info,
}

/// State transition used to pair durations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Transition {
    Raise,
    Clear,
    Instant, // point-in-time event (count metric, no duration)
}

/// One normalized event — the row written to `fact_event`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanonicalEvent {
    pub event_time: DateTime<Utc>,
    pub source: Source,
    pub raw_site: String,
    pub site_key: String,
    pub region: String,
    pub alarm_class: AlarmClass,
    pub severity: Severity,
    pub transition: Transition,
    pub raw_alarm: String,
    pub device_ip: Option<String>,
}

/// Why a raw line could not be normalized (kept for the quarantine table).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DropReason {
    BlankOrNoComma,
    UnknownSystem(String),
    FieldCount(String),
    BadTimestamp(String),
}
