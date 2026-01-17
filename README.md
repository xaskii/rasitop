# Rasitop

Sudoless performance monitoring for Apple Silicon. This is still evolving, with a
longer term goal of becoming a menubar app.

## Development Setup

```sh
# Get a rust toolchain somehow
cargo build --bin rasitop (only guaranteed to build on stable)
```

## Usage

Rasitop uses private macOS APIs (IOReport, SMC, and IOHID) to sample power and
utilization without sudo. These APIs can change across macOS releases.

```sh
rasitop [OPTIONS]
```

### Options

| Flag | Description | Default |
|------|-------------|---------|
| `-i, --interval <SECONDS>` | Refresh interval in seconds | `1` |
| `--format <FORMAT>` | Output format: `json`, `csv`, or `human` | `human` |
| `-h, --help` | Print help | - |
| `-V, --version` | Print version | - |

### Examples

```sh
# Run with default settings (1 second interval, human-readable output)
rasitop

# Run with 2 second refresh interval
rasitop -i 2

# Output as JSON
rasitop --format json

# Output as CSV
rasitop --format csv
```

## Credits

- Portions of the IOReport/SMC/IOHID sampling code are adapted from
  https://github.com/vladkens/macmon (MIT License) by vladkens.
