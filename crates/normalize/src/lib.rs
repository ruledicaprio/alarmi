//! BHT alarm normalization.
//!
//! Stage 1 of the pipeline: take the heterogeneous `/alarmi/ispadnap` log feed
//! and the `/alarmi/` out-of-service table and collapse them into one
//! [`CanonicalEvent`] stream keyed to a stable `site_key` and a controlled
//! [`AlarmClass`] taxonomy. The Modbus poller (later stage) emits the same
//! [`CanonicalEvent`] type via the `Source::ModbusEaton` variant.

pub mod chinese;
pub mod classify;
pub mod html;
pub mod parse;
pub mod types;

pub use chinese::translate as translate_zh;
pub use classify::{classify, norm_severity, norm_transition};
pub use html::{parse_oos_table, parse_smetnje_html};
pub use parse::{normalize_line, parse_ts, site_key};
pub use types::{
    AlarmClass, CanonicalEvent, DropReason, Severity, Source, Transition,
};

/// Outcome of normalizing a batch of raw lines.
#[derive(Debug, Default)]
pub struct BatchStats {
    pub total: usize,
    pub ok: usize,
    pub dropped: usize,
}

/// Normalize an iterator of raw ispadnap lines, calling `sink` for each event
/// and `on_drop` for each rejected line. Returns batch statistics.
pub fn normalize_lines<'a, I, S, D>(lines: I, mut sink: S, mut on_drop: D) -> BatchStats
where
    I: IntoIterator<Item = &'a str>,
    S: FnMut(CanonicalEvent),
    D: FnMut(&'a str, DropReason),
{
    let mut stats = BatchStats::default();
    for line in lines {
        stats.total += 1;
        match normalize_line(line) {
            Ok(ev) => {
                stats.ok += 1;
                sink(ev);
            }
            Err(reason) => {
                stats.dropped += 1;
                on_drop(line, reason);
            }
        }
    }
    stats
}
