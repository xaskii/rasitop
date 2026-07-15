# Initial Phase 0 validation baseline

This is a short correctness and performance baseline, not the Phase 0 exit
run. It was recorded on an Apple M4 Pro (`Mac16,8`, 14 logical CPUs, 24 GiB)
running macOS 27.0.

## Controlled captures

The capture suite ran each scenario for 15 seconds at a one-second interval:

```sh
uv run scripts/run_phase0_validation.py \
  benchmarks/phase0/2026-07-15-m4-pro --skip-build
```

| Scenario | Rows | Missing deadlines | Mean CPU | Maximum CPU | Temperature range | Error flags |
|---|---:|---:|---:|---:|---:|---:|
| Idle | 15/15 | 0 | 12.19% | 17.41% | 40.76–45.37 °C | 0 |
| Single thread | 15/15 | 0 | 16.45% | 17.94% | 41.02–64.02 °C | 0 |
| All cores | 15/15 | 0 | 100.00% | 100.00% | 49.00–100.44 °C | 0 |

The single busy process migrated across logical CPUs, so its work appears as a
smaller increase across several per-core series instead of one permanently
pinned core. The all-core run saturated every logical CPU. All runs exposed
temperature, fan, and system-power capabilities.

Raw aggregate and per-core CSVs, generated plots, hardware metadata, and exact
summary values are stored beside this file.

## Engine latency

The release CLI ran aggregate and per-core modes for 10 seconds at a 10 ms poll
interval. Mach counters produced 58 snapshots from 1,000 attempts in both
modes; the remaining polls returned immediately without a new counter delta.

| Mode | Attempt p50 | Attempt p95 | Snapshot p50 | Snapshot p95 | Missed deadlines |
|---|---:|---:|---:|---:|---:|
| Aggregate | 13.5 µs | 2.85 ms | 3.11 ms | 3.65 ms | 0 |
| Per core | 70.2 µs | 2.50 ms | 2.64 ms | 3.56 ms | 0 |

The snapshot timing includes SMC reads. Its p95 exceeds the provisional 3 ms
tick budget, which keeps adaptive SMC cadence and hot-path work reduction open.

### Adaptive-request follow-up

After adding explicit per-core and sensor request flags, the same 10-second,
10 ms release measurements intentionally requested only the CPU mode under
test. The menu bar app requests per-core counters every second and cached SMC
values immediately, then every five seconds.

| Mode | Attempt p50 | Attempt p95 | Snapshot p50 | Snapshot p95 | Missed deadlines |
|---|---:|---:|---:|---:|---:|
| Aggregate | 14.8 µs | 36.3 µs | 12.0 µs | 45.3 µs | 0 |
| Per core | 47.1 µs | 143 µs | 69.0 µs | 167 µs | 0 |

The warmed aggregate-only allocation-count test also observed zero Rust heap
allocations around an emitted snapshot. The machine-readable follow-up results
are `measure-aggregate-adaptive.json` and `measure-per-core-adaptive.json`.

## Release app activity

After a five-second warmup, a 30-second Activity Monitor trace reported:

- 0.0586% average process CPU;
- 1.017 idle wakeups per second;
- 12.63 MB maximum physical footprint;
- 65.6 kB physical-footprint growth during the measurement;
- no disk reads or writes during the measurement window.

These short-run figures are below the provisional closed-popover CPU, wakeup,
and footprint budgets. The validated trace and XML export remain under
`target/profiling`; `activity-summary.json` is the checked-in machine-readable
summary.

## Remaining Phase 0 evidence

- Run the 30-minute release capture and controlled-load validation.
- Record an Allocations trace and verify no growing object graph.
- Record the packaged sleep/wake behavior separately in Phase 2.
