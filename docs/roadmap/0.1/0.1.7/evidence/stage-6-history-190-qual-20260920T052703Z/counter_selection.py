#!/usr/bin/env python3
"""What an O3 pin set for the `history.*` rows would contain, and which parts of it
can carry an invariance argument.

Read-only over evidence that already exists: the four retained performance traces
of `stage-6-history-190-read-20260920T042617Z` and the frozen 217-row table
`tests/golden/expected.tsv`. This script measures; it does not pin. Nothing here
writes to the golden table, and no value is proposed for freezing.

Screens, in the order the disposition states them:

  R1  pinnable at all. `main.rs::pinned_gates` collects only records with
      `kind == "counter"` and a numeric value, so a `resource`-kind record can
      never be gated by O3 however structural it looks.
  R2  not a duration. A counter whose name ends `_ns`, or whose basis names a
      time/elapsed quantity, is not invariant across machines by construction.
  R3  invariant under the retained optimization. The pair baseline2/candidate2
      differs only by the recorded instrumentation patch, which is present in
      both arms (README); the treatment is the only legitimate product change
      between them. A counter that moved is a counter a legitimate change moved.
  R4  precedent. Whether the frozen 217-row table already pins a counter of the
      same family-local name, i.e. whether the tree already treats this kind of
      quantity as structural enough to freeze.

Family-local name: `history.state.<N>.filesystem.references.rows_read` ->
`filesystem.references.rows_read`, matching how the 217 table labels keys.
"""

import json
import re
import sys
from collections import Counter
from pathlib import Path

EVIDENCE = Path(__file__).resolve().parent
CAMPAIGN = EVIDENCE.parent / "stage-6-history-190-read-20260920T042617Z"
# The harness root is resolved from this file's own depth in the repository.
HARNESS = Path(__file__).resolve().parents[6] / "core/benchmark/fs-bench-pro-storage-content"
PIN_TABLE = HARNESS / "tests" / "golden" / "expected.tsv"

ARMS = ("baseline2", "candidate2")
CASES = ("history-stride10", "history-stride3")
STATE = re.compile(r"^history\.state\.\d+\.")


def numeric_counters(path: Path) -> dict[str, dict[str, object]]:
    """Every `kind == "counter"` record with an integer value, with its metadata."""
    out: dict[str, dict[str, object]] = {}
    for line in path.read_text(encoding="utf-8").splitlines():
        if not line.strip():
            continue
        record = json.loads(line)
        if record.get("kind") != "counter":
            continue
        value = record.get("value")
        if isinstance(value, bool) or not isinstance(value, int):
            continue
        out[record["key"]] = {
            "value": value,
            "unit": record.get("unit", ""),
            "basis": record.get("basis", ""),
        }
    return out


def pinned_key_names() -> set[str]:
    """The family-local counter names the frozen 217-row table pins."""
    names: set[str] = set()
    for line in PIN_TABLE.read_text(encoding="utf-8").splitlines():
        if not line or line.startswith("#"):
            continue
        _, label, _ = line.split("\t")
        if label.startswith("counter:"):
            names.add(label[len("counter:"):])
    return names


def family_local(key: str) -> str:
    return STATE.sub("", key)


def classify(name: str, basis: str) -> str:
    if name.endswith("_ns") or "ns" in basis.split() or "elapsed" in basis:
        return "duration"
    return "candidate"


