//! neteco-poller — NetEco iSitePower NBI REST poller.
//!
//! Polls the NetEco REST API on three cadences:
//!   - Topology (sites + devices): every 6h + on startup
//!   - Device metrics (all types):  every 5 min
//!   - Active alarms:               every 2 min
//!
//! Config: TOML file + env overrides (NETECO_URL, NETECO_USER, NETECO_PASSWORD,
//! DATABASE_URL). See config/neteco.toml.example.
//!
//! Usage: neteco-poller [--config FILE] [--once] [--fingerprint]

mod client;
mod db;
mod types;

use anyhow::{Context, Result};
use client::NetEcoClient;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::time::sleep;
use types::{Config, DeviceRecord};

#[tokio::main]
async fn main() -> Result<()> {
    let mut cfg_path = "config/neteco.toml".to_string();
    let (mut once, mut fingerprint) = (false, false);
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--config" => cfg_path = args.next().unwrap_or(cfg_path),
            "--once" => once = true,
            "--fingerprint" => fingerprint = true,
            other => eprintln!("[neteco] ignoring arg: {other}"),
        }
    }

    let mut cfg: Config = toml::from_str(
        &std::fs::read_to_string(&cfg_path).with_context(|| format!("read {cfg_path}"))?,
    )?;

    // Env overrides (systemd EnvironmentFile)
    if let Ok(url) = std::env::var("NETECO_URL") { cfg.neteco.url = url; }
    if let Ok(u)   = std::env::var("NETECO_USER") { cfg.neteco.user = u; }
    if let Ok(p)   = std::env::var("NETECO_PASSWORD") { cfg.neteco.password = p; }
    if let Ok(dsn) = std::env::var("DATABASE_URL") { cfg.database.dsn = dsn; }

    if cfg.database.dsn.is_empty() {
        cfg.database.dsn = "host=localhost port=5432 user=bht password=bht_dev_pw dbname=alarms".into();
    }

    let nbi = NetEcoClient::new(&cfg.neteco).context("init NetEco client")?;

    // Connect to Postgres
    let (pg, conn) = tokio_postgres::connect(&cfg.database.dsn, tokio_postgres::NoTls)
        .await
        .context("postgres connect")?;
    tokio::spawn(async move {
        if let Err(e) = conn.await { eprintln!("[db] connection error: {e}"); }
    });

    // --fingerprint: print devTypeId mapping and exit
    if fingerprint {
        return run_fingerprint(&nbi).await;
    }

    // Initial topology sync
    eprintln!("[neteco] initial topology sync…");
    if let Err(e) = refresh_topology(&nbi, &pg).await {
        eprintln!("[neteco] topology sync failed (will retry): {e}");
    }

    let topo_interval = Duration::from_secs(cfg.intervals.topology_secs);
    let metric_interval = Duration::from_secs(cfg.intervals.metrics_secs);
    let alarm_interval = Duration::from_secs(cfg.intervals.alarms_secs);

    let mut last_topo = Instant::now();
    let mut last_metric = Instant::now() - metric_interval; // fire immediately
    let mut last_alarm = Instant::now() - alarm_interval;   // fire immediately

    // Build key maps — initially identity (column name = REST key).
    // After fingerprinting the live payload, populate these from config.
    // TODO: load from [key_maps.*] in neteco.toml after fingerprinting.
    let empty_keys: HashMap<String, String> = HashMap::new();

    eprintln!("[neteco] poll loop: metrics={}s alarms={}s topo={}s",
             cfg.intervals.metrics_secs, cfg.intervals.alarms_secs, cfg.intervals.topology_secs);

    loop {
        let now_inst = Instant::now();

        // Topology refresh
        if now_inst.duration_since(last_topo) >= topo_interval {
            if let Err(e) = refresh_topology(&nbi, &pg).await {
                eprintln!("[neteco] topology: {e}");
            }
            last_topo = Instant::now();
        }

        // Metric poll
        if now_inst.duration_since(last_metric) >= metric_interval {
            if let Err(e) = poll_all_metrics(&nbi, &pg, &empty_keys).await {
                eprintln!("[neteco] metrics: {e}");
            }
            last_metric = Instant::now();
        }

        // Alarm poll
        if now_inst.duration_since(last_alarm) >= alarm_interval {
            if let Err(e) = poll_alarms(&nbi, &pg).await {
                eprintln!("[neteco] alarms: {e}");
            }
            last_alarm = Instant::now();
        }

        if once { break; }

        // Sleep until next action needed (min of remaining intervals)
        let next_metric = metric_interval.saturating_sub(Instant::now().duration_since(last_metric));
        let next_alarm = alarm_interval.saturating_sub(Instant::now().duration_since(last_alarm));
        let wait = next_metric.min(next_alarm).max(Duration::from_secs(1));

        tokio::select! {
            _ = sleep(wait) => {},
            _ = tokio::signal::ctrl_c() => { eprintln!("[neteco] shutdown"); break; }
        }
    }
    Ok(())
}

