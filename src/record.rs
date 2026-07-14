use std::io::Write;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, ensure};
use chrono::{SecondsFormat, Utc};
use serde::Serialize;

use crate::cpu::PerCoreSample;
use crate::engine::{CpuEngine, EngineSnapshot};

const SCHEMA_VERSION: u8 = 1;
const CSV_HEADER: [&str; 11] = [
    "schema_version",
    "sequence",
    "timestamp_utc",
    "monotonic_ms",
    "interval_ms",
    "sample_duration_us",
    "cpu_total_ratio",
    "cpu_user_ratio",
    "cpu_system_ratio",
    "cpu_nice_ratio",
    "cpu_idle_ratio",
];
const PER_CORE_CSV_HEADER: [&str; 10] = [
    "schema_version",
    "sequence",
    "timestamp_utc",
    "logical_cpu",
    "cluster",
    "cpu_total_ratio",
    "cpu_user_ratio",
    "cpu_system_ratio",
    "cpu_nice_ratio",
    "cpu_idle_ratio",
];

#[derive(Clone, Copy, Debug)]
pub struct RecordOptions {
    pub interval: Duration,
    pub duration: Option<Duration>,
    pub count: Option<u64>,
}

#[derive(Debug, Serialize)]
struct CsvRecord<'a> {
    schema_version: u8,
    sequence: u64,
    timestamp_utc: &'a str,
    monotonic_ms: f64,
    interval_ms: f64,
    sample_duration_us: u128,
    cpu_total_ratio: f64,
    cpu_user_ratio: f64,
    cpu_system_ratio: f64,
    cpu_nice_ratio: f64,
    cpu_idle_ratio: f64,
}

#[derive(Debug, Serialize)]
struct PerCoreCsvRecord<'a> {
    schema_version: u8,
    sequence: u64,
    timestamp_utc: &'a str,
    logical_cpu: u32,
    cluster: &'a str,
    cpu_total_ratio: f64,
    cpu_user_ratio: f64,
    cpu_system_ratio: f64,
    cpu_nice_ratio: f64,
    cpu_idle_ratio: f64,
}

pub fn record<W: Write>(
    writer: W,
    per_core_writer: Option<Box<dyn Write>>,
    options: RecordOptions,
) -> Result<()> {
    ensure!(
        !options.interval.is_zero(),
        "record interval must be greater than zero"
    );

    let (shutdown_tx, shutdown_rx) = mpsc::channel();
    ctrlc::set_handler(move || {
        let _ = shutdown_tx.send(());
    })
    .context("install Ctrl-C handler")?;

    let mut engine = CpuEngine::new(per_core_writer.is_some()).context("initialize CPU engine")?;
    let mut per_core_csv = per_core_writer.map(per_core_csv_writer).transpose()?;

    let start = Instant::now();
    let mut next_deadline = start + options.interval;
    let end = options.duration.map(|duration| start + duration);
    let mut samples_written = 0_u64;
    let mut csv = csv_writer(writer)?;

    loop {
        if options.count.is_some_and(|count| samples_written >= count) {
            break;
        }
        if end.is_some_and(|end| next_deadline > end) {
            break;
        }

        let wait = next_deadline.saturating_duration_since(Instant::now());
        match shutdown_rx.recv_timeout(wait) {
            Ok(()) | Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => {}
        }

        if let Some(snapshot) = engine.sample().context("sample CPU engine")? {
            samples_written += 1;
            let timestamp = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
            write_sample(&mut csv, &timestamp, snapshot)?;
            if let Some(csv) = &mut per_core_csv {
                write_per_core_samples(csv, snapshot.sequence, &timestamp, snapshot.per_core())?;
            }
        }

        next_deadline += options.interval;
    }

    csv.flush().context("flush CPU CSV output")?;
    if let Some(csv) = &mut per_core_csv {
        csv.flush().context("flush per-core CPU CSV output")?;
    }
    Ok(())
}

fn csv_writer<W: Write>(writer: W) -> Result<csv::Writer<W>> {
    let mut csv = csv::WriterBuilder::new()
        .has_headers(false)
        .from_writer(writer);
    csv.write_record(CSV_HEADER)
        .context("write CPU CSV header")?;
    Ok(csv)
}

fn per_core_csv_writer<W: Write>(writer: W) -> Result<csv::Writer<W>> {
    let mut csv = csv::WriterBuilder::new()
        .has_headers(false)
        .from_writer(writer);
    csv.write_record(PER_CORE_CSV_HEADER)
        .context("write per-core CPU CSV header")?;
    Ok(csv)
}

