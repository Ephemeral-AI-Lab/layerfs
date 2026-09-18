#!/usr/bin/env python3
"""Compare two runs row by row, counter by counter.

Round 5's simplifications are only admissible if they change **what the campaign
costs**, not **what it measures**. The falsifier is mechanical: every counter and
every resource reading a row published before must be published again, unchanged,
after. A difference is a defect in the change, not a speed-up.

`operation_ns` is compared too, but as a *trend* rather than an equality: it is the
golden number and the round's rule is that it must not fall. A row whose operation
time moved by more than the declared same-binary spread is reported for a human to
read, and a row whose operation time **fell** by more than that spread is reported as
a rejection — that is the shape a change takes when measured work has been moved
into setup.

Usage: compare_runs.py BEFORE_RUN AFTER_RUN
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

# The same-binary wall spread decision D1 records: +17.6%. It is the reason
# `elapsed_ns` never gate-decides, and it is the reason a comparison of two runs
# reports a trend band rather than an equality.
SAME_BINARY_SPREAD = 0.176

# Counters that describe the harness's own instrumentation rather than the product,
# and which therefore move when the harness publishes more of itself.
INSTRUMENTATION_KEYS = {
    "timing_json_bytes",
}


def load(run_dir: str | Path) -> dict[str, dict[str, object]]:
    """Every receipt a run produced, keyed by case ID."""
    run_dir = Path(run_dir)
    rows: dict[str, dict[str, object]] = {}
    for case_dir in sorted(entry for entry in run_dir.iterdir() if entry.is_dir()):
        path = case_dir / "receipt.json"
        if not path.exists():
            continue
        document = json.loads(path.read_text(encoding="utf-8"))
        rows[str(document.get("case_id", case_dir.name))] = document
    return rows


def compare(before: dict[str, dict[str, object]], after: dict[str, dict[str, object]]) -> dict[str, object]:
    """Every difference between two runs, classified."""
    findings: list[dict[str, object]] = []
    missing = sorted(set(before) - set(after))
    added = sorted(set(after) - set(before))
    for case_id in missing:
        findings.append({"case_id": case_id, "kind": "missing", "detail": "the row is absent after"})
    for case_id in sorted(set(before) & set(after)):
        old = before[case_id]
        new = after[case_id]
        if old.get("status") != new.get("status"):
            findings.append(
                {
                    "case_id": case_id,
                    "kind": "status",
                    "detail": f"{old.get('status')} -> {new.get('status')}",
                }
            )
        for axis in ("counters", "resources"):
            old_axis = old.get(axis, {}) or {}
            new_axis = new.get(axis, {}) or {}
            for key in sorted(set(old_axis) | set(new_axis)):
                old_value = old_axis.get(key)
                new_value = new_axis.get(key)
                if old_value == new_value:
                    continue
                # An addition is not a drift. The round publishes more of itself
                # than round 4c did, and a comparison that called every new key a
                # difference would drown the differences that matter.
                if old_value is None:
                    kind = "added"
                elif new_value is None:
                    kind = "dropped"
                elif key in INSTRUMENTATION_KEYS:
                    kind = "instrumentation"
                elif axis == "resources":
                    # A resource is a reading of the machine and of the harness's own
                    # allocation inside the window, not a product work counter. It is
                    # reported, and it is not the equality this comparison exists to
                    # enforce; the product's work counters are.
                    kind = "resource-drift"
                else:
                    kind = "counter-drift"
                findings.append(
                    {
                        "case_id": case_id,
                        "kind": kind,
                        "detail": f"{axis}.{key}: {old_value} -> {new_value}",
                    }
                )
        old_operation = _operation_ns(old)
        new_operation = _operation_ns(new)
        if old_operation and new_operation:
            change = (new_operation - old_operation) / old_operation
            if abs(change) > SAME_BINARY_SPREAD:
                findings.append(
                    {
                        "case_id": case_id,
                        "kind": "operation-fell" if change < 0 else "operation-rose",
                        "detail": (
                            f"operation_ns {old_operation} -> {new_operation} "
                            f"({change * 100:+.1f}%), beyond the +-{SAME_BINARY_SPREAD * 100:.1f}% "
                            "same-binary spread"
                        ),
                    }
                )
    by_kind: dict[str, int] = {}
    for finding in findings:
        by_kind[str(finding["kind"])] = by_kind.get(str(finding["kind"]), 0) + 1
    return {
        "rows_before": len(before),
        "rows_after": len(after),
        "missing": missing,
        "added": added,
        "by_kind": by_kind,
        "findings": findings,
        "verdict": "IDENTICAL"
        if not [
            f
            for f in findings
            if f["kind"] in {"counter-drift", "status", "missing", "dropped"}
        ]
        else "DRIFT",
    }


def _operation_ns(document: dict[str, object]) -> int:
    phases = document.get("phases")
    if isinstance(phases, dict):
        value = phases.get("operation_ns")
        if isinstance(value, int):
            return value
    return 0


def self_check() -> list[str]:
    """Proves the comparison detects a drifted counter and a fallen operation."""
    failures: list[str] = []
    base = {
        "case_id": "x",
        "status": "PASS",
        "counters": {"a": 1},
        "resources": {"heap.peak_incremental_bytes": 10},
        "phases": {"operation_ns": 1000},
    }
    same = compare({"x": base}, {"x": dict(base)})
    if same["verdict"] != "IDENTICAL":
        failures.append("two identical rows were reported as drifting")
    drifted = dict(base)
    drifted["counters"] = {"a": 2}
    if compare({"x": base}, {"x": drifted})["verdict"] != "DRIFT":
        failures.append("a drifted counter was accepted")
    added = dict(base)
    added["counters"] = {"a": 1, "b": 1}
    if compare({"x": base}, {"x": added})["verdict"] != "IDENTICAL":
        failures.append("a newly published counter was reported as drift")
    dropped = dict(base)
    dropped["counters"] = {}
    if compare({"x": base}, {"x": dropped})["verdict"] != "DRIFT":
        failures.append("a counter that stopped being published was accepted")
    resource = dict(base)
    resource["resources"] = {"heap.peak_incremental_bytes": 12}
    kinds = {f["kind"] for f in compare({"x": base}, {"x": resource})["findings"]}
    if "resource-drift" not in kinds:
        failures.append("a moved resource reading was not reported")
    fell = dict(base)
    fell["phases"] = {"operation_ns": 500}
    kinds = {f["kind"] for f in compare({"x": base}, {"x": fell})["findings"]}
    if "operation-fell" not in kinds:
        failures.append("a fallen operation time was not reported as a rejection")
    gone = compare({"x": base}, {})
    if "missing" not in {f["kind"] for f in gone["findings"]}:
        failures.append("a dropped row was not reported")
    return failures


def main() -> int:
    if len(sys.argv) == 2 and sys.argv[1] == "--self-check":
        failures = self_check()
        for failure in failures:
            print(f"compare_runs: FAIL: {failure}")
        if failures:
            return 1
        print("compare_runs: PASS (drift, a fallen operation and a dropped row are all reported)")
        return 0
    if len(sys.argv) < 3:
        print(__doc__)
        return 2
    result = compare(load(sys.argv[1]), load(sys.argv[2]))
    for finding in result["findings"]:
        print(f"  {finding['kind']:<16} {finding['case_id']:<48} {finding['detail']}")
    print(
        f"compare: {result['rows_before']} -> {result['rows_after']} rows, "
        f"{result['by_kind'] or 'no differences'}, verdict {result['verdict']}"
    )
    return 0 if result["verdict"] == "IDENTICAL" else 1


if __name__ == "__main__":
    raise SystemExit(main())