def main() -> int:
    precedent = pinned_key_names()
    report: dict[str, object] = {
        "campaign": CAMPAIGN.name,
        "pin_table": str(PIN_TABLE.relative_to(HARNESS.parents[2])),
        "precedent_keys": len(precedent),
        "cases": {},
    }
    for case in CASES:
        traces = {arm: numeric_counters(CAMPAIGN / "runs" / f"{arm}-{case}" / "trace-perf.jsonl")
                  for arm in ARMS}
        base, cand = traces["baseline2"], traces["candidate2"]
        keys = sorted(set(base) | set(cand))
        moved = [k for k in keys if base[k]["value"] != cand[k]["value"]]
        durations = [k for k in keys if classify(family_local(k), str(cand[k]["basis"])) == "duration"]
        invariant = [k for k in keys if k not in moved and k not in durations]
        with_precedent = [k for k in invariant if family_local(k) in precedent]
        local_names = sorted({family_local(k) for k in keys})
        entry = {
            "published_numeric_counters": len(keys),
            "distinct_family_local_names": len(local_names),
            "moved_under_the_retained_optimization": [
                {"key": k, "baseline": base[k]["value"], "candidate": cand[k]["value"],
                 "unit": cand[k]["unit"], "basis": cand[k]["basis"]} for k in moved
            ],
            "durations_excluded_by_r2": len(durations),
            "invariant_candidates_r3": len(invariant),
            "invariant_with_217_precedent_r4": len(with_precedent),
            "invariant_without_precedent": sorted(
                {family_local(k) for k in invariant} - precedent
            ),
            "families": dict(sorted(Counter(
                re.sub(r"\.\d+\.", ".N.", k).split(".")[1] if "." in k else k for k in keys
            ).items(), key=lambda kv: -kv[1])),
            "per_state_name_template_example": local_names[:1],
        }
        report["cases"][case] = entry
    (EVIDENCE / "counter-selection.json").write_text(
        json.dumps(report, indent=2, sort_keys=False) + "\n", encoding="utf-8"
    )

    text = []
    for case, entry in report["cases"].items():
        text.append(f"=== {case}")
        text.append(f"  published numeric counters (kind=counter, integer) : {entry['published_numeric_counters']}")
        text.append(f"  distinct family-local counter names                  : {entry['distinct_family_local_names']}")
        text.append(f"  moved under the retained optimization (R3 fails)    : {len(entry['moved_under_the_retained_optimization'])}")
        for item in entry["moved_under_the_retained_optimization"]:
            text.append(f"      {item['key']}: {item['baseline']} -> {item['candidate']} {item['unit']!r} ({item['basis']})")
        text.append(f"  excluded as durations (R2)                           : {entry['durations_excluded_by_r2']}")
        text.append(f"  invariant candidates (R1+R2+R3)                      : {entry['invariant_candidates_r3']}")
        text.append(f"  ... of which the 217 table already pins the name (R4) : {entry['invariant_with_217_precedent_r4']}")
        text.append(f"  invariant names with no 217 precedent                 : {len(entry['invariant_without_precedent'])}")
        for name in entry["invariant_without_precedent"]:
            text.append(f"      {name}")
    text.append("")
    text.append("=== every published counter name, once, with its metadata")
    text.append(f"{'name':52s} {'unit':12s} r4 {'moved':5s} basis")
    names = sorted({family_local(k) for k in numeric_counters(
        CAMPAIGN / "runs" / "candidate2-history-stride10" / "trace-perf.jsonl")})
    cand = numeric_counters(CAMPAIGN / "runs" / "candidate2-history-stride10" / "trace-perf.jsonl")
    by_local = {family_local(k): k for k in cand}
    for name in names:
        meta = cand[by_local[name]]
        moved = any(family_local(k) == name for k in
                    report["cases"]["history-stride10"]["moved_under_the_retained_optimization"]
                    and [m["key"] for m in report["cases"]["history-stride10"]["moved_under_the_retained_optimization"]]
                    or [])
        moved = any(m["key"] and family_local(m["key"]) == name
                    for m in report["cases"]["history-stride10"]["moved_under_the_retained_optimization"])
        text.append(f"{name:52s} {str(meta['unit']):12s} "
                    f"{'yes' if name in precedent else 'no ':3s} "
                    f"{'yes' if moved else 'no ':5s} {meta['basis']}")
    row_level = [k for k in cand if not STATE.match(k)]
    per_state = len({family_local(k) for k in cand if STATE.match(k)})
    text.append("")
    text.append("=== pin-table size this would add")
    text.append(f"  history-stride10: {len(cand)} lines ({len(row_level)} row-level + 17 states x {per_state} names)")
    text.append(f"  history-stride3 : 1238 lines (measured; 53 states)")
    text.append(f"  history-stride1 : NOT_MEASURED; the same per-state code path at 157 states")
    text.append(f"                    projects ~{len(row_level) + 157 * per_state} lines")
    text.append("")
    text.append(f"217-table pinned counter names: {report['precedent_keys']}")
    (EVIDENCE / "counter-selection.txt").write_text("\n".join(text) + "\n", encoding="utf-8")
    print("\n".join(text))
    return 0


if __name__ == "__main__":
    sys.exit(main())
