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
mod datakom;
mod decode;
mod poll;
mod profile;
mod sink;
mod smartlogger;
mod state;
mod types;

use anyhow::{Context, Result};
use breaker::Breaker;
use profile::Profile;
use sink::{MeasRow, Sink};
use smartlogger::SmartloggerProfile;
use state::AlarmStore;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tokio::time::sleep;
use types::{AlarmFile, DeviceCfg, DevicesFile, DkAlarmFile, PollerConfig, SlAlarmFile};
// DevicesFile is retained for --dry-run TOML fallback only; DB mode loads from dim_device.
use bht_normalize::Source;

#[tokio::main]
async fn main() -> Result<()> {
    let mut cfg_path = "config/poller.toml".to_string();
    let mut dev_path = "config/devices.toml".to_string();
    let mut alarm_path = "config/eaton_alarms.toml".to_string();
    let mut sl_alarm_path = "config/smartlogger_alarms.toml".to_string();
    let mut dk_alarm_path = "config/datakom_alarms.toml".to_string();
    let (mut dry_run, mut once) = (false, false);
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--config"  => cfg_path = args.next().unwrap_or(cfg_path),
            "--devices" => dev_path = args.next().unwrap_or(dev_path),
            "--alarms"  => alarm_path = args.next().unwrap_or(alarm_path),
            "--smartlogger-alarms" => sl_alarm_path = args.next().unwrap_or(sl_alarm_path),
            "--datakom-alarms"     => dk_alarm_path = args.next().unwrap_or(dk_alarm_path),
            "--dry-run" => dry_run = true,
            "--once"    => once = true,
            other => eprintln!("[poller] ignoring arg: {other}"),
        }
    }

    let cfg: PollerConfig = toml::from_str(
        &std::fs::read_to_string(&cfg_path).with_context(|| format!("read {cfg_path}"))?)?;
    let alarms: AlarmFile = toml::from_str(
        &std::fs::read_to_string(&alarm_path).with_context(|| format!("read {alarm_path}"))?)?;
    let sl_alarms: SlAlarmFile = std::fs::read_to_string(&sl_alarm_path)
        .ok()
        .and_then(|s| toml::from_str(&s).ok())
        .unwrap_or(SlAlarmFile { alarm: vec![] });

    let mut cfg = cfg;
    if let Ok(url) = std::env::var("DATABASE_URL") { cfg.database.dsn = url; } // container override
    let cfg = Arc::new(cfg);
    let profile = Arc::new(Profile::from_file(alarms));
    let sl_profile = Arc::new(SmartloggerProfile::from_file(sl_alarms));

    let dk_alarms: DkAlarmFile = std::fs::read_to_string(&dk_alarm_path)
        .ok()
        .and_then(|s| toml::from_str(&s).ok())
        .unwrap_or(DkAlarmFile { alarm: vec![] });
    let dk_profile = Arc::new(datakom::DatakomProfile::from_file(dk_alarms));

    let sink = if dry_run || cfg.database.dsn.is_empty() {
        eprintln!("[poller] sink = DRY-RUN");
        Sink::DryRun
    } else {
        eprintln!("[poller] sink = TimescaleDB");
        Sink::connect(&cfg.database.dsn).await.context("db connect")?
    };

    // Load initial device list: DB when connected, TOML file when dry-run.
    let mut active_devs: Vec<DeviceCfg> = if matches!(sink, Sink::Db(_)) {
        sink.load_devices().await.context("initial device load from DB")?
    } else {
        let devices: DevicesFile = toml::from_str(
            &std::fs::read_to_string(&dev_path).with_context(|| format!("read {dev_path}"))?)?;
        devices.device.into_iter().filter(|d| d.enabled).filter(|d| {
            matches!(d.dev_type.as_str(), "eaton" | "smartlogger" | "datakom")
        }).collect()
    };
    log_device_summary(&active_devs, cfg.poll_interval_secs, cfg.max_concurrent);
    let mut cycle_count: u64 = 0;

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
        let mut set: JoinSet<(String, i16, Result<poll::PollResult>, Source)> = JoinSet::new();

        let now = Instant::now();
        let mut open_skipped = 0;
        for (i, dev) in active_devs.iter().enumerate() {
            let br = breakers.entry(dev.ip.clone()).or_insert_with(|| Breaker::new(thr, cooldown));
            if !br.allow(now) { open_skipped += 1; continue; }
            let delay   = Duration::from_millis(((i as u64) * stagger_ms).min(stagger_cap));
            let cfg2    = cfg.clone();
            let sem2    = sem.clone();
            let dev2    = dev.clone();
            let ip      = dev.ip.clone();
            let unit_id = dev.unit as i16;
            match dev.dev_type.as_str() {
                "eaton" => {
                    let prof2 = profile.clone();
                    set.spawn(async move {
                        sleep(delay).await;
                        let _permit = sem2.acquire_owned().await.unwrap();
                        let res = poll::poll_device(dev2, cfg2, prof2).await
                            .map(|pr| poll::PollResult {
                                ip: pr.ip, site_key: pr.site_key,
                                status: pr.status, active: pr.active, measurements: pr.measurements,
                            });
                        (ip, unit_id, res, Source::ModbusEaton)
                    });
                }
                "smartlogger" => {
                    let prof2 = sl_profile.clone();
                    set.spawn(async move {
                        sleep(delay).await;
                        let _permit = sem2.acquire_owned().await.unwrap();
                        let res = smartlogger::poll_smartlogger(dev2, cfg2, prof2).await
                            .map(|pr| poll::PollResult {
                                ip: pr.ip, site_key: pr.site_key,
                                status: Default::default(),
                                active: pr.active, measurements: pr.measurements,
                            });
                        (ip, unit_id, res, Source::SmartloggerHuawei)
                    });
                }
                "datakom" => {
                    let prof2 = dk_profile.clone();
                    set.spawn(async move {
                        sleep(delay).await;
                        let _permit = sem2.acquire_owned().await.unwrap();
                        let res = datakom::poll_datakom(dev2, cfg2, prof2).await
                            .map(|pr| poll::PollResult {
                                ip: pr.ip, site_key: pr.site_key,
                                status: Default::default(),
                                active: pr.active, measurements: pr.measurements,
                            });
                        (ip, unit_id, res, Source::ModbusDatakom)
                    });
                }
                _ => continue,
            }
        }

        let mut all_meas: Vec<MeasRow> = Vec::new();
        let mut all_events = Vec::new();
        let (mut ok, mut fail) = (0u32, 0u32);
        let mut ok_pairs:   Vec<(String, i16)> = Vec::new();
        let mut fail_pairs: Vec<(String, i16)> = Vec::new();
        while let Some(joined) = set.join_next().await {
            let (ip, uid, res, src) = match joined {
                Ok(v) => v,
                Err(e) => { eprintln!("[poller] task panic: {e}"); continue; }
            };
            match res {
                Ok(pr) => {
                    ok += 1;
                    ok_pairs.push((ip.clone(), uid));
                    if let Some(br) = breakers.get_mut(&ip) { br.on_success(); }
                    let ts = chrono::Utc::now();
                    for (metric, value) in &pr.measurements {
                        all_meas.push(MeasRow {
                            ts, site_key: pr.site_key.clone(), ip: ip.clone(),
                            metric: metric.clone(), value: *value,
                        });
                    }
                    all_events.extend(store.diff(&ip, &pr.site_key, src, pr.active));
                }
                Err(e) => {
                    fail += 1;
                    fail_pairs.push((ip.clone(), uid));
                    if let Some(br) = breakers.get_mut(&ip) { br.on_failure(Instant::now()); }
                    eprintln!("[poller] {ip} poll failed: {e}");
                }
            }
        }

        let m = sink.write_measurements(&all_meas).await
            .unwrap_or_else(|e| { eprintln!("[poller] meas write: {e}"); 0 });
        let ev = sink.write_events(&all_events).await
            .unwrap_or_else(|e| { eprintln!("[poller] event write: {e}"); 0 });

        // Write per-device health back to dim_device (makes v_device_health live).
        sink.write_health(&ok_pairs, &fail_pairs).await
            .unwrap_or_else(|e| eprintln!("[poller] health write: {e}"));

        eprintln!("[poller] cycle ok={ok} fail={fail} breaker_open={open_skipped} meas={m} events={ev} took={:?}",
                  cycle_start.elapsed());

        if once { break; }

        // Hot-reload device list from dim_device every N cycles.
        cycle_count += 1;
        if matches!(sink, Sink::Db(_)) && cycle_count % cfg.reload_interval_cycles == 0 {
            match sink.load_devices().await {
                Ok(new_devs) => {
                    if new_devs.len() != active_devs.len() {
                        eprintln!("[poller] hot-reload: {} → {} devices", active_devs.len(), new_devs.len());
                        log_device_summary(&new_devs, cfg.poll_interval_secs, cfg.max_concurrent);
                    }
                    active_devs = new_devs;
                }
                Err(e) => eprintln!("[poller] hot-reload failed, keeping current list: {e}"),
            }
        }

        let wait = interval.checked_sub(cycle_start.elapsed()).unwrap_or(Duration::ZERO);
        tokio::select! {
            _ = sleep(wait) => {},
            _ = tokio::signal::ctrl_c() => { eprintln!("[poller] shutdown"); break; }
        }
    }
    Ok(())
}

fn log_device_summary(devs: &[DeviceCfg], interval_secs: u64, max_concurrent: usize) {
    let mut by_type: HashMap<String, usize> = HashMap::new();
    let mut skipped = 0;
    for d in devs {
        match d.dev_type.as_str() {
            "eaton" | "smartlogger" | "datakom" => *by_type.entry(d.dev_type.clone()).or_insert(0) += 1,
            _ => skipped += 1,
        }
    }
    if skipped > 0 { eprintln!("[poller] {skipped} unknown-type devices skipped"); }
    eprintln!("[poller] {} devices ({:?}), interval={interval_secs}s, max_concurrent={max_concurrent}",
              devs.len(), by_type);
}
