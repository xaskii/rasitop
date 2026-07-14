use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};

use rasitop::record::{self, RecordOptions};

#[derive(Debug, Parser)]
#[command(version, about = "Low-overhead Apple Silicon performance recorder")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Record aggregate CPU utilization as CSV.
    Record(RecordArgs),
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
    }
}
