from __future__ import annotations

import tempfile
import unittest
from pathlib import Path

import matplotlib

from plot_recording import render_plot
from recording import CSV_COLUMNS, RecordingError, load_recording, summarize

matplotlib.use("Agg")


class RecordingTests(unittest.TestCase):
    def test_loads_summarizes_and_plots_versioned_recording(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "samples.csv"
            path.write_text(
                ",".join(CSV_COLUMNS)
                + "\n"
                + _row(sequence=1, monotonic_ms=250.0, total=0.25, idle=0.75)
                + _row(sequence=2, monotonic_ms=500.0, total=0.75, idle=0.25),
                encoding="utf-8",
            )
            samples = load_recording(path)
            summary = summarize(samples)
            figure = render_plot(samples, "fixture")
            output = Path(directory) / "plot.png"
            figure.savefig(output)

            self.assertEqual(summary.rows, 2)
            self.assertEqual(summary.mean_cpu_total_ratio, 0.5)
            self.assertEqual(summary.max_cpu_total_ratio, 0.75)
            self.assertEqual(summary.capability_flags, 7)
            self.assertGreater(output.stat().st_size, 0)

    def test_rejects_sequence_gaps(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "samples.csv"
            path.write_text(
                ",".join(CSV_COLUMNS)
                + "\n"
                + _row(sequence=1, monotonic_ms=250.0, total=0.25, idle=0.75)
                + _row(sequence=3, monotonic_ms=500.0, total=0.75, idle=0.25),
                encoding="utf-8",
            )
            with self.assertRaisesRegex(RecordingError, "sequence jumps"):
                load_recording(path)

    def test_accepts_unavailable_sensors_as_empty_fields(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "samples.csv"
            path.write_text(
                ",".join(CSV_COLUMNS)
                + "\n"
                + _row(
                    sequence=1,
                    monotonic_ms=250.0,
                    total=0.25,
                    idle=0.75,
                    sensors=("", "", "", ""),
                    capability_flags=0,
                ),
                encoding="utf-8",
            )
            summary = summarize(load_recording(path))
            self.assertIsNone(summary.min_cpu_temp_c)
            self.assertEqual(summary.capability_flags, 0)


def _row(
    *,
    sequence: int,
    monotonic_ms: float,
    total: float,
    idle: float,
    sensors: tuple[str, str, str, str] = ("60.0", "55.0", "0.0", "12.5"),
    capability_flags: int = 7,
) -> str:
    user = total * 0.6
    system = total * 0.4
    values: tuple[object, ...] = (
        1,
        sequence,
        "2026-07-15T12:00:00.000Z",
        monotonic_ms,
        250.0,
        20,
        total,
        user,
        system,
        0.0,
        idle,
        *sensors,
        capability_flags,
        0,
    )
    return ",".join(str(value) for value in values) + "\n"


if __name__ == "__main__":
    unittest.main()
