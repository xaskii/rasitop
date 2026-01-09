use std::process::{ExitCode, Stdio};

use clap::Parser;
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command,
};

mod output;
mod pm;

#[derive(Parser, Debug)]
#[command(version)]
struct Opts {
    /// Refresh interval (seconds)
    #[arg(short, long, default_value_t = 1)]
    interval: u16,
    /// Parse a plist sample from a file instead of running powermetrics (for testing)
    #[arg(long)]
    from_file: Option<std::path::PathBuf>,
    /// Enable verbose mode with formatted text output
    #[arg(short, long)]
    verbose: bool,
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

#[tokio::main]
async fn main() -> ExitCode {
    let result = run().await;
    match result {
        Ok(code) => code,
        Err(err) => {
            eprintln!("[rasitop error]: {:#}", err);
            ExitCode::FAILURE
        }
    }
}

async fn run() -> anyhow::Result<ExitCode> {
    let args = Opts::parse();

    // Testing path: parse from file once and exit
    if let Some(path) = args.from_file.as_ref() {
        let bytes = tokio::fs::read(path).await?;
        let doc: pm::PowermetricsPlist = plist::from_bytes(&bytes)?;
        if let Some(sample) = pm::PowermetricsSample::from_plist(&doc) {
            if args.verbose {
                let formatter = create_formatter(&args.format);
                formatter.print_header();
                formatter.print_sample(&sample);
            } else {
                // Original debug output
                println!(
                    "from_file => cpu_power={:.2} gpu_power={:.2} combined={:.2} e_busy={:?} p_busy={:?} e_freq={:?} p_freq={:?}",
                    sample.cpu_power_mw,
                    sample.gpu_power_mw,
                    sample.combined_power_mw,
                    sample.e_busy_ratio,
                    sample.p_busy_ratio,
                    sample.e_freq_hz,
                    sample.p_freq_hz,
                );
            }
        }
        return Ok(ExitCode::SUCCESS);
    }

    // TODO: validate that the interval is greater than 100ms. The min
    // collection interval that I found powermetrics is able to do is 22ms.
    // When we switch to read directly from SMC, we can probably go lower, but
    // I'm not sure if that makes any sense.
    let interval_ms = args.interval * 1000;
    let mut child = Command::new("sudo")
        .args([
            "powermetrics",
            "-i",
            &interval_ms.to_string(),
            "-f",
            "plist",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let pm_out = child.stdout.take().unwrap();
    let (tx_pm, mut rx_pm) = tokio::sync::mpsc::channel::<pm::PowermetricsSample>(512);

    // Reader and parser for powermetrics plist stream
    let tx_reader = tx_pm.clone();
    tokio::spawn(async move {
        let reader = BufReader::with_capacity(128 * 1024, pm_out);
        let mut segments = reader.split(b'\0');
        let mut sample_count = 0;
        let mut error_count = 0;

        while let Ok(Some(segment)) = segments.next_segment().await {
            // Skip empty segments
            if segment.is_empty() {
                continue;
            }

            sample_count += 1;

            match plist::from_bytes::<pm::PowermetricsPlist>(&segment) {
                Ok(doc) => {
                    if let Some(sample) = pm::PowermetricsSample::from_plist(&doc) {
                        if tx_reader.send(sample).await.is_err() {
                            eprintln!("[powermetrics_parser]: receiver dropped, stopping parser");
                            break;
                        }
                    } else {
                        eprintln!(
                            "[powermetrics_parser]: sample {} - valid plist but no usable sample data",
                            sample_count
                        );
                    }
                }
                Err(err) => {
                    error_count += 1;
                    eprintln!(
                        "[powermetrics_parser]: sample {} - parse error: {:#}",
                        sample_count, err
                    );

                    // Debug: show segment preview for first few errors
                    if error_count <= 3 {
                        eprintln!(
                            "[powermetrics_parser]: segment size: {} bytes",
                            segment.len()
                        );
                        eprintln!(
                            "[powermetrics_parser]: first 200 chars: {:?}",
                            String::from_utf8_lossy(&segment)
                                .chars()
                                .take(200)
                                .collect::<String>()
                        );
                    }

                    // Stop if too many consecutive errors
                    if error_count > 10 {
                        eprintln!("[powermetrics_parser]: too many errors, stopping parser");
                        break;
                    }
                }
            }
        }

        eprintln!(
            "[powermetrics_parser]: parser stopped - samples: {}, errors: {}",
            sample_count, error_count
        );
    });

    // Stderr handler
    let pm_stderr = child.stderr.take().unwrap();
    tokio::spawn(async move {
        let mut reader = BufReader::new(pm_stderr).lines();
        while let Some(line) = reader.next_line().await.unwrap() {
            eprintln!("[powermetrics_stderr]: {}", line);
        }
    });

    // Set up Ctrl-C handler
    let shutdown = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let shutdown_clone = shutdown.clone();
    ctrlc::set_handler(move || {
        shutdown_clone.store(true, std::sync::atomic::Ordering::SeqCst);
    })?;

    // Main output loop
    let formatter = create_formatter(&args.format);
    formatter.print_header();

    loop {
        // Check for Ctrl-C
        if shutdown.load(std::sync::atomic::Ordering::SeqCst) {
            eprintln!("[main]: received Ctrl-C, shutting down");
            break;
        }

        // Try to receive a sample with timeout
        match tokio::time::timeout(std::time::Duration::from_millis(100), rx_pm.recv()).await {
            Ok(Some(sample)) => {
                formatter.print_sample(&sample);
            }
            Ok(None) => {
                eprintln!("[main]: parser channel closed, shutting down");
                break;
            }
            Err(_) => {
                // Timeout, continue loop to check for Ctrl-C
                continue;
            }
        }
    }

    // Clean shutdown
    drop(tx_pm);
    let _ = child.kill().await;

    match child.wait().await?.code() {
        Some(0) | Some(137) => Ok(ExitCode::SUCCESS), // 137 = SIGKILL is expected
        Some(code) => {
            eprintln!("Command exited with code: {}", code);
            Ok(ExitCode::FAILURE)
        }
        None => {
            eprintln!("Command terminated by signal");
            Ok(ExitCode::SUCCESS) // Expected since we killed it
        }
    }
}

fn create_formatter(format: &OutputFormat) -> Box<dyn output::OutputFormatter> {
    match format {
        OutputFormat::Human => Box::new(output::HumanFormatter::new()),
        OutputFormat::Csv => Box::new(output::CsvFormatter::new()),
        OutputFormat::Json => Box::new(output::JsonFormatter::new()),
    }
}
