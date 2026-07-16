# Packaged sleep/wake validation

This run validates the signed release app across a real system sleep/wake cycle
on an Apple M4 Pro (`Mac16,8`, 14 logical CPUs, 24 GiB) running macOS 27.0
(`26A5378j`).

The release bundle passed `codesign --verify --deep --strict`. One packaged app
process was launched from
`target/release/rasitop.app/Contents/MacOS/rasitop`, warmed for five seconds,
and measured with separate 30-second Activity Monitor traces before sleep and
after wake. Both traces attached to PID 92961, proving that the packaged process
survived the cycle rather than being relaunched.

`pmset` recorded software sleep at 19:25:38 CDT and a user-input wake at
19:26:14 CDT, for 36 seconds asleep. The relevant power events are saved in
`power-events.txt`.

## Results

| Window | Samples | Average CPU | Idle wakeups/second | Maximum footprint | Footprint delta |
|---|---:|---:|---:|---:|---:|
| Before sleep | 30 | 0.0495% | 1.063 | 12.57 MB | -49.2 kB |
| After wake | 30 | 0.0391% | 1.085 | 12.76 MB | +16.4 kB |

The post-wake window remains below the provisional closed-popover budgets of
0.20% CPU, two wakeups per second, and a 30 MB physical footprint. Its maximum
footprint is 196,632 bytes (0.188 MiB) above the pre-sleep maximum, well below
the 2 MB long-run growth allowance. Neither window performed disk I/O.

The machine-readable summaries are checked in beside this report. The raw
Activity Monitor traces and XML exports remain under
`target/profiling/sleep-wake-20260715-1930`.
