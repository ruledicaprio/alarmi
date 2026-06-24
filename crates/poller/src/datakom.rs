//! DATAKOM D-500 MK3 genset controller poll over Modbus TCP.
//!
//! Register map per "500 Modbus Application Manual" rev 03 (firmware 5.6).
//! All addresses are decimal, FC 03 (Read Holding Registers), high-word-first
//! for 32-bit values. The D-500 limits each read to 16 registers max.
//!
//! Alarm model: three 256-bit bitmaps (16 registers × 16 bits each) sharing
//! the same bit→name mapping, differentiated only by severity:
//!   10504–10519  Shutdown  → Critical
//!   10520–10535  Loaddump  → Major
//!   10536–10551  Warning   → Warning

use crate::state::Active;
use crate::types::{AlarmDef, DeviceCfg, DkAlarmFile, PollerConfig};
use anyhow::{anyhow, Context, Result};
use bht_normalize::{AlarmClass, Severity};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::{sleep, timeout};
use tokio_modbus::client::{tcp, Context as ModbusContext};
use tokio_modbus::prelude::*;

// ---------------------------------------------------------------------------
// Profile
// ---------------------------------------------------------------------------

pub struct DatakomProfile {
    /// bit index → (name, AlarmClass) — shared across all three severity ranges
    pub bits: HashMap<u16, (String, AlarmClass)>,
}

impl DatakomProfile {
    pub fn from_file(f: DkAlarmFile) -> Self {
        let bits = f.alarm.into_iter().map(|a| (a.bit, (a.name, a.class))).collect();
        Self { bits }
    }
}

// ---------------------------------------------------------------------------
// Register layout
// ---------------------------------------------------------------------------

/// Alarm bitmap register ranges (start_reg, count_regs=16, severity).
const ALARM_RANGES: &[(u16, Severity)] = &[
    (10504, Severity::Critical), // Shutdown
    (10520, Severity::Major),    // Loaddump
    (10536, Severity::Warning),  // Warning
];
const ALARM_REGS_PER_RANGE: u16 = 16; // 256 bits = 16 × 16-bit registers

/// Measurement definitions: 32-bit (2 registers) read via FC 03.
struct Meas32 {
    key: &'static str,
    addr: u16,
    scale: f64, // divide raw u32 by this
}

/// Measurement definitions: 16-bit (1 register) read via FC 03.
struct Meas16 {
    key: &'static str,
    addr: u16,
    scale: f64,
    signed: bool,
}

// Phase voltages + currents + power — all 32-bit ×10
const MEAS_32: &[Meas32] = &[
    // Mains phase voltages
    Meas32 { key: "u_mains_l1_v",    addr: 10240, scale: 10.0 },
    Meas32 { key: "u_mains_l2_v",    addr: 10242, scale: 10.0 },
    Meas32 { key: "u_mains_l3_v",    addr: 10244, scale: 10.0 },
    // Genset phase voltages
    Meas32 { key: "u_gen_l1_v",      addr: 10246, scale: 10.0 },
    Meas32 { key: "u_gen_l2_v",      addr: 10248, scale: 10.0 },
    Meas32 { key: "u_gen_l3_v",      addr: 10250, scale: 10.0 },
    // Genset currents
    Meas32 { key: "i_gen_l1_a",      addr: 10270, scale: 10.0 },
    Meas32 { key: "i_gen_l2_a",      addr: 10272, scale: 10.0 },
    Meas32 { key: "i_gen_l3_a",      addr: 10274, scale: 10.0 },
    // Genset total active / reactive / apparent power
    Meas32 { key: "p_gen_total_kw",  addr: 10294, scale: 10.0 },
    Meas32 { key: "q_gen_total_kvar",addr: 10310, scale: 10.0 },
    Meas32 { key: "s_gen_total_kva", addr: 10326, scale: 10.0 },
    // Mains total active power
    Meas32 { key: "p_mains_total_kw",addr: 10292, scale: 10.0 },
    // Counters
    Meas32 { key: "engine_hours",    addr: 10622, scale: 100.0 },
    Meas32 { key: "e_gen_total_kwh", addr: 10628, scale: 10.0 },
    Meas32 { key: "genset_runs",     addr: 10616, scale: 1.0 },
];

