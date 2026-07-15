from __future__ import annotations

import csv
import math
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Any

SCHEMA_VERSION = 1
CSV_COLUMNS = (
    "schema_version",
    "sequence",
    "timestamp_utc",
    "monotonic_ms",
    "interval_ms",
    "sample_duration_us",
    "cpu_total_ratio",
    "cpu_user_ratio",
    "cpu_system_ratio",
    "cpu_nice_ratio",
    "cpu_idle_ratio",
    "cpu_temp_max_c",
    "cpu_temp_avg_c",
    "fan_rpm",
    "system_power_w",
    "capability_flags",
    "error_flags",
)


class RecordingError(ValueError):
    """A recording does not satisfy the versioned CSV contract."""


@dataclass(frozen=True, slots=True)
class Sample:
    sequence: int
    timestamp_utc: str
    monotonic_ms: float
    interval_ms: float
    sample_duration_us: float
    cpu_total_ratio: float
    cpu_user_ratio: float
    cpu_system_ratio: float
    cpu_nice_ratio: float
    cpu_idle_ratio: float
    cpu_temp_max_c: float | None
    cpu_temp_avg_c: float | None
    fan_rpm: float | None
    system_power_w: float | None
    capability_flags: int
    error_flags: int


@dataclass(frozen=True, slots=True)
class RecordingSummary:
    rows: int
    first_sequence: int
    last_sequence: int
    elapsed_ms: float
    mean_interval_ms: float
    p95_interval_ms: float
    max_interval_ms: float
    mean_sample_duration_us: float
    p95_sample_duration_us: float
    max_sample_duration_us: float
    mean_cpu_total_ratio: float
    p95_cpu_total_ratio: float
    max_cpu_total_ratio: float
    min_cpu_temp_c: float | None
    max_cpu_temp_c: float | None
    capability_flags: int
    error_flags: int
    deadline_slips: int

    def as_json(self) -> dict[str, Any]:
        return asdict(self)


def load_recording(path: Path) -> list[Sample]:
    try:
        with path.open(newline="", encoding="utf-8") as stream:
            reader = csv.DictReader(stream)
            if tuple(reader.fieldnames or ()) != CSV_COLUMNS:
                raise RecordingError(
                    f"{path}: unexpected columns; expected {','.join(CSV_COLUMNS)}"
                )
            samples = [_parse_row(path, row_number, row) for row_number, row in enumerate(reader, 2)]
    except OSError as error:
        raise RecordingError(f"{path}: {error}") from error

    if not samples:
        raise RecordingError(f"{path}: recording has no samples")
    _validate_sequence(path, samples)
    return samples


def summarize(samples: list[Sample]) -> RecordingSummary:
    if not samples:
        raise RecordingError("cannot summarize an empty recording")

    intervals = [sample.interval_ms for sample in samples]
    durations = [sample.sample_duration_us for sample in samples]
    cpu_totals = [sample.cpu_total_ratio for sample in samples]
    temperatures = [
        value
        for sample in samples
        for value in (sample.cpu_temp_avg_c, sample.cpu_temp_max_c)
        if value is not None
    ]
    capability_flags = 0
    error_flags = 0
    for sample in samples:
        capability_flags |= sample.capability_flags
        error_flags |= sample.error_flags

    return RecordingSummary(
        rows=len(samples),
        first_sequence=samples[0].sequence,
        last_sequence=samples[-1].sequence,
        elapsed_ms=samples[-1].monotonic_ms - samples[0].monotonic_ms,
        mean_interval_ms=sum(intervals) / len(intervals),
        p95_interval_ms=_percentile(intervals, 95),
        max_interval_ms=max(intervals),
        mean_sample_duration_us=sum(durations) / len(durations),
        p95_sample_duration_us=_percentile(durations, 95),
        max_sample_duration_us=max(durations),
        mean_cpu_total_ratio=sum(cpu_totals) / len(cpu_totals),
        p95_cpu_total_ratio=_percentile(cpu_totals, 95),
        max_cpu_total_ratio=max(cpu_totals),
        min_cpu_temp_c=min(temperatures) if temperatures else None,
        max_cpu_temp_c=max(temperatures) if temperatures else None,
        capability_flags=capability_flags,
        error_flags=error_flags,
        deadline_slips=sum(
            sample.sample_duration_us > sample.interval_ms * 1_000.0 for sample in samples
        ),
    )


