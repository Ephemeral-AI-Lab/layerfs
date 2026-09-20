#!/usr/bin/env python3
"""Where the per-state cost grows with chain depth, and whether it is content.

Stride1's last decile costs 24x its first. Content explains most of the spread
(correlation +0.90 to +0.99 with changed bytes) but not all of it: nanoseconds per
changed byte doubles down the chain. This script separates the two.

1. Sub-phase split, early states against late states, from the product's own timing
   tree. Note the recording difference: `history-stride1` runs without
   `LAYERFS_HISTORY_PHASES` (the driver refuses the phase nodes above 53 states), so
   its tree names only `content`, `store.create`, `storage.begin` and
   `storage.finish`; `history-stride3` and `history-stride10` name `filesystem`,
   `storage.accept_loop` and the `harness.*` spans as well. Stride1's two product
   sub-phases are therefore inside its state total but unnamed, which is stated
   rather than worked around.
2. Matched-volume comparison: states in the same changed-bytes band, early half of
   the chain against late half. A ratio above 1 at equal content is a depth term.
"""
import collections
import json
import sys
from pathlib import Path

CAMPAIGN = Path(__file__).resolve().parent
ROWS = ("history-stride10", "history-stride3", "history-stride1")
BANDS = ((4_000_000, 7_000_000), (7_000_000, 10_000_000),
         (10_000_000, 14_000_000), (14_000_000, 22_000_000))


def load(case: str):
    raw = CAMPAIGN / "runs" / f"diagnostic-{case}" / "raw"
    tree = json.loads((raw / "timing.json").read_text())
    elapsed = {int(child["name"].rsplit(".", 1)[-1]): int(child["elapsed_ns"])
               for child in tree["children"]}
    sub = collections.defaultdict(lambda: collections.defaultdict(int))
    for child in tree["children"]:
        ordinal = int(child["name"].rsplit(".", 1)[-1])
        for node in child.get("children", []):
            sub[node["name"]][ordinal] += int(node["elapsed_ns"])
    changed = {}
    for line in (raw / "trace.jsonl").read_text(encoding="utf-8").splitlines():
        if not line.strip():
            continue
        record = json.loads(line)
        key = str(record.get("key", ""))
        if record.get("kind") == "counter" and key.startswith("history.state.") and key.endswith(".changed_bytes"):
            changed[int(key.split(".")[2])] = int(record["value"])
    return elapsed, sub, changed


def main() -> int:
    lines = []
    for case in ROWS:
        elapsed, sub, changed = load(case)
        ordinals = sorted(elapsed)
        early, late = ordinals[1:6], ordinals[-5:]
        lines.append(f"=== {case}: sub-phases, states 2-6 against the last five")
        lines.append(f"  {'sub-phase':26s} {'early ms/state':>15} {'late ms/state':>14} {'ratio':>7}   named segments")
        for name in sorted(sub, key=lambda name: -sum(sub[name].values())):
            early_ns = sum(sub[name][o] for o in early) / len(early)
            late_ns = sum(sub[name][o] for o in late) / len(late)
            lines.append(f"  {name:26s} {early_ns / 1e6:>15.1f} {late_ns / 1e6:>14.1f}"
                         f" {late_ns / early_ns if early_ns else float('nan'):>7.2f}   "
                         f"{len(sub[name])} of {len(ordinals)} states")
        early_total = sum(elapsed[o] for o in early) / len(early)
        named = sum(sum(sub[name][o] for o in early) for name in sub) / len(early)
        lines.append(f"  {'UNNAMED in the state total':26s} {(early_total - named) / 1e6:>15.1f}"
                     f" {(sum(elapsed[o] for o in late) / len(late) - sum(sum(sub[name][o] for o in late) for name in sub) / len(late)) / 1e6:>14.1f}")
        lines.append("")

        lines.append(f"=== {case}: per-state cost at matched changed volume, early half against late half")
        half = max(ordinals) // 2
        lines.append(f"  {'band (MB)':>12} {'early n':>7} {'early ms':>9} {'late n':>6} {'late ms':>8}"
                     f" {'ratio':>6} {'early ns/B':>11} {'late ns/B':>10}")
        for lo, hi in BANDS:
            early_band = [o for o in ordinals if lo <= changed.get(o, 0) < hi and o <= half]
            late_band = [o for o in ordinals if lo <= changed.get(o, 0) < hi and o > half]
            if not early_band or not late_band:
                lines.append(f"  {lo / 1e6:5.1f}-{hi / 1e6:5.1f} {len(early_band):>7} {'-':>9} {len(late_band):>6} {'-':>8}"
                             f" {'-':>6} {'-':>11} {'-':>10}")
                continue
            em = sum(elapsed[o] for o in early_band) / len(early_band)
            lm = sum(elapsed[o] for o in late_band) / len(late_band)
            ec = sum(changed[o] for o in early_band) / len(early_band)
            lc = sum(changed[o] for o in late_band) / len(late_band)
            lines.append(f"  {lo / 1e6:5.1f}-{hi / 1e6:5.1f} {len(early_band):>7} {em / 1e6:>9.1f} {len(late_band):>6}"
                         f" {lm / 1e6:>8.1f} {lm / em:>6.2f} {em / ec:>11.1f} {lm / lc:>10.1f}")
        lines.append("")
    (CAMPAIGN / "depth-term.txt").write_text("\n".join(lines) + "\n")
    print("\n".join(lines))
    return 0


if __name__ == "__main__":
    sys.exit(main())
