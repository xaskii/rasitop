use std::process::{ExitCode, Stdio};

use clap::Parser;
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command,
};

mod metrics;
mod pm;
mod ui;

#[derive(Parser, Debug)]
#[command(version)]
struct Opts {
    /// Refresh interval (seconds)
    #[arg(short, long, default_value_t = 1)]
    interval: u16,
    /// Parse a plist sample from a file instead of running powermetrics (for testing)
    #[arg(long)]
    from_file: Option<std::path::PathBuf>,
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
        return Ok(ExitCode::SUCCESS);
    }

    let interval = args.interval * 1000;
    let mut child = Command::new("sudo")
        .args(["powermetrics", "-i", &interval.to_string(), "-f", "plist"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let pm_out = child.stdout.take().unwrap();
    let (tx_pm, mut rx_pm) = tokio::sync::mpsc::channel::<pm::PowermetricsSample>(512);

    // Reader and parser for powermetrics plist stream
    let tx_reader = tx_pm.clone();
    tokio::spawn(async move {
        let mut reader = BufReader::new(pm_out);
        let mut buf = Vec::new();
        while let Ok(n) = reader.read_until(b'\0', &mut buf).await {
            if n == 0 {
                break;
            }
            if let Some(last) = buf.last()
                && *last == 0
            {
                buf.pop();
            }
            match plist::from_bytes::<pm::PowermetricsPlist>(&buf) {
                Ok(doc) => {
                    if let Some(sample) = pm::PowermetricsSample::from_plist(&doc) {
                        let _ = tx_reader.send(sample).await;
                    }
                }
                Err(err) => {
                    eprintln!("[powermetrics_parser]: failed to parse plist: {:#}", err);
                }
            }
            buf.clear();
        }
    });

    // UI task
    tokio::spawn(async move {
        let mut term = match ui::setup_terminal() {
            Ok(t) => t,
            Err(err) => {
                eprintln!("UI init failed: {:#}", err);
                return;
            }
        };
        let mut app = ui::AppState::new(600);

        loop {
            // Drain all pending samples to keep up with producer
            while let Ok(sample) = rx_pm.try_recv() {
                app.history.push(sample);
            }

            if let Err(err) = term.draw(|f| ui::draw_ui(f, &app)) {
                eprintln!("UI draw error: {:#}", err);
                break;
            }

            // Non-blocking event to allow quit on 'q'
            match crossterm::event::poll(std::time::Duration::from_millis(0)) {
                Ok(true) => match crossterm::event::read() {
                    Ok(crossterm::event::Event::Key(key))
                        if key.code == crossterm::event::KeyCode::Char('q') =>
                    {
                        break;
                    }
                    Ok(_) => {}
                    Err(err) => {
                        eprintln!("UI input error: {:#}", err);
                        break;
                    }
                },
                Ok(false) => {}
                Err(err) => {
                    eprintln!("UI poll error: {:#}", err);
                    break;
                }
            }

            // Small sleep to avoid busy-looping the draw
            tokio::time::sleep(std::time::Duration::from_millis(16)).await;
        }

        let _ = ui::restore_terminal();
    });

    let pm_stderr = child.stderr.take().unwrap();
    tokio::spawn(async move {
        let mut reader = BufReader::new(pm_stderr).lines();
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
