use std::fs::File;
use std::io::{self, BufReader, BufWriter, Write};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;
#[cfg(feature = "gpu-profiling")]
use std::time::Instant;

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};
use serde::Serialize;

use rasitop::measure::{self, MeasureMode, MeasureOptions};
use rasitop::record::{self, RecordOptions};
use rasitop::smc;
use rasitop::{gpu, ioreport};

#[derive(Debug, Parser)]
#[command(version, about = "Low-overhead Apple Silicon performance recorder")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Measure the CPU engine without timestamp or CSV overhead.
    Measure(MeasureArgs),

    /// Record aggregate CPU utilization as CSV.
    Record(RecordArgs),

    /// Enumerate and decode every AppleSMC key for diagnostics.
    SmcScan(SmcScanArgs),

    /// Run accelerator diagnostics that never execute during normal sampling.
    Gpu(GpuArgs),
}

#[derive(Debug, Args)]
struct GpuArgs {
    #[command(subcommand)]
    command: GpuCommand,
}

#[derive(Debug, Subcommand)]
enum GpuCommand {
    /// Inventory every IOReport channel and its declared states as CSV.
    Discover(GpuDiscoverArgs),

    /// Record raw state residencies for one exact IOReport channel.
    Residency(GpuResidencyArgs),

    /// Decode the exact validated M4 Pro layout as whole-device busy ratio.
    Validate(GpuValidateArgs),

    /// Decode an existing raw residency capture using the exact local catalog.
    Decode(GpuDecodeArgs),

    /// Exercise the narrow provider lifecycle without engine integration.
    Provider(GpuProviderArgs),

    /// Measure the real provider's construction and sample hot path as JSON.
    #[cfg(feature = "gpu-profiling")]
    Measure(GpuMeasureArgs),
}

