#!/usr/bin/env python3
"""Ladders, bands and the four-axis human view.

The band table is **re-implemented here** rather than imported from the Rust
gates. That is deliberate: the runner's job is to re-derive a published figure
independently of the process that produced it, and a band that is only computed
where the number was collected is not a re-derivation.

Frozen rules this module applies:

* `doublings_i = log2(t_{i+1} / t_i)`; the byte ladder `{1, 10, 100, 500} MiB`
  contains **no** doublings, so a raw ratio is never reported as a per-doubling
  figure;
* `per_doubling_i = ratio_i ** (1 / doublings_i)`, reported because the
  specification names it;
* `b` = least-squares slope of `log2(v)` on `log2(t)`, reported because "O(n)" is
  a statement about a slope and a single ratio is not;
* **time is diagnostic.** Under decision D1 the time band is printed and never
  decides a row.
"""

from __future__ import annotations

import json
import math
import sys
from dataclasses import dataclass, field
from pathlib import Path

# The frozen band table, re-stated. (low, high) with `None` for an open end.
BANDS = {
    ("O(1)", "counter"): (0.90, 1.11),
    ("O(1)", "heap"): (0.90, 1.11),
    ("O(1)", "disk"): (0.90, 1.11),
    ("O(1)", "time"): (0.60, 1.67),
    ("O(n)", "counter"): (1.90, 2.10),
    ("O(n)", "heap"): (1.80, 2.22),
    ("O(n)", "disk"): (1.80, 2.22),
    ("O(n)", "time"): (1.60, 2.40),
    ("O(n log n)", "counter"): (1.95, 2.55),
    ("O(n log n)", "heap"): (1.90, 2.60),
    ("O(n log n)", "disk"): (1.90, 2.60),
    ("O(n log n)", "time"): (1.60, 2.90),
    ("O(n^2)", "counter"): (3.6, None),
    ("O(n^2)", "heap"): (3.4, None),
    ("O(n^2)", "disk"): (3.4, None),
    ("O(n^2)", "time"): (2.6, None),
}

# The scaling claim each family carries, from `gates_and_oracles.md` section 5.
# A family absent from this map has **no** scaling claim, and therefore no ratio
# gate: a family may not acquire one after seeing results.
SCALING_CLAIMS = {
    "c1.construct.whole-file": ("O(n)", "heap"),
    "c1.construct.chunked": ("O(1)", "heap"),
    "c1.edit.length-changing": ("O(n)", "counter"),
    "c1.tree.construct-traverse": ("O(n log n)", "counter"),
    "c1.change-locality": ("O(n)", "counter"),
    "c2.reuse.cross-file": ("O(1)", "counter"),
    "c2.footprint": ("O(n)", "disk"),
    "c2.read.waves": ("O(1)", "heap"),
    "pipeline.*": ("O(n)", "counter"),
}

# Which counter carries the claim, per family.
CLAIM_COUNTER = {
    "c1.construct.whole-file": "heap.peak_incremental_bytes",
    "c1.construct.chunked": "heap.peak_incremental_bytes",
    "c1.edit.length-changing": "edit.payload_bytes",
    "c1.tree.construct-traverse": "construct.objects",
    "c1.change-locality": "construct.objects",
    "c2.reuse.cross-file": "reuse.inserted",
    "c2.footprint": "space.allocated_bytes",
    "c2.read.waves": "heap.peak_incremental_bytes",
    "pipeline.*": "construct.objects",
}


@dataclass
class Row:
    """One case's re-derived receipt."""

    case_id: str
    family: str
    status: str
    tier: str
    declared_bytes: int
    declared_entries: int
    wall_ns: int
    counters: dict[str, int] = field(default_factory=dict)
    resources: dict[str, int] = field(default_factory=dict)
    notes: list[str] = field(default_factory=list)
    gates: list[dict[str, str]] = field(default_factory=list)
    phases: dict[str, object] = field(default_factory=dict)

    def phase(self, field: str) -> int:
        """One published phase field, or zero when the row published none."""
        value = self.phases.get(field)
        return int(value) if isinstance(value, int) else 0

    def non_operation_ns(self) -> int:
        """Everything the complete command paid that was not the measured work."""
        return max(0, self.wall_ns - self.phase("operation_ns"))

    def size_axis(self) -> float | None:
        """The x axis a ladder is plotted against: bytes first, then entries."""
        if self.declared_bytes > 0:
            return float(self.declared_bytes)
        if self.declared_entries > 0:
            return float(self.declared_entries)
        return None


