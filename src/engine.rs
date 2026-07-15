use std::time::{Duration, Instant};

use thiserror::Error;

use crate::cpu::{CpuError, CpuSample, MachCpuProvider, MachPerCoreProvider, PerCoreSample};
use crate::smc::{SensorSample, SmcError, SmcProvider};

pub const MAX_LOGICAL_CPUS: usize = 64;

type Result<T> = std::result::Result<T, EngineError>;

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("failed to establish aggregate CPU counter baseline")]
    AggregateBaseline(#[source] CpuError),

    #[error("failed to establish per-core CPU counter baseline")]
    PerCoreBaseline(#[source] CpuError),

    #[error("failed to sample aggregate CPU utilization")]
    AggregateSample(#[source] CpuError),

    #[error("failed to sample per-core CPU utilization")]
    PerCoreSample(#[source] CpuError),
}

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
    pub sensors: SensorSample,
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
            sensors: SensorSample::default(),
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
    sensors: Option<SmcProvider>,
    sensor_fallback: SensorSample,
    sensor_error: Option<SmcError>,
    emissions: EmissionTimeline,
    snapshot: EngineSnapshot,
}

impl CpuEngine {
    pub fn new(sample_per_core: bool) -> Result<Self> {
        let mut aggregate = MachCpuProvider::new();
        aggregate.sample().map_err(EngineError::AggregateBaseline)?;

        let per_core = if sample_per_core {
            let mut provider = MachPerCoreProvider::new();
            provider.sample().map_err(EngineError::PerCoreBaseline)?;
            Some(provider)
        } else {
            None
        };

        let (sensors, sensor_fallback, sensor_error) = match SmcProvider::new() {
            Ok(provider) => (Some(provider), SensorSample::default(), None),
            Err(error) => {
                let fallback = SensorSample::unavailable(&error);
                (None, fallback, Some(error))
            }
        };

        let started_at = Instant::now();
        Ok(Self {
            aggregate,
            per_core,
            // Sensor availability must not prevent CPU-only operation. The
            // fallback keeps a stable failure category in every snapshot.
            sensors,
            sensor_fallback,
            sensor_error,
            emissions: EmissionTimeline::new(started_at),
            snapshot: EngineSnapshot::default(),
        })
    }

    /// Returns the initialization failure retained for diagnostics when the
    /// engine is operating in CPU-only mode.
    pub fn sensor_error(&self) -> Option<&SmcError> {
        self.sensor_error.as_ref()
    }

    pub fn sample(&mut self) -> Result<Option<&EngineSnapshot>> {
        let sample_started_at = Instant::now();
        let aggregate = self
            .aggregate
            .sample()
            .map_err(EngineError::AggregateSample)?;
        let per_core_available = match &mut self.per_core {
            Some(provider) => provider.sample().map_err(EngineError::PerCoreSample)?,
            None => false,
        };

        let Some(aggregate) = aggregate else {
            return Ok(None);
        };
        let sensors = self
            .sensors
            .as_mut()
            .map_or(self.sensor_fallback, SmcProvider::sample);

        let per_core_count = if per_core_available {
            let samples = self
                .per_core
                .as_ref()
                .map_or(&[][..], MachPerCoreProvider::samples);
            assert!(
                samples.len() <= MAX_LOGICAL_CPUS,
                "logical CPU count exceeds engine capacity"
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
        self.snapshot.sensors = sensors;
        self.snapshot.sample_duration_ns = duration_ns(sample_started_at.elapsed());
        Ok(Some(&self.snapshot))
    }
}

fn duration_ns(duration: Duration) -> u64 {
    u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use std::error::Error as _;
    use std::time::{Duration, Instant};

    use super::{EmissionTimeline, EngineError, duration_ns};
    use crate::cpu::CpuError;

    #[test]
    fn engine_errors_keep_the_cpu_error_as_their_source() {
        let error = EngineError::AggregateSample(CpuError::HostStatistics { status: 5 });
        assert_eq!(
            error.to_string(),
            "failed to sample aggregate CPU utilization"
        );
        assert_eq!(
            error.source().map(ToString::to_string).as_deref(),
            Some("host_statistics(HOST_CPU_LOAD_INFO) failed with kern_return_t 5")
        );
    }

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
