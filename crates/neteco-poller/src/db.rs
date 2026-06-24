//! Database writes for NetEco data — topology + metrics + alarms.
//! Uses tokio-postgres (matching existing bht-poller pattern).
//! All metric inserts use ON CONFLICT DO NOTHING (idempotent re-polls).

use crate::types::{Alarm, Device, DeviceRecord, KpiRecord, Station};
use anyhow::{Context, Result};
use std::collections::HashMap;
use tokio_postgres::Client;

/// Extract f64 from dataItemMap.
fn fv(map: &HashMap<String, serde_json::Value>, key: &str) -> Option<f64> {
    map.get(key).and_then(|v| v.as_f64())
}

/// Extract i32 from dataItemMap.
fn iv(map: &HashMap<String, serde_json::Value>, key: &str) -> Option<i32> {
    map.get(key).and_then(|v| v.as_i64()).map(|x| x as i32)
}

/// Extract i16 from dataItemMap.
fn sv(map: &HashMap<String, serde_json::Value>, key: &str) -> Option<i16> {
    map.get(key).and_then(|v| v.as_i64()).map(|x| x as i16)
}

/// Resolve column name → actual REST key via key_map.
fn key<'a>(keys: &'a HashMap<String, String>, col: &'a str) -> &'a str {
    keys.get(col).map(|s| s.as_str()).unwrap_or(col)
}

// =================================================================== TOPOLOGY

pub async fn upsert_sites(c: &Client, sites: &[Station]) -> Result<u64> {
    if sites.is_empty() { return Ok(0); }
    let codes: Vec<&str> = sites.iter().map(|s| s.station_code.as_str()).collect();
    let names: Vec<Option<&str>> = sites.iter()
        .map(|s| s.station_name.as_deref()).collect();
    let n = c.execute(
        "INSERT INTO neteco.sites (station_code, station_name, updated_at)
         SELECT sc, sn, now()
         FROM UNNEST($1::text[], $2::text[]) AS u(sc, sn)
         ON CONFLICT (station_code) DO UPDATE
           SET station_name = EXCLUDED.station_name, updated_at = now()",
        &[&codes, &names],
    ).await.context("upsert sites")?;
    Ok(n)
}

pub async fn upsert_devices(c: &Client, devices: &[Device]) -> Result<u64> {
    if devices.is_empty() { return Ok(0); }
    let ids:    Vec<i64>         = devices.iter().map(|d| d.id).collect();
    let codes:  Vec<&str>        = devices.iter().map(|d| d.station_code.as_str()).collect();
    let names:  Vec<Option<&str>>= devices.iter().map(|d| d.dev_name.as_deref()).collect();
    let esns:   Vec<Option<&str>>= devices.iter().map(|d| d.esn_code.as_deref()).collect();
    let types:  Vec<i32>         = devices.iter().map(|d| d.dev_type_id).collect();
    let lons:   Vec<Option<f64>> = devices.iter().map(|d| d.longitude).collect();
    let lats:   Vec<Option<f64>> = devices.iter().map(|d| d.latitude).collect();
    let n = c.execute(
        "INSERT INTO neteco.devices (device_id, station_code, dev_name, esn_code, dev_type_id,
                                     longitude, latitude, updated_at)
         SELECT did, sc, dn, esn, dt, lon, lat, now()
         FROM UNNEST($1::bigint[], $2::text[], $3::text[], $4::text[], $5::int[],
                     $6::float8[], $7::float8[])
           AS u(did, sc, dn, esn, dt, lon, lat)
         ON CONFLICT (device_id) DO UPDATE
           SET dev_name = EXCLUDED.dev_name,
               esn_code = EXCLUDED.esn_code,
               dev_type_id = EXCLUDED.dev_type_id,
               longitude = EXCLUDED.longitude,
               latitude = EXCLUDED.latitude,
               updated_at = now()",
        &[&ids, &codes, &names, &esns, &types, &lons, &lats],
    ).await.context("upsert devices")?;
    Ok(n)
}

pub async fn load_device_cache(c: &Client) -> Result<Vec<DeviceRecord>> {
    let rows = c.query(
        "SELECT device_id, station_code, dev_type_id, std_type_name
         FROM neteco.devices ORDER BY device_id", &[],
    ).await.context("load device cache")?;
    Ok(rows.iter().map(|r| DeviceRecord {
        device_id: r.get(0),
        station_code: r.get(1),
        dev_type_id: r.get(2),
        std_type_name: r.get(3),
    }).collect())
}

