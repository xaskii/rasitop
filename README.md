# rasitop

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

## CSV recorder

```sh
cargo run --release -- record --duration 1m > samples.csv

cargo run --release -- record --duration 1m \
  --per-core-csv cores.csv > samples.csv
```

Run `cargo run --release -- record --help` for all recording options.

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