def load_run(run_dir: str | Path) -> list[Row]:
    """Reads every `receipt.json` a run produced, in directory order."""
    run_dir = Path(run_dir)
    rows: list[Row] = []
    for case_dir in sorted(entry for entry in run_dir.iterdir() if entry.is_dir()):
        receipt_path = case_dir / "receipt.json"
        if not receipt_path.exists():
            continue
        document = json.loads(receipt_path.read_text(encoding="utf-8"))
        rows.append(
            Row(
                case_id=document.get("case_id", case_dir.name),
                family=document.get("family", ""),
                status=document.get("status", "NOT_RUN"),
                tier=document.get("tier_label", ""),
                declared_bytes=int(document.get("declared_bytes", 0)),
                declared_entries=int(document.get("declared_entries", 0)),
                wall_ns=int(document.get("wall_ns", 0)),
                counters=document.get("counters", {}) or {},
                resources=document.get("resources", {}) or {},
                notes=document.get("notes", []) or [],
                gates=document.get("gates", []) or [],
                phases=document.get("phases", {}) or {},
            )
        )
    return rows


def metric_of(row: Row, family: str) -> tuple[str, float] | None:
    """The claim metric of one row, as `(kind, value)`."""
    key = CLAIM_COUNTER.get(family)
    if key is None:
        return None
    if key in row.counters:
        return (key, float(row.counters[key]))
    if key in row.resources:
        return (key, float(row.resources[key]))
    return None


def _collected(row: Row) -> bool:
    """Whether the row actually produced a measurement.

    A `NOT_RUN` or `INCOMPLETE` row contributes no point to a ladder: a missing
    tier makes the band `INCOMPLETE`, and a zero standing in for a missing
    measurement would make the ratio meaningless.
    """
    return row.status in {"PASS", "FAIL", "TARGET_MISS", "INELIGIBLE"}


def ladder(rows: list[Row], family: str) -> list[tuple[float, str, float]]:
    """Points of one family's ladder, ordered by the size axis."""
    points: list[tuple[float, str, float]] = []
    for row in rows:
        if row.family != family or not _collected(row):
            continue
        axis = row.size_axis()
        metric = metric_of(row, family)
        if axis is None or metric is None:
            continue
        points.append((axis, metric[0], metric[1]))
    points.sort()
    return points


def per_doubling(ratio: float, first: float, second: float) -> float | None:
    """`ratio ** (1 / log2(second / first))`."""
    if first <= 0 or second <= 0 or second == first or ratio <= 0:
        return None
    doublings = math.log2(second / first)
    if doublings == 0:
        return None
    return ratio ** (1.0 / doublings)


def slope(points: list[tuple[float, float]]) -> float | None:
    """Least-squares slope of `log2(v)` on `log2(t)`."""
    usable = [(math.log2(t), math.log2(v)) for t, v in points if t > 0 and v > 0]
    if len(usable) < 2:
        return None
    count = len(usable)
    mean_x = sum(x for x, _ in usable) / count
    mean_y = sum(y for _, y in usable) / count
    covariance = sum((x - mean_x) * (y - mean_y) for x, y in usable)
    variance = sum((x - mean_x) ** 2 for x, _ in usable)
    if variance == 0:
        return None
    return covariance / variance


def band_status(value: float | None, growth: str, metric: str) -> str:
    """Classifies one per-doubling figure against its declared band."""
    if value is None:
        return "INCOMPLETE"
    if metric == "time":
        return "DIAGNOSTIC"
    low, high = BANDS.get((growth, metric), (None, None))
    if low is not None and value < low:
        return "FAIL"
    if high is not None and value > high:
        return "FAIL"
    return "PASS"


