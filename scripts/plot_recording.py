from __future__ import annotations

import argparse
from pathlib import Path
from collections.abc import Callable

import matplotlib.pyplot as plt
from matplotlib.axes import Axes
from matplotlib.figure import Figure

from recording import RecordingError, Sample, load_recording


def render_plot(samples: list[Sample], title: str) -> Figure:
    start_ms = samples[0].monotonic_ms
    seconds = [(sample.monotonic_ms - start_ms) / 1_000.0 for sample in samples]
    figure, (cpu_axis, temperature_axis) = plt.subplots(
        2,
        1,
        figsize=(11, 7),
        sharex=True,
        layout="constrained",
    )
    figure.suptitle(title)

    cpu_axis.plot(seconds, [sample.cpu_total_ratio * 100.0 for sample in samples], label="total")
    cpu_axis.plot(seconds, [sample.cpu_user_ratio * 100.0 for sample in samples], label="user")
    cpu_axis.plot(
        seconds,
        [sample.cpu_system_ratio * 100.0 for sample in samples],
        label="system",
    )
    cpu_axis.set_ylabel("CPU utilization (%)")
    cpu_axis.set_ylim(0.0, 100.0)
    cpu_axis.grid(alpha=0.25)
    cpu_axis.legend(loc="upper right")

    _plot_optional_series(
        temperature_axis,
        seconds,
        samples,
        "CPU max",
        lambda sample: sample.cpu_temp_max_c,
    )
    _plot_optional_series(
        temperature_axis,
        seconds,
        samples,
        "CPU average",
        lambda sample: sample.cpu_temp_avg_c,
    )
    if not temperature_axis.lines:
        temperature_axis.text(
            0.5,
            0.5,
            "CPU temperature unavailable",
            ha="center",
            va="center",
            transform=temperature_axis.transAxes,
        )
    else:
        temperature_axis.legend(loc="upper right")
    temperature_axis.set_xlabel("Elapsed time (s)")
    temperature_axis.set_ylabel("Temperature (°C)")
    temperature_axis.grid(alpha=0.25)
    return figure


def _plot_optional_series(
    axis: Axes,
    seconds: list[float],
    samples: list[Sample],
    label: str,
    value_of: Callable[[Sample], float | None],
) -> None:
    values = [value_of(sample) for sample in samples]
    points = [(second, value) for second, value in zip(seconds, values, strict=True) if value is not None]
    if points:
        x_values, y_values = zip(*points, strict=True)
        axis.plot(x_values, y_values, label=label)


def main() -> int:
    parser = argparse.ArgumentParser(description="Plot a rasitop CSV recording")
    parser.add_argument("recording", type=Path)
    parser.add_argument("--output", type=Path, help="write a PNG/PDF/SVG instead of opening a window")
    parser.add_argument("--title", help="override the figure title")
    args = parser.parse_args()

    try:
        samples = load_recording(args.recording)
    except RecordingError as error:
        parser.error(str(error))
    figure = render_plot(samples, args.title or args.recording.name)
    if args.output is None:
        plt.show()
    else:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        figure.savefig(args.output, dpi=160)
        plt.close(figure)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
