# rasitop

<img src="docs/assets/rasitop-menu-bar.png" alt="rasitop menu bar CPU monitor" width="320">

rasitop is a low-overhead macOS menu bar CPU monitor written in Rust and Swift.
It reads Mach CPU counters directly and displays current total utilization for
each logical core in a compact graph. User, system, and nice time remain
separate in the sampling engine and CSV recorder.

## Build

Requires macOS, a Rust toolchain, and the Xcode command-line tools.

```sh
cargo build
# target/debug/rasitop.app

cargo build --release
# target/release/rasitop.app
```

Cargo builds the Rust engine, Swift/AppKit shell, signed local app bundle, and
the `rasitop` command-line recorder.

Launch the release app directly with:

```sh
open -n target/release/rasitop.app
```

Install the current release bundle in `/Applications` with:

```sh
cargo install-app
```

This verifies the newly built bundle, replaces the installed copy with rollback
on failure, and relaunches rasitop if it was already running.

## CSV recorder

```sh
cargo run --release -- record --duration 1m > samples.csv

cargo run --release -- record --duration 1m \
  --per-core-csv cores.csv > samples.csv
```

Run `cargo run --release -- record --help` for all recording options.

## Recording validation

Python development tools are locked by `uv` and typechecked with Pyrefly:

```sh
uv sync --locked
uv run pyrefly check
uv run python -m unittest discover -s scripts/tests -t scripts -v
```

Validate or plot an existing versioned CSV recording:

```sh
uv run scripts/validate_recording.py samples.csv --require-clean
uv run scripts/plot_recording.py samples.csv --output samples.png
```

Produce short idle, single-thread, and all-core captures with plots, per-core
CSVs, hardware metadata, and a machine-readable summary:

```sh
uv run scripts/run_phase0_validation.py benchmarks/phase0/local
```

Checked-in baseline runs live under `benchmarks/phase0/`. They are evidence for
the machine and OS named in each run, not portable performance claims.

## Tests

```sh
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo nextest run
cargo +nightly miri test --lib ffi::tests
xcrun swift-format lint --recursive app-macos/Sources/rasitop_app
```

Use `cargo test` if `cargo-nextest` is unavailable.

## Inspiration

Inspired by [macmon](https://github.com/vladkens/macmon) and
[Stats](https://github.com/exelban/stats) by exelban.

## CPU profiling

```sh
nix run .#profile -- record --interval 1ms --duration 10s
```

This uses the Rust version and components in `rust-toolchain.toml` plus
cargo-instruments, compiles `std` and rasitop from source with full debug info
and frame pointers, records a CPU Profiler trace, and opens it in Instruments.
Builds are incremental after the first run. Set `INSTRUMENTS_NO_OPEN=1` to
leave the trace unopened or `INSTRUMENTS_OUTPUT` to choose its path.

Profile the actual menu bar app for one minute with separate CPU and allocation
traces:

```sh
nix run .#profile -- app cpu
nix run .#profile -- app allocations

# Or record both sequentially. The optional final argument is seconds.
nix run .#profile -- app all 60
```

CPU and allocation recording use separate runs so allocation instrumentation
does not distort the CPU baseline.