// =================================================================== TOPOLOGY

async fn refresh_topology(nbi: &NetEcoClient, pg: &tokio_postgres::Client) -> Result<()> {
    let stations = nbi.get_stations().await?;
    let n_sites = db::upsert_sites(pg, &stations).await?;
    eprintln!("[neteco] topology: {} sites synced", n_sites);

    // Fetch devices for all stations
    let codes: Vec<&str> = stations.iter().map(|s| s.station_code.as_str()).collect();
    let mut total_devs = 0u64;
    // Batch station codes in groups of 100 (API limit unknown, be safe)
    for chunk in codes.chunks(100) {
        let codes_str = chunk.join(",");
        match nbi.get_devices(&codes_str).await {
            Ok(devs) => {
                let n = db::upsert_devices(pg, &devs).await?;
                total_devs += n;
            }
            Err(e) => eprintln!("[neteco] getDevList chunk error: {e}"),
        }
    }
    eprintln!("[neteco] topology: {} device rows upserted", total_devs);
    Ok(())
}

// =================================================================== METRICS

async fn poll_all_metrics(
    nbi: &NetEcoClient,
    pg: &tokio_postgres::Client,
    keys: &HashMap<String, String>,
) -> Result<()> {
    let start = Instant::now();
    let devices = db::load_device_cache(pg).await?;
    if devices.is_empty() {
        eprintln!("[neteco] no devices in cache, skipping metrics");
        return Ok(());
    }

    // Group devices by devTypeId
    let mut by_type: HashMap<i32, Vec<&DeviceRecord>> = HashMap::new();
    for d in &devices {
        by_type.entry(d.dev_type_id).or_default().push(d);
    }

    let mut total_inserted = 0u64;
    for (dev_type_id, type_devs) in &by_type {
        // Chunk device IDs (max 100 per API call)
        for chunk in type_devs.chunks(100) {
            let ids_str = chunk.iter()
                .map(|d| d.device_id.to_string())
                .collect::<Vec<_>>()
                .join(",");

            let records = match nbi.get_dev_kpi(*dev_type_id, &ids_str).await {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("[neteco] KPI devType={}: {e}", dev_type_id);
                    continue;
                }
            };

            // Build station_code lookup
            let station_map: HashMap<i64, &str> = chunk.iter()
                .map(|d| (d.device_id, d.station_code.as_str()))
                .collect();

            // Determine which table based on std_type_name from device cache
            let type_name = chunk.first()
                .and_then(|d| d.std_type_name.as_deref())
                .unwrap_or("");

            let ts = chrono::Utc::now().to_rfc3339();
            for rec in &records {
                let sc = match station_map.get(&rec.dev_id) {
                    Some(sc) => *sc,
                    None => continue,
                };
                let n = match type_name {
                    "Site Unit"              => db::insert_site_unit(pg, &ts, rec.dev_id, sc, rec, keys).await,
                    "Power System"           => db::insert_power_system(pg, &ts, rec.dev_id, sc, rec, keys).await,
                    "Battery Group"          => db::insert_battery_group(pg, &ts, rec.dev_id, sc, rec, keys).await,
                    "Mains"                  => db::insert_mains(pg, &ts, rec.dev_id, sc, rec, keys).await,
                    "DC Output Distribution" => db::insert_dpdu(pg, &ts, rec.dev_id, sc, rec, keys).await,
                    "AC Input Distribution"  => db::insert_ac_input(pg, &ts, rec.dev_id, sc, rec, keys).await,
                    "Genset"                 => db::insert_genset(pg, &ts, rec.dev_id, sc, rec, keys).await,
                    "Rectifier Group"        => db::insert_rectifier_group(pg, &ts, rec.dev_id, sc, rec, keys).await,
                    other => {
                        if !other.is_empty() {
                            eprintln!("[neteco] unknown std_type_name '{}' (devTypeId={}), skipping", other, dev_type_id);
                        }
                        Ok(0)
                    }
                };
                match n {
                    Ok(rows) => total_inserted += rows,
                    Err(e) => eprintln!("[neteco] insert error: {e}"),
                }
            }
        }
    }
    eprintln!("[neteco] metrics: {} rows inserted across {} types in {:?}",
             total_inserted, by_type.len(), start.elapsed());
    Ok(())
}

