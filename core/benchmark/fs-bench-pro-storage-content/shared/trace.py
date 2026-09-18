#!/usr/bin/env python3
"""Reader and independent re-derivation for `layerfs-trace-v1`.

The runner must not trust the collector's own summary. This module re-reads the
raw JSONL a case wrote and recomputes:

* whether the file is genuinely **flat** (one JSON object per line, scalar values
  only) — a nested record would mean the writer had grown a shape the reader only
  thinks it understands;
* whether the sequence numbers are contiguous, so a truncated or interleaved file
  is caught rather than partially read;
* the row status, from the gate records alone, by the same severity order the Rust
  side uses.

What it deliberately does **not** do is re-run the product. A figure that only a
product call can produce (`sha256` of a read-back, a counter the crate reports) is
re-derived by cross-checking it against the artifact it names — the presence of
the digest, the store it describes, the gate it decided — never by inventing it.
"""

from __future__ import annotations

import json
import sys
from dataclasses import dataclass, field
from pathlib import Path

SCHEMA = "layerfs-trace-v1"

SEVERITY = {
    "PASS": 0,
    "TARGET_MISS": 1,
    "NOT_RUN": 2,
    "INELIGIBLE": 3,
    "INCOMPLETE": 4,
    "FAIL": 5,
}


class TraceError(Exception):
    """The trace is not a valid `layerfs-trace-v1` file."""


@dataclass(frozen=True)
class Record:
    """One flat record."""

    seq: int
    kind: str
    key: str
    value: object
    unit: str
    basis: str
    numeric: bool


@dataclass
class Gate:
    """One gate, as the child recorded it."""

    identifier: str
    status: str
    measured: str
    limit: str
    gate_class: str


@dataclass
class Trace:
    """A parsed trace, with the counts a receipt needs."""

    path: str
    records: list[Record] = field(default_factory=list)
    defects: list[str] = field(default_factory=list)

    def of_kind(self, kind: str) -> list[Record]:
        return [record for record in self.records if record.kind == kind]

    def first(self, kind: str, key: str) -> Record | None:
        for record in self.records:
            if record.kind == kind and record.key == key:
                return record
        return None

    def values(self, kind: str) -> dict[str, object]:
        out: dict[str, object] = {}
        for record in self.records:
            if record.kind == kind and record.key != "note":
                out.setdefault(record.key, record.value)
        return out

    def counters(self) -> dict[str, int]:
        out: dict[str, int] = {}
        for record in self.of_kind("counter"):
            if isinstance(record.value, int):
                out[record.key] = record.value
        return out

    def resources(self) -> dict[str, int]:
        out: dict[str, int] = {}
        for record in self.of_kind("resource"):
            if isinstance(record.value, int):
                out[record.key] = record.value
        return out

    def notes(self) -> list[str]:
        return [
            str(record.value)
            for record in self.of_kind("receipt")
            if record.key == "note"
        ]

    def identity(self) -> dict[str, object]:
        return {
            record.key: record.value
            for record in self.of_kind("run")
        }

    def gates(self) -> list[Gate]:
        out = []
        for record in self.of_kind("gate"):
            parts = str(record.value).split("|")
            if len(parts) != 3:
                self.defects.append(
                    f"gate {record.key!r} is not 'status|measured|limit': {record.value!r}"
                )
                continue
            out.append(
                Gate(
                    identifier=record.key,
                    status=parts[0],
                    measured=parts[1],
                    limit=parts[2],
                    gate_class=record.basis,
                )
            )
        return out

    def status(self) -> str:
        """The row status, re-derived from the gate records alone."""
        gates = self.gates()
        if not gates:
            return "NOT_RUN"
        worst = "PASS"
        for gate in gates:
            if SEVERITY.get(gate.status, 5) > SEVERITY[worst]:
                worst = gate.status
        return worst

    def windows(self) -> list[Record]:
        return self.of_kind("window")


