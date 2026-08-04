set shell := ["bash", "-eu", "-o", "pipefail", "-c"]

# List available project commands.
default:
    @just --list

# Build the debug CLI and app bundle.
build:
    cargo build

# Build the optimized CLI and app bundle.
release:
    cargo build --release

# Format, lint, and test the Rust and Swift code.
check:
    cargo fmt --all --check
    cargo clippy --all-targets --all-features -- -D warnings
    cargo nextest run
    xcrun swift-format lint --recursive app-macos/Sources/rasitop_app

# Run the focused unsafe-boundary checks under Miri.
miri:
    cargo +nightly miri test --lib ffi::tests
    cargo +nightly miri test --lib gpu::tests
    cargo +nightly miri test --lib ioreport::tests

# Open the release app's live-data preview for a bounded number of seconds.
preview seconds="30": release
    open -n target/release/rasitop.app --args --ui-preview --profile-duration-seconds {{ seconds }}

# Record aggregate CPU and GPU utilization as CSV.
record duration="1m":
    cargo run --release -- record --duration {{ duration }}

# Verify that the release bundle is ready to install.
install-check: release
    test -x target/release/rasitop.app/Contents/MacOS/rasitop
    /usr/bin/codesign --verify --deep --strict target/release/rasitop.app

# Install the release bundle in /Applications, with rollback and relaunch.
install: install-check
    #!/usr/bin/env bash
    set -euo pipefail

    source_bundle="$PWD/target/release/rasitop.app"
    target_bundle="/Applications/rasitop.app"
    staged_bundle="/Applications/.rasitop.app.installing-$$"
    backup_bundle="/Applications/.rasitop.app.previous-$$"
    process_pattern='^/Applications/rasitop\.app/Contents/MacOS/rasitop($| )'
    was_running=false
    stopped=false

    cleanup() {
      if [[ -e "$staged_bundle" ]]; then
        rm -rf "$staged_bundle"
      fi
      if [[ -e "$backup_bundle" && ! -e "$target_bundle" ]]; then
        mv "$backup_bundle" "$target_bundle"
      fi
      if [[ "$stopped" == true && -e "$target_bundle" ]]; then
        /usr/bin/open -n "$target_bundle" || true
      fi
    }
    trap cleanup EXIT

    [[ ! -e "$staged_bundle" && ! -e "$backup_bundle" ]]
    /usr/bin/ditto "$source_bundle" "$staged_bundle"
    /usr/bin/codesign --verify --deep --strict "$staged_bundle"

    if /usr/bin/pgrep -f "$process_pattern" >/dev/null; then
      was_running=true
      /usr/bin/pkill -TERM -f "$process_pattern"
      for _ in {1..30}; do
        /usr/bin/pgrep -f "$process_pattern" >/dev/null || break
        sleep 0.1
      done
      ! /usr/bin/pgrep -f "$process_pattern" >/dev/null
      stopped=true
    fi

    if [[ -e "$target_bundle" ]]; then
      mv "$target_bundle" "$backup_bundle"
    fi
    mv "$staged_bundle" "$target_bundle"
    if [[ -e "$backup_bundle" ]]; then
      rm -rf "$backup_bundle" || echo "warning: could not remove $backup_bundle" >&2
    fi
    trap - EXIT

    if [[ "$was_running" == true ]]; then
      /usr/bin/open -n "$target_bundle"
    fi
    echo "Installed $target_bundle"

# Profile the release app with Instruments through the pinned Nix environment.
profile kind="cpu" seconds="60":
    nix run .#profile -- app {{ kind }} {{ seconds }}
