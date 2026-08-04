# M4 Pro GPU residency validation

Captured 2026-08-04 on a `Mac16,8` with an Apple M4 Pro, macOS 27.0
build `26A5388g`. No serial number, UUID, UDID, user name, host name, or
absolute user path is retained.

The candidate layout is exactly:

```text
group: GPU Stats
subgroup: GPU Performance States
channel: GPUPH
unit: 24Mticks
states: OFF,P1,P2,P3,P4,P5,P6,P7,P8,P9,P10,P11,P12,P13,P14,P15
idle: OFF
busy: P1..P15
```

Each profile has three independent raw captures and three outputs decoded by
the checked-in catalog. Captures use one-second intervals. Load runs contain a
one-second lead-in, five seconds of controlled Metal compute, and a final
transition interval. `low`, `medium`, and `max` request 0.25, 0.5, and 1.0
submitted-work duty cycles respectively. Duty cycle controls command submission
and rest time; it is not an expected utilization percentage.

Build the development-only workload with:

```sh
xcrun swiftc scripts/gpu_load.swift -framework Metal -o /tmp/rasitop-gpu-load
```

Capture raw residency and decode it with:

```sh
cargo run --release -- gpu residency \
  --group 'GPU Stats' \
  --subgroup 'GPU Performance States' \
  --channel GPUPH \
  --interval 1s --count 7 --output raw.csv
cargo run --release -- gpu decode --input raw.csv --output decoded.csv
```

The summary shows a monotonic directional response across idle, low, medium,
and maximum profiles. Residency totals cover the nominal one-second interval
within 0.14% below to 0.86% above. The diagnostic sampling path has a 1.907 ms
p95 here, above the provisional 0.5 ms production target; this path still uses
diagnostic allocations and must not be mistaken for the later optimized
provider.

Two Instruments `Metal System Trace` references were also taken: a sleeping
target and the maximum Metal workload. The exported GPU-state table shows far
more active events and active duration under load. Only the privacy-safe
aggregate is retained in `instruments-summary.json`; raw traces enumerate
unrelated processes and machine identifiers and are not committed.
