use std::fs;
use std::io::{self, BufWriter, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::Parser;

use rasitop::activity::summarize_activity_xml;

#[derive(Debug, Parser)]
#[command(about = "Summarize an xctrace Activity Monitor process export as JSON")]
struct Cli {
    /// XML exported from the activity-monitor-process-live schema.
    input: PathBuf,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("rasitop-activity-summary: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<()> {
    let args = Cli::parse();
    let xml = fs::read_to_string(&args.input)
        .with_context(|| format!("read Activity Monitor export at {}", args.input.display()))?;
    let summary = summarize_activity_xml(&xml)?;

    let stdout = io::stdout();
    let mut writer = BufWriter::new(stdout.lock());
    serde_json::to_writer(&mut writer, &summary).context("write JSON summary")?;
    writer.write_all(b"\n").context("finish JSON summary")?;
    writer.flush().context("flush JSON summary")
}
