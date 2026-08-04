# M4 Pro provider lifecycle probe

Captured 2026-08-04 on the validated `Mac16,8` / macOS build `26A5388g`
layout with:

```sh
cargo run --release -- gpu provider \
  --interval 250ms --count 4 --output provider-probe.csv
```

The first call establishes the IOReport baseline and emits an explicit gap.
Each subsequent call returns a bounded whole-device busy ratio and the actual
monotonic interval. This is a lifecycle smoke test, not performance evidence;
revision 4 owns timing and allocation gates.
