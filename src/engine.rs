use std::time::{Duration, Instant};

use anyhow::{Context, Result, ensure};

use crate::cpu::{CpuSample, MachCpuProvider, MachPerCoreProvider, PerCoreSample};

pub const MAX_LOGICAL_CPUS: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct EmissionTiming {
    sequence: u64,
    monotonic: Duration,
    interval: Duration,
}

#[derive(Debug)]
struct EmissionTimeline {
    started_at: Instant,
    previous_emitted_at: Instant,
    sequence: u64,
}

impl EmissionTimeline {
    fn new(started_at: Instant) -> Self {
        Self {
            started_at,
            previous_emitted_at: started_at,
            sequence: 0,
        }
    }

    fn record_emission(&mut self, emitted_at: Instant) -> EmissionTiming {
        self.sequence += 1;
        let timing = EmissionTiming {
            sequence: self.sequence,
            monotonic: emitted_at.duration_since(self.started_at),
            interval: emitted_at.duration_since(self.previous_emitted_at),
        };
        self.previous_emitted_at = emitted_at;
        timing
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct EngineSnapshot {
    pub sequence: u64,
    pub monotonic_ns: u64,
    pub interval_ns: u64,
    pub sample_duration_ns: u64,
    pub aggregate: CpuSample,
    pub per_core_count: u32,
    pub per_core: [PerCoreSample; MAX_LOGICAL_CPUS],
}

impl Default for EngineSnapshot {
    fn default() -> Self {
        Self {
            sequence: 0,
            monotonic_ns: 0,
            interval_ns: 0,
            sample_duration_ns: 0,
            aggregate: CpuSample::default(),
            per_core_count: 0,
            per_core: [PerCoreSample::default(); MAX_LOGICAL_CPUS],
        }
    }
}

impl EngineSnapshot {
    pub fn per_core(&self) -> &[PerCoreSample] {
        let count = (self.per_core_count as usize).min(MAX_LOGICAL_CPUS);
        &self.per_core[..count]
    }
}

#[derive(Debug)]
pub struct CpuEngine {
    aggregate: MachCpuProvider,
    per_core: Option<MachPerCoreProvider>,
    emissions: EmissionTimeline,
    snapshot: EngineSnapshot,
}

impl CpuEngine {
    pub fn new(sample_per_core: bool) -> Result<Self> {
        let mut aggregate = MachCpuProvider::new();
        aggregate
            .sample()
            .context("establish aggregate CPU counter baseline")?;

        let per_core = if sample_per_core {
            let mut provider = MachPerCoreProvider::new();
            provider
                .sample()
                .context("establish per-core CPU counter baseline")?;
            Some(provider)
        } else {
            None
        };

        let started_at = Instant::now();
        Ok(Self {
            aggregate,
            per_core,
            emissions: EmissionTimeline::new(started_at),
            snapshot: EngineSnapshot::default(),
        })
    }

    pub fn sample(&mut self) -> Result<Option<&EngineSnapshot>> {
        let sample_started_at = Instant::now();
        let aggregate = self
            .aggregate
            .sample()
            .context("sample aggregate CPU utilization")?;
        let per_core_available = match &mut self.per_core {
            Some(provider) => provider
                .sample()
                .context("sample per-core CPU utilization")?,
            None => false,
        };

        let Some(aggregate) = aggregate else {
            return Ok(None);
        };

        let per_core_count = if per_core_available {
            let samples = self
                .per_core
                .as_ref()
                .map_or(&[][..], MachPerCoreProvider::samples);
            ensure!(
                samples.len() <= MAX_LOGICAL_CPUS,
                "{} logical CPUs exceeds engine capacity {MAX_LOGICAL_CPUS}",
                samples.len()
            );
            self.snapshot.per_core[..samples.len()].copy_from_slice(samples);
            samples.len() as u32
        } else {
            0
        };

        let timing = self.emissions.record_emission(sample_started_at);
        self.snapshot.sequence = timing.sequence;
        self.snapshot.monotonic_ns = duration_ns(timing.monotonic);
        self.snapshot.interval_ns = duration_ns(timing.interval);
        self.snapshot.aggregate = aggregate;
        self.snapshot.per_core_count = per_core_count;
        self.snapshot.sample_duration_ns = duration_ns(sample_started_at.elapsed());
        Ok(Some(&self.snapshot))
    }
}

fn duration_ns(duration: Duration) -> u64 {
    u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::{EmissionTimeline, duration_ns};
    use std::time::{Duration, Instant};

    #[test]
    fn intervals_span_only_emitted_snapshots() {
        let started_at = Instant::now();
        let mut timeline = EmissionTimeline::new(started_at);

        let first = timeline.record_emission(started_at + Duration::from_millis(100));
        assert_eq!(first.sequence, 1);
        assert_eq!(first.monotonic, Duration::from_millis(100));
        assert_eq!(first.interval, Duration::from_millis(100));

        // A poll at 200 ms that produces no snapshot never reaches
        // `record_emission`, so it cannot shorten the following interval.
        let second = timeline.record_emission(started_at + Duration::from_millis(1_100));
        assert_eq!(second.sequence, 2);
        assert_eq!(second.monotonic, Duration::from_millis(1_100));
        assert_eq!(second.interval, Duration::from_secs(1));
    }

    #[test]
    fn numeric_duration_saturates_instead_of_wrapping() {
        assert_eq!(duration_ns(Duration::from_nanos(42)), 42);
        assert_eq!(duration_ns(Duration::MAX), u64::MAX);
    }
}
