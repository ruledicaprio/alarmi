//! bht-loader — bulk normalizer.
//!
//! Reads the raw ispadnap log (file arg or stdin) and emits normalized events
//! in one of two formats on stdout, with a stats summary on stderr:
//!
//!   tsv  (default) : tab-separated, ready for  COPY fact_event(...) FROM STDIN
//!   jsonl          : one CanonicalEvent JSON per line
//!
//! No database driver is linked: the deliberate design is to pipe TSV straight
//! into `psql \copy`, which is the fastest bulk path into TimescaleDB and keeps
//! the loader free of compile-time DB coupling. Dropped lines go to stderr-side
//! counters (and, with --quarantine FILE, to a quarantine file).
//!
//! Usage:
//!   bht-loader master_alarms.log > events.tsv
//!   bht-loader --format jsonl master_alarms.log > events.jsonl
//!   cat master_alarms.log | bht-loader --quarantine bad.txt > events.tsv

use anyhow::Result;
use bht_normalize::{normalize_line, CanonicalEvent};
use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};

enum Format {
    Tsv,
    Jsonl,
}

fn main() -> Result<()> {
    let mut format = Format::Tsv;
    let mut input: Option<String> = None;
    let mut quarantine: Option<String> = None;

    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--format" => {
                format = match args.next().as_deref() {
                    Some("jsonl") => Format::Jsonl,
                    _ => Format::Tsv,
                }
            }
            "--quarantine" => quarantine = args.next(),
            other => input = Some(other.to_string()),
        }
    }

    let reader: Box<dyn BufRead> = match input {
        Some(path) => Box::new(BufReader::new(File::open(path)?)),
        None => Box::new(BufReader::new(io::stdin())),
    };
    let mut qfile = match quarantine {
        Some(p) => Some(File::create(p)?),
        None => None,
    };

    let stdout = io::stdout();
    let mut out = io::BufWriter::new(stdout.lock());

    let (mut total, mut ok, mut dropped) = (0u64, 0u64, 0u64);
    for line in reader.lines() {
        let line = line?;
        total += 1;
        match normalize_line(&line) {
            Ok(ev) => {
                ok += 1;
                match format {
                    Format::Tsv => writeln!(out, "{}", to_tsv(&ev))?,
                    Format::Jsonl => writeln!(out, "{}", serde_json::to_string(&ev)?)?,
                }
            }
            Err(reason) => {
                dropped += 1;
                if let Some(q) = qfile.as_mut() {
                    writeln!(q, "{reason:?}\t{line}")?;
                }
            }
        }
    }
    out.flush()?;
    eprintln!(
        "bht-loader: total={total} normalized={ok} dropped={dropped} ({:.2}% ok)",
        if total > 0 { 100.0 * ok as f64 / total as f64 } else { 0.0 }
    );
    Ok(())
}

/// Escape a field for Postgres COPY text format.
fn esc(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\t', " ").replace('\n', " ")
}

/// Column order MUST match the COPY target in db/schema.sql (fact_event).
fn to_tsv(e: &CanonicalEvent) -> String {
    // event_time, source, site_key, region, alarm_class, severity, transition,
    // raw_site, raw_alarm, device_ip
    format!(
        "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
        e.event_time.to_rfc3339(),
        json_enum(&e.source),
        esc(&e.site_key),
        esc(&e.region),
        json_enum(&e.alarm_class),
        json_enum(&e.severity),
        json_enum(&e.transition),
        esc(&e.raw_site),
        esc(&e.raw_alarm),
        e.device_ip.as_deref().map(esc).unwrap_or_else(|| "\\N".into()),
    )
}

/// Render an enum via its serde representation (snake_case / SCREAMING_SNAKE),
/// stripping the surrounding JSON quotes.
fn json_enum<T: serde::Serialize>(v: &T) -> String {
    serde_json::to_string(v).unwrap_or_default().trim_matches('"').to_string()
}