pub async fn load_station_codes(c: &Client) -> Result<Vec<String>> {
    let rows = c.query("SELECT station_code FROM neteco.sites", &[]).await?;
    Ok(rows.iter().map(|r| r.get::<_, String>(0)).collect())
}

// =================================================================== METRICS
// Per-record insert functions. Each takes one KPI record, extracts fields
// via the key_map, and inserts into the appropriate hypertable.

pub async fn insert_site_unit(
    c: &Client, ts: &str, dev_id: i64, station_code: &str,
    rec: &KpiRecord, keys: &HashMap<String, String>,
) -> Result<u64> {
    let m = &rec.data_item_map;
    let p1  = fv(m, key(keys, "indoor_temp_c"));
    let p2  = fv(m, key(keys, "outdoor_temp_c"));
    let p3  = fv(m, key(keys, "indoor_humidity_pct"));
    let p4  = fv(m, key(keys, "ac_input_power_kw"));
    let p5  = fv(m, key(keys, "dc_output_power_kw"));
    let p6  = fv(m, key(keys, "ac_output_power_kw"));
    let p7  = fv(m, key(keys, "max_bbu_temp_c"));
    let p8  = fv(m, key(keys, "total_ac_input_energy_kwh"));
    let p9  = fv(m, key(keys, "total_dc_output_energy_kwh"));
    let p10 = iv(m, key(keys, "staggering_exception_cause"));
    c.execute(
        "INSERT INTO neteco.site_unit_metrics
         (ts, device_id, station_code,
          indoor_temp_c, outdoor_temp_c, indoor_humidity_pct,
          ac_input_power_kw, dc_output_power_kw, ac_output_power_kw,
          max_bbu_temp_c, total_ac_input_energy_kwh, total_dc_output_energy_kwh,
          staggering_exception_cause)
         VALUES ($1::timestamptz, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
         ON CONFLICT (device_id, ts) DO NOTHING",
        &[&ts, &dev_id, &station_code, &p1, &p2, &p3, &p4, &p5, &p6, &p7, &p8, &p9, &p10],
    ).await.map_err(|e| anyhow::anyhow!("site_unit dev={dev_id}: {e}"))
}

pub async fn insert_power_system(
    c: &Client, ts: &str, dev_id: i64, station_code: &str,
    rec: &KpiRecord, keys: &HashMap<String, String>,
) -> Result<u64> {
    let m = &rec.data_item_map;
    let p1  = iv(m, key(keys, "current_power_supply_type"));
    let p2  = fv(m, key(keys, "dc_output_voltage_v"));
    let p3  = fv(m, key(keys, "total_dc_load_current_a"));
    let p4  = fv(m, key(keys, "total_dc_load_power_kw"));
    let p5  = fv(m, key(keys, "system_load_ratio_pct"));
    let p6  = fv(m, key(keys, "total_ac_input_energy_kwh"));
    let p7  = fv(m, key(keys, "total_dc_load_energy_kwh"));
    let p8  = fv(m, key(keys, "total_temp_control_energy_kwh"));
    let p9  = fv(m, key(keys, "port_48v_current_a"));
    let p10 = fv(m, key(keys, "dc_load_48v_current_a"));
    c.execute(
        "INSERT INTO neteco.power_system_metrics
         (ts, device_id, station_code,
          current_power_supply_type, dc_output_voltage_v, total_dc_load_current_a,
          total_dc_load_power_kw, system_load_ratio_pct,
          total_ac_input_energy_kwh, total_dc_load_energy_kwh,
          total_temp_control_energy_kwh, port_48v_current_a, dc_load_48v_current_a)
         VALUES ($1::timestamptz, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
         ON CONFLICT (device_id, ts) DO NOTHING",
        &[&ts, &dev_id, &station_code, &p1, &p2, &p3, &p4, &p5, &p6, &p7, &p8, &p9, &p10],
    ).await.map_err(|e| anyhow::anyhow!("power_system dev={dev_id}: {e}"))
}

pub async fn insert_battery_group(
    c: &Client, ts: &str, dev_id: i64, station_code: &str,
    rec: &KpiRecord, keys: &HashMap<String, String>,
) -> Result<u64> {
    let m = &rec.data_item_map;
    let p1  = iv(m, key(keys, "battery_state"));
    let p2  = fv(m, key(keys, "voltage_v"));
    let p3  = fv(m, key(keys, "current_a"));
    let p4  = fv(m, key(keys, "soc_pct"));
    let p5  = iv(m, key(keys, "soh_pct"));
    let p6  = fv(m, key(keys, "temp_c"));
    let p7  = fv(m, key(keys, "backup_time_h"));
    let p8  = fv(m, key(keys, "backup_time_ai_h"));
    let p9  = fv(m, key(keys, "charge_discharge_power_kw"));
    let p10 = iv(m, key(keys, "rated_capacity_ah"));
    let p11 = iv(m, key(keys, "remaining_capacity_ah"));
    let p12 = iv(m, key(keys, "total_cycle_times"));
    let p13 = iv(m, key(keys, "current_limiting_state"));
    let p14 = iv(m, key(keys, "on_off_state"));
    c.execute(
        "INSERT INTO neteco.battery_group_metrics
         (ts, device_id, station_code,
          battery_state, voltage_v, current_a, soc_pct, soh_pct, temp_c,
          backup_time_h, backup_time_ai_h, charge_discharge_power_kw,
          rated_capacity_ah, remaining_capacity_ah,
          total_cycle_times, current_limiting_state, on_off_state)
         VALUES ($1::timestamptz, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17)
         ON CONFLICT (device_id, ts) DO NOTHING",
        &[&ts, &dev_id, &station_code, &p1, &p2, &p3, &p4, &p5, &p6,
          &p7, &p8, &p9, &p10, &p11, &p12, &p13, &p14],
    ).await.map_err(|e| anyhow::anyhow!("battery_group dev={dev_id}: {e}"))
}

pub async fn insert_mains(
    c: &Client, ts: &str, dev_id: i64, station_code: &str,
    rec: &KpiRecord, keys: &HashMap<String, String>,
) -> Result<u64> {
    let m = &rec.data_item_map;
    let p1  = iv(m, key(keys, "mains_state"));
    let p2  = fv(m, key(keys, "ac_voltage_v"));
    let p3  = fv(m, key(keys, "phase_l1_v"));
    let p4  = fv(m, key(keys, "phase_l2_v"));
    let p5  = fv(m, key(keys, "phase_l3_v"));
    let p6  = fv(m, key(keys, "ac_current_a"));
    let p7  = fv(m, key(keys, "phase_l1_a"));
    let p8  = fv(m, key(keys, "phase_l2_a"));
    let p9  = fv(m, key(keys, "phase_l3_a"));
    let p10 = fv(m, key(keys, "active_power_kw"));
    let p11 = fv(m, key(keys, "ac_freq_hz"));
    let p12 = fv(m, key(keys, "power_factor"));
    let p13 = fv(m, key(keys, "total_energy_kwh"));
    let p14 = iv(m, key(keys, "grid_quality_grade"));
    c.execute(
        "INSERT INTO neteco.mains_metrics
         (ts, device_id, station_code,
          mains_state, ac_voltage_v,
          phase_l1_v, phase_l2_v, phase_l3_v,
          ac_current_a, phase_l1_a, phase_l2_a, phase_l3_a,
          active_power_kw, ac_freq_hz, power_factor,
          total_energy_kwh, grid_quality_grade)
         VALUES ($1::timestamptz, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17)
         ON CONFLICT (device_id, ts) DO NOTHING",
        &[&ts, &dev_id, &station_code, &p1, &p2, &p3, &p4, &p5, &p6,
          &p7, &p8, &p9, &p10, &p11, &p12, &p13, &p14],
    ).await.map_err(|e| anyhow::anyhow!("mains dev={dev_id}: {e}"))
}

pub async fn insert_dpdu(
    c: &Client, ts: &str, dev_id: i64, station_code: &str,
    rec: &KpiRecord, keys: &HashMap<String, String>,
) -> Result<u64> {
    let m = &rec.data_item_map;
    let p1 = fv(m, key(keys, "dc_output_voltage_v"));
    let p2 = fv(m, key(keys, "total_dc_load_current_a"));
    let p3 = fv(m, key(keys, "total_dc_load_power_kw"));
    let p4 = fv(m, key(keys, "total_dc_load_energy_kwh"));
    let p5 = fv(m, key(keys, "other_power_input_current_a"));
    let p6 = sv(m, key(keys, "num_llvd"));
    c.execute(
        "INSERT INTO neteco.dpdu_metrics
         (ts, device_id, station_code,
          dc_output_voltage_v, total_dc_load_current_a, total_dc_load_power_kw,
          total_dc_load_energy_kwh, other_power_input_current_a, num_llvd)
         VALUES ($1::timestamptz, $2, $3, $4, $5, $6, $7, $8, $9)
         ON CONFLICT (device_id, ts) DO NOTHING",
        &[&ts, &dev_id, &station_code, &p1, &p2, &p3, &p4, &p5, &p6],
    ).await.map_err(|e| anyhow::anyhow!("dpdu dev={dev_id}: {e}"))
}

pub async fn insert_ac_input(
    c: &Client, ts: &str, dev_id: i64, station_code: &str,
    rec: &KpiRecord, keys: &HashMap<String, String>,
) -> Result<u64> {
    let m = &rec.data_item_map;
    let p1  = iv(m, key(keys, "ac_input_state"));
    let p2  = fv(m, key(keys, "phase_l1_v"));
    let p3  = fv(m, key(keys, "phase_l2_v"));
    let p4  = fv(m, key(keys, "phase_l3_v"));
    let p5  = fv(m, key(keys, "phase_l1_a"));
    let p6  = fv(m, key(keys, "phase_l2_a"));
    let p7  = fv(m, key(keys, "phase_l3_a"));
    let p8  = fv(m, key(keys, "ac_freq_hz"));
    let p9  = fv(m, key(keys, "active_power_kw"));
    let p10 = fv(m, key(keys, "apparent_power_kva"));
    let p11 = fv(m, key(keys, "power_factor"));
    let p12 = fv(m, key(keys, "total_energy_kwh"));
    let p13 = iv(m, key(keys, "agregat_u_radu"));
    c.execute(
        "INSERT INTO neteco.ac_input_metrics
         (ts, device_id, station_code,
          ac_input_state,
          phase_l1_v, phase_l2_v, phase_l3_v,
          phase_l1_a, phase_l2_a, phase_l3_a,
          ac_freq_hz, active_power_kw, apparent_power_kva,
          power_factor, total_energy_kwh, agregat_u_radu)
         VALUES ($1::timestamptz, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)
         ON CONFLICT (device_id, ts) DO NOTHING",
        &[&ts, &dev_id, &station_code, &p1, &p2, &p3, &p4, &p5, &p6,
          &p7, &p8, &p9, &p10, &p11, &p12, &p13],
    ).await.map_err(|e| anyhow::anyhow!("ac_input dev={dev_id}: {e}"))
}

pub async fn insert_genset(
    c: &Client, ts: &str, dev_id: i64, station_code: &str,
    rec: &KpiRecord, keys: &HashMap<String, String>,
) -> Result<u64> {
    let m = &rec.data_item_map;
    let p1  = iv(m, key(keys, "running_state"));
    let p2  = fv(m, key(keys, "load_rate_pct"));
    let p3  = fv(m, key(keys, "cabin_temp_c"));
    let p4  = fv(m, key(keys, "coolant_temp_c"));
    let p5  = fv(m, key(keys, "oil_pressure_bar"));
    let p6  = iv(m, key(keys, "oil_level_pct"));
    let p7  = iv(m, key(keys, "rotation_speed_rpm"));
    let p8  = fv(m, key(keys, "output_power_kw"));
    let p9  = fv(m, key(keys, "ac_freq_hz"));
    let p10 = fv(m, key(keys, "phase_l1_v"));
    let p11 = fv(m, key(keys, "phase_l2_v"));
    let p12 = fv(m, key(keys, "phase_l3_v"));
    let p13 = fv(m, key(keys, "phase_l1_a"));
    let p14 = fv(m, key(keys, "phase_l2_a"));
    let p15 = fv(m, key(keys, "phase_l3_a"));
    let p16 = fv(m, key(keys, "total_runtime_h"));
    let p17 = fv(m, key(keys, "total_fuel_l"));
    let p18 = fv(m, key(keys, "estimated_runtime_h"));
    let p19 = fv(m, key(keys, "total_energy_yield_kwh"));
    let p20 = fv(m, key(keys, "genset_battery_voltage_v"));
    c.execute(
        "INSERT INTO neteco.genset_metrics
         (ts, device_id, station_code,
          running_state, load_rate_pct, cabin_temp_c, coolant_temp_c,
          oil_pressure_bar, oil_level_pct, rotation_speed_rpm,
          output_power_kw, ac_freq_hz,
          phase_l1_v, phase_l2_v, phase_l3_v,
          phase_l1_a, phase_l2_a, phase_l3_a,
          total_runtime_h, total_fuel_l, estimated_runtime_h,
          total_energy_yield_kwh, genset_battery_voltage_v)
         VALUES ($1::timestamptz,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22,$23)
         ON CONFLICT (device_id, ts) DO NOTHING",
        &[&ts, &dev_id, &station_code,
          &p1, &p2, &p3, &p4, &p5, &p6, &p7, &p8, &p9,
          &p10, &p11, &p12, &p13, &p14, &p15,
          &p16, &p17, &p18, &p19, &p20],
    ).await.map_err(|e| anyhow::anyhow!("genset dev={dev_id}: {e}"))
}

pub async fn insert_rectifier_group(
    c: &Client, ts: &str, dev_id: i64, station_code: &str,
    rec: &KpiRecord, keys: &HashMap<String, String>,
) -> Result<u64> {
    let m = &rec.data_item_map;
    let p1 = iv(m, key(keys, "qty_rectifiers"));
    let p2 = fv(m, key(keys, "total_dc_output_current_a"));
    let p3 = fv(m, key(keys, "total_dc_output_power_kw"));
    let p4 = fv(m, key(keys, "load_usage_rate_pct"));
    let p5 = fv(m, key(keys, "output_voltage_v"));
    let p6 = fv(m, key(keys, "total_input_power_kw"));
    let p7 = fv(m, key(keys, "total_input_energy_kwh"));
    c.execute(
        "INSERT INTO neteco.rectifier_group_metrics
         (ts, device_id, station_code,
          qty_rectifiers, total_dc_output_current_a, total_dc_output_power_kw,
          load_usage_rate_pct, output_voltage_v,
          total_input_power_kw, total_input_energy_kwh)
         VALUES ($1::timestamptz, $2, $3, $4, $5, $6, $7, $8, $9, $10)
         ON CONFLICT (device_id, ts) DO NOTHING",
        &[&ts, &dev_id, &station_code, &p1, &p2, &p3, &p4, &p5, &p6, &p7],
    ).await.map_err(|e| anyhow::anyhow!("rectifier_group dev={dev_id}: {e}"))
}

// =================================================================== ALARMS

pub async fn upsert_alarms(c: &Client, alarms: &[Alarm], source: &str) -> Result<u64> {
    if alarms.is_empty() { return Ok(0); }
    let mut count = 0u64;
    for a in alarms {
        let alarm_id = match &a.alarm_id {
            Some(id) if !id.is_empty() => id.as_str(),
            _ => continue,
        };
        let raise_str = a.raise_time
            .and_then(|ms| chrono::DateTime::from_timestamp_millis(ms))
            .map(|dt| dt.to_rfc3339());
        let repair_str = a.repair_time
            .and_then(|ms| chrono::DateTime::from_timestamp_millis(ms))
            .map(|dt| dt.to_rfc3339());

        let res = c.execute(
            "INSERT INTO neteco.alarms
             (alarm_id, station_code, station_name, device_id, dev_name,
              dev_type_id, alarm_name, alarm_cause, alarm_type,
              severity, status, raise_time, repair_time, source, last_seen)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,
                     $12::timestamptz, $13::timestamptz, $14, now())
             ON CONFLICT (alarm_id) DO UPDATE SET
               status = EXCLUDED.status,
               repair_time = EXCLUDED.repair_time,
               last_seen = now()",
            &[
                &alarm_id,
                &a.station_code.as_deref(),
                &a.station_name.as_deref(),
                &a.dev_id,
                &a.dev_name.as_deref(),
                &a.dev_type_id,
                &a.alarm_name.as_deref(),
                &a.alarm_cause.as_deref(),
                &a.alarm_type,
                &a.lev,
                &a.status,
                &raise_str.as_deref(),
                &repair_str.as_deref(),
                &source,
            ],
        ).await;
        match res {
            Ok(n) => count += n,
            Err(e) => eprintln!("[db] alarm upsert {}: {e}", alarm_id),
        }
    }
    Ok(count)
}
