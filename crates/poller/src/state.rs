//! Edge detection: convert per-poll active-alarm snapshots into RAISE/CLEAR
//! transitions, so Modbus alarms pair into episodes exactly like the other
//! stateful sources (see Stage-1 rebuild_episodes()).

use crate::types::AlarmDef;
use bht_normalize::{CanonicalEvent, Source, Transition};
use chrono::Utc;
use std::collections::HashMap;

/// One active alarm in a poll snapshot. `key` is stable per device+addr+kind
/// (names repeat across addresses, so we key by addr, not name).
#[derive(Debug, Clone)]
pub struct Active {
    pub key: String,
    pub def: AlarmDef,
}

/// Remembers the last active set per device ip.
#[derive(Default)]
pub struct AlarmStore {
    per_device: HashMap<String, HashMap<String, AlarmDef>>,
}

impl AlarmStore {
    /// Diff a fresh snapshot against the prior one; emit RAISE for new alarms,
    /// CLEAR for ones that disappeared. Updates stored state.
    pub fn diff(&mut self, ip: &str, site_key: &str, now_active: Vec<Active>) -> Vec<CanonicalEvent> {
        let ts = Utc::now();
        let new_map: HashMap<String, AlarmDef> =
            now_active.into_iter().map(|a| (a.key, a.def)).collect();
        let prev = self.per_device.remove(ip).unwrap_or_default();
        let mut events = Vec::new();

        for (k, def) in &new_map {
            if !prev.contains_key(k) {
                events.push(make_event(ts, site_key, ip, def, Transition::Raise));
            }
        }
        for (k, def) in &prev {
            if !new_map.contains_key(k) {
                events.push(make_event(ts, site_key, ip, def, Transition::Clear));
            }
        }
        self.per_device.insert(ip.to_string(), new_map);
        events
    }
}

fn make_event(
    ts: chrono::DateTime<Utc>, site_key: &str, ip: &str, def: &AlarmDef, transition: Transition,
) -> CanonicalEvent {
    CanonicalEvent {
        event_time: ts,
        source: Source::ModbusEaton,
        raw_site: site_key.to_string(),
        site_key: site_key.to_string(),
        region: String::new(),
        alarm_class: def.class,
        severity: def.severity,
        transition,
        raw_alarm: def.name.clone(),
        device_ip: Some(ip.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bht_normalize::{AlarmClass, Severity};

    fn act(addr: u16, name: &str) -> Active {
        Active { key: format!("d:{addr}"), def: AlarmDef {
            addr, name: name.into(), class: AlarmClass::MainsFailure, severity: Severity::Critical } }
    }

    #[test]
    fn raise_then_clear() {
        let mut s = AlarmStore::default();
        let e1 = s.diff("10.10.1.1", "SITE", vec![act(1210, "AC-Fail")]);
        assert_eq!(e1.len(), 1);
        assert_eq!(e1[0].transition, Transition::Raise);

        // same alarm still active next poll -> no event
        let e2 = s.diff("10.10.1.1", "SITE", vec![act(1210, "AC-Fail")]);
        assert!(e2.is_empty());

        // alarm gone -> CLEAR
        let e3 = s.diff("10.10.1.1", "SITE", vec![]);
        assert_eq!(e3.len(), 1);
        assert_eq!(e3[0].transition, Transition::Clear);
    }
}
