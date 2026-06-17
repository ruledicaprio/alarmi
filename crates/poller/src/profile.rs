//! Eaton SC200/300 register layout + resolved alarm map.
//! Register addresses/scaling mirror modbus_working.py (the validated reference).

use crate::types::{AlarmDef, AlarmFile};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub enum Decode { F32, U32, I32 }

#[derive(Debug, Clone, Copy)]
pub struct MetricDef {
    pub key: &'static str,   // stored as fact_measurement.metric
    pub doc_addr: u16,       // input register, count = 2
    pub decode: Decode,
    pub scale: f64,
    pub fne_only: bool,
}

/// Input-register measurements (function 0x04, 2 regs each).
pub const METRICS: &[MetricDef] = &[
    MetricDef { key: "u_battery_v", doc_addr: 7001, decode: Decode::F32, scale: 1.0, fne_only: false },
    MetricDef { key: "p_load_kw",   doc_addr: 7009, decode: Decode::F32, scale: 1.0, fne_only: false },
    MetricDef { key: "ac_voltage_v",doc_addr: 7017, decode: Decode::F32, scale: 1.0, fne_only: false },
    MetricDef { key: "p_solar_kw",  doc_addr: 7317, decode: Decode::F32, scale: 1.0, fne_only: true  },
    MetricDef { key: "e_total_kwh", doc_addr: 7031, decode: Decode::F32, scale: 1.0, fne_only: true  },
    MetricDef { key: "e_load_kwh",  doc_addr: 7035, decode: Decode::F32, scale: 1.0, fne_only: true  },
];

/// Discrete-input alarm segments (function 0x02), documented 1-based ranges.
pub const ALARM_SEGMENTS: &[(u16, u16)] = &[(1101, 1107), (1201, 1272), (1301, 1304)];
/// Summary status bits live at 1001..1004.
pub const SUMMARY_ADDR: u16 = 1001;
pub const SUMMARY_COUNT: u16 = 4;
/// Coil alarms (function 0x01) documented 1..12.
pub const COIL_START: u16 = 1;
pub const COIL_COUNT: u16 = 12;

/// Resolved alarm lookup tables built from config/eaton_alarms.toml.
pub struct Profile {
    pub discrete: HashMap<u16, AlarmDef>,
    pub coils: HashMap<u16, AlarmDef>,
}

impl Profile {
    pub fn from_file(f: AlarmFile) -> Self {
        let discrete = f.alarm.into_iter().map(|a| (a.addr, a)).collect();
        let coils = f.coil.into_iter().map(|a| (a.addr, a)).collect();
        Self { discrete, coils }
    }
}
