//! Parser for the /alarmi/ "out of service" table.
//! The page is sectioned by technology dividers (PRISTUP / BTS / MPLS / DC /
//! DWDM / RR / SDH). Each data row is: `<site>  <yyyy-MM-dd HH:mm:ss>  <city>`.
//! Every row becomes a ServiceOutage event whose technology = current section.

use crate::parse::{parse_ts, site_key};
use crate::types::{AlarmClass, CanonicalEvent, Severity, Source, Transition};
use once_cell::sync::Lazy;
use regex::Regex;

static SECTION_RX: Lazy<Regex> = Lazy::new(|| Regex::new(r"^-+([A-Z]+)-+").unwrap());
static ROW_RX: Lazy<Regex> = Lazy::new(|| {
    // site (greedy up to ts)  ts  city
    Regex::new(r"^(.*?)\s+(\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2})\s+(.+?)\s*$").unwrap()
});

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
