//! Output sink: either a dry-run printer or batched writes to TimescaleDB.
//! Measurements and alarm events are inserted per cycle via UNNEST array
//! parameters (one round-trip each). Enum values render to their Stage-1
//! serde labels and are cast to the DB enum types.

use crate::types::DeviceCfg;
use anyhow::Result;
use bht_normalize::CanonicalEvent;
use serde::Serialize;
use tokio_postgres::{Client, NoTls};

pub struct MeasRow {
    pub ts: chrono::DateTime<chrono::Utc>,
    pub site_key: String,
    pub ip: String,
    pub metric: String,
    pub value: f64,
}

pub enum Sink {
    DryRun,
    Db(Client),
}

impl Sink {
    pub async fn connect(dsn: &str) -> Result<Sink> {
        let (client, conn) = tokio_postgres::connect(dsn, NoTls).await?;
        tokio::spawn(async move {
            if let Err(e) = conn.await { eprintln!("[db] connection error: {e}"); }
        });
        Ok(Sink::Db(client))
    }

    pub async fn write_measurements(&self, rows: &[MeasRow]) -> Result<u64> {
        if rows.is_empty() { return Ok(0); }
        match self {
            Sink::DryRun => {
                eprintln!("[dry-run] {} measurements", rows.len());
                Ok(rows.len() as u64)
            }
            Sink::Db(c) => {
                let ts: Vec<String> = rows.iter().map(|r| r.ts.to_rfc3339()).collect();
                let sk: Vec<&str> = rows.iter().map(|r| r.site_key.as_str()).collect();
                let ip: Vec<&str> = rows.iter().map(|r| r.ip.as_str()).collect();
                let mc: Vec<&str> = rows.iter().map(|r| r.metric.as_str()).collect();
                let vl: Vec<f64> = rows.iter().map(|r| r.value).collect();
                let n = c.execute(
                    "INSERT INTO fact_measurement(ts,site_key,device_ip,metric,value) \
                     SELECT t::timestamptz, s, NULLIF(i,'')::inet, m, v \
                     FROM UNNEST($1::text[],$2::text[],$3::text[],$4::text[],$5::float8[]) AS u(t,s,i,m,v)",
                    &[&ts, &sk, &ip, &mc, &vl],
                ).await?;
                Ok(n)
            }
        }
    }

    pub async fn write_events(&self, evs: &[CanonicalEvent]) -> Result<u64> {
        if evs.is_empty() { return Ok(0); }
        match self {
            Sink::DryRun => {
                for e in evs {
                    eprintln!("[dry-run] {:?} {} {} {:?} {:?}",
                        e.transition, e.site_key, e.raw_alarm, e.alarm_class, e.severity);
                }
                Ok(evs.len() as u64)
            }
            Sink::Db(c) => {
                let t:  Vec<String> = evs.iter().map(|e| e.event_time.to_rfc3339()).collect();
                let s:  Vec<String> = evs.iter().map(|e| label(&e.source)).collect();
                let sk: Vec<&str>   = evs.iter().map(|e| e.site_key.as_str()).collect();
                let rg: Vec<&str>   = evs.iter().map(|e| e.region.as_str()).collect();
                let ac: Vec<String> = evs.iter().map(|e| label(&e.alarm_class)).collect();
                let sv: Vec<String> = evs.iter().map(|e| label(&e.severity)).collect();
                let tr: Vec<String> = evs.iter().map(|e| label(&e.transition)).collect();
                let rs: Vec<&str>   = evs.iter().map(|e| e.raw_site.as_str()).collect();
                let ra: Vec<&str>   = evs.iter().map(|e| e.raw_alarm.as_str()).collect();
                let ip: Vec<String> = evs.iter().map(|e| e.device_ip.clone().unwrap_or_default()).collect();
                let n = c.execute(
                    "INSERT INTO fact_event(event_time,source,site_key,region,alarm_class,severity,transition,raw_site,raw_alarm,device_ip) \
                     SELECT t::timestamptz, s::source_t, sk, rg, ac::alarm_class_t, sv::severity_t, tr::transition_t, rs, ra, NULLIF(i,'')::inet \
                     FROM UNNEST($1::text[],$2::text[],$3::text[],$4::text[],$5::text[],$6::text[],$7::text[],$8::text[],$9::text[],$10::text[]) \
                       AS u(t,s,sk,rg,ac,sv,tr,rs,ra,i)",
                    &[&t,&s,&sk,&rg,&ac,&sv,&tr,&rs,&ra,&ip],
                ).await?;
                Ok(n)
            }
        }
    }

    /// Load all enabled devices from dim_device (DB mode) or return empty vec (dry-run).
    pub async fn load_devices(&self) -> Result<Vec<DeviceCfg>> {
        let c = match self { Sink::Db(c) => c, Sink::DryRun => return Ok(Vec::new()) };
        let rows = c.query(
            "SELECT host(ip)::text, port, unit_id, site_key, name, dev_type, base0, fne \
             FROM dim_device WHERE enabled = true ORDER BY id",
            &[],
        ).await?;
        Ok(rows.iter().map(|r| DeviceCfg {
            ip:       r.get(0),
            port:     r.get::<_, i32>(1) as u16,
            unit:     r.get::<_, i16>(2) as u8,
            site_key: r.get(3),
            name:     r.get(4),
            dev_type: r.get(5),
            base0:    r.get(6),
            fne:      r.get(7),
            enabled:  true,
        }).collect())
    }

    /// Batch-update dim_device health columns after a poll cycle.
    /// ok:   [(ip, unit_id)] — successful polls → last_ok=now, fail_streak=0
    /// fail: [(ip, unit_id)] — failed polls      → fail_streak+1
    /// Devices skipped by open circuit breakers are not in either slice.
    pub async fn write_health(&self, ok: &[(String, i16)], fail: &[(String, i16)]) -> Result<()> {
        let c = match self { Sink::Db(c) => c, Sink::DryRun => return Ok(()) };
        if !ok.is_empty() {
            let ips:  Vec<&str> = ok.iter().map(|(ip, _)| ip.as_str()).collect();
            let uids: Vec<i16>  = ok.iter().map(|(_, u)| *u).collect();
            c.execute(
                "UPDATE dim_device d \
                 SET last_polled = now(), last_ok = now(), fail_streak = 0, updated_at = now() \
                 FROM UNNEST($1::text[], $2::smallint[]) AS u(ip, uid) \
                 WHERE d.ip::text = u.ip AND d.unit_id = u.uid",
                &[&ips, &uids],
            ).await?;
        }
        if !fail.is_empty() {
            let ips:  Vec<&str> = fail.iter().map(|(ip, _)| ip.as_str()).collect();
            let uids: Vec<i16>  = fail.iter().map(|(_, u)| *u).collect();
            c.execute(
                "UPDATE dim_device d \
                 SET last_polled = now(), fail_streak = d.fail_streak + 1, updated_at = now() \
                 FROM UNNEST($1::text[], $2::smallint[]) AS u(ip, uid) \
                 WHERE d.ip::text = u.ip AND d.unit_id = u.uid",
                &[&ips, &uids],
            ).await?;
        }
        Ok(())
    }
}

/// Enum -> Stage-1 serde label (e.g. MainsFailure -> "MAINS_FAILURE").
fn label<T: Serialize>(v: &T) -> String {
    serde_json::to_string(v).unwrap_or_default().trim_matches('"').to_string()
}