fn write_sample<W: Write>(
    csv: &mut csv::Writer<W>,
    timestamp_utc: &str,
    snapshot: &EngineSnapshot,
) -> Result<()> {
    let record = CsvRecord {
        schema_version: SCHEMA_VERSION,
        sequence: snapshot.sequence,
        timestamp_utc,
        monotonic_ms: snapshot.monotonic_ns as f64 / 1_000_000.0,
        interval_ms: snapshot.interval_ns as f64 / 1_000_000.0,
        sample_duration_us: u128::from(snapshot.sample_duration_ns) / 1_000,
        cpu_total_ratio: snapshot.aggregate.total_ratio,
        cpu_user_ratio: snapshot.aggregate.user_ratio,
        cpu_system_ratio: snapshot.aggregate.system_ratio,
        cpu_nice_ratio: snapshot.aggregate.nice_ratio,
        cpu_idle_ratio: snapshot.aggregate.idle_ratio,
    };

    csv.serialize(record).context("write CPU CSV row")
}

fn write_per_core_samples<W: Write>(
    csv: &mut csv::Writer<W>,
    sequence: u64,
    timestamp_utc: &str,
    samples: &[PerCoreSample],
) -> Result<()> {
    for sample in samples {
        let record = PerCoreCsvRecord {
            schema_version: SCHEMA_VERSION,
            sequence,
            timestamp_utc,
            logical_cpu: sample.logical_cpu,
            cluster: "",
            cpu_total_ratio: sample.usage.total_ratio,
            cpu_user_ratio: sample.usage.user_ratio,
            cpu_system_ratio: sample.usage.system_ratio,
            cpu_nice_ratio: sample.usage.nice_ratio,
            cpu_idle_ratio: sample.usage.idle_ratio,
        };
        csv.serialize(record)
            .context("write per-core CPU CSV row")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        EngineSnapshot, PerCoreSample, csv_writer, per_core_csv_writer, write_per_core_samples,
        write_sample,
    };
    use crate::cpu::CpuSample;
    use crate::engine::MAX_LOGICAL_CPUS;

    #[test]
    fn writes_versioned_csv_with_units_in_headers() {
        let mut output = Vec::new();
        let mut csv = csv_writer(&mut output).expect("write CSV header");
        let snapshot = EngineSnapshot {
            sequence: 1,
            monotonic_ns: 1_000_000_000,
            interval_ns: 1_000_000_000,
            sample_duration_ns: 42_000,
            aggregate: CpuSample {
                total_ratio: 0.7,
                user_ratio: 0.4,
                system_ratio: 0.2,
                nice_ratio: 0.1,
                idle_ratio: 0.3,
            },
            per_core_count: 0,
            per_core: [PerCoreSample::default(); MAX_LOGICAL_CPUS],
        };

        write_sample(&mut csv, "2026-07-14T20:00:00.000Z", &snapshot).expect("write sample");
        csv.flush().expect("flush CSV");
        drop(csv);

        let output = String::from_utf8(output).expect("UTF-8 CSV");
        let mut lines = output.lines();
        assert_eq!(
            lines.next(),
            Some(
                "schema_version,sequence,timestamp_utc,monotonic_ms,interval_ms,sample_duration_us,cpu_total_ratio,cpu_user_ratio,cpu_system_ratio,cpu_nice_ratio,cpu_idle_ratio"
            )
        );
        let row = lines.next().expect("data row");
        assert!(row.starts_with("1,1,"));
        assert!(row.contains(",1000.0,1000.0,42,0.7,0.4,0.2,0.1,0.3"));
        assert_eq!(lines.next(), None);
    }

    #[test]
    fn writes_long_form_per_core_csv_with_cpu_components() {
        let mut output = Vec::new();
        let mut csv = per_core_csv_writer(&mut output).expect("write per-core CSV header");
        let samples = [PerCoreSample {
            logical_cpu: 3,
            usage: CpuSample {
                total_ratio: 0.7,
                user_ratio: 0.4,
                system_ratio: 0.2,
                nice_ratio: 0.1,
                idle_ratio: 0.3,
            },
        }];

        write_per_core_samples(&mut csv, 7, "2026-07-14T20:00:00.000Z", &samples)
            .expect("write per-core samples");
        csv.flush().expect("flush per-core CSV");
        drop(csv);

        let output = String::from_utf8(output).expect("UTF-8 CSV");
        let mut lines = output.lines();
        assert_eq!(
            lines.next(),
            Some(
                "schema_version,sequence,timestamp_utc,logical_cpu,cluster,cpu_total_ratio,cpu_user_ratio,cpu_system_ratio,cpu_nice_ratio,cpu_idle_ratio"
            )
        );
        assert_eq!(
            lines.next(),
            Some("1,7,2026-07-14T20:00:00.000Z,3,,0.7,0.4,0.2,0.1,0.3")
        );
        assert_eq!(lines.next(), None);
    }
}
