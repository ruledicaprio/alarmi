//! Per-source line parsers for the /alarmi/ispadnap CSV-ish feed.
//! Each system has its own column order, status vocabulary, and even
//! timestamp position (Benning puts the timestamp LAST). Verified against
//! the real master_alarms.log.

use crate::classify::{classify, norm_severity, norm_transition};
use crate::types::{CanonicalEvent, DropReason, Source};
use chrono::{DateTime, FixedOffset, NaiveDateTime, Utc};
use once_cell::sync::Lazy;
use regex::Regex;

/// Sarajevo CEST (+02:00). DST refinement is a later seam (see DATA_MODEL.md).
const LOCAL_OFFSET_SECS: i32 = 2 * 3600;

static IGN_SITE_RX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^([A-Za-zŠšĐđČčĆćŽž]+)\s*-\s*(.+)$").unwrap());
static WS_RX: Lazy<Regex> = Lazy::new(|| Regex::new(r"\s+").unwrap());
static SITE_PREFIX_RX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^(BTS_|BS_|RRST_|RR_|DEA_|_DSE_)").unwrap());
static UNDERSCORES_RX: Lazy<Regex> = Lazy::new(|| Regex::new(r"_+").unwrap());

fn split_csv(line: &str) -> Vec<String> {
    line.split(',')
        .map(|p| p.trim().trim_matches('"').trim().to_string())
        .collect()
}

/// Canonicalize a raw site name into a stable site_key.
pub fn site_key(raw: &str) -> String {
    if raw.is_empty() {
        return String::new();
    }
    let mut s = raw.trim().to_uppercase();
    // normalize separators FIRST so space-separated prefixes ("BS Cadjavica")
    // strip the same as underscore ones ("BTS_CADJAVICA") -> cross-source unify.
    s = s.replace(' ', "_").replace('-', "_");
    s = UNDERSCORES_RX.replace_all(&s, "_").into_owned();
    s = SITE_PREFIX_RX.replace(&s, "").into_owned();
    s.trim_matches('_').to_string()
}

/// Parse `yyyy-MM-dd[ _]HH:mm:ss` (and a couple of fallbacks) to UTC.
pub fn parse_ts(s: &str) -> Option<DateTime<Utc>> {
    let cleaned = s.trim().replace('_', " ");
    let cleaned = WS_RX.replace_all(&cleaned, " ");
    let offset = FixedOffset::east_opt(LOCAL_OFFSET_SECS).unwrap();
    for fmt in ["%Y-%m-%d %H:%M:%S", "%d %b %Y %H:%M:%S", "%Y-%m-%d %H:%M"] {
        if let Ok(naive) = NaiveDateTime::parse_from_str(cleaned.trim(), fmt) {
            let local = naive.and_local_timezone(offset).single()?;
            return Some(local.with_timezone(&Utc));
        }
    }
    None
}

/// Intermediate, pre-canonical record produced by a source parser.
struct Raw {
    source: Source,
    raw_site: String,
    region: String,
    raw_alarm: String,
    sev: String,
    status: String,
    ts: String,
    ip: Option<String>,
}

fn finalize(r: Raw) -> Result<CanonicalEvent, DropReason> {
    let event_time = parse_ts(&r.ts)
        .ok_or_else(|| DropReason::BadTimestamp(format!("{:?}", r.source)))?;
    Ok(CanonicalEvent {
        event_time,
        source: r.source,
        site_key: site_key(&r.raw_site),
        raw_site: r.raw_site,
        region: r.region.to_uppercase(),
        alarm_class: classify(&r.raw_alarm),
        severity: norm_severity(&r.sev),
        transition: norm_transition(&r.status),
        raw_alarm: r.raw_alarm,
        device_ip: r.ip.filter(|s| !s.is_empty()),
    })
}

/// Normalize one raw line from the ispadnap feed.
pub fn normalize_line(line: &str) -> Result<CanonicalEvent, DropReason> {
    let line = line.trim_start_matches('\u{feff}').trim_end();
    if line.is_empty() || !line.contains(',') {
        return Err(DropReason::BlankOrNoComma);
    }
    let f = split_csv(line);
    let sysname = f[0].as_str();

    let raw = match sysname {
        "IgnitionSCADA" => {
            if f.len() < 6 {
                return Err(DropReason::FieldCount(sysname.into()));
            }
            let (region, site) = match IGN_SITE_RX.captures(&f[1]) {
                Some(c) => (c[1].trim().to_string(), c[2].trim().to_string()),
                None => (String::new(), f[1].clone()),
            };
            Raw { source: Source::Ignition, raw_site: site, region,
                  raw_alarm: f[3].clone(), sev: f[4].clone(), status: f[4].clone(),
                  ts: f[5].clone(), ip: None }
        }
        "NetEco" => {
            if f.len() < 4 {
                return Err(DropReason::FieldCount(sysname.into()));
            }
            let status = if f.len() >= 5 { f[4].clone() } else { "active".to_string() };
            Raw { source: Source::NetEco, raw_site: f[1].clone(), region: String::new(),
                  raw_alarm: f[2].clone(), sev: status.clone(), status,
                  ts: f[3].clone(), ip: None }
        }
        "U2020" => {
            if f.len() < 5 {
                return Err(DropReason::FieldCount(sysname.into()));
            }
            Raw { source: Source::U2020, raw_site: f[1].clone(), region: String::new(),
                  raw_alarm: f[2].clone(), sev: f[4].clone(), status: f[4].clone(),
                  ts: f[3].clone(), ip: None }
        }
        "RPS-SC200-MIB" | "RpsSc300Mib" => {
            if f.len() < 8 {
                return Err(DropReason::FieldCount(sysname.into()));
            }
            let source = if sysname == "RPS-SC200-MIB" { Source::RpsSc200 } else { Source::RpsSc300 };
            Raw { source, raw_site: f[1].clone(), region: f[2].clone(),
                  raw_alarm: f[3].clone(), sev: f[7].clone(), status: f[7].clone(),
                  ts: f[5].clone(), ip: Some(f[6].clone()) }
        }
        "DSE-74xx" => {
            if f.len() < 9 {
                return Err(DropReason::FieldCount(sysname.into()));
            }
            let alarm = if f[5].is_empty() { f[4].clone() } else { f[5].clone() };
            Raw { source: Source::Dse74xx, raw_site: f[1].clone(), region: String::new(),
                  raw_alarm: alarm, sev: f[8].clone(), status: f[8].clone(),
                  ts: f[6].clone(), ip: Some(f[7].clone()) }
        }
        "Benning_napajanje" => {
            if f.len() < 7 {
                return Err(DropReason::FieldCount(sysname.into()));
            }
            // ts is the LAST field here; ip is field 6.
            let status = if f[2].contains("Removed") { "removed" } else { "added" };
            Raw { source: Source::Benning, raw_site: f[1].clone(), region: String::new(),
                  raw_alarm: f[3].clone(), sev: f[4].clone(), status: status.into(),
                  ts: f[6].clone(), ip: Some(f[5].clone()) }
        }
        "BARAN_klima" => {
            if f.len() < 8 {
                return Err(DropReason::FieldCount(sysname.into()));
            }
            Raw { source: Source::Baran, raw_site: f[1].clone(), region: String::new(),
                  raw_alarm: f[4].clone(), sev: f[7].clone(), status: f[7].clone(),
                  ts: f[5].clone(), ip: Some(f[6].clone()) }
        }
        other => return Err(DropReason::UnknownSystem(other.chars().take(24).collect())),
    };

    finalize(raw)
}
