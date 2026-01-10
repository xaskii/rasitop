# Rasitop

Intended to be a rewrite of asitop in Rust, but not sure what direction I'm gonna go with this.

## Development Setup

```sh
# Get a rust toolchain somehow
cargo build --bin rasitop (only guaranteed to build on stable)
```

## Usage

Rasitop requires `sudo` to run `powermetrics` under the hood.

## Documentation

- `docs/POWERMETRICS_NOTES.md` collects powermetrics details and parser notes.

```sh
sudo rasitop [OPTIONS]
```

### Options

| Flag | Description | Default |
|------|-------------|---------|
| `-i, --interval <SECONDS>` | Refresh interval in seconds | `1` |
| `--format <FORMAT>` | Output format: `json`, `csv`, or `human` | `human` |
| `--log-level <LEVEL>` | Log level: `error`, `warn`, `info`, `debug`, `trace` (or use `RUST_LOG` env var) | `warn` |
| `-v, --verbose` | Enable verbose mode with formatted text output | off |
| `--from-file <PATH>` | Parse a plist sample from a file instead of running powermetrics (for testing) | - |
| `-h, --help` | Print help | - |
| `-V, --version` | Print version | - |

### Examples

```sh
# Run with default settings (1 second interval, human-readable output)
sudo rasitop

# Run with 2 second refresh interval
sudo rasitop -i 2

# Output as JSON
sudo rasitop --format json

# Output as CSV
sudo rasitop --format csv

# Test with a saved plist file
rasitop --from-file sample.plist -v
```

