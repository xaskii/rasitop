from __future__ import annotations

import argparse
import json
import sys
import xml.etree.ElementTree as ET
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Any, Iterable

ACTIVITY_SCHEMA = "activity-monitor-process-live"
U32_MAX = (1 << 32) - 1
U64_MAX = (1 << 64) - 1
I64_MIN = -(1 << 63)
I64_MAX = (1 << 63) - 1


class ActivitySummaryError(ValueError):
    """An Activity Monitor export cannot be summarized safely."""


@dataclass(frozen=True, slots=True)
class Sample:
    start_ns: int
    process_name: str
    pid: int
    cpu_total_ns: int
    thread_count: int
    port_count: int
    physical_footprint_bytes: int
    private_bytes: int
    idle_wakeups: int
    disk_bytes_written: int
    disk_bytes_read: int


@dataclass(frozen=True, slots=True)
class CounterSummary:
    start: int
    end: int
    delta: int
    per_second: float


@dataclass(frozen=True, slots=True)
class GaugeSummary:
    start: int
    end: int
    delta: int
    min: int
    max: int


def summarize_activity_xml(xml: str) -> dict[str, Any]:
    try:
        root = ET.fromstring(xml)
    except ET.ParseError as error:
        raise ActivitySummaryError(f"parse Activity Monitor XML: {error}") from error

    node = _find_activity_node(root)
    if node is None:
        raise ActivitySummaryError(
            f"export does not contain the {ACTIVITY_SCHEMA} schema"
        )
    schema = _child(node, "schema")
    if schema is None:
        raise ActivitySummaryError("Activity Monitor export is missing its schema")
    columns = _schema_columns(schema)
    ids = _collect_ids(root)
    samples = [
        _parse_sample(row, columns, ids)
        for row in node
        if row.tag == "row"
    ]
    if len(samples) < 2:
        raise ActivitySummaryError(
            f"Activity Monitor export needs at least two samples, found {len(samples)}"
        )

    first = samples[0]
    last = samples[-1]
    if any(
        sample.pid != first.pid or sample.process_name != first.process_name
        for sample in samples
    ):
        raise ActivitySummaryError(
            "Activity Monitor export contains more than one process"
        )
    if any(
        previous.start_ns >= current.start_ns
        for previous, current in zip(samples, samples[1:], strict=False)
    ):
        raise ActivitySummaryError(
            "Activity Monitor sample times are not strictly increasing"
        )

    duration_ns = last.start_ns - first.start_ns
    if duration_ns == 0:
        raise ActivitySummaryError("Activity Monitor observation duration is zero")
    duration_seconds = duration_ns / 1_000_000_000.0
    cpu_delta_ns = _monotonic_delta(
        first.cpu_total_ns,
        last.cpu_total_ns,
        "CPU time",
    )

    return {
        "schema_version": 1,
        "process": {
            "name": first.process_name,
            "pid": first.pid,
        },
        "measurement": {
            "samples": len(samples),
            "start_ns": first.start_ns,
            "end_ns": last.start_ns,
            "duration_ns": duration_ns,
            "duration_seconds": duration_seconds,
        },
        "cpu": {
            "time_start_ns": first.cpu_total_ns,
            "time_end_ns": last.cpu_total_ns,
            "time_delta_ns": cpu_delta_ns,
            "time_delta_seconds": cpu_delta_ns / 1_000_000_000.0,
            "average_percent": cpu_delta_ns / duration_ns * 100.0,
        },
        "idle_wakeups": asdict(
            _counter_summary(
                first.idle_wakeups,
                last.idle_wakeups,
                duration_seconds,
                "idle wakeups",
            )
        ),
        "memory": {
            "physical_footprint_bytes": asdict(
                _gauge_summary(
                    sample.physical_footprint_bytes for sample in samples
                )
            ),
            "private_bytes": asdict(
                _gauge_summary(sample.private_bytes for sample in samples)
            ),
        },
        "threads": asdict(
            _gauge_summary(sample.thread_count for sample in samples)
        ),
        "ports": asdict(
            _gauge_summary(sample.port_count for sample in samples)
        ),
        "disk_io": {
            "bytes_read": asdict(
                _counter_summary(
                    first.disk_bytes_read,
                    last.disk_bytes_read,
                    duration_seconds,
                    "disk bytes read",
                )
            ),
            "bytes_written": asdict(
                _counter_summary(
                    first.disk_bytes_written,
                    last.disk_bytes_written,
                    duration_seconds,
                    "disk bytes written",
                )
            ),
        },
    }


