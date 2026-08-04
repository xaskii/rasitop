use std::io::Write;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, ensure};
use chrono::{SecondsFormat, Utc};
use serde::Serialize;

use crate::cpu::PerCoreSample;
use crate::engine::{CpuEngine, EngineSnapshot, SampleRequest};

const SCHEMA_VERSION: u8 = 2;
const GPU_INTERVAL: Duration = Duration::from_secs(2);
const CSV_HEADER: [&str; 20] = [
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
    "cpu_temp_max_c",
    "cpu_temp_avg_c",
    "fan_rpm",
    "system_power_w",
    "gpu_busy_ratio",
    "capability_flags",
    "error_flags",
    "gpu_capability_flags",
    "gpu_error_flags",
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
    cpu_temp_max_c: Option<f64>,
    cpu_temp_avg_c: Option<f64>,
    fan_rpm: Option<f64>,
    system_power_w: Option<f64>,
    gpu_busy_ratio: Option<f64>,
    capability_flags: u64,
    error_flags: u64,
    gpu_capability_flags: u64,
    gpu_error_flags: u64,
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

    let sample_per_core = per_core_writer.is_some();
    let base_request = if sample_per_core {
        SampleRequest::PER_CORE | SampleRequest::SENSORS
    } else {
        SampleRequest::SENSORS
    };
    let mut engine = CpuEngine::new(sample_per_core).context("initialize CPU engine")?;
    if let Some(error) = engine.sensor_error() {
        eprintln!(
            "rasitop: SMC unavailable (error_flags={:#018x}): {error}",
            crate::smc::ERROR_SMC_INITIALIZATION | error.category_flag()
        );
    }
    let mut per_core_csv = per_core_writer.map(per_core_csv_writer).transpose()?;

    let start = Instant::now();
    let mut next_deadline = start + options.interval;
    let mut next_gpu_deadline = start + GPU_INTERVAL;
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

        let gpu_requested = gpu_sample_due(next_deadline, &mut next_gpu_deadline);
        let request = if gpu_requested {
            base_request | SampleRequest::GPU
        } else {
            base_request
        };
        if let Some(snapshot) = engine.sample(request).context("sample CPU engine")? {
            samples_written += 1;
            let timestamp = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
            write_sample(&mut csv, &timestamp, snapshot, gpu_requested)?;
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

fn gpu_sample_due(sample_deadline: Instant, next_gpu_deadline: &mut Instant) -> bool {
    if sample_deadline < *next_gpu_deadline {
        return false;
    }
    while *next_gpu_deadline <= sample_deadline {
        *next_gpu_deadline += GPU_INTERVAL;
    }
    true
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
    gpu_requested: bool,
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
        cpu_temp_max_c: snapshot.sensors.cpu_temp_max_c(),
        cpu_temp_avg_c: snapshot.sensors.cpu_temp_avg_c(),
        fan_rpm: snapshot.sensors.fan_rpm(),
        system_power_w: snapshot.sensors.system_power_w(),
        gpu_busy_ratio: (gpu_requested && snapshot.gpu.busy_ratio.is_finite())
            .then_some(snapshot.gpu.busy_ratio),
        capability_flags: snapshot.sensors.capability_flags,
        error_flags: snapshot.sensors.error_flags,
        gpu_capability_flags: snapshot.gpu.capability_flags,
        gpu_error_flags: snapshot.gpu.error_flags,
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
    use std::time::{Duration, Instant};

    use super::{
        EngineSnapshot, PerCoreSample, csv_writer, gpu_sample_due, per_core_csv_writer,
        write_per_core_samples, write_sample,
    };
    use crate::cpu::CpuSample;
    use crate::engine::MAX_LOGICAL_CPUS;
    use crate::gpu::{CAPABILITY_GPU_UTILIZATION, GpuReading};
    use crate::smc::{
        CAPABILITY_CPU_TEMPERATURE, CAPABILITY_FAN_SPEED, CAPABILITY_SYSTEM_POWER, SensorSample,
    };

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
            sensors: SensorSample {
                cpu_temp_max_c: 67.5,
                cpu_temp_avg_c: 61.25,
                fan_rpm: 2_300.0,
                system_power_w: 12.5,
                capability_flags: CAPABILITY_CPU_TEMPERATURE
                    | CAPABILITY_FAN_SPEED
                    | CAPABILITY_SYSTEM_POWER,
                error_flags: 0,
            },
            gpu: GpuReading {
                busy_ratio: 0.625,
                capability_flags: CAPABILITY_GPU_UTILIZATION,
                error_flags: 0,
            },
        };

        write_sample(&mut csv, "2026-07-14T20:00:00.000Z", &snapshot, true).expect("write sample");
        csv.flush().expect("flush CSV");
        drop(csv);

        let output = String::from_utf8(output).expect("UTF-8 CSV");
        let mut lines = output.lines();
        assert_eq!(
            lines.next(),
            Some(
                "schema_version,sequence,timestamp_utc,monotonic_ms,interval_ms,sample_duration_us,cpu_total_ratio,cpu_user_ratio,cpu_system_ratio,cpu_nice_ratio,cpu_idle_ratio,cpu_temp_max_c,cpu_temp_avg_c,fan_rpm,system_power_w,gpu_busy_ratio,capability_flags,error_flags,gpu_capability_flags,gpu_error_flags"
            )
        );
        let row = lines.next().expect("data row");
        assert!(row.starts_with("2,1,"));
        assert!(row.contains(
            ",1000.0,1000.0,42,0.7,0.4,0.2,0.1,0.3,67.5,61.25,2300.0,12.5,0.625,7,0,1,0"
        ));
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
            Some("2,7,2026-07-14T20:00:00.000Z,3,,0.7,0.4,0.2,0.1,0.3")
        );
        assert_eq!(lines.next(), None);
    }

    #[test]
    fn gpu_cadence_uses_existing_deadlines_without_catch_up_bursts() {
        let start = Instant::now();
        let mut next_gpu = start + Duration::from_secs(2);
        assert!(!gpu_sample_due(
            start + Duration::from_secs(1),
            &mut next_gpu
        ));
        assert!(gpu_sample_due(
            start + Duration::from_secs(2),
            &mut next_gpu
        ));
        assert!(!gpu_sample_due(
            start + Duration::from_secs(3),
            &mut next_gpu
        ));
        assert!(gpu_sample_due(
            start + Duration::from_secs(9),
            &mut next_gpu
        ));
        assert_eq!(next_gpu, start + Duration::from_secs(10));
    }
}
