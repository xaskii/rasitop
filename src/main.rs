use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};

use rasitop::measure::{self, MeasureMode, MeasureOptions};
use rasitop::record::{self, RecordOptions};
use rasitop::smc;

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
    }
}