def _parse_sample(
    row: ET.Element,
    columns: dict[str, int],
    ids: dict[str, ET.Element],
) -> Sample:
    def value(mnemonic: str) -> ET.Element:
        index = columns.get(mnemonic)
        if index is None:
            raise ActivitySummaryError(
                f"Activity Monitor schema is missing {mnemonic}"
            )
        children = list(row)
        if index >= len(children):
            raise ActivitySummaryError(
                f"Activity Monitor row has no value for {mnemonic}"
            )
        try:
            return _resolve_reference(children[index], ids)
        except ActivitySummaryError as error:
            raise ActivitySummaryError(
                f"resolve Activity Monitor value for {mnemonic}: {error}"
            ) from error

    pid = _parse_number(value("pid"), "pid")
    if pid > U32_MAX:
        raise ActivitySummaryError("Activity Monitor pid does not fit in u32")
    process = value("process")
    process_name = process.get("fmt") or (process.text or "").strip()
    if not process_name:
        raise ActivitySummaryError("Activity Monitor process name is empty")
    pid_suffix = f" ({pid})"
    if process_name.endswith(pid_suffix):
        process_name = process_name[: -len(pid_suffix)]

    return Sample(
        start_ns=_parse_number(value("start"), "start"),
        process_name=process_name,
        pid=pid,
        cpu_total_ns=_parse_number(value("cpu-total"), "cpu-total"),
        thread_count=_parse_number(value("thread-count"), "thread-count"),
        port_count=_parse_number(value("mach-port-count"), "mach-port-count"),
        physical_footprint_bytes=_parse_number(
            value("memory-physical-footprint"),
            "memory-physical-footprint",
        ),
        private_bytes=_parse_number(
            value("memory-real-private"),
            "memory-real-private",
        ),
        idle_wakeups=_parse_number(value("idle-wakeups"), "idle-wakeups"),
        disk_bytes_written=_parse_number(
            value("disk-bytes-written"),
            "disk-bytes-written",
        ),
        disk_bytes_read=_parse_number(
            value("disk-bytes-read"),
            "disk-bytes-read",
        ),
    )


def _parse_number(element: ET.Element, mnemonic: str) -> int:
    if element.tag == "sentinel":
        raise ActivitySummaryError(
            f"Activity Monitor value for {mnemonic} is unavailable"
        )
    text = (element.text or "").strip()
    try:
        value = int(text)
    except ValueError as error:
        raise ActivitySummaryError(
            f"parse Activity Monitor value for {mnemonic}"
        ) from error
    if not 0 <= value <= U64_MAX:
        raise ActivitySummaryError(
            f"parse Activity Monitor value for {mnemonic}"
        )
    return value


def _counter_summary(
    start: int,
    end: int,
    duration_seconds: float,
    name: str,
) -> CounterSummary:
    delta = _monotonic_delta(start, end, name)
    return CounterSummary(
        start=start,
        end=end,
        delta=delta,
        per_second=delta / duration_seconds,
    )


def _monotonic_delta(start: int, end: int, name: str) -> int:
    if end < start:
        raise ActivitySummaryError(
            f"Activity Monitor {name} counter decreased from {start} to {end}"
        )
    return end - start


def _gauge_summary(values: Iterable[int]) -> GaugeSummary:
    collected = list(values)
    if not collected:
        raise ActivitySummaryError("Activity Monitor gauge is empty")
    start = collected[0]
    end = collected[-1]
    delta = end - start
    if not I64_MIN <= delta <= I64_MAX:
        raise ActivitySummaryError(
            "Activity Monitor gauge delta does not fit in i64"
        )
    return GaugeSummary(
        start=start,
        end=end,
        delta=delta,
        min=min(collected),
        max=max(collected),
    )


def _schema_columns(schema: ET.Element) -> dict[str, int]:
    columns: dict[str, int] = {}
    for index, column in enumerate(
        child for child in schema if child.tag == "col"
    ):
        mnemonic_element = _child(column, "mnemonic")
        mnemonic = (
            (mnemonic_element.text or "").strip()
            if mnemonic_element is not None
            else ""
        )
        if not mnemonic:
            raise ActivitySummaryError(
                "Activity Monitor schema column has no mnemonic"
            )
        if mnemonic in columns:
            raise ActivitySummaryError(
                f"Activity Monitor schema contains duplicate column {mnemonic}"
            )
        columns[mnemonic] = index
    if not columns:
        raise ActivitySummaryError("Activity Monitor schema has no columns")
    return columns


def _find_activity_node(root: ET.Element) -> ET.Element | None:
    for element in root.iter("node"):
        schema = _child(element, "schema")
        if schema is not None and schema.get("name") == ACTIVITY_SCHEMA:
            return element
    return None


def _collect_ids(root: ET.Element) -> dict[str, ET.Element]:
    ids: dict[str, ET.Element] = {}
    for element in root.iter():
        identifier = element.get("id")
        if identifier is None:
            continue
        if identifier in ids:
            raise ActivitySummaryError(
                f"Activity Monitor export contains duplicate id {identifier}"
            )
        ids[identifier] = element
    return ids


def _resolve_reference(
    element: ET.Element,
    ids: dict[str, ET.Element],
) -> ET.Element:
    visited: set[str] = set()
    reference = element.get("ref")
    while reference is not None:
        if reference in visited:
            raise ActivitySummaryError(
                f"Activity Monitor export contains a reference cycle at {reference}"
            )
        visited.add(reference)
        referenced = ids.get(reference)
        if referenced is None:
            raise ActivitySummaryError(
                f"Activity Monitor export references unknown id {reference}"
            )
        element = referenced
        reference = element.get("ref")
    return element


def _child(element: ET.Element, name: str) -> ET.Element | None:
    return next((child for child in element if child.tag == name), None)


def _parse_args(arguments: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Summarize an xctrace Activity Monitor process export as JSON"
    )
    parser.add_argument(
        "input",
        type=Path,
        help="XML exported from the activity-monitor-process-live schema",
    )
    return parser.parse_args(arguments)


def main(arguments: list[str] | None = None) -> int:
    args = _parse_args(sys.argv[1:] if arguments is None else arguments)
    input_path: Path = args.input
    try:
        xml = input_path.read_text(encoding="utf-8")
        summary = summarize_activity_xml(xml)
    except (OSError, ActivitySummaryError) as error:
        print(f"activity_summary.py: {error}", file=sys.stderr)
        return 1
    json.dump(summary, sys.stdout, separators=(",", ":"))
    sys.stdout.write("\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
