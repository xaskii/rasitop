from __future__ import annotations

import argparse
import json
import os
import platform
import subprocess
import sys
import time
from pathlib import Path
from typing import Any

from plot_recording import render_plot
from recording import load_recording, summarize

LOAD_COMMAND = "while True: pass"


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Capture idle, single-thread, and all-core Phase 0 recordings"
    )
    parser.add_argument("output_dir", type=Path)
    parser.add_argument("--duration-seconds", type=int, default=15)
    parser.add_argument("--interval-ms", type=int, default=1_000)
    parser.add_argument("--binary", type=Path, default=Path("target/release/rasitop"))
    parser.add_argument("--skip-build", action="store_true")
    args = parser.parse_args()
    if args.duration_seconds < 2:
        parser.error("--duration-seconds must be at least 2")
    if args.interval_ms < 1:
        parser.error("--interval-ms must be positive")

    if not args.skip_build:
        subprocess.run(["cargo", "build", "--release", "--bin", "rasitop"], check=True)
    if not args.binary.is_file():
        parser.error(f"rasitop binary does not exist: {args.binary}")

    args.output_dir.mkdir(parents=True, exist_ok=True)
    logical_cpus = os.cpu_count() or 1
    scenarios = (("idle", 0), ("single-thread", 1), ("all-core", logical_cpus))
    summaries: dict[str, dict[str, Any]] = {}
    for index, (name, workers) in enumerate(scenarios):
        recording = args.output_dir / f"{name}.csv"
        per_core_recording = args.output_dir / f"{name}-cores.csv"
        print(f"capturing {name} ({workers} load workers)", file=sys.stderr)
        _capture(
            args.binary,
            recording,
            args.interval_ms,
            args.duration_seconds,
            workers,
            per_core_recording,
        )
        samples = load_recording(recording)
        summary = summarize(samples).as_json()
        expected_rows = args.duration_seconds * 1_000 // args.interval_ms
        summary["expected_rows"] = expected_rows
        summary["missing_deadlines"] = max(expected_rows - len(samples), 0)
        summaries[name] = summary
        figure = render_plot(samples, f"Phase 0: {name}")
        figure.savefig(args.output_dir / f"{name}.png", dpi=160)
        figure.clear()
        if index + 1 < len(scenarios):
            time.sleep(2.0)

    report: dict[str, Any] = {
        "schema_version": 1,
        "platform": platform.platform(),
        "machine": platform.machine(),
        "hardware_model": _sysctl("hw.model"),
        "cpu_brand": _sysctl("machdep.cpu.brand_string"),
        "physical_memory_bytes": int(_sysctl("hw.memsize")),
        "logical_cpus": logical_cpus,
        "interval_ms": args.interval_ms,
        "duration_seconds_per_scenario": args.duration_seconds,
        "scenarios": summaries,
    }
    (args.output_dir / "summary.json").write_text(
        json.dumps(report, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    return 0


def _capture(
    binary: Path,
    recording: Path,
    interval_ms: int,
    duration_seconds: int,
    workers: int,
    per_core_recording: Path,
) -> None:
    load_processes = [
        subprocess.Popen(
            [sys.executable, "-c", LOAD_COMMAND],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        for _ in range(workers)
    ]
    try:
        with recording.open("wb") as output:
            subprocess.run(
                [
                    str(binary),
                    "record",
                    "--interval",
                    f"{interval_ms}ms",
                    "--duration",
                    f"{duration_seconds}s",
                    "--per-core-csv",
                    str(per_core_recording),
                ],
                check=True,
                stdout=output,
            )
    finally:
        for process in load_processes:
            process.terminate()
        for process in load_processes:
            process.wait()


def _sysctl(name: str) -> str:
    return subprocess.run(
        ["sysctl", "-n", name],
        check=True,
        capture_output=True,
        text=True,
    ).stdout.strip()


if __name__ == "__main__":
    raise SystemExit(main())
