#!/usr/bin/env python3
"""Reproduce every derived number of the confirmation window from the raw files.

Arm convention, identical to the round that kept the treatment: the
`LAYERFS_STORAGE_POLICY_REASSERT` lever **unset** is the control (the per-step
re-assertion the treatment removed) and `=0` is the treatment.
"""
import hashlib
import json
from pathlib import Path

D = Path(__file__).resolve().parent
ROWS = [("control", "confirm-1a"), ("control", "confirm-4a"),
        ("treatment", "confirm-2b"), ("treatment", "confirm-3b"),
        ("treatment", "shipped-confirm")]
PREV = D.parent / "stage-6-history-209-commit-20260920T194819Z"


def load(root, label):
    raw = root / "runs" / f"{label}-history-stride10" / "raw"
    timing = json.load(open(raw / "timing.json"))
    trace = {}
    for line in open(raw / "trace.jsonl"):
        record = json.loads(line)
        trace.setdefault(record["key"], record["value"])
    span = {}
    for state in timing["children"]:
        for child in state["children"]:
            span[child["name"]] = span.get(child["name"], 0) + child["elapsed_ns"]
    declaration = json.load(open(root / "runs" / f"{label}-history-stride10"
                                  / "perf-declaration.json"))
    return {
        "operation_ns": sum(child["elapsed_ns"] for child in timing["children"]),
        "commit_ns": trace["delta.profile_commit_ns"],
        "resolve_ns": trace["delta.profile_resolve_ns"],
        "sql_ns": trace["delta.profile_sql_ns"],
        "appends": trace["delta.pack_appends"],
        "filesystem_ns": span.get("filesystem", 0),
        "commits": sum(value for key, value in trace.items() if key.endswith(".save.commits")),
        "store": hashlib.sha256((raw / "sample.sqlite").read_bytes()).hexdigest(),
        "extra_environment": declaration["extra_environment"],
        "binary": declaration["binary_sha256"],
    }


def main():
    rows = [(arm, label, load(D, label)) for arm, label in ROWS]
    print(f"{'row':<18}{'arm':<11}{'operation':>11}{'commit_ns':>11}{'per append':>12}"
          f"{'resolve_ns':>12}{'filesystem':>12}{'store':>18}")
    for arm, label, row in rows:
        print(f"{label:<18}{arm:<11}{row['operation_ns'] / 1e9:>10.3f}s"
              f"{row['commit_ns'] / 1e9:>10.3f}s"
              f"{row['commit_ns'] / row['appends'] / 1000:>11.2f}u"
              f"{row['resolve_ns'] / 1e9:>11.3f}s{row['filesystem_ns'] / 1e9:>11.3f}s"
              f"{row['store'][:16]:>18}")
    for arm in ("control", "treatment"):
        chosen = [row for name, _, row in rows if name == arm]
        for term in ("operation_ns", "commit_ns", "resolve_ns", "sql_ns"):
            values = [row[term] / 1e9 for row in chosen]
            print(f"  {arm:<10}{term:<14}{[f'{v:.3f}' for v in values]}")
    control = [row for name, _, row in rows if name == "control"]
    treated = [row for name, _, row in rows if name == "treatment"]
    for term in ("operation_ns", "commit_ns", "resolve_ns", "sql_ns"):
        a = sum(row[term] for row in control) / len(control)
        b = sum(row[term] for row in treated) / len(treated)
        print(f"{term:<14} control {a / 1e9:>8.3f}s  treatment {b / 1e9:>8.3f}s  "
              f"delta {(b - a) / 1e9:>+7.3f}s  ({(b / a - 1) * 100:>+6.2f} %)")
    print(f"\nprevious window, same contrast: commit_ns 1.884 -> 1.773 s (-0.111 s, -5.90 %), "
          f"operation 16.754 -> 16.517 s (-0.237 s)")
    print(f"stores: {sorted({row['store'][:16] for _, _, row in rows})}; "
          f"commits: {sorted({row['commits'] for _, _, row in rows})}; "
          f"binaries: {sorted({row['binary'][:16] for _, _, row in rows})}")
    print("extra_environment:",
          {label: row['extra_environment'] for _, label, row in rows})


if __name__ == "__main__":
    main()
