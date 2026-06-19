//! Parsers for the /alarmi/ HTML feeds.
//!
//! - `parse_oos_table` takes already-de-HTMLed lines (the old loader path) for
//!   3-column rows: `<site>  <ts>  <city>`.
//! - `parse_smetnje_html` takes the raw `/smetnje.html` body (4-column table:
//!   `<site> | <ip> | <ts> | <city>`) and does its own HTML scraping so the
//!   API endpoint can accept the raw curl output directly.
//!
//! Both produce `ServiceOutage` events keyed to the section technology
//! (PRISTUP / BTS / MPLS / DC / DWDM / RR / SDH).

use crate::parse::{parse_ts, site_key};
use crate::types::{AlarmClass, CanonicalEvent, Severity, Source, Transition};
use once_cell::sync::Lazy;
use regex::Regex;

static SECTION_RX: Lazy<Regex> = Lazy::new(|| Regex::new(r"^-+([A-Z]+)-+").unwrap());
static ROW_RX: Lazy<Regex> = Lazy::new(|| {
    // site (greedy up to ts)  ts  city
    Regex::new(r"^(.*?)\s+(\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2})\s+(.+?)\s*$").unwrap()
});

static TR_RX: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?is)<tr\b[^>]*>(.*?)</tr>").unwrap());
static TD_RX: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?is)<td\b[^>]*>(.*?)</td>").unwrap());
static INNER_TAG_RX: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?is)<[^>]+>").unwrap());
static IPV4_RX: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\d{1,3}(?:\.\d{1,3}){3}$").unwrap());
static TS_RX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}$").unwrap());

fn html_decode(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", " ")
        .replace("&#39;", "'")
        .replace("&quot;", "\"")
}

fn cell_text(raw_inner: &str) -> String {
    let stripped = INNER_TAG_RX.replace_all(raw_inner, " ");
    html_decode(&stripped).split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Parse the plaintext-rendered out-of-service table into outage events.
/// `lines` are the visible rows (dividers + data rows), already de-HTMLed.
pub fn parse_oos_table(lines: &[&str]) -> Vec<CanonicalEvent> {
    let mut section = String::from("UNKNOWN");
    let mut out = Vec::new();
    for line in lines {
        let line = line.trim();
        if let Some(c) = SECTION_RX.captures(line) {
            section = c[1].to_string();
            continue;
        }
        if let Some(c) = ROW_RX.captures(line) {
            let raw_site = c[1].trim().to_string();
            if raw_site.is_empty() || raw_site.chars().all(|ch| ch == '-') {
                continue;
            }
            let Some(event_time) = parse_ts(&c[2]) else { continue };
            let region = c[3].trim().to_uppercase();
            out.push(CanonicalEvent {
                event_time,
                source: Source::HtmlOos,
                site_key: site_key(&raw_site),
                raw_site,
                region,
                alarm_class: AlarmClass::ServiceOutage,
                severity: Severity::Major,
                transition: Transition::Raise, // present in table = outage active
                raw_alarm: format!("OUT_OF_SERVICE:{section}"),
                device_ip: None,
            });
        }
    }
    out
}

/// Parse the raw HTML of `/smetnje.html` (4-column table) into outage events.
/// Per data row: `<site> | <ip> | <yyyy-MM-dd HH:mm:ss> | <city>`.
/// Section dividers (`-----PRISTUP-----` etc.) switch the technology tag.
/// Page-header date rows, empty rows, and all-dashes rows are skipped.
pub fn parse_smetnje_html(html: &str) -> Vec<CanonicalEvent> {
    let mut section = String::from("UNKNOWN");
    let mut out = Vec::new();

    for tr in TR_RX.captures_iter(html) {
        let cells: Vec<String> = TD_RX
            .captures_iter(&tr[1])
            .map(|c| cell_text(&c[1]))
            .collect();

        if cells.is_empty() {
            continue;
        }
        if let Some(c) = SECTION_RX.captures(&cells[0]) {
            section = c[1].to_string();
            continue;
        }
        if cells.len() < 4 {
            continue;
        }
        let (site, ip, ts, city) = (&cells[0], &cells[1], &cells[2], &cells[3]);
        if site.is_empty() || site.chars().all(|ch| ch == '-') {
            continue;
        }
        if !TS_RX.is_match(ts) {
            continue;
        }
        let Some(event_time) = parse_ts(ts) else { continue };
        let device_ip = if IPV4_RX.is_match(ip) { Some(ip.clone()) } else { None };
        out.push(CanonicalEvent {
            event_time,
            source: Source::HtmlOos,
            site_key: site_key(site),
            raw_site: site.clone(),
            region: city.to_uppercase(),
            alarm_class: AlarmClass::ServiceOutage,
            severity: Severity::Major,
            transition: Transition::Raise,
            raw_alarm: format!("OUT_OF_SERVICE:{section}"),
            device_ip,
        });
    }
    out
}