def render(rows: list[Row], run_identity: dict[str, object] | None = None) -> str:
    """Renders the four-axis human view."""
    out: list[str] = []
    out.append("Stage 6 structural-complexity report")
    out.append("=" * 72)
    if run_identity:
        out.append(f"run directory      : {run_identity.get('run_dir', '')}")
        out.append(f"source commit      : {run_identity.get('source_commit', '')}")
        out.append(f"dirty tree         : {run_identity.get('source_dirty', '')}")
        out.append(f"harness binary     : {run_identity.get('harness_binary_sha256', '')}")
        out.append(f"workers            : {run_identity.get('construction_workers', '')}")
        out.append(f"lane               : {run_identity.get('lane', '')}")
        mode = run_identity.get("verification_mode", "")
        out.append(f"verification mode  : {mode or '(unpublished)'}")
        if mode == "sample":
            out.append(f"sample rule        : {run_identity.get('verification_sample_rule', '')}")
        if mode and mode != "full":
            out.append(
                "  every row in a non-full mode is INCOMPLETE: an iteration run is never "
                "admission evidence"
            )
        if run_identity.get("reused_proof"):
            out.append(f"reused proof       : {run_identity.get('reused_proof')}")
            out.append("  the deferred verification invocation was not re-run where the proof covers the row")
    out.append("")
    out.append("Row statuses")
    out.append("-" * 72)
    tally: dict[str, int] = {}
    by_family: dict[str, list[Row]] = {}
    for row in rows:
        tally[row.status] = tally.get(row.status, 0) + 1
        by_family.setdefault(row.family, []).append(row)
    for status in ("PASS", "FAIL", "TARGET_MISS", "INCOMPLETE", "INELIGIBLE", "NOT_RUN"):
        if status in tally:
            out.append(f"  {status:<12} {tally[status]}")
    out.append(f"  {'TOTAL':<12} {len(rows)}")
    out.append("")
    out.append("The four phases, for the lane")
    out.append("-" * 72)
    out.append(
        "  operation_ns is the golden benchmark number: the product's own telemetry root "
        "for the row's"
    )
    out.append(
        "  declared operation. The complete-command wall is what the budget classifies, and "
        "the"
    )
    out.append(
        "  non-operation share is what the campaign pays for preparation, verification and "
        "cleanup."
    )
    out.append(
        f"  {'status':<11} {'rows':>4} {'preparation s':>14} {'operation s':>12} "
        f"{'verification s':>15} {'cleanup s':>10} {'acquisition s':>14} {'wall s':>10}"
    )
    phase_fields = (
        "preparation_wall_ns",
        "operation_ns",
        "verification_wall_ns",
        "cleanup_wall_ns",
        "acquisition_wall_ns",
    )
    lane_totals: dict[str, int] = {field: 0 for field in phase_fields}
    lane_wall = 0
    for status in ("PASS", "FAIL", "TARGET_MISS", "INCOMPLETE", "INELIGIBLE", "NOT_RUN"):
        bucket = [row for row in rows if row.status == status]
        if not bucket:
            continue
        sums = {field: sum(row.phase(field) for row in bucket) for field in phase_fields}
        wall = sum(row.wall_ns for row in bucket)
        for field in phase_fields:
            lane_totals[field] += sums[field]
        lane_wall += wall
        out.append(
            f"  {status:<11} {len(bucket):>4} {sums['preparation_wall_ns'] / 1e9:>14.3f} "
            f"{sums['operation_ns'] / 1e9:>12.3f} {sums['verification_wall_ns'] / 1e9:>15.3f} "
            f"{sums['cleanup_wall_ns'] / 1e9:>10.3f} "
            f"{sums['acquisition_wall_ns'] / 1e9:>14.3f} {wall / 1e9:>10.3f}"
        )
    if lane_wall:
        non_operation = max(0, lane_wall - lane_totals["operation_ns"])
        out.append(
            f"  {'TOTAL':<11} {len(rows):>4} "
            f"{lane_totals['preparation_wall_ns'] / 1e9:>14.3f} "
            f"{lane_totals['operation_ns'] / 1e9:>12.3f} "
            f"{lane_totals['verification_wall_ns'] / 1e9:>15.3f} "
            f"{lane_totals['cleanup_wall_ns'] / 1e9:>10.3f} "
            f"{lane_totals['acquisition_wall_ns'] / 1e9:>14.3f} {lane_wall / 1e9:>10.3f}"
        )
        out.append(
            f"  non-operation share: {non_operation / 1e9:.3f} s of {lane_wall / 1e9:.3f} s "
            f"= {100.0 * non_operation / lane_wall:.1f}%"
        )
        out.append(
            f"  sum(operation_ns) = {lane_totals['operation_ns'] / 1e9:.3f} s "
            "(the golden number; it must not fall)"
        )
    out.append("")
    out.append("Four axes, per family")
    out.append("-" * 72)
    out.append(
        f"  {'family':<32} {'rows':>4} {'pass':>5} {'heap peak max':>14} "
        f"{'operation max ms':>17} {'wall max ms':>12} {'non-op %':>9}"
    )
    for family in sorted(by_family):
        family_rows = by_family[family]
        passes = sum(1 for row in family_rows if row.status == "PASS")
        heap = max(
            (row.resources.get("heap.peak_incremental_bytes", 0) for row in family_rows),
            default=0,
        )
        operation = max((row.phase("operation_ns") for row in family_rows), default=0) / 1e6
        wall_ns = max((row.wall_ns for row in family_rows), default=0)
        non_operation = sum(row.non_operation_ns() for row in family_rows)
        total_wall = sum(row.wall_ns for row in family_rows)
        share = 100.0 * non_operation / total_wall if total_wall else 0.0
        out.append(
            f"  {family:<32} {len(family_rows):>4} {passes:>5} {heap:>14} "
            f"{operation:>17.1f} {wall_ns / 1e6:>12.1f} {share:>8.1f}%"
        )
    out.append("")
    out.append("Scaling ladders (counters, heap and disk decide; time is diagnostic)")
    out.append("-" * 72)
    out.append(
        f"  {'family':<32} {'claim':<11} {'metric':<28} {'b':>7} {'per-doubling':>28}"
    )
    for family in sorted(by_family):
        claim = SCALING_CLAIMS.get(family)
        if claim is None:
            out.append(f"  {family:<32} {'-':<11} no scaling claim registered")
            continue
        growth, kind = claim
        points = ladder(rows, family)
        key = CLAIM_COUNTER.get(family, "")
        if kind == "heap":
            pairs = [
                (row.size_axis(), float(row.resources.get(key, 0)))
                for row in by_family[family]
                if _collected(row) and row.size_axis()
            ]
        else:
            pairs = [
                (row.size_axis(), float(row.counters.get(key, 0)))
                for row in by_family[family]
                if _collected(row) and row.size_axis()
            ]
        pairs.sort()
        ratios: list[str] = []
        for index in range(1, len(pairs)):
            previous = pairs[index - 1]
            current = pairs[index]
            if previous[1] <= 0:
                ratios.append("n/a (zero divisor)")
                continue
            ratio = current[1] / previous[1]
            value = per_doubling(ratio, previous[0], current[0])
            status = band_status(value, growth, kind)
            ratios.append(
                f"{value:.3f} {status}" if value is not None else f"INCOMPLETE {status}"
            )
        b = slope(pairs) if pairs else None
        out.append(
            f"  {family:<32} {growth:<11} {key:<28} "
            f"{(f'{b:.3f}' if b is not None else 'n/a'):>7} {'; '.join(ratios) or '-':>28}"
        )
        missing = [row for row in by_family[family] if not _collected(row)]
        if missing:
            out.append(
                f"      {len(missing)} row(s) contributed no point: "
                + ", ".join(f"{row.case_id}={row.status}" for row in missing[:4])
            )
    out.append("")
    out.append("Rows that are not PASS")
    out.append("-" * 72)
    for row in rows:
        if row.status == "PASS":
            continue
        reasons = [
            gate for gate in row.gates if gate.get("status") not in (None, "PASS")
        ]
        detail = "; ".join(
            f"{gate.get('id')}={gate.get('status')} ({gate.get('measured')})"
            for gate in reasons[:3]
        )
        out.append(f"  {row.status:<11} {row.case_id:<48} {detail[:120]}")
    out.append("")
    return "\n".join(out)


def main() -> int:
    if len(sys.argv) < 2:
        print("usage: analyze.py RUN_DIR [identity.json]", file=sys.stderr)
        return 2
    run_dir = Path(sys.argv[1])
    identity = None
    if len(sys.argv) > 2 and Path(sys.argv[2]).exists():
        identity = json.loads(Path(sys.argv[2]).read_text(encoding="utf-8"))
        identity.setdefault("run_dir", str(run_dir))
    print(render(load_run(run_dir), identity))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
