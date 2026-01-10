# Powermetrics Notes

Information extracted from `man powermetrics` relevant to this project.

## Overview

Powermetrics gathers CPU usage, timer/interrupt wakeups, package C-state statistics, CPU frequency distribution, and **estimated power consumption** by various SoC subsystems (CPU, GPU, ANE).

**CRITICAL WARNINGS:**
- Power values are **ESTIMATED** and may be inaccurate
- Should **NOT** be used for comparison between devices
- Can be used to optimize apps for energy efficiency
- Battery discharge rates are not comparable across machine models
- Energy model data is not comparable across machine models

## Current Implementation

We currently use:
```bash
powermetrics -f plist -i <sample_interval_ms> -n <sample_count>
```

- `-f plist`: Machine-readable property list, NUL-separated (what we parse)
- `-i N`: Sample every N ms (default: 5000ms)
- `-n N`: Obtain N periodic samples (0=infinite, default: 0)

## Output Format

Two formats available:
1. `text` - Human-readable text output (default)
2. `plist` - Machine-readable property list, NUL-separated (**we use this**)

## Key Data Available

### Current Data We Parse

From the `cpu_power` sampler (enabled by default):
- **CPU power** (mW) - Processor package power
- **GPU power** (mW) - Integrated GPU power
- **Combined power** (mW) - Total package power
- **ANE power** (mW) - Apple Neural Engine (we parse schema but don't display)
- **Cluster information**:
  - E-Cluster (efficiency cores)
  - P-Cluster (performance cores)
  - Frequency (Hz)
  - Idle ratio (0.0-1.0)
  - DVFM states (frequency/voltage operating points)

From the `battery` sampler (enabled by default):
- Battery charge percentage
- Discharge rate
- Current/maximum charge levels
- Cycle count
- Degradation from design capacity

### Additional Data Available (Not Yet Parsed)

#### Processor Details
- **C-states**: Idle residency states
  - Package C-states (entire CPU complex idle)
  - Per-core C-states
  - Higher residency = better energy efficiency
- **P-states**: Frequency/voltage operating points
- **Turbo mode**: Execution > 100% nominal frequency
  - Energy inefficient (quadratic power increase)
- **Per-CPU QoS breakdowns** (`--show-cpu-qos`)
- **PStates distribution** (`--show-pstates`)
- **PLimits** (`--show-plimits`)

#### Per-Process Metrics
- CPU time (user/kernel) (`tasks` sampler)
- Timer wakeups (short timers < 5ms) (`tasks` sampler)
- Interrupt wakeups (`tasks` sampler)
- Package idle exits (`tasks` sampler)
- Energy impact number (`--show-process-energy`)
  - Rough proxy for total energy (CPU + GPU + disk + network)
  - Platform-specific weighting
- GPU time (`--show-process-gpu`)
- IO statistics (`--show-process-io`)
- Network statistics (`--show-process-netstats`)
- QoS times (`--show-process-qos`)
- Instructions and cycles (`--show-process-ipc`)
- Wait times (`--show-process-wait-times`)
- Coalition grouping (`--show-process-coalition`)

#### System-Wide Metrics
- **Interrupts** (`interrupts` sampler)
  - Frequency by vector and device
  - Per-CPU basis
  - Useful for identifying misconfigured devices
- **Interrupt sources** (`int_sources` sampler)
  - Attributes interrupts to InterruptEventSources
- **Disk activity** (`disk` sampler)
  - Read/write operations
  - Per-process with `--show-process-io`
- **Network activity** (`network` sampler)
  - Packet counts and byte rates
  - Per-process with `--show-process-netstats`
- **SMC sensors** (`smc` sampler)
  - Fan speeds
  - Temperature sensors (instantaneous)
- **Thermal pressure** (`thermal` sampler)
  - Current thermal throttling state
- **SFI** (`sfi` sampler)
  - Selective Forced Idle statistics
  - Thread throttling for power limiting
- **Backlight level** (`battery` sampler)
  - Display brightness (not comparable across models)
- **Device states** (`devices` sampler)
  - Time spent in each device state
  - L = low power, U = usable, O = power on

## Energy Efficiency Metrics

**Lower is better:**
- CPU time
- Deadlines
- Interrupt wakeups
- Interrupt counts
- Package idle exits

**Higher is better:**
- C-state residency (idle time)

## Signal Handling

Powermetrics responds to signals:
- `SIGINFO`: Take immediate sample
- `SIGIO`: Flush buffered output
- `SIGINT/SIGTERM/SIGHUP`: Stop sampling and exit

## Samplers

Run `powermetrics -h` to see full list. Common samplers:
- `default` - Default set
- `all` - All supported samplers
- `tasks` - Process/task information
- `cpu_power` - CPU energy model (what we use)
- `battery` - Battery statistics
- `interrupts` - Interrupt distribution
- `network` - Network activity
- `disk` - Disk usage
- `smc` - System Management Controller
- `thermal` - Thermal state

Use `-s <samplers>` to specify comma-separated list.

## Known Issues

From the manpage:
- Changes in system time and sleep/wake can cause minor inaccuracies in reported CPU time
- Battery data may arrive out-of-phase with samples (aliasing issues on short intervals)
- Discharge rates across sleep/wake discontinuities may be inaccurate

## Recommendations for This Project

### Current Status
✅ Parsing CPU/GPU/combined power
✅ Parsing cluster frequency and busy ratios
✅ Parsing battery percentage
✅ Multiple output formats (human, CSV, JSON)
✅ Handling null-separated plist stream

### Potential Improvements

1. **Add more metrics from existing data:**
   - Timestamp in more formats
   - ANE (Apple Neural Engine) power (already in schema)
   - Per-cluster details beyond E/P (some systems may have more)

2. **Add new samplers:**
   - Battery discharge rate and cycle count
   - Thermal state (important for throttling detection)
   - Network and disk activity
   - Per-process energy impact (with `--show-process-energy`)

3. **Better error handling:**
   - Handle sleep/wake discontinuities
   - Filter out obviously incorrect battery data
   - Validate power values are reasonable

4. **Add sampler selection:**
   - Allow `-s` flag to pass samplers to powermetrics
   - Parse additional sampler outputs

5. **Documentation:**
   - Clarify that power values are estimates
   - Note that comparisons should only be within same device
   - Document that higher C-state residency = better efficiency

6. **Advanced features:**
   - Calculate energy over time (integrate power)
   - Track thermal throttling events
   - Correlate power spikes with process activity
   - Alert on high package idle exits (energy inefficiency)

### Architectural Considerations

- **Extensibility**: Schema is partial by design (comment in pm.rs)
  - Easy to add new fields to existing structs
  - Can add new sampler structs as needed
- **Streaming**: Already handles NUL-separated plist stream correctly
- **Format flexibility**: Trait-based formatters make adding new outputs easy
