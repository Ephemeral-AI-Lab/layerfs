#!/usr/bin/env python3
"""`unittest` entry points for `trace.py`, wrapping its `self_check`.

The runner must not trust the collector's own summary, so the reader re-derives
three things from the raw JSONL: that the file is **flat**, that the sequence
numbers are **contiguous**, and that the row status follows the same worst-gate
severity order the Rust side uses. Each of those is tested in both directions —
what it accepts and what it must refuse — because a reader that accepts everything
proves nothing about the traces it is handed.
"""

from __future__ import annotations

import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import trace


def record(
    seq: int,
    kind: str = "gate",
    key: str = "g1",
    value: object = "PASS|1|1",
    basis: str = "G1",
    unit: str = "",
) -> dict:
    """One flat `layerfs-trace-v1` line."""
    return {
        "schema": trace.SCHEMA,
        "seq": seq,
        "kind": kind,
        "key": key,
        "value": value,
        "unit": unit,
        "basis": basis,
    }


def write(path: Path, rows: list[dict]) -> Path:
    path.write_text("".join(json.dumps(row) + "\n" for row in rows), encoding="utf-8")
    return path


class SelfCheckTest(unittest.TestCase):
    def test_the_module_self_check_is_clean(self) -> None:
        self.assertEqual(trace.self_check(), [])


