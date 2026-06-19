//! Huawei SmartLogger 3000 poll, per "SmartLogger ModBus Interface Definitions"
//! Issue 35 (2020-02-20). All addresses in this file are the documented PDU
//! addresses from the Huawei spec; configure the device with `base0 = true` so
//! `wire_addr()` does NOT subtract 1.
//!
//! The SmartLogger itself responds on **logic device ID 0** (unit = 0). IDs
//! 1..247 reach individual inverters connected to it (different register
//! map — out of scope here). Configure `unit = 0` in devices.toml.
//!
//! Read via function 0x03 (Read Holding Registers).

use crate::state::Active;
use crate::types::{AlarmDef, DeviceCfg, PollerConfig, SlAlarmFile};
use anyhow::{anyhow, Context, Result};
use bht_normalize::{translate_zh, AlarmClass, Severity};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::{sleep, timeout};
use tokio_modbus::client::{tcp, Context as ModbusContext};
use tokio_modbus::prelude::*;

pub struct SmartloggerProfile {
    /// (register, bit) -> alarm definition
    pub bits: std::collections::HashMap<(u16, u8), AlarmDef>,
}

impl SmartloggerProfile {
    pub fn from_file(f: SlAlarmFile) -> Self {
        let mut bits = std::collections::HashMap::new();
        for a in f.alarm {
            bits.insert((a.addr, a.bit), AlarmDef {
                addr: a.addr, name: a.name, class: a.class, severity: a.severity,
            });
        }
        Self { bits }
    }
}

#[allow(dead_code)]
pub struct PollResult {
    pub ip: String,
    pub site_key: String,
    pub active: Vec<Active>,
    pub measurements: Vec<(String, f64)>,
    pub status_code: Option<u16>,
}

fn wire(doc: u16, base0: bool) -> u16 { if base0 { doc } else { doc - 1 } }

/// Decode SmartLogger "Plant status" reg 40543 (used by Qinghai region) into
/// a short English label. Defensive: unknown codes get a generic label.
fn decode_plant_status(code: u16) -> &'static str {
    match code {
        1 => "unlimited_power_operation",
        2 => "limited_power_operation",
        3 => "idle",
        4 => "outage_fault_or_maintenance",
        5 => "communication_interrupt",
        _ => "unknown",
    }
}

