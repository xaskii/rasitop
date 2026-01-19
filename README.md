# Rasitop

Sudoless performance monitoring for Apple Silicon. This is still evolving, with a
menu bar app target alongside the CLI.

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

### Menu bar app

Build and run the menu bar UI:

```sh
cargo run --bin rasitop-menubar
```

Click the status item to open the popover UI. Use the Quit button to exit.

## Credits

- Portions of the IOReport/SMC/IOHID sampling code are adapted from
  https://github.com/vladkens/macmon (MIT License) by vladkens.