class FlatnessTest(unittest.TestCase):
    def test_a_flat_record_is_accepted(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = write(Path(directory) / "trace.jsonl", [record(0)])
            self.assertEqual(trace.read(path).defects, [])

    def test_a_nested_object_is_refused(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = write(Path(directory) / "trace.jsonl", [record(0, value={"nested": True})])
            defects = trace.read(path).defects
            self.assertTrue(any("flat" in defect for defect in defects), defects)

    def test_a_nested_list_is_refused(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = write(Path(directory) / "trace.jsonl", [record(0, value=[1, 2])])
            defects = trace.read(path).defects
            self.assertTrue(any("flat" in defect for defect in defects), defects)

    def test_a_wrong_schema_is_refused(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            row = record(0)
            row["schema"] = "layerfs-trace-v0"
            path = write(Path(directory) / "trace.jsonl", [row])
            defects = trace.read(path).defects
            self.assertTrue(any("schema" in defect for defect in defects), defects)

    def test_a_non_json_line_is_refused(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "trace.jsonl"
            path.write_text("{not json}\n", encoding="utf-8")
            self.assertTrue(trace.read(path).defects)

    def test_a_json_array_is_not_a_record(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "trace.jsonl"
            path.write_text("[1, 2, 3]\n", encoding="utf-8")
            defects = trace.read(path).defects
            self.assertTrue(any("not a JSON object" in defect for defect in defects), defects)

    def test_a_missing_trace_is_a_defect_not_an_empty_pass(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            reading = trace.read(Path(directory) / "absent.jsonl")
            self.assertEqual(reading.records, [])
            self.assertTrue(reading.defects)
            self.assertEqual(reading.status(), "NOT_RUN")


class SequenceTest(unittest.TestCase):
    def test_contiguous_sequence_numbers_are_accepted(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = write(
                Path(directory) / "trace.jsonl",
                [record(0, kind="run"), record(1), record(2, key="g2")],
            )
            self.assertEqual(trace.read(path).defects, [])

    def test_a_repeated_sequence_number_is_refused(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = write(Path(directory) / "trace.jsonl", [record(0), record(0)])
            defects = trace.read(path).defects
            self.assertTrue(any("seq" in defect for defect in defects), defects)

    def test_a_gap_in_the_sequence_is_refused(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = write(Path(directory) / "trace.jsonl", [record(0), record(5)])
            defects = trace.read(path).defects
            self.assertTrue(any("seq" in defect for defect in defects), defects)

    def test_a_non_integer_sequence_number_is_refused(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            row = record(0)
            row["seq"] = "zero"
            path = write(Path(directory) / "trace.jsonl", [row])
            defects = trace.read(path).defects
            self.assertTrue(any("seq" in defect for defect in defects), defects)


class AggregationTest(unittest.TestCase):
    def status_of(self, values: list[str]) -> str:
        with tempfile.TemporaryDirectory() as directory:
            rows = [record(0, kind="run", key="case_id", value="x")]
            for index, value in enumerate(values, start=1):
                rows.append(record(index, key=f"g{index}", value=value))
            return trace.read(write(Path(directory) / "trace.jsonl", rows)).status()

    def test_no_gates_is_not_run(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = write(Path(directory) / "trace.jsonl", [record(0, kind="run")])
            self.assertEqual(trace.read(path).status(), "NOT_RUN")

    def test_all_passing_gates_pass(self) -> None:
        self.assertEqual(self.status_of(["PASS|1|1", "PASS|2|2"]), "PASS")

    def test_the_worst_gate_wins(self) -> None:
        self.assertEqual(self.status_of(["PASS|1|1", "INELIGIBLE|2|0"]), "INELIGIBLE")
        self.assertEqual(self.status_of(["FAIL|1|0", "PASS|2|2"]), "FAIL")

    def test_not_run_outranks_target_miss(self) -> None:
        self.assertEqual(self.status_of(["TARGET_MISS|1|1", "NOT_RUN|0|1"]), "NOT_RUN")
        self.assertEqual(self.status_of(["NOT_RUN|0|1", "TARGET_MISS|1|1"]), "NOT_RUN")

    def test_incomplete_outranks_not_run_and_fail_outranks_incomplete(self) -> None:
        self.assertEqual(self.status_of(["NOT_RUN|0|1", "INCOMPLETE|-|1"]), "INCOMPLETE")
        self.assertEqual(self.status_of(["INCOMPLETE|-|1", "FAIL|1|0"]), "FAIL")

    def test_the_severity_order_matches_the_rust_side(self) -> None:
        # `gates.rs` orders PASS < TARGET_MISS < NOT_RUN < INELIGIBLE < INCOMPLETE
        # < FAIL. Two readers that disagree would make `verify` report a
        # disagreement that is really a transcription error.
        order = ["PASS", "TARGET_MISS", "NOT_RUN", "INELIGIBLE", "INCOMPLETE", "FAIL"]
        self.assertEqual([trace.SEVERITY[token] for token in order], sorted(trace.SEVERITY[token] for token in order))
        self.assertEqual(len(set(trace.SEVERITY[token] for token in order)), len(order))

    def test_an_unknown_gate_token_fails_closed(self) -> None:
        # `SEVERITY.get(status, 5)` treats an unrecognised token as maximally
        # severe, so a corrupted status can never read as a pass.
        self.assertEqual(self.status_of(["PASS|1|1", "WOBBLE|1|0"]), "WOBBLE")
        self.assertEqual(trace.SEVERITY.get("WOBBLE", 5), 5)

    def test_a_malformed_gate_value_is_a_defect_and_is_skipped(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = write(
                Path(directory) / "trace.jsonl",
                [record(0, kind="run"), record(1, key="g1", value="PASS"), record(2, key="g2", value="PASS|1|1")],
            )
            reading = trace.read(path)
            # The gate defects are found lazily, when the gates are parsed rather
            # than when the file is read, so `gates()` must run before `defects`
            # can be inspected.
            self.assertEqual([gate.identifier for gate in reading.gates()], ["g2"])
            self.assertTrue(any("status|measured|limit" in defect for defect in reading.defects))
            self.assertEqual(reading.status(), "PASS")


class AccessorTest(unittest.TestCase):
    def sample(self) -> trace.Trace:
        rows = [
            record(0, kind="run", key="case_id", value="x"),
            record(1, kind="run", key="lane", value="smoke"),
            record(2, kind="counter", key="chunks_emitted", value=12, unit="count", basis="counter"),
            record(3, kind="counter", key="objects", value=3, unit="count", basis="counter"),
            record(4, kind="resource", key="heap_peak_bytes", value=4096, unit="bytes", basis="heap"),
            record(5, kind="receipt", key="note", value="fresh output"),
            record(6, kind="window", key="timed", value="0|100", basis="monotonic-raw"),
        ]
        with tempfile.TemporaryDirectory() as directory:
            return trace.read(write(Path(directory) / "trace.jsonl", rows))

    def test_counters_and_resources_are_split_by_kind(self) -> None:
        reading = self.sample()
        self.assertEqual(reading.counters(), {"chunks_emitted": 12, "objects": 3})
        self.assertEqual(reading.resources(), {"heap_peak_bytes": 4096})

    def test_values_skips_notes_and_first_finds_by_key(self) -> None:
        reading = self.sample()
        self.assertEqual(reading.identity(), {"case_id": "x", "lane": "smoke"})
        self.assertEqual(reading.notes(), ["fresh output"])
        self.assertNotIn("note", reading.values("receipt"))
        self.assertEqual(reading.first("counter", "objects").value, 3)
        self.assertIsNone(reading.first("counter", "absent"))

    def test_windows_are_their_own_kind(self) -> None:
        self.assertEqual(len(self.sample().windows()), 1)


if __name__ == "__main__":
    unittest.main()
