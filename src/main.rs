use std::process::ExitCode;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use clap::Parser;

mod output;
mod metrics;
mod error;
mod sources;

#[derive(Parser, Debug)]
#[command(version)]
struct Opts {
    /// Refresh interval (seconds)
    #[arg(short, long, default_value_t = 1)]
    interval: u16,
    /// Output format: json, csv, or human
    #[arg(long, default_value = "human")]
    format: OutputFormat,
}

#[derive(Debug, Clone, clap::ValueEnum)]
enum OutputFormat {
    Json,
    Csv,
    Human,
}

fn main() -> ExitCode {
    let result = run();
    match result {
        Ok(code) => code,
        Err(err) => {
            eprintln!("[rasitop error]: {:#}", err);
            ExitCode::FAILURE
        }
    }
}

fn run() -> anyhow::Result<ExitCode> {
    let args = Opts::parse();

    if std::env::consts::OS != "macos" {
        return Err(error::RasitopError::UnsupportedOs(std::env::consts::OS.to_string()).into());
    }

    let mut sampler = metrics::Sampler::new()?;

    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_clone = shutdown.clone();
    ctrlc::set_handler(move || {
        shutdown_clone.store(true, Ordering::SeqCst);
    })?;

    let formatter = create_formatter(&args.format);
    formatter.print_header();

    let interval_ms = (args.interval as u64 * 1000).clamp(100, 60_000) as u32;

    while !shutdown.load(Ordering::SeqCst) {
        let sample = sampler.sample(interval_ms)?;
        formatter.print_sample(&sample);
    }

    Ok(ExitCode::SUCCESS)
}

fn create_formatter(format: &OutputFormat) -> Box<dyn output::OutputFormatter> {
    match format {
        OutputFormat::Human => Box::new(output::HumanFormatter::new()),
        OutputFormat::Csv => Box::new(output::CsvFormatter::new()),
        OutputFormat::Json => Box::new(output::JsonFormatter::new()),
    }
}
