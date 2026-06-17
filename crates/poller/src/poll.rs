//! Async poll of one Eaton SC200/300 over Modbus TCP. Each read is bounded by a
//! timeout and retried with backoff; segment reads are chunked into blocks like
//! the Python reference.

use crate::decode::{self, DeviceStatus};
use crate::profile::{Decode, Profile, ALARM_SEGMENTS, COIL_COUNT, COIL_START, METRICS, SUMMARY_ADDR, SUMMARY_COUNT};
use crate::state::Active;
use crate::types::{DeviceCfg, PollerConfig};
use anyhow::{anyhow, Context, Result};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::{sleep, timeout};
use tokio_modbus::client::{tcp, Context as ModbusContext};
use tokio_modbus::prelude::*;

#[allow(dead_code)]
pub struct PollResult {
    pub ip: String,
    pub site_key: String,
    pub status: DeviceStatus,
    pub active: Vec<Active>,
    pub measurements: Vec<(String, f64)>,
}

pub async fn poll_device(dev: DeviceCfg, cfg: Arc<PollerConfig>, profile: Arc<Profile>) -> Result<PollResult> {
    let socket: SocketAddr = format!("{}:{}", dev.ip, dev.port).parse().context("bad socket addr")?;
    let connect_to = Duration::from_millis(cfg.connect_timeout_ms);
    let read_to = Duration::from_millis(cfg.read_timeout_ms);

    let mut ctx = timeout(connect_to, tcp::connect_slave(socket, Slave(dev.unit)))
        .await
        .map_err(|_| anyhow!("connect timeout to {}", dev.ip))?
        .context("connect failed")?;

    // 1. summary status (discrete 1001..1004)
    let summary = read_discrete(&mut ctx, decode::wire_addr(SUMMARY_ADDR, dev.base0),
                                SUMMARY_COUNT, read_to, cfg.retries, cfg.retry_backoff_ms)
        .await.unwrap_or_default();
    let status = decode::status_from_summary(&summary);

    // 2. discrete alarm segments
    let mut active: Vec<Active> = Vec::new();
    for &(seg_start, seg_end) in ALARM_SEGMENTS {
        let bits = read_discrete_segmented(&mut ctx, seg_start, seg_end, dev.base0,
                                           cfg.discrete_block_size, read_to,
                                           cfg.retries, cfg.retry_backoff_ms).await;
        for (idx, addr) in (seg_start..=seg_end).enumerate() {
            if idx < bits.len() && bits[idx] {
                if let Some(def) = profile.discrete.get(&addr) {
                    active.push(Active { key: format!("d:{addr}"), def: def.clone() });
                }
            }
        }
    }

    // 3. coil alarms (1..12)
    if let Some(coils) = read_coils(&mut ctx, decode::wire_addr(COIL_START, dev.base0),
                                    COIL_COUNT, read_to, cfg.retries, cfg.retry_backoff_ms).await {
        for (i, addr) in (COIL_START..COIL_START + COIL_COUNT).enumerate() {
            if i < coils.len() && coils[i] {
                if let Some(def) = profile.coils.get(&addr) {
                    active.push(Active { key: format!("c:{addr}"), def: def.clone() });
                }
            }
        }
    }

    // 4. measurements (input registers, 2 regs each)
    let mut measurements = Vec::new();
    for m in METRICS {
        if m.fne_only && !dev.fne { continue; }
        if let Some(regs) = read_input(&mut ctx, decode::wire_addr(m.doc_addr, dev.base0),
                                       2, read_to, cfg.retries, cfg.retry_backoff_ms).await {
            if regs.len() >= 2 {
                let val = match m.decode {
                    Decode::F32 => decode::f32_be(regs[0], regs[1]).map(|v| v as f64),
                    Decode::U32 => Some(decode::u32_be(regs[0], regs[1]) as f64),
                    Decode::I32 => Some(decode::i32_be(regs[0], regs[1]) as f64),
                };
                if let Some(v) = val {
                    measurements.push((m.key.to_string(), v * m.scale));
                }
            }
        }
    }

    drop(ctx);
    Ok(PollResult { ip: dev.ip, site_key: dev.site_key, status, active, measurements })
}

async fn read_discrete(ctx: &mut ModbusContext, addr: u16, cnt: u16, to: Duration,
                       retries: u32, backoff_ms: u64) -> Option<Vec<bool>> {
    for attempt in 0..=retries {
        if let Ok(Ok(v)) = timeout(to, ctx.read_discrete_inputs(addr, cnt)).await {
            return Some(v);
        }
        if attempt < retries { sleep(Duration::from_millis(backoff_ms)).await; }
    }
    None
}

async fn read_coils(ctx: &mut ModbusContext, addr: u16, cnt: u16, to: Duration,
                    retries: u32, backoff_ms: u64) -> Option<Vec<bool>> {
    for attempt in 0..=retries {
        if let Ok(Ok(v)) = timeout(to, ctx.read_coils(addr, cnt)).await {
            return Some(v);
        }
        if attempt < retries { sleep(Duration::from_millis(backoff_ms)).await; }
    }
    None
}

async fn read_input(ctx: &mut ModbusContext, addr: u16, cnt: u16, to: Duration,
                    retries: u32, backoff_ms: u64) -> Option<Vec<u16>> {
    for attempt in 0..=retries {
        if let Ok(Ok(v)) = timeout(to, ctx.read_input_registers(addr, cnt)).await {
            return Some(v);
        }
        if attempt < retries { sleep(Duration::from_millis(backoff_ms)).await; }
    }
    None
}

#[allow(clippy::too_many_arguments)]
async fn read_discrete_segmented(ctx: &mut ModbusContext, start_doc: u16, end_doc: u16, base0: bool,
                                 block: u16, to: Duration, retries: u32, backoff_ms: u64) -> Vec<bool> {
    let mut total = Vec::new();
    let mut addr = start_doc;
    while addr <= end_doc {
        let count = block.min(end_doc - addr + 1);
        let wire = decode::wire_addr(addr, base0);
        match read_discrete(ctx, wire, count, to, retries, backoff_ms).await {
            Some(mut bits) => {
                if (bits.len() as u16) < count { bits.resize(count as usize, false); }
                total.extend_from_slice(&bits[..count as usize]);
            }
            None => total.extend(std::iter::repeat(false).take(count as usize)),
        }
        addr += count;
    }
    total
}