// =================================================================== ALARMS

async fn poll_alarms(nbi: &NetEcoClient, pg: &tokio_postgres::Client) -> Result<()> {
    let station_codes = db::load_station_codes(pg).await?;
    if station_codes.is_empty() {
        eprintln!("[neteco] no stations, skipping alarm poll");
        return Ok(());
    }
    let codes_str = station_codes.join(",");
    // status=1 → active alarms only
    let alarms = nbi.get_alarms(&codes_str, 1).await?;
    let n = db::upsert_alarms(pg, &alarms, "nbi_rest").await?;
    eprintln!("[neteco] alarms: {} active, {} upserted", alarms.len(), n);
    Ok(())
}

// =================================================================== FINGERPRINT

/// --fingerprint mode: discover devTypeId mapping and dataItemMap keys, then exit.
async fn run_fingerprint(nbi: &NetEcoClient) -> Result<()> {
    eprintln!("=== NetEco API Fingerprint ===\n");

    // 1. Sites
    let stations = nbi.get_stations().await?;
    eprintln!("Sites ({}):", stations.len());
    for s in &stations {
        eprintln!("  {} — {:?}", s.station_code, s.station_name);
    }

    // 2. Devices per site
    for s in &stations {
        let devs = nbi.get_devices(&s.station_code).await?;
        eprintln!("\nSite '{}' devices ({}):", s.station_code, devs.len());

        let mut by_type: HashMap<i32, Vec<&types::Device>> = HashMap::new();
        for d in &devs {
            by_type.entry(d.dev_type_id).or_default().push(d);
        }

        for (type_id, type_devs) in &by_type {
            let sample_name = type_devs.first().and_then(|d| d.dev_name.as_deref()).unwrap_or("?");
            eprintln!("  devTypeId={} ({} devices, e.g. '{}')", type_id, type_devs.len(), sample_name);

            // 3. KPI sample for first device of each type
            if let Some(first) = type_devs.first() {
                match nbi.get_dev_kpi(*type_id, &first.id.to_string()).await {
                    Ok(kpis) => {
                        if let Some(kpi) = kpis.first() {
                            eprintln!("    dataItemMap keys:");
                            let mut sorted_keys: Vec<_> = kpi.data_item_map.keys().collect();
                            sorted_keys.sort();
                            for key in sorted_keys {
                                let val = &kpi.data_item_map[key];
                                eprintln!("      {}: {}", key, val);
                            }
                        }
                    }
                    Err(e) => eprintln!("    KPI error: {e}"),
                }
            }
        }
    }

    // 4. Active alarms sample
    let all_codes = stations.iter().map(|s| s.station_code.as_str()).collect::<Vec<_>>().join(",");
    match nbi.get_alarms(&all_codes, 1).await {
        Ok(alarms) => {
            eprintln!("\nActive alarms: {}", alarms.len());
            for a in alarms.iter().take(10) {
                eprintln!("  [sev={}] {} @ {:?}",
                    a.lev.unwrap_or(-1),
                    a.alarm_name.as_deref().unwrap_or("?"),
                    a.dev_name.as_deref().unwrap_or("?"));
            }
        }
        Err(e) => eprintln!("\nAlarm list error: {e}"),
    }

    eprintln!("\n=== Fingerprint complete ===");
    eprintln!("Use the devTypeId and dataItemMap keys above to populate config/neteco.toml [key_maps].");
    Ok(())
}