// Single-register measurements
const MEAS_16: &[Meas16] = &[
    Meas16 { key: "f_mains_hz",       addr: 10338, scale: 100.0, signed: false },
    Meas16 { key: "f_gen_hz",         addr: 10339, scale: 100.0, signed: false },
    Meas16 { key: "u_battery_v",      addr: 10341, scale: 100.0, signed: false },
    Meas16 { key: "pf_gen_total",     addr: 10335, scale: 10.0,  signed: true  },
    Meas16 { key: "oil_pressure_bar", addr: 10361, scale: 10.0,  signed: false },
    Meas16 { key: "engine_temp_c",    addr: 10362, scale: 10.0,  signed: true  },
    Meas16 { key: "fuel_level_pct",   addr: 10363, scale: 10.0,  signed: false },
    Meas16 { key: "oil_temp_c",       addr: 10364, scale: 10.0,  signed: true  },
    Meas16 { key: "canopy_temp_c",    addr: 10365, scale: 10.0,  signed: true  },
    Meas16 { key: "ambient_temp_c",   addr: 10366, scale: 10.0,  signed: true  },
    Meas16 { key: "engine_rpm",       addr: 10376, scale: 1.0,   signed: false },
];

// Operation status codes (register 10604)
#[allow(dead_code)]
fn op_status_label(code: u16) -> &'static str {
    match code {
        0  => "genset_at_rest",
        1  => "wait_before_fuel",
        2  => "engine_preheat",
        3  => "wait_oil_flash_off",
        4  => "crank_rest",
        5  => "cranking",
        6  => "engine_run_idle",
        7  => "engine_heating",
        8  => "running_off_load",
        9  => "syncing_to_mains",
        10 => "load_transfer_to_genset",
        11 => "gen_cb_activation",
        12 => "genset_cb_timer",
        13 => "master_on_load",
        14 => "peak_lopping",
        15 => "power_exporting",
        16 => "slave_on_load",
        17 => "syncing_back_to_mains",
        18 => "load_transfer_to_mains",
        19 => "mains_cb_activation",
        20 => "mains_cb_timer",
        21 => "stop_with_cooldown",
        22 => "cooling_down",
        23 => "engine_stop_idle",
        24 => "immediate_stop",
        25 => "engine_stopping",
        _  => "unknown",
    }
}

// Mode register 10605 (bitmask, not sequential)
#[allow(dead_code)]
fn mode_label(code: u16) -> &'static str {
    match code {
        1 => "stop",
        2 => "run",      // D-500: RUN; D-700: MANUAL
        4 => "auto",
        8 => "test",
        _ => "unknown",
    }
}

// ---------------------------------------------------------------------------
// Poll
// ---------------------------------------------------------------------------

#[allow(dead_code)]
pub struct PollResult {
    pub ip: String,
    pub site_key: String,
    pub active: Vec<Active>,
    pub measurements: Vec<(String, f64)>,
    pub op_status: Option<u16>,
}

