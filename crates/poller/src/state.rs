//! Edge detection: convert per-poll active-alarm snapshots into RAISE/CLEAR
//! transitions, so device alarms pair into episodes exactly like the other
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

/// Remembers the last active set per device ip + source (so the same IP
/// served by two device types doesn't cross-contaminate state).
#[derive(Default)]
pub struct AlarmStore {
    per_device: HashMap<(String, Source), HashMap<String, AlarmDef>>,
}

impl AlarmStore {
    /// Diff a fresh snapshot against the prior one; emit RAISE for new alarms,
    /// CLEAR for ones that disappeared. Updates stored state.
    pub fn diff(&mut self, ip: &str, site_key: &str, source: Source,
                now_active: Vec<Active>) -> Vec<CanonicalEvent> {
        let ts = Utc::now();
        let new_map: HashMap<String, AlarmDef> =
            now_active.into_iter().map(|a| (a.key, a.def)).collect();
        let prev = self.per_device.remove(&(ip.to_string(), source)).unwrap_or_default();
        let mut events = Vec::new();

        for (k, def) in &new_map {
            if !prev.contains_key(k) {
                events.push(make_event(ts, site_key, ip, source, def, Transition::Raise));
            }
        }
        for (k, def) in &prev {
            if !new_map.contains_key(k) {
                events.push(make_event(ts, site_key, ip, source, def, Transition::Clear));
            }
        }
        self.per_device.insert((ip.to_string(), source), new_map);
        events
    }
}

fn make_event(
    ts: chrono::DateTime<Utc>, site_key: &str, ip: &str, source: Source,
    def: &AlarmDef, transition: Transition,
) -> CanonicalEvent {
    CanonicalEvent {
        event_time: ts,
        source,
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
        let e1 = s.diff("10.10.1.1", "SITE", Source::ModbusEaton, vec![act(1210, "AC-Fail")]);
        assert_eq!(e1.len(), 1);
        assert_eq!(e1[0].transition, Transition::Raise);

        let e2 = s.diff("10.10.1.1", "SITE", Source::ModbusEaton, vec![act(1210, "AC-Fail")]);
        assert!(e2.is_empty());

        let e3 = s.diff("10.10.1.1", "SITE", Source::ModbusEaton, vec![]);
        assert_eq!(e3.len(), 1);
        assert_eq!(e3[0].transition, Transition::Clear);
    }
}
