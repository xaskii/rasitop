from __future__ import annotations

import unittest

from activity_summary import ActivitySummaryError, summarize_activity_xml

XML = """<?xml version="1.0"?>
<trace-query-result><node><schema name="activity-monitor-process-live">
<col><mnemonic>process</mnemonic></col>
<col><mnemonic>start</mnemonic></col>
<col><mnemonic>pid</mnemonic></col>
<col><mnemonic>cpu-total</mnemonic></col>
<col><mnemonic>thread-count</mnemonic></col>
<col><mnemonic>mach-port-count</mnemonic></col>
<col><mnemonic>memory-physical-footprint</mnemonic></col>
<col><mnemonic>memory-real-private</mnemonic></col>
<col><mnemonic>idle-wakeups</mnemonic></col>
<col><mnemonic>disk-bytes-written</mnemonic></col>
<col><mnemonic>disk-bytes-read</mnemonic></col>
</schema>
<row><process id="1" fmt="rasitop &amp; helper (42)"/><start-time>0</start-time><pid id="2">42</pid><duration-on-core>50000000</duration-on-core><event-count id="3">4</event-count><event-count id="4">100</event-count><size-in-bytes>1000</size-in-bytes><size-in-bytes>500</size-in-bytes><event-count>10</event-count><disk-size-in-bytes>100</disk-size-in-bytes><disk-size-in-bytes>200</disk-size-in-bytes></row>
<row><process ref="1"/><start-time>1000000000</start-time><pid ref="2"/><duration-on-core>100000000</duration-on-core><event-count>6</event-count><event-count>102</event-count><size-in-bytes>1100</size-in-bytes><size-in-bytes>550</size-in-bytes><event-count>12</event-count><disk-size-in-bytes>125</disk-size-in-bytes><disk-size-in-bytes>220</disk-size-in-bytes></row>
<row><process ref="1"/><start-time>2000000000</start-time><pid ref="2"/><duration-on-core>150000000</duration-on-core><event-count>5</event-count><event-count>99</event-count><size-in-bytes>900</size-in-bytes><size-in-bytes>600</size-in-bytes><event-count>14</event-count><disk-size-in-bytes>150</disk-size-in-bytes><disk-size-in-bytes>260</disk-size-in-bytes></row>
</node></trace-query-result>"""


class ActivitySummaryTests(unittest.TestCase):
    def test_resolves_refs_and_calculates_steady_state_deltas(self) -> None:
        summary = summarize_activity_xml(XML)

        self.assertEqual(summary["process"]["name"], "rasitop & helper")
        self.assertEqual(summary["process"]["pid"], 42)
        self.assertEqual(summary["measurement"]["samples"], 3)
        self.assertEqual(summary["measurement"]["duration_ns"], 2_000_000_000)
        self.assertEqual(summary["cpu"]["time_delta_ns"], 100_000_000)
        self.assertEqual(summary["cpu"]["average_percent"], 5.0)
        self.assertEqual(summary["idle_wakeups"]["delta"], 4)
        self.assertEqual(summary["idle_wakeups"]["per_second"], 2.0)
        self.assertEqual(
            summary["memory"]["physical_footprint_bytes"]["delta"],
            -100,
        )
        self.assertEqual(
            summary["memory"]["physical_footprint_bytes"]["max"],
            1100,
        )
        self.assertEqual(summary["memory"]["private_bytes"]["delta"], 100)
        self.assertEqual(summary["threads"]["min"], 4)
        self.assertEqual(summary["threads"]["max"], 6)
        self.assertEqual(summary["ports"]["delta"], -1)
        self.assertEqual(summary["disk_io"]["bytes_written"]["delta"], 50)
        self.assertEqual(summary["disk_io"]["bytes_read"]["per_second"], 30.0)

    def test_rejects_decreasing_counters(self) -> None:
        xml = XML.replace(
            "<duration-on-core>150000000</duration-on-core>",
            "<duration-on-core>40000000</duration-on-core>",
        )
        with self.assertRaisesRegex(
            ActivitySummaryError,
            "CPU time counter decreased",
        ):
            summarize_activity_xml(xml)

    def test_rejects_unknown_refs(self) -> None:
        xml = XML.replace('<pid ref="2"/>', '<pid ref="missing"/>')
        with self.assertRaisesRegex(ActivitySummaryError, "unknown id missing"):
            summarize_activity_xml(xml)


if __name__ == "__main__":
    unittest.main()