#[derive(Debug, Args)]
struct GpuDiscoverArgs {
    /// Write CSV diagnostics to this path instead of stdout.
    #[arg(long, value_name = "PATH")]
    output: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct GpuResidencyArgs {
    /// Exact IOReport group name.
    #[arg(long)]
    group: String,

    /// Exact IOReport subgroup name.
    #[arg(long)]
    subgroup: String,

    /// Exact IOReport channel name.
    #[arg(long)]
    channel: String,

    /// Time between raw samples.
    #[arg(long, default_value = "1s", value_parser = humantime::parse_duration)]
    interval: Duration,

    /// Number of sample deltas to write.
    #[arg(long, default_value_t = 10)]
    count: u64,

    /// Write CSV diagnostics to this path instead of stdout.
    #[arg(long, value_name = "PATH")]
    output: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct GpuValidateArgs {
    /// Time between samples.
    #[arg(long, default_value = "1s", value_parser = humantime::parse_duration)]
    interval: Duration,

    /// Number of sample deltas to write.
    #[arg(long, default_value_t = 10)]
    count: u64,

    /// Write decoded CSV diagnostics to this path instead of stdout.
    #[arg(long, value_name = "PATH")]
    output: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct GpuDecodeArgs {
    /// Raw residency CSV produced by `gpu residency`.
    #[arg(long, value_name = "PATH")]
    input: PathBuf,

    /// Write decoded CSV diagnostics to this path instead of stdout.
    #[arg(long, value_name = "PATH")]
    output: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct GpuProviderArgs {
    /// Time between provider samples after the initial baseline.
    #[arg(long, default_value = "1s", value_parser = humantime::parse_duration)]
    interval: Duration,

    /// Number of provider calls, including the initial gap.
    #[arg(long, default_value_t = 4)]
    count: u64,

    /// Write lifecycle CSV diagnostics to this path instead of stdout.
    #[arg(long, value_name = "PATH")]
    output: Option<PathBuf>,
}

#[cfg(feature = "gpu-profiling")]
#[derive(Debug, Args)]
struct GpuMeasureArgs {
    /// Time between provider samples after the initial baseline.
    #[arg(long, default_value = "100ms", value_parser = humantime::parse_duration)]
    interval: Duration,

    /// Number of recurring samples to measure after the baseline.
    #[arg(long, default_value_t = 100)]
    count: u64,

    /// Write the JSON summary to this path instead of stdout.
    #[arg(long, value_name = "PATH")]
    output: Option<PathBuf>,
}

#[cfg(feature = "gpu-profiling")]
#[derive(Debug, Serialize)]
struct GpuMeasureSummary {
    schema_version: u32,
    machine_model: &'static str,
    os_build: &'static str,
    interval_ms: u64,
    recurring_samples: u64,
    construction_us: u64,
    construction_allocations: usize,
    baseline_us: u64,
    baseline_allocations: usize,
    warm_sample_us: Percentiles,
    warm_sample_allocations_max: usize,
    regression_gate: GpuRegressionGate,
}

#[cfg(feature = "gpu-profiling")]
#[derive(Debug, Serialize)]
struct Percentiles {
    p50: u64,
    p95: u64,
    p99: u64,
    max: u64,
}

#[cfg(feature = "gpu-profiling")]
#[derive(Debug, Serialize)]
struct GpuRegressionGate {
    p95_us_max: u64,
    single_sample_us_max: u64,
    allocations_max: usize,
    passed: bool,
}

#[derive(Debug, Serialize)]
struct GpuProviderRow {
    sequence: u64,
    status: &'static str,
    interval_ms: Option<u64>,
    gpu_busy_ratio: Option<f64>,
}

#[derive(Debug, Args)]
struct SmcScanArgs {
    /// Keep only four-character keys beginning with this prefix. Repeatable.
    #[arg(long = "prefix")]
    prefixes: Vec<String>,

    /// Write JSON diagnostics to this path instead of stdout.
    #[arg(long, value_name = "PATH")]
    output: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct MeasureArgs {
    /// Engine path to measure.
    #[arg(long, value_enum, default_value_t = MeasureMode::PerCore)]
    mode: MeasureMode,

    /// Sampling interval, such as 1ms, 250ms, or 1s.
    #[arg(long, default_value = "1s", value_parser = humantime::parse_duration)]
    interval: Duration,

    /// Total measurement duration, such as 10s or 1m.
    #[arg(long, default_value = "10s", value_parser = humantime::parse_duration)]
    duration: Duration,

    /// Write the JSON summary to this path instead of stdout.
    #[arg(long, value_name = "PATH")]
    output: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct RecordArgs {
    /// Sampling interval, such as 250ms, 1s, or 2s.
    #[arg(long, default_value = "1s", value_parser = humantime::parse_duration)]
    interval: Duration,

    /// Stop after this duration, such as 30s or 10m.
    #[arg(long, value_parser = humantime::parse_duration, conflicts_with = "count")]
    duration: Option<Duration>,

    /// Stop after writing this many samples.
    #[arg(long, conflicts_with = "duration")]
    count: Option<u64>,

    /// Also write long-form per-logical-CPU samples to this CSV file.
    #[arg(long, value_name = "PATH")]
    per_core_csv: Option<PathBuf>,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("rasitop: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    anyhow::ensure!(
        cfg!(target_os = "macos"),
        "rasitop CPU recording requires macOS"
    );

    match cli.command {
        Command::Measure(args) => {
            let summary = measure::measure(MeasureOptions {
                mode: args.mode,
                interval: args.interval,
                duration: args.duration,
            })
            .context("measure CPU engine")?;
            let writer: Box<dyn Write> = match args.output {
                Some(path) => {
                    let file = File::create(&path)
                        .with_context(|| format!("create summary at {}", path.display()))?;
                    Box::new(file)
                }
                None => Box::new(io::stdout()),
            };
            let mut writer = BufWriter::new(writer);
            serde_json::to_writer(&mut writer, &summary).context("write measurement summary")?;
            writer
                .write_all(b"\n")
                .context("finish measurement summary")
        }
        Command::Record(args) => {
            let stdout = io::stdout();
            let writer = BufWriter::new(stdout.lock());
            let per_core_writer = args
                .per_core_csv
                .map(|path| -> Result<Box<dyn Write>> {
                    let file = File::create(&path)
                        .with_context(|| format!("create per-core CSV at {}", path.display()))?;
                    Ok(Box::new(BufWriter::new(file)))
                })
                .transpose()?;
            record::record(
                writer,
                per_core_writer,
                RecordOptions {
                    interval: args.interval,
                    duration: args.duration,
                    count: args.count,
                },
            )
            .context("record CPU samples")
        }
        Command::SmcScan(args) => {
            let mut report = smc::discover().context("enumerate AppleSMC keys")?;
            if !args.prefixes.is_empty() {
                report.keys.retain(|record| {
                    args.prefixes
                        .iter()
                        .any(|prefix| record.key.starts_with(prefix))
                });
            }
            let writer: Box<dyn Write> = match args.output {
                Some(path) => {
                    let file = File::create(&path)
                        .with_context(|| format!("create SMC scan at {}", path.display()))?;
                    Box::new(file)
                }
                None => Box::new(io::stdout()),
            };
            let mut writer = BufWriter::new(writer);
            serde_json::to_writer_pretty(&mut writer, &report).context("write SMC diagnostics")?;
            writer.write_all(b"\n").context("finish SMC diagnostics")
        }
        Command::Gpu(GpuArgs {
            command: GpuCommand::Discover(args),
        }) => {
            let inventory = ioreport::discover().context("inventory IOReport channels")?;
            let writer: Box<dyn Write> = match args.output {
                Some(path) => {
                    let file = File::create(&path)
                        .with_context(|| format!("create GPU inventory at {}", path.display()))?;
                    Box::new(file)
                }
                None => Box::new(io::stdout()),
            };
            ioreport::write_csv(BufWriter::new(writer), &inventory)
                .context("write GPU channel inventory")
        }
        Command::Gpu(GpuArgs {
            command: GpuCommand::Residency(args),
        }) => {
            anyhow::ensure!(
                !args.interval.is_zero(),
                "GPU residency interval must be non-zero"
            );
            anyhow::ensure!(args.count != 0, "GPU residency count must be non-zero");
            let writer: Box<dyn Write> = match args.output {
                Some(path) => {
                    let file = File::create(&path).with_context(|| {
                        format!("create GPU residency capture at {}", path.display())
                    })?;
                    Box::new(file)
                }
                None => Box::new(io::stdout()),
            };
            ioreport::record_residencies(
                BufWriter::new(writer),
                &ioreport::ResidencySelector {
                    group: args.group,
                    subgroup: args.subgroup,
                    channel: args.channel,
                },
                args.interval,
                args.count,
            )
            .context("record GPU state residencies")
        }
        Command::Gpu(GpuArgs {
            command: GpuCommand::Validate(args),
        }) => {
            anyhow::ensure!(
                !args.interval.is_zero(),
                "GPU validation interval must be non-zero"
            );
            anyhow::ensure!(args.count != 0, "GPU validation count must be non-zero");
            let samples = gpu::capture_validated(args.interval, args.count)
                .context("capture validated GPU residency")?;
            let writer: Box<dyn Write> = match args.output {
                Some(path) => {
                    let file = File::create(&path)
                        .with_context(|| format!("create GPU validation at {}", path.display()))?;
                    Box::new(file)
                }
                None => Box::new(io::stdout()),
            };
            gpu::write_csv(BufWriter::new(writer), &samples)
                .context("write validated GPU residency")
        }
        Command::Gpu(GpuArgs {
            command: GpuCommand::Decode(args),
        }) => {
            let input = File::open(&args.input)
                .with_context(|| format!("open raw GPU capture at {}", args.input.display()))?;
            let samples =
                gpu::decode_csv(BufReader::new(input)).context("decode validated GPU residency")?;
            let writer: Box<dyn Write> = match args.output {
                Some(path) => {
                    let file = File::create(&path)
                        .with_context(|| format!("create GPU validation at {}", path.display()))?;
                    Box::new(file)
                }
                None => Box::new(io::stdout()),
            };
            gpu::write_csv(BufWriter::new(writer), &samples)
                .context("write validated GPU residency")
        }
        Command::Gpu(GpuArgs {
            command: GpuCommand::Provider(args),
        }) => {
            anyhow::ensure!(
                !args.interval.is_zero(),
                "GPU provider interval must be non-zero"
            );
            anyhow::ensure!(args.count != 0, "GPU provider count must be non-zero");
            let discovery = gpu::GpuProvider::discover().context("discover GPU provider")?;
            let mut provider =
                gpu::GpuProvider::new(discovery.catalog).context("initialize GPU provider")?;
            let writer: Box<dyn Write> = match args.output {
                Some(path) => {
                    let file = File::create(&path).with_context(|| {
                        format!("create GPU provider diagnostics at {}", path.display())
                    })?;
                    Box::new(file)
                }
                None => Box::new(io::stdout()),
            };
            let mut csv = csv::Writer::from_writer(BufWriter::new(writer));
            for sequence in 0..args.count {
                if sequence != 0 {
                    std::thread::sleep(args.interval);
                }
                let sample = provider.sample().context("sample GPU provider")?;
                csv.serialize(GpuProviderRow {
                    sequence,
                    status: if sample.is_some() { "sample" } else { "gap" },
                    interval_ms: sample
                        .map(|sample| sample.interval.as_millis().try_into().unwrap_or(u64::MAX)),
                    gpu_busy_ratio: sample.map(|sample| sample.busy_ratio),
                })?;
            }
            csv.flush().context("finish GPU provider diagnostics")
        }
        #[cfg(feature = "gpu-profiling")]
        Command::Gpu(GpuArgs {
            command: GpuCommand::Measure(args),
        }) => measure_gpu_provider(args),
    }
}

#[cfg(feature = "gpu-profiling")]
fn measure_gpu_provider(args: GpuMeasureArgs) -> Result<()> {
    anyhow::ensure!(
        !args.interval.is_zero(),
        "GPU measure interval must be non-zero"
    );
    anyhow::ensure!(args.count != 0, "GPU measure count must be non-zero");
    let discovery = gpu::GpuProvider::discover().context("discover GPU provider")?;

    let construction_start = Instant::now();
    let (provider, construction_allocations) =
        rasitop::test_allocator::count_allocations(|| gpu::GpuProvider::new(discovery.catalog));
    let construction_us = elapsed_us(construction_start.elapsed());
    let mut provider = provider.context("initialize GPU provider")?;

    let baseline_start = Instant::now();
    let (baseline, baseline_allocations) =
        rasitop::test_allocator::count_allocations(|| provider.sample());
    let baseline_us = elapsed_us(baseline_start.elapsed());
    anyhow::ensure!(
        baseline.context("establish GPU baseline")?.is_none(),
        "first GPU sample must be a gap"
    );

    let mut latencies = Vec::with_capacity(args.count as usize);
    let mut allocations_max = 0;
    for _ in 0..args.count {
        std::thread::sleep(args.interval);
        let start = Instant::now();
        let (sample, allocations) =
            rasitop::test_allocator::count_allocations(|| provider.sample());
        latencies.push(elapsed_us(start.elapsed()));
        allocations_max = allocations_max.max(allocations);
        anyhow::ensure!(
            sample.context("measure GPU sample")?.is_some(),
            "warm GPU sample unexpectedly produced a gap"
        );
    }
    latencies.sort_unstable();
    let warm_sample_us = Percentiles {
        p50: percentile(&latencies, 50),
        p95: percentile(&latencies, 95),
        p99: percentile(&latencies, 99),
        max: *latencies.last().expect("count validated nonzero"),
    };
    const P95_US_MAX: u64 = 2_500;
    const SINGLE_SAMPLE_US_MAX: u64 = 5_000;
    const ALLOCATIONS_MAX: usize = 0;
    let passed = warm_sample_us.p95 <= P95_US_MAX
        && warm_sample_us.max <= SINGLE_SAMPLE_US_MAX
        && allocations_max == ALLOCATIONS_MAX;
    let summary = GpuMeasureSummary {
        schema_version: 1,
        machine_model: discovery.catalog.machine_model,
        os_build: discovery.catalog.os_build,
        interval_ms: args.interval.as_millis().try_into().unwrap_or(u64::MAX),
        recurring_samples: args.count,
        construction_us,
        construction_allocations,
        baseline_us,
        baseline_allocations,
        warm_sample_us,
        warm_sample_allocations_max: allocations_max,
        regression_gate: GpuRegressionGate {
            p95_us_max: P95_US_MAX,
            single_sample_us_max: SINGLE_SAMPLE_US_MAX,
            allocations_max: ALLOCATIONS_MAX,
            passed,
        },
    };
    let writer: Box<dyn Write> = match args.output {
        Some(path) => Box::new(
            File::create(&path)
                .with_context(|| format!("create GPU measurement at {}", path.display()))?,
        ),
        None => Box::new(io::stdout()),
    };
    let mut writer = BufWriter::new(writer);
    serde_json::to_writer_pretty(&mut writer, &summary).context("write GPU measurement")?;
    writer.write_all(b"\n").context("finish GPU measurement")?;
    anyhow::ensure!(passed, "GPU provider performance regression gate failed");
    Ok(())
}

#[cfg(feature = "gpu-profiling")]
fn elapsed_us(duration: Duration) -> u64 {
    duration.as_micros().try_into().unwrap_or(u64::MAX)
}

#[cfg(feature = "gpu-profiling")]
fn percentile(sorted: &[u64], percentile: usize) -> u64 {
    let index = (sorted.len() * percentile).div_ceil(100).saturating_sub(1);
    sorted[index]
}