def _parse_row(path: Path, row_number: int, row: dict[str, str | None]) -> Sample:
    try:
        schema_version = _integer(row, "schema_version")
        if schema_version != SCHEMA_VERSION:
            raise RecordingError(
                f"schema_version is {schema_version}, expected {SCHEMA_VERSION}"
            )
        sample = Sample(
            sequence=_integer(row, "sequence"),
            timestamp_utc=_required(row, "timestamp_utc"),
            monotonic_ms=_number(row, "monotonic_ms"),
            interval_ms=_number(row, "interval_ms"),
            sample_duration_us=_number(row, "sample_duration_us"),
            cpu_total_ratio=_ratio(row, "cpu_total_ratio"),
            cpu_user_ratio=_ratio(row, "cpu_user_ratio"),
            cpu_system_ratio=_ratio(row, "cpu_system_ratio"),
            cpu_nice_ratio=_ratio(row, "cpu_nice_ratio"),
            cpu_idle_ratio=_ratio(row, "cpu_idle_ratio"),
            cpu_temp_max_c=_optional_number(row, "cpu_temp_max_c"),
            cpu_temp_avg_c=_optional_number(row, "cpu_temp_avg_c"),
            fan_rpm=_optional_number(row, "fan_rpm"),
            system_power_w=_optional_number(row, "system_power_w"),
            capability_flags=_integer(row, "capability_flags"),
            error_flags=_integer(row, "error_flags"),
        )
        _validate_sample(sample)
        return sample
    except (RecordingError, ValueError) as error:
        raise RecordingError(f"{path}:{row_number}: {error}") from error


def _validate_sample(sample: Sample) -> None:
    if sample.sequence < 1:
        raise RecordingError("sequence must be positive")
    if sample.monotonic_ms < 0.0:
        raise RecordingError("monotonic_ms must be non-negative")
    if sample.interval_ms <= 0.0:
        raise RecordingError("interval_ms must be positive")
    if sample.sample_duration_us < 0.0:
        raise RecordingError("sample_duration_us must be non-negative")
    if not math.isclose(
        sample.cpu_total_ratio + sample.cpu_idle_ratio, 1.0, abs_tol=1e-9
    ):
        raise RecordingError("total and idle CPU ratios must sum to one")
    if not math.isclose(
        sample.cpu_user_ratio + sample.cpu_system_ratio + sample.cpu_nice_ratio,
        sample.cpu_total_ratio,
        abs_tol=1e-9,
    ):
        raise RecordingError("CPU components must sum to total utilization")
    _bounded_optional(sample.cpu_temp_max_c, "cpu_temp_max_c", 0.0, 125.0)
    _bounded_optional(sample.cpu_temp_avg_c, "cpu_temp_avg_c", 0.0, 125.0)
    _bounded_optional(sample.fan_rpm, "fan_rpm", 0.0, 30_000.0)
    _bounded_optional(sample.system_power_w, "system_power_w", 0.0, 1_000.0)


def _validate_sequence(path: Path, samples: list[Sample]) -> None:
    for previous, current in zip(samples, samples[1:], strict=False):
        if current.sequence != previous.sequence + 1:
            raise RecordingError(
                f"{path}: sequence jumps from {previous.sequence} to {current.sequence}"
            )
        if current.monotonic_ms <= previous.monotonic_ms:
            raise RecordingError(f"{path}: monotonic_ms does not increase")


def _required(row: dict[str, str | None], name: str) -> str:
    value = row.get(name)
    if value is None or value == "":
        raise RecordingError(f"{name} is missing")
    return value


def _integer(row: dict[str, str | None], name: str) -> int:
    return int(_required(row, name))


def _number(row: dict[str, str | None], name: str) -> float:
    value = float(_required(row, name))
    if not math.isfinite(value):
        raise RecordingError(f"{name} must be finite")
    return value


def _optional_number(row: dict[str, str | None], name: str) -> float | None:
    value = row.get(name)
    if value is None or value == "":
        return None
    parsed = float(value)
    if not math.isfinite(parsed):
        raise RecordingError(f"{name} must be finite when present")
    return parsed


def _ratio(row: dict[str, str | None], name: str) -> float:
    value = _number(row, name)
    if not 0.0 <= value <= 1.0:
        raise RecordingError(f"{name} must be between zero and one")
    return value


def _bounded_optional(value: float | None, name: str, minimum: float, maximum: float) -> None:
    if value is not None and not minimum <= value <= maximum:
        raise RecordingError(f"{name} must be between {minimum:g} and {maximum:g}")


def _percentile(values: list[float], percentile: int) -> float:
    ordered = sorted(values)
    rank = math.ceil(percentile * len(ordered) / 100)
    return ordered[max(rank - 1, 0)]
