//! Alarm-class + severity + transition normalization.
//! Rules are ordered (first match wins) and validated against the real
//! master_alarms.log (99.27% classified). See tools/normalize_ref.py oracle.

use crate::types::{AlarmClass, Severity, Transition};
use once_cell::sync::Lazy;
use regex::Regex;

/// (class, case-insensitive pattern). Order matters: specific before generic.
static RULES: Lazy<Vec<(AlarmClass, Regex)>> = Lazy::new(|| {
    let raw: &[(AlarmClass, &str)] = &[
        (AlarmClass::NeDisconnected,  r"\bne is disconnected\b|\bdisconnected\b"),
        (AlarmClass::CommsLost,       r"gubitak komunikacije|prisustvo komunikacije|comms[- ]?lost|communication|snmpv2-mib|nepoznat|node info"),
        (AlarmClass::GensetEvent,     r"engine ?(start|stop)|generator ?(start|stop|enable|over|under)|notifengine|notiflevel|levelstatus|namedalarm|\bdse\b|\bdea[_ ]|nivo goriva|fuel|oilpressure"),
        (AlarmClass::MainsFailure,    r"nestanak 220|mains ?(failure|fail)|mains phase l[123]|mains\d*voltage|ac[_ ]?fail|acinputfault|ac input fault|input fault|ac phase l[123]|partial[- ]ac[- ]fail|phase[- ]?fail|phase l[123]|under ?voltage|nestanak (mreže|mreze)|undervoltage|ispad faze|ispad[_ ]?mreze|blackout|notifmains|mains ?return"),
        (AlarmClass::RectifierFailure, r"kvar ispravlja|rectifier (power )?(failure|fail)|rectifier[- ]fail|ispravlja"),
        (AlarmClass::RectifierComms,  r"rectifier[- ]comms[- ]lost"),
        (AlarmClass::SolarFault,      r"solar[_ ]?fail|solar[- ]comms[- ]lost"),
        (AlarmClass::UpsModule,       r"ups .*modula|ups .*module|ups[- ]?fail|alarmi modula|inverter (fault|fail)|ispad[_ ]?invertora|invertor|bypass"),
        (AlarmClass::HighVoltage,     r"visok napon|over[- ]?voltage|overvoltage|high voltage|overfrequency|prenap"),
        (AlarmClass::BatteryLow,      r"low[- ]?float|in[- ]?discharge|battdischarge|battery discharg|overdischarge|over[- ]?charge|discharge|lithium battery|busbar ?voltage ?low|bus bar undervoltage|ubbr|napon[_ ]?sab|battery (current[- ]limit|temperature)|low voltage|nizak napon|<\s*4[0-9]|prazn|low battery|fusbat"),
        (AlarmClass::BatteryFault,    r"battery[- ]?(fuse|test)[- ]?(fail|break)|battery fault|fuse break"),
        (AlarmClass::CoolingFault,    r"poorcooling|fcsoff|compressor (fault|fail)|cooling|klima|hvac|fan[- ]?fail|dirty filter|filter|filterblock|high ?pressure|low ?pressure|high ?temperature|low ?temperature|air conditioner"),
        (AlarmClass::DoorOpen,        r"door|vrata|otvorena"),
        (AlarmClass::FuseLoad,        r"load[- ]?fuse[- ]?fail|load_fuse_fail|mov[- ]?fail|system[- ]?overload|overload"),
        (AlarmClass::GenericError,    r"nurerr|urgerr|non-urgent error|presence of alarm|prisustvo alarma|surge voltage|svp|certificate|\berr\b"),
    ];
    raw.iter()
        .map(|(c, p)| (*c, Regex::new(&format!("(?i){p}")).expect("valid taxonomy regex")))
        .collect()
});

/// Classify raw alarm text into the canonical taxonomy.
pub fn classify(raw_text: &str) -> AlarmClass {
    for (class, rx) in RULES.iter() {
        if rx.is_match(raw_text) {
            return *class;
        }
    }
    AlarmClass::Unclassified
}

/// Normalize a source-specific severity token.
pub fn norm_severity(s: &str) -> Severity {
    let s = s.trim().to_lowercase();
    match s.as_str() {
        "critical" | "crit" => Severity::Critical,
        "major" | "alarm" | "high" => Severity::Major,
        "minor" => Severity::Minor,
        "warning" | "low" | "warn" => Severity::Warning,
        "info" | "information" | "node info" => Severity::Info,
        // Benning numeric severity 1..3
        "1" => Severity::Warning,
        "2" => Severity::Minor,
        "3" => Severity::Major,
        _ => Severity::Major, // default for power events
    }
}

/// Status-driven transition. Every source carries a status token that resolves
/// to raise/clear (Ignition field 5 = critical/cleared, NetEco field 5, U2020/RPS
/// status, DSE last field, Benning added/removed, etc.). Unknown -> Instant.
pub fn norm_transition(status: &str) -> Transition {
    match status.trim().to_lowercase().as_str() {
        "cleared" | "clear" | "normal" | "alarmnormal" | "removed" | "entryremoved" => {
            Transition::Clear
        }
        "critical" | "major" | "minor" | "warning" | "low" | "active" | "alarmactive"
        | "added" | "entryadded" => Transition::Raise,
        _ => Transition::Instant,
    }
}
