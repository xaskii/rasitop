use std::time::{Duration, Instant};

use anyhow::{Context, Result, ensure};

use crate::cpu::{CpuSample, MachCpuProvider, MachPerCoreProvider, PerCoreSample};

pub const MAX_LOGICAL_CPUS: usize = 64;

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
    started_at: Instant,
    previous_sample_at: Instant,
    sequence: u64,
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
            started_at,
            previous_sample_at: started_at,
            sequence: 0,
            snapshot: EngineSnapshot::default(),
        })
    }

    pub fn sample(&mut self) -> Result<Option<&EngineSnapshot>> {
        let sample_started_at = Instant::now();
        let interval = sample_started_at.duration_since(self.previous_sample_at);
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
        let sample_duration = sample_started_at.elapsed();
        self.previous_sample_at = sample_started_at;

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

        self.sequence += 1;
        self.snapshot.sequence = self.sequence;
        self.snapshot.monotonic_ns = duration_ns(sample_started_at.duration_since(self.started_at));
        self.snapshot.interval_ns = duration_ns(interval);
        self.snapshot.sample_duration_ns = duration_ns(sample_duration);
        self.snapshot.aggregate = aggregate;
        self.snapshot.per_core_count = per_core_count;
        Ok(Some(&self.snapshot))
    }
}

fn duration_ns(duration: Duration) -> u64 {
    u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::duration_ns;
    use std::time::Duration;

    #[test]
    fn numeric_duration_saturates_instead_of_wrapping() {
        assert_eq!(duration_ns(Duration::from_nanos(42)), 42);
        assert_eq!(duration_ns(Duration::MAX), u64::MAX);
    }
}
