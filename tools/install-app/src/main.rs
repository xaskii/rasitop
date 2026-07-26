use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::Path;
use std::process::{Command, ExitStatus, Output};
use std::thread;
use std::time::Duration;

const APP_NAME: &str = "rasitop.app";
const APP_EXECUTABLE: &str = "Contents/MacOS/rasitop";

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let dry_run = parse_arguments()?;
    if !cfg!(target_os = "macos") {
        return Err("cargo install-app is only supported on macOS".to_owned());
    }

    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| "could not locate the repository root".to_owned())?;
    let target_dir = repo_root.join("target");
    let bundle = target_dir.join("release").join(APP_NAME);

    println!("Building release bundle...");
    let cargo = env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    run_command(
        Command::new(cargo)
            .current_dir(repo_root)
            .arg("build")
            .arg("--release")
            .arg("--bin")
            .arg("rasitop-app")
            .arg("--target-dir")
            .arg(&target_dir),
        "build release bundle",
    )?;
    verify_bundle(&bundle)?;

    if dry_run {
        println!(
            "Verified {} (dry run; /Applications was not changed).",
            bundle.display()
        );
        return Ok(());
    }

    install_bundle(&bundle, Path::new("/Applications"))
}

fn parse_arguments() -> Result<bool, String> {
    let mut dry_run = false;
    for argument in env::args().skip(1) {
        match argument.as_str() {
            "--dry-run" => dry_run = true,
            "-h" | "--help" => {
                println!(
                    "\
Build and install the current rasitop app bundle.

Usage: cargo install-app [--dry-run]

Options:
  --dry-run  Build and verify without changing /Applications
  -h, --help Show this help"
                );
                std::process::exit(0);
            }
            _ => return Err(format!("unknown argument: {argument}")),
        }
    }
    Ok(dry_run)
}

fn install_bundle(source: &Path, install_dir: &Path) -> Result<(), String> {
    let target = install_dir.join(APP_NAME);
    let nonce = std::process::id();
    let staged = install_dir.join(format!(".{APP_NAME}.installing-{nonce}"));
    let backup = install_dir.join(format!(".{APP_NAME}.previous-{nonce}"));

    if staged.exists() || backup.exists() {
        return Err(format!(
            "temporary install paths already exist in {}",
            install_dir.display()
        ));
    }

    println!("Staging {}...", target.display());
    if let Err(error) = copy_bundle(source, &staged) {
        remove_temporary_bundle(&staged);
        return Err(error);
    }
    if let Err(error) = verify_bundle(&staged) {
        remove_temporary_bundle(&staged);
        return Err(error);
    }

    let process_pattern = format!("^{}/{}($| )", regex_escape_path(&target), APP_EXECUTABLE);
    let was_running = match process_is_running(&process_pattern) {
        Ok(running) => running,
        Err(error) => {
            remove_temporary_bundle(&staged);
            return Err(error);
        }
    };
    if was_running && let Err(error) = stop_running_app(&process_pattern) {
        remove_temporary_bundle(&staged);
        return Err(error);
    }

    if let Err(error) = replace_staged_bundle(&staged, &target, &backup) {
        remove_temporary_bundle(&staged);
        relaunch_after_failed_install(was_running, &target);
        return Err(error);
    }

    println!("Installed {}.", target.display());
    if was_running {
        run_command(
            Command::new("/usr/bin/open").arg("-n").arg(&target),
            "relaunch rasitop",
        )
        .map_err(|error| format!("installed successfully, but {error}"))?;
        println!("Relaunched rasitop.");
    }
    Ok(())
}

fn replace_staged_bundle(staged: &Path, target: &Path, backup: &Path) -> Result<(), String> {
    let had_existing_bundle = target.exists();
    if had_existing_bundle && let Err(error) = fs::rename(target, backup) {
        return Err(format!("move existing {} aside: {error}", target.display()));
    }

    if let Err(error) = fs::rename(staged, target) {
        let rollback = if had_existing_bundle {
            fs::rename(backup, target)
                .map_err(|restore_error| format!("; rollback failed: {restore_error}"))
        } else {
            Ok(())
        };
        return Err(format!(
            "install {}: {error}{}",
            target.display(),
            rollback.err().unwrap_or_default()
        ));
    }

    if had_existing_bundle && let Err(error) = fs::remove_dir_all(backup) {
        eprintln!(
            "warning: installed successfully but could not remove {}: {error}",
            backup.display()
        );
    }
    Ok(())
}

