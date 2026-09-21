#!/usr/bin/env python3
"""Where the scope change paid, from the product's own per-state timing tree.

The counter comparison says the treatment removed pack acquisition and nothing
else: `physical_record_calls`, `chain_edges` and the decoded-group cache's hits are
identical between the arms, while `pack_fetches` falls to 0.3% of the baseline. This
script asks the timing side of the same question - which named sub-phase lost the
time, and does the loss grow with chain depth, as the depth term did.
"""
import collections
import json
import sys
from pathlib import Path

CAMPAIGN = Path(__file__).resolve().parent
ARMS = ("baseline", "candidate")
CASES = ("history-stride10", "history-stride3")
SUB = ("filesystem", "storage.accept_loop", "content", "storage.finish", "storage.begin",
       "harness.input", "harness.index", "harness.predecessors")


def load(arm, case):
    raw = CAMPAIGN / "runs" / f"{arm}-{case}" / "raw"
    tree = json.loads((raw / "timing.json").read_text())
    elapsed, sub = {}, collections.defaultdict(dict)
    for child in tree["children"]:
        ordinal = int(child["name"].rsplit(".", 1)[-1])
        elapsed[ordinal] = int(child["elapsed_ns"])
        for node in child.get("children", []):
            sub[node["name"]][ordinal] = sub[node["name"]].get(ordinal, 0) + int(node["elapsed_ns"])
    return elapsed, sub


def main() -> int:
    out = []
    for case in CASES:
        base, base_sub = load("baseline", case)
        cand, cand_sub = load("candidate", case)
        ordinals = sorted(base)
        half = max(ordinals) // 2
        out.append(f"=== {case}: per-state elapsed and named sub-phases, baseline -> candidate")
        out.append(f"  {'st':>3} {'elapsed b':>10} {'c':>10} {'delta':>9} | "
                   + " ".join(f"{name[:13]:>15}" for name in SUB[:3]))
        for ordinal in ordinals:
            b, c = base[ordinal] / 1e6, cand[ordinal] / 1e6
            cells = " ".join(
                f"{base_sub[name].get(ordinal, 0) / 1e6:>6.1f}->{cand_sub[name].get(ordinal, 0) / 1e6:<7.1f}"
                for name in SUB[:3])
            out.append(f"  {ordinal:>3} {b:>10.1f} {c:>10.1f} {c - b:>+9.1f} | {cells}")
        out.append("")
        out.append(f"  {'sub-phase':22s} {'baseline ms':>12} {'candidate ms':>13} {'delta ms':>10} "
                   f"{'early b->c':>16} {'late b->c':>16}")
        for name in SUB:
            b_total = sum(base_sub[name].get(o, 0) for o in ordinals) / 1e6
            c_total = sum(cand_sub[name].get(o, 0) for o in ordinals) / 1e6
            if b_total == 0 and c_total == 0:
                continue
            early = [o for o in ordinals if o <= half]
            late = [o for o in ordinals if o > half]
            b_e = sum(base_sub[name].get(o, 0) for o in early) / len(early) / 1e6
            c_e = sum(cand_sub[name].get(o, 0) for o in early) / len(early) / 1e6
            b_l = sum(base_sub[name].get(o, 0) for o in late) / len(late) / 1e6
            c_l = sum(cand_sub[name].get(o, 0) for o in late) / len(late) / 1e6
            out.append(f"  {name:22s} {b_total:>12.1f} {c_total:>13.1f} {c_total - b_total:>+10.1f} "
                       f"{b_e:>7.1f}->{c_e:<8.1f} {b_l:>7.1f}->{c_l:<8.1f}")
        b_total = sum(base.values()) / 1e6
        c_total = sum(cand.values()) / 1e6
        out.append(f"  {'state total':22s} {b_total:>12.1f} {c_total:>13.1f} {c_total - b_total:>+10.1f}")
        out.append(f"  early-half mean  {sum(base[o] for o in ordinals if o <= half) / len([o for o in ordinals if o <= half]) / 1e6:8.1f} ->"
                   f" {sum(cand[o] for o in ordinals if o <= half) / len([o for o in ordinals if o <= half]) / 1e6:8.1f} ms")
        out.append(f"  late-half mean   {sum(base[o] for o in ordinals if o > half) / len([o for o in ordinals if o > half]) / 1e6:8.1f} ->"
                   f" {sum(cand[o] for o in ordinals if o > half) / len([o for o in ordinals if o > half]) / 1e6:8.1f} ms")
        out.append("")
    (CAMPAIGN / "subphases.txt").write_text("\n".join(out) + "\n")
    print("\n".join(out))
    return 0


if __name__ == "__main__":
    sys.exit(main())
