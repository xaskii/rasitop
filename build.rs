use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn main() {
    println!("cargo::rerun-if-changed=app-macos/rasitop-info.plist");
    println!("cargo::rerun-if-changed=app-macos/include/rasitop.h");
    println!("cargo::rerun-if-changed=app-macos/Sources/rasitop_app");

    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }

    build_swift_app().expect("build Swift application shell");
}

fn build_swift_app() -> Result<(), String> {
    let root = PathBuf::from(required_env("CARGO_MANIFEST_DIR")?);
    let out_dir = PathBuf::from(required_env("OUT_DIR")?);
    let opt_level = required_env("OPT_LEVEL")?;
    let debug_info = required_env("DEBUG")? == "true";
    let architecture = match required_env("CARGO_CFG_TARGET_ARCH")?.as_str() {
        "aarch64" => "arm64",
        "x86_64" => "x86_64",
        architecture => return Err(format!("unsupported macOS architecture: {architecture}")),
    };
    let deployment_target =
        env::var("MACOSX_DEPLOYMENT_TARGET").unwrap_or_else(|_| "13.0".to_owned());

    let sdk_path = command_stdout("xcrun", ["--sdk", "macosx", "--show-sdk-path"])?;
    let swiftc = command_stdout("xcrun", ["--find", "swiftc"])?;

    let module_cache = out_dir.join("module-cache");
    let swift_module = out_dir.join("rasitop_app.swiftmodule");
    let swift_library = out_dir.join("librasitop_swift.a");
    fs::create_dir_all(&module_cache).map_err(display_error)?;

    let mut sources = swift_sources(&root.join("app-macos/Sources/rasitop_app"))?;
    sources.sort();

    let mut command = Command::new(swiftc);
    command
        .arg("-emit-library")
        .arg("-static")
        .arg("-parse-as-library")
        .arg("-module-name")
        .arg("rasitop_app")
        .arg("-emit-module-path")
        .arg(swift_module)
        .arg("-sdk")
        .arg(sdk_path)
        .arg("-target")
        .arg(format!("{architecture}-apple-macosx{deployment_target}"))
        .arg("-module-cache-path")
        .arg(&module_cache)
        .arg("-import-objc-header")
        .arg(root.join("app-macos/include/rasitop.h"));

    if opt_level == "0" {
        command.arg("-Onone");
    } else {
        command.arg("-Osize").arg("-whole-module-optimization");
    }
    if debug_info {
        command.arg("-g");
    }

    command.args(&sources).arg("-o").arg(&swift_library);
    run(&mut command, "compile Swift application shell")?;

    println!(
        "cargo::rustc-link-arg-bin=rasitop-app={}",
        swift_library.display()
    );
    println!("cargo::rustc-link-arg-bin=rasitop-app=-Wl,-rpath,/usr/lib/swift");
    assemble_bundle(&root, &out_dir)
}

fn assemble_bundle(root: &Path, out_dir: &Path) -> Result<(), String> {
    let profile_dir = out_dir
        .ancestors()
        .nth(3)
        .ok_or_else(|| format!("unexpected Cargo OUT_DIR: {}", out_dir.display()))?;
    let app_dir = profile_dir.join("rasitop.app");
    let contents = app_dir.join("Contents");
    let macos = contents.join("MacOS");
    let resources = contents.join("Resources");

    fs::create_dir_all(&macos).map_err(display_error)?;
    fs::create_dir_all(resources).map_err(display_error)?;
    fs::copy(
        root.join("app-macos/rasitop-info.plist"),
        contents.join("Info.plist"),
    )
    .map_err(display_error)?;

    Ok(())
}

fn swift_sources(directory: &Path) -> Result<Vec<PathBuf>, String> {
    let entries = fs::read_dir(directory).map_err(display_error)?;
    entries
        .filter_map(|entry| match entry {
            Ok(entry) if entry.path().extension() == Some(OsStr::new("swift")) => {
                Some(Ok(entry.path()))
            }
            Ok(_) => None,
            Err(error) => Some(Err(display_error(error))),
        })
        .collect()
}

fn required_env(name: &str) -> Result<String, String> {
    env::var(name).map_err(|error| format!("{name}: {error}"))
}

fn command_stdout<I, S>(program: &str, arguments: I) -> Result<String, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let output = Command::new(program)
        .args(arguments)
        .output()
        .map_err(display_error)?;
    check_output(program, output)
}

fn run(command: &mut Command, context: &str) -> Result<(), String> {
    let output = command.output().map_err(display_error)?;
    check_output(context, output).map(|_| ())
}

fn check_output(context: &str, output: Output) -> Result<String, String> {
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    } else {
        Err(format!(
            "{context} failed with {}:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

fn display_error(error: impl std::fmt::Display) -> String {
    error.to_string()
}
