//! DB pool + insert helpers shared by ingest handlers. Inserts use UNNEST array
//! params (one round-trip) and match the Stage-1/2 column order + enum labels.

use anyhow::Result;
use bht_normalize::CanonicalEvent;
use deadpool_postgres::{Manager, ManagerConfig, Pool, RecyclingMethod};
use serde::Deserialize;
use serde::Serialize;
use tokio_postgres::NoTls;

pub fn make_pool(dsn: &str) -> Result<Pool> {
    let pg: tokio_postgres::Config = dsn.parse()?;
    let mgr = Manager::from_config(pg, NoTls, ManagerConfig { recycling_method: RecyclingMethod::Fast });
    Ok(Pool::builder(mgr).max_size(16).build()?)
}

/// Measurement as accepted by POST /ingest/measurements.
#[derive(Debug, Deserialize)]
pub struct MeasIn {
    pub ts: Option<String>,           // rfc3339; defaults to now()
    pub site_key: String,
    pub device_ip: Option<String>,
    pub metric: String,
    pub value: f64,
}

fn label<T: Serialize>(v: &T) -> String {
    serde_json::to_string(v).unwrap_or_default().trim_matches('"').to_string()
}

pub async fn insert_events(pool: &Pool, evs: &[CanonicalEvent]) -> Result<u64> {
    if evs.is_empty() { return Ok(0); }
    let client = pool.get().await?;
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
    let n = client.execute(
        "INSERT INTO fact_event(event_time,source,site_key,region,alarm_class,severity,transition,raw_site,raw_alarm,device_ip) \
         SELECT t::timestamptz, s::source_t, sk, rg, ac::alarm_class_t, sv::severity_t, tr::transition_t, rs, ra, NULLIF(i,'')::inet \
         FROM UNNEST($1::text[],$2::text[],$3::text[],$4::text[],$5::text[],$6::text[],$7::text[],$8::text[],$9::text[],$10::text[]) \
           AS u(t,s,sk,rg,ac,sv,tr,rs,ra,i)",
        &[&t,&s,&sk,&rg,&ac,&sv,&tr,&rs,&ra,&ip],
    ).await?;
    Ok(n)
}

pub async fn insert_measurements(pool: &Pool, rows: &[MeasIn]) -> Result<u64> {
    if rows.is_empty() { return Ok(0); }
    let client = pool.get().await?;
    let now = chrono::Utc::now().to_rfc3339();
    let ts: Vec<String> = rows.iter().map(|r| r.ts.clone().unwrap_or_else(|| now.clone())).collect();
    let sk: Vec<&str>   = rows.iter().map(|r| r.site_key.as_str()).collect();
    let ip: Vec<String> = rows.iter().map(|r| r.device_ip.clone().unwrap_or_default()).collect();
    let mc: Vec<&str>   = rows.iter().map(|r| r.metric.as_str()).collect();
    let vl: Vec<f64>    = rows.iter().map(|r| r.value).collect();
    let n = client.execute(
        "INSERT INTO fact_measurement(ts,site_key,device_ip,metric,value) \
         SELECT t::timestamptz, s, NULLIF(i,'')::inet, m, v \
         FROM UNNEST($1::text[],$2::text[],$3::text[],$4::text[],$5::float8[]) AS u(t,s,i,m,v)",
        &[&ts,&sk,&ip,&mc,&vl],
    ).await?;
    Ok(n)
}