def read(path: str | Path) -> Trace:
    """Parses one trace file, recording every structural defect it finds."""
    path = Path(path)
    trace = Trace(path=str(path))
    if not path.exists():
        trace.defects.append("trace file does not exist")
        return trace
    expected_seq = 0
    with path.open("r", encoding="utf-8") as handle:
        for number, line in enumerate(handle, start=1):
            line = line.strip()
            if not line:
                continue
            try:
                document = json.loads(line)
            except json.JSONDecodeError as error:
                trace.defects.append(f"line {number}: not JSON: {error}")
                continue
            if not isinstance(document, dict):
                trace.defects.append(f"line {number}: not a JSON object")
                continue
            if document.get("schema") != SCHEMA:
                trace.defects.append(
                    f"line {number}: schema {document.get('schema')!r} is not {SCHEMA!r}"
                )
                continue
            for key, value in document.items():
                if isinstance(value, (dict, list)):
                    trace.defects.append(
                        f"line {number}: key {key!r} holds a nested value; the trace must be flat"
                    )
            seq = document.get("seq")
            if seq != expected_seq:
                trace.defects.append(
                    f"line {number}: seq {seq!r} where {expected_seq} was expected"
                )
                expected_seq = (seq if isinstance(seq, int) else expected_seq) + 1
            else:
                expected_seq += 1
            trace.records.append(
                Record(
                    seq=seq if isinstance(seq, int) else -1,
                    kind=str(document.get("kind", "")),
                    key=str(document.get("key", "")),
                    value=document.get("value"),
                    unit=str(document.get("unit", "")),
                    basis=str(document.get("basis", "")),
                    numeric=bool(document.get("numeric", False)),
                )
            )
    return trace


def self_check() -> list[str]:
    """Proves the reader rejects what it claims to reject."""
    import tempfile

    failures: list[str] = []
    good = [
        {"schema": SCHEMA, "seq": 0, "kind": "run", "key": "case_id", "value": "x", "unit": "", "basis": ""},
        {"schema": SCHEMA, "seq": 1, "kind": "gate", "key": "g1", "value": "PASS|1|1", "unit": "", "basis": "G1"},
    ]
    with tempfile.TemporaryDirectory() as directory:
        path = Path(directory) / "trace.jsonl"
        path.write_text("".join(json.dumps(row) + "\n" for row in good))
        trace = read(path)
        if trace.defects:
            failures.append(f"a valid trace reported defects: {trace.defects}")
        if trace.status() != "PASS":
            failures.append(f"status re-derived as {trace.status()}, expected PASS")
        nested = dict(good[0])
        nested["value"] = {"nested": True}
        path.write_text(json.dumps(nested) + "\n")
        if not read(path).defects:
            failures.append("a nested record was accepted as flat")
        path.write_text(json.dumps(good[0]) + "\n" + json.dumps(good[0]) + "\n")
        if not read(path).defects:
            failures.append("a repeated sequence number was accepted")
        failing = dict(good[1])
        failing["value"] = "INELIGIBLE|2 resident pages|== 0"
        path.write_text(json.dumps(good[0]) + "\n" + json.dumps(failing) + "\n")
        trace = read(path)
        if trace.status() != "INELIGIBLE":
            failures.append(f"worst-gate aggregation produced {trace.status()}")
        mixed = dict(good[1])
        mixed["value"] = "PASS|ok|ok"
        path.write_text(
            json.dumps(good[0])
            + "\n"
            + json.dumps(mixed)
            + "\n"
            + json.dumps({**good[1], "seq": 2, "value": "FAIL|1|0"})
            + "\n"
        )
        if read(path).status() != "FAIL":
            failures.append("FAIL did not outrank PASS in the aggregation")
    return failures


def main() -> int:
    failures = self_check()
    for failure in failures:
        print(f"trace: FAIL: {failure}", file=sys.stderr)
    if failures:
        return 1
    print("trace: PASS (flatness, sequence and worst-gate aggregation re-derived)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
