use std::fs::File;
use std::io::{self, BufReader, BufWriter, Write};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};

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
    }
}
