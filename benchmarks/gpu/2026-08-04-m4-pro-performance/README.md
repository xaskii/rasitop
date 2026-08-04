# M4 Pro GPU provider performance

Captured on `Mac16,8`, macOS build `26A5388g`, from an optimized build:

```sh
cargo run --release --features gpu-profiling -- gpu measure \
  --interval 100ms --count 100 --output provider.json
```

The command exercises the real IOReport subscription. It does not run a GPU
workload. Construction and the first baseline are reported separately from the
recurring path. The recurring sample gate requires zero Rust heap allocations,
p95 latency no greater than 2.5 ms, and every sample below 5 ms. Those limits
leave measured headroom at the production one-second cadence without introducing
adaptive behavior.

IOReport subscription creation is denied inside the Codex filesystem sandbox,
so the evidence command was run with approved host access. Unknown layouts still
fail before measurement.
