//! bht-poller — async Modbus collector for Eaton SC200/300.
//!
//! Per cycle: schedule all enabled Eaton devices with bounded concurrency and
//! staggered starts, gated by a per-device circuit breaker. Decode summary +
//! discrete + coil alarms and input-register measurements, run edge detection
//! (RAISE/CLEAR), and batch-write events + telemetry to TimescaleDB (or print
//! in --dry-run). Ctrl-C shuts down cleanly between cycles.
//!
//! Usage: bht-poller [--config FILE] [--devices FILE] [--alarms FILE] [--dry-run] [--once]

mod breaker;
mod decode;
mod poll;
mod profile;
mod sink;
mod state;
mod types;

use anyhow::{Context, Result};
use breaker::Breaker;
use profile::Profile;
use sink::{MeasRow, Sink};
use state::AlarmStore;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tokio::time::sleep;
use types::{AlarmFile, DeviceCfg, DevicesFile, PollerConfig};

#[tokio::main]
async fn main() -> Result<()> {
    let mut cfg_path = "config/poller.toml".to_string();
    let mut dev_path = "config/devices.toml".to_string();
    let mut alarm_path = "config/eaton_alarms.toml".to_string();
    let (mut dry_run, mut once) = (false, false);
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--config"  => cfg_path = args.next().unwrap_or(cfg_path),
            "--devices" => dev_path = args.next().unwrap_or(dev_path),
            "--alarms"  => alarm_path = args.next().unwrap_or(alarm_path),
            "--dry-run" => dry_run = true,
            "--once"    => once = true,
            other => eprintln!("[poller] ignoring arg: {other}"),
        }
    }

    let cfg: PollerConfig = toml::from_str(
        &std::fs::read_to_string(&cfg_path).with_context(|| format!("read {cfg_path}"))?)?;
    let devices: DevicesFile = toml::from_str(
        &std::fs::read_to_string(&dev_path).with_context(|| format!("read {dev_path}"))?)?;
    let alarms: AlarmFile = toml::from_str(
        &std::fs::read_to_string(&alarm_path).with_context(|| format!("read {alarm_path}"))?)?;

    let mut cfg = cfg;
    if let Ok(url) = std::env::var("DATABASE_URL") { cfg.database.dsn = url; } // container override
    let cfg = Arc::new(cfg);
    let profile = Arc::new(Profile::from_file(alarms));

    let sink = if dry_run || cfg.database.dsn.is_empty() {
        eprintln!("[poller] sink = DRY-RUN");
        Sink::DryRun
    } else {
        eprintln!("[poller] sink = TimescaleDB");
        Sink::connect(&cfg.database.dsn).await.context("db connect")?
    };

    let mut active_devs: Vec<DeviceCfg> = Vec::new();
    let mut skipped = 0;
    for d in devices.device {
        if !d.enabled { continue; }
        if d.dev_type != "eaton" { skipped += 1; continue; }
        active_devs.push(d);
    }
    if skipped > 0 {
        eprintln!("[poller] skipping {skipped} non-eaton devices (smartlogger = later stage)");
    }
    eprintln!("[poller] {} eaton devices, interval={}s, max_concurrent={}",
              active_devs.len(), cfg.poll_interval_secs, cfg.max_concurrent);

    let mut breakers: HashMap<String, Breaker> = HashMap::new();
    let mut store = AlarmStore::default();
    let cooldown = Duration::from_secs(cfg.circuit_breaker.open_cooldown_secs);
    let thr = cfg.circuit_breaker.failure_threshold;
    let interval = Duration::from_secs(cfg.poll_interval_secs);

    loop {
        let cycle_start = Instant::now();
        let stagger_ms = if active_devs.is_empty() { 0 }
                         else { (cfg.poll_interval_secs * 1000) / active_devs.len() as u64 };
        let stagger_cap = cfg.poll_interval_secs * 900; // ms (90% of interval)
        let sem = Arc::new(Semaphore::new(cfg.max_concurrent.max(1)));
        let mut set: JoinSet<(String, Result<poll::PollResult>)> = JoinSet::new();

        let now = Instant::now();
        let mut open_skipped = 0;
        for (i, dev) in active_devs.iter().enumerate() {
            let br = breakers.entry(dev.ip.clone()).or_insert_with(|| Breaker::new(thr, cooldown));
            if !br.allow(now) { open_skipped += 1; continue; }
            let delay = Duration::from_millis(((i as u64) * stagger_ms).min(stagger_cap));
            let (cfg2, prof2, sem2, dev2, ip) =
                (cfg.clone(), profile.clone(), sem.clone(), dev.clone(), dev.ip.clone());
            set.spawn(async move {
                sleep(delay).await;
                let _permit = sem2.acquire_owned().await.unwrap();
                (ip, poll::poll_device(dev2, cfg2, prof2).await)
            });
        }

        let mut all_meas: Vec<MeasRow> = Vec::new();
        let mut all_events = Vec::new();
        let (mut ok, mut fail) = (0u32, 0u32);
        while let Some(joined) = set.join_next().await {
            let (ip, res) = match joined {
                Ok(v) => v,
                Err(e) => { eprintln!("[poller] task panic: {e}"); continue; }
            };
            match res {
                Ok(pr) => {
                    ok += 1;
                    if let Some(br) = breakers.get_mut(&ip) { br.on_success(); }
                    let ts = chrono::Utc::now();
                    for (metric, value) in &pr.measurements {
                        all_meas.push(MeasRow {
                            ts, site_key: pr.site_key.clone(), ip: ip.clone(),
                            metric: metric.clone(), value: *value,
                        });
                    }
                    all_events.extend(store.diff(&ip, &pr.site_key, pr.active));
                }
                Err(e) => {
                    fail += 1;
                    if let Some(br) = breakers.get_mut(&ip) { br.on_failure(Instant::now()); }
                    eprintln!("[poller] {ip} poll failed: {e}");
                }
            }
        }

        let m = sink.write_measurements(&all_meas).await
            .unwrap_or_else(|e| { eprintln!("[poller] meas write: {e}"); 0 });
        let ev = sink.write_events(&all_events).await
            .unwrap_or_else(|e| { eprintln!("[poller] event write: {e}"); 0 });
        eprintln!("[poller] cycle ok={ok} fail={fail} breaker_open={open_skipped} meas={m} events={ev} took={:?}",
                  cycle_start.elapsed());

        if once { break; }
        let wait = interval.checked_sub(cycle_start.elapsed()).unwrap_or(Duration::ZERO);
        tokio::select! {
            _ = sleep(wait) => {},
            _ = tokio::signal::ctrl_c() => { eprintln!("[poller] shutdown"); break; }
        }
    }
    Ok(())
}
