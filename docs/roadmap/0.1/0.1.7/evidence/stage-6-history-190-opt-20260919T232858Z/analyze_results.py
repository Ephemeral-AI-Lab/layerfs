#!/usr/bin/env python3
"""Reproduce #190 diagnostic totals and semantic comparisons from retained raw files."""
import importlib.util
import json
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parent
analyzer = ROOT.parent / "stage-6-history-190-20260919T225614Z/squad-s1/analyze_spans.py"
spec = importlib.util.spec_from_file_location("history_spans", analyzer)
spans = importlib.util.module_from_spec(spec)
spec.loader.exec_module(spans)


def read_arm(arm, case):
    run = ROOT / "runs" / f"{arm}-{case}"
    raw = run / "raw"
    timing = json.loads((raw / "timing.json").read_text())
    trace_rows = [json.loads(line) for line in (run / "trace-perf.jsonl").read_text().splitlines()]
    counters = {row["key"]: row["value"] for row in trace_rows}
    phases = json.loads((raw / "phases-perf.json").read_text())
    exclusive = defaultdict(int)
    states = []
    for state in timing["children"]:
        parts = {}
        for child in state["children"]:
            for key, value in spans.exclusive(child).items():
                assert key not in parts
                parts[key] = value
        parts["state.residual"] = state["elapsed_ns"] - sum(parts.values())
        assert all(value >= 0 for value in parts.values())
        assert sum(parts.values()) == state["elapsed_ns"]
        for key, value in parts.items():
            exclusive[key] += value
        prefix = state["name"] + "."
        state_work = {key[len(prefix):]: value for key, value in counters.items()
                      if key.startswith(prefix) and key[len(prefix):].startswith(
                          ("filesystem.", "content.predecessor_provider.", "save."))}
        states.append({"state": state["name"], "operation_ns": state["elapsed_ns"],
                       "exclusive_ns": parts, "work": state_work})
    operation = sum(state["operation_ns"] for state in states)
    assert operation == phases["operation_ns"] == sum(exclusive.values())
    work = defaultdict(int)
    for row in trace_rows:
        if row["key"].startswith("history.state.") and row.get("numeric"):
            suffix = row["key"].split(".", 3)[3]
            if suffix.startswith(("filesystem.", "content.predecessor_provider.", "save.")):
                work[suffix] += row["value"]
    verification = json.loads((raw / "phases-verify.json").read_text())
    all_rows = [json.loads(line) for line in (raw / "trace.jsonl").read_text().splitlines()]
    verified = {row["key"]: row["value"] for row in all_rows if row["key"].startswith("verify.")}
    target = 10_000_000_000 if case.endswith("10") else 20_000_000_000
    storage_target = 49_344_512 if case.endswith("10") else 64_024_576
    return {"operation_ns": operation, "root_ns": timing["elapsed_ns"],
            "exclusive_ns": dict(exclusive), "work": dict(work), "states": states,
            "phase_receipt": phases,
            "complete_wall_ns": json.loads((run / "perf-receipt.json").read_text())["wall_ns"],
            "store_sha256": json.loads((run / "perf-receipt.json").read_text())["artifacts"]["sample.sqlite"],
            "storage": {"apparent_bytes": counters["space.apparent_bytes"],
                        "allocated_bytes": counters["space.allocated_bytes"],
                        "allocated_limit_bytes": storage_target,
                        "allocated_comparison": "PASS" if counters["space.allocated_bytes"] < storage_target else "FAIL"},
            "roots": {key: value for key, value in counters.items() if key.startswith("history.state.") and key.endswith(".root")},
            "save_counters": {key: value for key, value in counters.items() if key.startswith("delta.")},
            "verification": {"work_ns": verification["verification_ns"], "target_ns": target,
                             "target_status": "PASS" if verification["verification_ns"] <= target else "TARGET_MISS",
                             "counters": verified},
            "nonpassing_trace_gates": [{"key": row["key"], "value": row["value"]}
                                       for row in all_rows if row["kind"] == "gate" and not str(row["value"]).startswith("PASS|")]}


def main():
    spans.self_test()
    cases = {}
    for case in ("history-stride10", "history-stride3"):
        baseline, candidate = read_arm("baseline", case), read_arm("candidate", case)
        assert len(baseline["roots"]) == len(candidate["roots"]) == (17 if case.endswith("10") else 53)
        cases[case] = {"baseline": baseline, "candidate": candidate,
                       "roots_equal": baseline["roots"] == candidate["roots"],
                       "save_counters_equal": baseline["save_counters"] == candidate["save_counters"],
                       "store_bytes_equal": baseline["store_sha256"] == candidate["store_sha256"],
                       "operation_delta_ns": candidate["operation_ns"] - baseline["operation_ns"],
                       "work_delta": {key: candidate["work"][key] - value for key, value in baseline["work"].items()}}
    print(json.dumps({"schema": "h190-derived-comparison-v1", "admission_eligible": False,
                      "timing_scope": "one instrumented diagnostic per arm/case; uncontrolled OS cache",
                      "cases": cases}, indent=2))


if __name__ == "__main__":
    main()
