use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, ensure};
use clap::ValueEnum;
use serde::Serialize;

use crate::engine::CpuEngine;

const SCHEMA_VERSION: u8 = 1;

#[derive(Clone, Copy, Debug, Serialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum MeasureMode {
    Aggregate,
    PerCore,
}

impl MeasureMode {
    fn samples_per_core(self) -> bool {
        matches!(self, Self::PerCore)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct MeasureOptions {
    pub mode: MeasureMode,
    pub interval: Duration,
    pub duration: Duration,
}

#[derive(Debug, Serialize)]
pub struct MeasurementSummary {
    schema_version: u8,
    mode: MeasureMode,
    requested_interval_ns: u64,
    requested_duration_ns: u64,
    elapsed_ns: u64,
    attempts: u64,
    samples_ready: u64,
    missed_deadlines: u64,
    attempt_duration_ns: LatencySummary,
    snapshot_duration_ns: Option<LatencySummary>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
struct LatencySummary {
    min: u64,
    mean: u64,
    p50: u64,
    p95: u64,
    p99: u64,
    max: u64,
}

pub fn measure(options: MeasureOptions) -> Result<MeasurementSummary> {
    ensure!(
        !options.interval.is_zero(),
        "measure interval must be greater than zero"
    );
    ensure!(
        options.duration >= options.interval,
        "measure duration must be at least one interval"
    );

    let (shutdown_tx, shutdown_rx) = mpsc::channel();
    ctrlc::set_handler(move || {
        let _ = shutdown_tx.send(());
    })
    .context("install Ctrl-C handler")?;

    let mut engine = CpuEngine::new(options.mode.samples_per_core())
        .context("initialize CPU measurement engine")?;
    let started_at = Instant::now();
    let end = started_at
        .checked_add(options.duration)
        .context("measure duration exceeds monotonic clock range")?;
    let mut next_deadline = started_at + options.interval;
    let expected_attempts =
        usize::try_from(options.duration.as_nanos() / options.interval.as_nanos())
            .unwrap_or(usize::MAX)
            .min(1_000_000);
    let mut attempt_durations = Vec::with_capacity(expected_attempts);
    let mut snapshot_durations = Vec::with_capacity(expected_attempts);
    let mut samples_ready = 0_u64;
    let mut missed_deadlines = 0_u64;

    while next_deadline <= end {
        let wait = next_deadline.saturating_duration_since(Instant::now());
        match shutdown_rx.recv_timeout(wait) {
            Ok(()) | Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => {}
        }

        let attempt_started_at = Instant::now();
        let snapshot_duration = engine
            .sample()
            .context("measure CPU engine sample")?
            .map(|snapshot| snapshot.sample_duration_ns);
        let attempt_duration = duration_ns(attempt_started_at.elapsed());

        attempt_durations.push(attempt_duration);
        if let Some(snapshot_duration) = snapshot_duration {
            samples_ready += 1;
            snapshot_durations.push(snapshot_duration);
        }

        next_deadline += options.interval;
        let now = Instant::now();
        while next_deadline < now && next_deadline <= end {
            missed_deadlines += 1;
            next_deadline += options.interval;
        }
    }

    ensure!(
        !attempt_durations.is_empty(),
        "measurement stopped before its first attempt"
    );
    let elapsed = started_at.elapsed();
    let attempts = attempt_durations.len() as u64;

    Ok(MeasurementSummary {
        schema_version: SCHEMA_VERSION,
        mode: options.mode,
        requested_interval_ns: duration_ns(options.interval),
        requested_duration_ns: duration_ns(options.duration),
        elapsed_ns: duration_ns(elapsed),
        attempts,
        samples_ready,
        missed_deadlines,
        attempt_duration_ns: summarize(&mut attempt_durations)
            .expect("attempt durations are known to be non-empty"),
        snapshot_duration_ns: summarize(&mut snapshot_durations),
    })
}

fn duration_ns(duration: Duration) -> u64 {
    u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX)
}

fn summarize(values: &mut [u64]) -> Option<LatencySummary> {
    if values.is_empty() {
        return None;
    }
    values.sort_unstable();
    let sum = values.iter().map(|&value| u128::from(value)).sum::<u128>();
    Some(LatencySummary {
        min: values[0],
        mean: u64::try_from(sum / values.len() as u128).unwrap_or(u64::MAX),
        p50: percentile(values, 50),
        p95: percentile(values, 95),
        p99: percentile(values, 99),
        max: values[values.len() - 1],
    })
}

fn percentile(sorted: &[u64], percentile: usize) -> u64 {
    debug_assert!(!sorted.is_empty());
    debug_assert!((1..=100).contains(&percentile));
    let rank = (percentile * sorted.len()).div_ceil(100);
    sorted[rank.saturating_sub(1)]
}

#[cfg(test)]
mod tests {
    use super::{LatencySummary, summarize};

    #[test]
    fn summarizes_latency_distribution_with_nearest_rank_percentiles() {
        let mut values = (1..=100).collect::<Vec<_>>();
        assert_eq!(
            summarize(&mut values),
            Some(LatencySummary {
                min: 1,
                mean: 50,
                p50: 50,
                p95: 95,
                p99: 99,
                max: 100,
            })
        );
    }

    #[test]
    fn empty_latency_distribution_has_no_summary() {
        assert_eq!(summarize(&mut []), None);
    }
}
