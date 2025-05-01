use std::process::{ExitCode, Stdio};

use clap::Parser;
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command,
};

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
    let mut child = Command::new("sudo")
        .args(["powermetrics", "-i", &interval.to_string(), "-f", "plist"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let pm_out = child.stdout.take().unwrap();
    tokio::spawn(async move {
        let mut reader = BufReader::new(pm_out);
        let mut buf = Vec::new();
        while let Ok(n) = reader.read_until(b'\0', &mut buf).await {
            if n == 0 {
                break;
            }
            println!("[powermetrics_task]: buffer length: {}", n);
            buf.clear();
        }
    });

    let pm_err = child.stderr.take().unwrap();
    tokio::spawn(async move {
        let mut reader = BufReader::new(pm_err).lines();
        while let Some(line) = reader.next_line().await.unwrap() {
            eprintln!("[powermetrics_stderr]: {}", line);
        }
    });

    match child.wait().await?.code() {
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
