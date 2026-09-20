#!/usr/bin/env python3
"""Re-derive this round's diagnostic runs with the runner's own functions.

Three questions, answered from the retained artifacts and nothing else:

1. Do the declared phases now account for the invocation? `phases.compose` and
   `reconcile_invocation` are the runner's own, imported rather than reimplemented.
2. What does the complete command classify as, under the ordinary and the declared
   -exception limits?
3. What did the corpus diagnostic read: pages resident before first read, and
   device bytes across the chain?
"""
import importlib.util
import json
import sys
from pathlib import Path

CAMPAIGN = Path(__file__).resolve().parent
REPO = Path("/Users/yifanxu/Ephemeral-AI-Lab/layerfs-190-qual")
HARNESS = REPO / "core/benchmark/fs-bench-pro-storage-content"
RETAINED = CAMPAIGN.parent / "stage-6-history-190-read-20260920T042617Z"
sys.path.insert(0, str(HARNESS / "shared"))
import phases as phases_module  # noqa: E402
import receipt  # noqa: E402

CASES = ("history-stride10", "history-stride3")
DIAGNOSTIC_KEYS = (
    "history.corpus_read_ns",
    "history.corpus.probe.files",
    "history.corpus.probe.bytes",
    "history.corpus.probe.pages",
    "history.corpus.probe.resident_pages",
    "history.operation.disk_read_bytes",
)


def records(path: Path):
    return [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines() if line.strip()]


def value(rows, key, kind):
    for row in rows:
        if row.get("kind") == kind and row.get("key") == key:
            return row.get("value")
    return None


def main() -> int:
    report = {"schema": "h190-corpus-phase-analysis-v1", "cases": {}}
    lines = []
    for case in CASES:
        run = CAMPAIGN / "runs" / f"diagnostic-{case}"
        raw = run / "raw"
        wall = int(json.loads((run / "perf-receipt.json").read_text())["wall_ns"])
        rows = records(raw / "trace.jsonl")
        composed = phases_module.compose(raw, {"perf": wall}, expects_operation=True)
        declared = sum(int(composed.get(f, 0) or 0) for f in (
            "preparation_wall_ns", "operation_ns", "verification_wall_ns", "cleanup_wall_ns"))
        ordinary = receipt.budget(wall, False, declared_ns=declared)
        exception = receipt.budget(wall, True, declared_ns=declared)
        reconciliation = composed["reconciliation"]
        diagnostics = {key: value(rows, key, "resource") for key in DIAGNOSTIC_KEYS}
        counts = {
            "root_ns": value(rows, "history.root_ns", "counter"),
            "children_ns": value(rows, "history.children_ns", "counter"),
            "operation_ns": composed["operation_ns"],
            "invocation_ns": composed["invocations"][0]["invocation_ns"],
        }
        retained = RETAINED / "runs" / f"candidate2-{case}" / "trace-perf.jsonl"
        retained_rows = records(retained) if retained.exists() else []
        retained_operation = value(retained_rows, "history.children_ns", "counter")
        entry = {
            "wall_ns": wall,
            "declared_ns": declared,
            "reconciliation": reconciliation,
            "budget": {
                "ordinary": {"status": ordinary.status, "limit_ns": ordinary.limit_ns},
                "declared_exception": {"status": exception.status, "limit_ns": exception.limit_ns},
            },
            "phases": {key: composed[key] for key in (
                "preparation_wall_ns", "operation_ns", "verification_wall_ns",
                "cleanup_wall_ns", "handoff_ns")},
            "counters": counts,
            "diagnostics": diagnostics,
            "retained_candidate2_operation_ns": retained_operation,
        }
        report["cases"][case] = entry

        lines.append(f"=== {case}")
        lines.append(f"  wall (complete command)               : {wall / 1e9:.3f} s")
        lines.append(f"  preparation (window + declared corpus): {composed['preparation_wall_ns'] / 1e9:.3f} s")
        lines.append(f"  operation (sum of children)           : {composed['operation_ns'] / 1e9:.3f} s")
        lines.append(f"  verification / cleanup                : {composed['verification_wall_ns'] / 1e9:.3f} / {composed['cleanup_wall_ns'] / 1e9:.6f} s")
        lines.append(f"  declared phases sum                   : {declared / 1e9:.3f} s")
        lines.append(f"  child invocation wall                 : {composed['invocations'][0]['invocation_ns'] / 1e9:.3f} s")
        lines.append(f"  reconciliation                        : {reconciliation['status']} - {reconciliation['reason']}")
        lines.append(f"  budget, ordinary 15 s                 : {ordinary.status}")
        lines.append(f"  budget, declared exception 25 s       : {exception.status}")
        lines.append(f"  corpus read (declared, own record)    : {diagnostics['history.corpus_read_ns'] / 1e9:.3f} s")
        lines.append(f"  corpus files probed before first read : {diagnostics['history.corpus.probe.files']:,}"
                     f" ({diagnostics['history.corpus.probe.bytes']:,} bytes,"
                     f" {diagnostics['history.corpus.probe.pages']:,} pages)")
        lines.append(f"  corpus pages resident before first rd : {diagnostics['history.corpus.probe.resident_pages']:,}")
        lines.append(f"  device bytes read across the chain    : {diagnostics['history.operation.disk_read_bytes']:,}")
        lines.append(f"  root_ns / children_ns                 : {counts['root_ns']:,} / {counts['children_ns']:,}")
        lines.append(f"  retained candidate2 operation         : {retained_operation:,}")
        lines.append("")
    (CAMPAIGN / "analysis.json").write_text(json.dumps(report, indent=2, sort_keys=False) + "\n")
    (CAMPAIGN / "analysis.txt").write_text("\n".join(lines) + "\n")
    print("\n".join(lines))
    return 0


if __name__ == "__main__":
    sys.exit(main())