pub async fn poll_datakom(
    dev: DeviceCfg,
    cfg: Arc<PollerConfig>,
    prof: Arc<DatakomProfile>,
) -> Result<PollResult> {
    let socket: SocketAddr = format!("{}:{}", dev.ip, dev.port)
        .parse().context("bad socket addr")?;
    let connect_to = Duration::from_millis(cfg.connect_timeout_ms);
    let read_to    = Duration::from_millis(cfg.read_timeout_ms);

    let mut ctx = timeout(connect_to, tcp::connect_slave(socket, Slave(dev.unit)))
        .await
        .map_err(|_| anyhow!("connect timeout to {}", dev.ip))?
        .context("connect failed")?;

    let mut measurements: Vec<(String, f64)> = Vec::new();
    let mut active: Vec<Active> = Vec::new();
    let mut op_status: Option<u16> = None;

    // ===== 32-BIT MEASUREMENTS =====
    for m in MEAS_32 {
        if let Some(r) = read_holding(&mut ctx, m.addr, 2,
                                      read_to, cfg.retries, cfg.retry_backoff_ms).await {
            if r.len() >= 2 {
                let raw = u32_be(r[0], r[1]);
                measurements.push((m.key.into(), raw as f64 / m.scale));
            }
        }
    }

    // ===== 16-BIT MEASUREMENTS =====
    for m in MEAS_16 {
        if let Some(r) = read_holding(&mut ctx, m.addr, 1,
                                      read_to, cfg.retries, cfg.retry_backoff_ms).await {
            if !r.is_empty() {
                let val = if m.signed { (r[0] as i16) as f64 } else { r[0] as f64 };
                measurements.push((m.key.into(), val / m.scale));
            }
        }
    }

    // ===== OPERATION STATUS + MODE =====
    if let Some(r) = read_holding(&mut ctx, 10604, 2,
                                  read_to, cfg.retries, cfg.retry_backoff_ms).await {
        if r.len() >= 2 {
            op_status = Some(r[0]);
            measurements.push(("op_status".into(), r[0] as f64));
            measurements.push(("op_mode".into(), r[1] as f64));

            // Synthetic alarm if engine is in a fault-stop state
            if r[0] == 24 { // immediate_stop
                active.push(Active {
                    key: "op:immediate_stop".into(),
                    def: AlarmDef {
                        addr: 10604,
                        name: "Engine Immediate Stop".into(),
                        class: AlarmClass::GensetEvent,
                        severity: Severity::Critical,
                    },
                });
            }
        }
    }

    // ===== ALARM BITMAPS =====
    // Three ranges, same bit definitions, different severity.
    // D-500 limits reads to 16 registers per request — each range is exactly 16.
    for &(base_reg, severity) in ALARM_RANGES {
        if let Some(regs) = read_holding(&mut ctx, base_reg, ALARM_REGS_PER_RANGE,
                                         read_to, cfg.retries, cfg.retry_backoff_ms).await {
            for (reg_idx, &val) in regs.iter().enumerate().take(ALARM_REGS_PER_RANGE as usize) {
                if val == 0 { continue; } // fast skip — no bits set in this register
                for bit in 0u16..16 {
                    if (val >> bit) & 1 == 1 {
                        let alarm_bit = (reg_idx as u16) * 16 + bit;
                        let sev_tag = match severity {
                            Severity::Critical => "sd",
                            Severity::Major    => "ld",
                            _                  => "wn",
                        };
                        if let Some((name, class)) = prof.bits.get(&alarm_bit) {
                            active.push(Active {
                                key: format!("dk:{sev_tag}:{alarm_bit}"),
                                def: AlarmDef {
                                    addr: base_reg + reg_idx as u16,
                                    name: name.clone(),
                                    class: *class,
                                    severity,
                                },
                            });
                        } else if alarm_bit < 142 {
                            // Known range but unmapped — surface it
                            active.push(Active {
                                key: format!("dk:{sev_tag}:{alarm_bit}"),
                                def: AlarmDef {
                                    addr: base_reg + reg_idx as u16,
                                    name: format!("D500 alarm bit {alarm_bit} (unmapped)"),
                                    class: AlarmClass::Unclassified,
                                    severity,
                                },
                            });
                        }
                        // bits 142–255 are reserved — ignore silently
                    }
                }
            }
        }
    }

    drop(ctx);
    Ok(PollResult { ip: dev.ip, site_key: dev.site_key, active, measurements, op_status })
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async fn read_holding(ctx: &mut ModbusContext, addr: u16, cnt: u16, to: Duration,
                      retries: u32, backoff_ms: u64) -> Option<Vec<u16>> {
    for attempt in 0..=retries {
        if let Ok(Ok(v)) = timeout(to, ctx.read_holding_registers(addr, cnt)).await {
            return Some(v);
        }
        if attempt < retries { sleep(Duration::from_millis(backoff_ms)).await; }
    }
    None
}

fn u32_be(hi: u16, lo: u16) -> u32 { ((hi as u32) << 16) | (lo as u32) }
