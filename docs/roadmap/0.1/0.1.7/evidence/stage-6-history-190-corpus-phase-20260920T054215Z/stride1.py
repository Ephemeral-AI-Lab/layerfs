#!/usr/bin/env python3
"""The third row: stride1's speed, what drives the per-state cost, and the Store axes.

`history-stride1` is the only `history.*` row no #190 campaign had ever sampled, and
it is the one row where a v0.1.6 figure **is** directly comparable: the historical
Store allocation target is a byte count of the same selection's content, not a phase
-scoped time from a different harness. Owner ruling 7 makes it a gate: the core figure
must land below the historical target, and above is a finding rather than a new
baseline.

Three questions, from retained artifacts only:

1. What does one state cost, and does the cost grow with chain depth? (The timing
   tree's 157 named children are the product's own spans.)
2. What drives it - changed content, or something per-state? (The row publishes its
   per-state counters even without the phase diagnostics.)
3. Where is the Store, against the historical targets?
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
import history_corpus  # noqa: E402
import space  # noqa: E402

ROWS = ("history-stride10", "history-stride3", "history-stride1")
V016_ALLOCATED = history_corpus.V016_ALLOCATED


def children(case: str):
    tree = json.loads((CAMPAIGN / "runs" / f"diagnostic-{case}" / "raw" / "timing.json").read_text())
    return [(child["name"], int(child["elapsed_ns"])) for child in tree["children"]]


def counters(case: str, arm_dir: Path):
    out: dict[str, int] = {}
    for line in (arm_dir / "trace.jsonl" if (arm_dir / "trace.jsonl").exists()
                 else arm_dir / "trace-perf.jsonl").read_text(encoding="utf-8").splitlines():
        if not line.strip():
            continue
        record = json.loads(line)
        if record.get("kind") == "counter" and isinstance(record.get("value"), int):
            out[record["key"]] = record["value"]
    return out


def mean(values):
    return sum(values) / len(values) if values else 0.0


def slope(values):
    n = len(values)
    xs = list(range(n))
    mx, my = mean(xs), mean(values)
    num = sum((x - mx) * (y - my) for x, y in zip(xs, values))
    den = sum((x - mx) ** 2 for x in xs)
    return num / den if den else 0.0


def main() -> int:
    report = {"schema": "h190-stride1-analysis-v1", "rows": {}}
    lines = []
    for case in ROWS:
        spans = children(case)
        values = [value for _name, value in spans]
        arm = CAMPAIGN / "runs" / f"diagnostic-{case}" / "raw"
        counters_seen = counters(case, arm)
        states = len(values)
        changed = [counters_seen.get(f"history.state.{n}.changed_bytes", 0) for n in range(1, states + 1)]
        paths = [counters_seen.get(f"history.state.{n}.changed_paths", 0) for n in range(1, states + 1)]
        objects = [counters_seen.get(f"history.state.{n}.objects", 0) for n in range(1, states + 1)]
        commits = [counters_seen.get(f"history.state.{n}.save.commits", 0) for n in range(1, states + 1)]
        decile = max(1, states // 10)
        entry = {
            "states": states,
            "operation_ns": sum(values),
            "per_state_mean_ns": mean(values),
            "per_state_median_ns": sorted(values)[states // 2],
            "per_state_min_ns": min(values),
            "per_state_max_ns": max(values),
            "first_decile_mean_ns": mean(values[:decile]),
            "last_decile_mean_ns": mean(values[-decile:]),
            "slope_ns_per_state": slope(values),
            "changed_bytes_total": sum(changed),
            "changed_paths_total": sum(paths),
            "objects_total": sum(objects),
            "save_commits_total": sum(commits),
            "ns_per_changed_byte": sum(values) / sum(changed) if sum(changed) else None,
        }
        report["rows"][case] = entry
        lines.append(f"=== {case}  ({states} states)")
        lines.append(f"  operation (sum of named children) : {sum(values) / 1e9:9.3f} s")
        lines.append(f"  per state: mean {mean(values) / 1e6:7.1f} ms, median {sorted(values)[states // 2] / 1e6:7.1f} ms, "
                     f"min {min(values) / 1e6:6.1f}, max {max(values) / 1e6:7.1f}")
        lines.append(f"  first decile mean {mean(values[:decile]) / 1e6:7.1f} ms -> last decile mean {mean(values[-decile:]) / 1e6:7.1f} ms"
                     f"   (slope {slope(values) / 1e3:+.2f} us per state)")
        lines.append(f"  changed bytes {sum(changed):,} over {sum(paths):,} paths; "
                     f"ns per changed byte {entry['ns_per_changed_byte']:.1f}")
        lines.append(f"  canonical objects emitted {sum(objects):,}; save commits {sum(commits):,}")

    # Store axes, and the byte-for-byte check against the retained campaign.
    lines.append("")
    lines.append("=== Store axes against the historical v0.1.6 targets (owner ruling 7: below is the gate)")
    for case in ROWS:
        store = CAMPAIGN / "runs" / f"diagnostic-{case}" / "raw" / "sample.sqlite"
        reading = space.footprint(store)
        fields = reading.as_fields()
        target = V016_ALLOCATED[case]
        allocated = fields.get("allocated_bytes", 0)
        retained_store = RETAINED / "runs" / f"candidate2-{case}" / "raw" / "sample.sqlite"
        same_as_retained = None
        if retained_store.exists():
            same_as_retained = space.sha256_file(store) == space.sha256_file(retained_store) \
                if hasattr(space, "sha256_file") else None
        entry = report["rows"][case]
        entry["store"] = {
            "apparent_bytes": fields.get("apparent_bytes"),
            "allocated_bytes": allocated,
            "v016_allocated_bytes": target,
            "delta_allocated_bytes": allocated - target,
            "below_target": allocated < target,
            "incomplete_axes": reading.incomplete,
            "byte_identical_to_retained_candidate2": same_as_retained,
        }
        lines.append(f"  {case:18s} apparent {fields.get('apparent_bytes'):>12,}  allocated {allocated:>12,}"
                     f"  target {target:>12,}  delta {allocated - target:+,}"
                     f"  {'BELOW' if allocated < target else 'ABOVE'}")
        if same_as_retained is not None:
            lines.append(f"      byte-identical to the retained campaign's candidate2 Store: {same_as_retained}")

    (CAMPAIGN / "stride1-analysis.json").write_text(json.dumps(report, indent=2) + "\n")
    (CAMPAIGN / "stride1-analysis.txt").write_text("\n".join(lines) + "\n")
    print("\n".join(lines))
    return 0


if __name__ == "__main__":
    sys.exit(main())
