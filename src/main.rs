use clap::Parser;
use std::process::{Command, ExitCode};

#[derive(Parser, Debug)]
#[command(version)]
struct Opts {
    /// Refresh interval (seconds)
    #[arg(short, long, default_value_t = 1)]
    interval: u16,
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

    let interval = args.interval * 1000;
    let status = Command::new("sudo")
        .args(["powermetrics", "-i", &interval.to_string()])
        .status()?;

    match status.code() {
        Some(0) => Ok(ExitCode::SUCCESS),
        Some(code) => {
            eprintln!("Command exited with code: {}", code);
            Ok(ExitCode::FAILURE)
        }
        None => {
            eprintln!("Command terminated by signal");
            Ok(ExitCode::FAILURE)
        }
    }
}