pub async fn poll_smartlogger(
    dev: DeviceCfg,
    cfg: Arc<PollerConfig>,
    prof: Arc<SmartloggerProfile>,
) -> Result<PollResult> {
    let socket: SocketAddr = format!("{}:{}", dev.ip, dev.port).parse().context("bad socket addr")?;
    let connect_to = Duration::from_millis(cfg.connect_timeout_ms);
    let read_to    = Duration::from_millis(cfg.read_timeout_ms);

    let mut ctx = timeout(connect_to, tcp::connect_slave(socket, Slave(dev.unit)))
        .await
        .map_err(|_| anyhow!("connect timeout to {}", dev.ip))?
        .context("connect failed")?;

    let mut measurements: Vec<(String, f64)> = Vec::new();
    let mut active: Vec<Active> = Vec::new();
    let mut status_code: Option<u16> = None;

    // ===== ENERGY (ABSOLUTE CUMULATIVE COUNTERS — these survive cold starts) =====

    // E-Total: total energy yield, U32 at 40560, gain 10 -> kWh
    if let Some(r) = read_holding(&mut ctx, wire(40560, dev.base0), 2,
                                  read_to, cfg.retries, cfg.retry_backoff_ms).await {
        if r.len() >= 2 {
            measurements.push(("e_total_kwh".into(), u32_be(r[0], r[1]) as f64 / 10.0));
        }
    }
    // E-Daily: today's yield, U32 at 40562, gain 10 -> kWh
    if let Some(r) = read_holding(&mut ctx, wire(40562, dev.base0), 2,
                                  read_to, cfg.retries, cfg.retry_backoff_ms).await {
        if r.len() >= 2 {
            measurements.push(("e_solar_day_kwh".into(), u32_be(r[0], r[1]) as f64 / 10.0));
        }
    }

    // ===== INSTANTANEOUS POWER =====

    // Active power (AC output): I32 at 40525, gain 1000 -> kW
    if let Some(r) = read_holding(&mut ctx, wire(40525, dev.base0), 2,
                                  read_to, cfg.retries, cfg.retry_backoff_ms).await {
        if r.len() >= 2 {
            measurements.push(("p_solar_kw".into(), i32_be(r[0], r[1]) as f64 / 1000.0));
        }
    }
    // Input power (DC side, all strings): U32 at 40521, gain 1000 -> kW
    if let Some(r) = read_holding(&mut ctx, wire(40521, dev.base0), 2,
                                  read_to, cfg.retries, cfg.retry_backoff_ms).await {
        if r.len() >= 2 {
            measurements.push(("p_solar_dc_kw".into(), u32_be(r[0], r[1]) as f64 / 1000.0));
        }
    }
    // Reactive power: I32 at 40544, gain 1000 -> kVar
    if let Some(r) = read_holding(&mut ctx, wire(40544, dev.base0), 2,
                                  read_to, cfg.retries, cfg.retry_backoff_ms).await {
        if r.len() >= 2 {
            measurements.push(("q_solar_kvar".into(), i32_be(r[0], r[1]) as f64 / 1000.0));
        }
    }

    // ===== AC GRID READINGS =====

    // Uab/Ubc/Uca line voltages: U16 each at 40575/40576/40577, gain 10 -> V
    if let Some(r) = read_holding(&mut ctx, wire(40575, dev.base0), 3,
                                  read_to, cfg.retries, cfg.retry_backoff_ms).await {
        if r.len() >= 3 {
            measurements.push(("u_ab_v".into(), r[0] as f64 / 10.0));
            measurements.push(("u_bc_v".into(), r[1] as f64 / 10.0));
            measurements.push(("u_ca_v".into(), r[2] as f64 / 10.0));
        }
    }
    // Phase currents I_a/I_b/I_c: I16 each at 40572/40573/40574, gain 1 -> A
    if let Some(r) = read_holding(&mut ctx, wire(40572, dev.base0), 3,
                                  read_to, cfg.retries, cfg.retry_backoff_ms).await {
        if r.len() >= 3 {
            measurements.push(("i_a_a".into(), (r[0] as i16) as f64));
            measurements.push(("i_b_a".into(), (r[1] as i16) as f64));
            measurements.push(("i_c_a".into(), (r[2] as i16) as f64));
        }
    }

    // ===== CO2 reduction (cumulative) =====
    // U32 at 40523, gain 10 -> kg
    if let Some(r) = read_holding(&mut ctx, wire(40523, dev.base0), 2,
                                  read_to, cfg.retries, cfg.retry_backoff_ms).await {
        if r.len() >= 2 {
            measurements.push(("co2_reduction_kg".into(), u32_be(r[0], r[1]) as f64 / 10.0));
        }
    }

    // ===== PLANT STATUS =====

    // 40543 plant status (Qinghai variant — most informative).
    if let Some(r) = read_holding(&mut ctx, wire(40543, dev.base0), 1,
                                  read_to, cfg.retries, cfg.retry_backoff_ms).await {
        if !r.is_empty() {
            status_code = Some(r[0]);
            let label = decode_plant_status(r[0]);
            // raise synthetic alarm only on fault/outage statuses
            if matches!(r[0], 4 | 5) {
                active.push(Active {
                    key: format!("status:{}", r[0]),
                    def: AlarmDef {
                        addr: 40543,
                        name: format!("Plant status: {}", translate_zh(label)),
                        class: if r[0] == 5 { AlarmClass::CommsLost } else { AlarmClass::GenericError },
                        severity: Severity::Major,
                    },
                });
            }
        }
    }

    // ===== ALARM BITMAPS 50000 / 50001 / 50002 (3 consecutive registers) =====
    if let Some(r) = read_holding(&mut ctx, wire(50000, dev.base0), 3,
                                  read_to, cfg.retries, cfg.retry_backoff_ms).await {
        for (i, &val) in r.iter().enumerate().take(3) {
            let addr = 50000 + i as u16;
            for bit in 0u8..16 {
                if (val >> bit) & 1 == 1 {
                    if let Some(def) = prof.bits.get(&(addr, bit)) {
                        active.push(Active {
                            key: format!("sl:{addr}:{bit}"),
                            def: def.clone(),
                        });
                    } else {
                        // unmapped bit — still surface it so we notice firmware drift
                        active.push(Active {
                            key: format!("sl:{addr}:{bit}"),
                            def: AlarmDef {
                                addr,
                                name: format!("SmartLogger {addr}:{bit} (unmapped — verify firmware)"),
                                class: AlarmClass::Unclassified,
                                severity: Severity::Warning,
                            },
                        });
                    }
                }
            }
        }
    }

    drop(ctx);
    Ok(PollResult { ip: dev.ip, site_key: dev.site_key, active, measurements, status_code })
}

// -------------------- helpers --------------------

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
fn i32_be(hi: u16, lo: u16) -> i32 { u32_be(hi, lo) as i32 }