fn copy_bundle(source: &Path, destination: &Path) -> Result<(), String> {
    run_command(
        Command::new("/usr/bin/ditto").arg(source).arg(destination),
        "stage app bundle",
    )
}

fn verify_bundle(bundle: &Path) -> Result<(), String> {
    if !bundle.join(APP_EXECUTABLE).is_file() {
        return Err(format!(
            "{} does not contain the rasitop executable",
            bundle.display()
        ));
    }
    run_command(
        Command::new("/usr/bin/codesign")
            .arg("--verify")
            .arg("--deep")
            .arg("--strict")
            .arg(bundle),
        "verify app bundle signature",
    )
}

fn process_is_running(pattern: &str) -> Result<bool, String> {
    let output = Command::new("/usr/bin/pgrep")
        .arg("-f")
        .arg(pattern)
        .output()
        .map_err(|error| format!("check whether rasitop is running: {error}"))?;
    match output.status.code() {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        _ => Err(command_error("check whether rasitop is running", output)),
    }
}

fn stop_running_app(pattern: &str) -> Result<(), String> {
    run_command(
        Command::new("/usr/bin/pkill")
            .arg("-TERM")
            .arg("-f")
            .arg(pattern),
        "stop running rasitop",
    )?;

    for _ in 0..30 {
        if !process_is_running(pattern)? {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(100));
    }
    Err("rasitop did not exit after 3 seconds".to_owned())
}

fn relaunch_after_failed_install(was_running: bool, target: &Path) {
    if was_running && target.exists() {
        let _ = Command::new("/usr/bin/open").arg("-n").arg(target).status();
    }
}

fn remove_temporary_bundle(path: &Path) {
    if path.exists() {
        let _ = fs::remove_dir_all(path);
    }
}

fn regex_escape_path(path: &Path) -> String {
    path.as_os_str()
        .to_string_lossy()
        .chars()
        .flat_map(|character| {
            if ".+*?()|[]{}^$\\".contains(character) {
                vec!['\\', character]
            } else {
                vec![character]
            }
        })
        .collect()
}

fn run_command(command: &mut Command, context: &str) -> Result<(), String> {
    let program = command.get_program().to_owned();
    let status = command
        .status()
        .map_err(|error| format!("{context}: failed to run {}: {error}", display(&program)))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{context}: {}", display_status(status)))
    }
}

fn command_error(context: &str, output: Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr);
    format!(
        "{context}: {}{}",
        display_status(output.status),
        if stderr.trim().is_empty() {
            String::new()
        } else {
            format!(": {}", stderr.trim())
        }
    )
}

fn display(program: &OsStr) -> String {
    program.to_string_lossy().into_owned()
}

fn display_status(status: ExitStatus) -> String {
    status.code().map_or_else(
        || "terminated by signal".to_owned(),
        |code| format!("exited with status {code}"),
    )
}

#[cfg(test)]
mod tests {
    use super::{regex_escape_path, replace_staged_bundle};
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn escapes_bundle_path_for_pgrep() {
        assert_eq!(
            regex_escape_path(Path::new("/Applications/rasitop.app")),
            "/Applications/rasitop\\.app"
        );
    }

    #[test]
    fn replaces_existing_bundle_and_removes_backup() {
        let root = temporary_directory();
        let staged = root.join("staged.app");
        let target = root.join("target.app");
        let backup = root.join("backup.app");
        fs::create_dir_all(&staged).unwrap();
        fs::create_dir_all(&target).unwrap();
        fs::write(staged.join("version"), "new").unwrap();
        fs::write(target.join("version"), "old").unwrap();

        replace_staged_bundle(&staged, &target, &backup).unwrap();

        assert_eq!(fs::read_to_string(target.join("version")).unwrap(), "new");
        assert!(!staged.exists());
        assert!(!backup.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn restores_existing_bundle_when_install_fails() {
        let root = temporary_directory();
        let staged = root.join("missing.app");
        let target = root.join("target.app");
        let backup = root.join("backup.app");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("version"), "old").unwrap();

        let error = replace_staged_bundle(&staged, &target, &backup).unwrap_err();

        assert!(error.contains("install"));
        assert_eq!(fs::read_to_string(target.join("version")).unwrap(), "old");
        assert!(!backup.exists());
        fs::remove_dir_all(root).unwrap();
    }

    fn temporary_directory() -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "rasitop-install-app-test-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&path).unwrap();
        path
    }
}
