#!/usr/bin/env python3
"""The scaling curve, before and after the L49 scope change, on its own terms.

L47 published the curve this round is about: the same 157 checkpoints split into 17,
53 and 157 retained versions cost 52.3 M, 68.6 M and 85.0 M ns per changed MB, and
within one chain a late state at matched changed volume cost 1.2-2.3x an early one.
L49 removed the read path's pack-acquisition term on stride10 and stride3 and left
stride1 unmeasured; this round samples it.

Everything here is recomputed from the runs' own artifacts: the pre-fix rows are the
retained campaign's (`stage-6-history-190-corpus-phase-...`), the post-fix rows are
this round's candidate arm, and the matched pair is this round's baseline arm, which
is the same product source as the pre-fix rows under the new harness.

Three questions:
1. What does the curve look like now, in the same units L47 used?
2. Is the matched-volume depth term (late half against early half at equal changed
   bytes) still there, and how much smaller is it?
3. What does the save path look like at 157 versions, where it publishes its own
   counters for the first time?
"""
import collections
import json
import math
import os
import statistics
from pathlib import Path

CAMPAIGN = Path(__file__).resolve().parent
PRE = Path(os.environ.get(
    "H190_RETAINED",
    "/Users/yifanxu/Ephemeral-AI-Lab/layerfs-190-qual/docs/roadmap/0.1/"
    "0.1.7/evidence/stage-6-history-190-corpus-phase-20260920T054215Z"))
CASES = ("history-stride10", "history-stride3", "history-stride1")
STATES = {"history-stride10": 17, "history-stride3": 53, "history-stride1": 157}
BANDS = ((4_000_000, 7_000_000), (7_000_000, 10_000_000),
         (10_000_000, 14_000_000), (14_000_000, 22_000_000))
SAVE = (("save.delta.trials", "prefix trials"),
        ("save.chain.objects", "chain objects read"),
        ("save.chain.edges", "chain edges walked"),
        ("save.delta.prepared_full", "FULL preparations"),
        ("save.pack_appends", "pack BLOB rewrites"),
        ("save.statements", "INSERT statements"),
        ("save.commits", "commits"))


def load(raw: Path):
    tree = json.loads((raw / "timing.json").read_text())
    elapsed, sub = {}, collections.defaultdict(dict)
    for child in tree["children"]:
        ordinal = int(child["name"].rsplit(".", 1)[-1])
        elapsed[ordinal] = int(child["elapsed_ns"])
        for node in child.get("children", []):
            sub[node["name"]][ordinal] = int(node["elapsed_ns"])
    values = collections.defaultdict(dict)
    for line in (raw / "trace.jsonl").read_text(encoding="utf-8").splitlines():
        if not line.strip():
            continue
        record = json.loads(line)
        key = str(record.get("key", ""))
        if key.startswith("history.state.") and isinstance(record.get("value"), int):
            parts = key.split(".")
            if parts[2].isdigit():
                values[int(parts[2])][".".join(parts[3:])] = record["value"]
    return elapsed, sub, values


def slope(xs, ys):
    n = len(xs)
    lx = [math.log(x) for x in xs]
    mx, my = sum(lx) / n, sum(ys) / n
    den = sum((x - mx) ** 2 for x in lx)
    return sum((x - mx) * (y - my) for x, y in zip(lx, ys)) / den if den else float("nan")


def bands(elapsed, changed):
    ordinals = sorted(elapsed)
    half = max(ordinals) // 2
    out = []
    for lo, hi in BANDS:
        early = [o for o in ordinals if lo <= changed[o] < hi and o <= half]
        late = [o for o in ordinals if lo <= changed[o] < hi and o > half]
        if not early or not late:
            out.append((f"{lo/1e6:.0f}-{hi/1e6:.0f}", None, len(early), len(late)))
            continue
        e = statistics.mean(elapsed[o] for o in early) / 1e6
        l = statistics.mean(elapsed[o] for o in late) / 1e6
        out.append((f"{lo/1e6:.0f}-{hi/1e6:.0f}", l / e, len(early), len(late)))
    return out


def main() -> int:
    out = []
    write = out.append
    rows = {}
    for case in CASES:
        pre_e, _, pre_v = load(PRE / "runs" / f"diagnostic-{case}" / "raw")
        base_e, _, base_v = load(CAMPAIGN / "runs" / f"baseline-{case}" / "raw")
        cand_e, _, cand_v = load(CAMPAIGN / "runs" / f"candidate-{case}" / "raw")
        rows[case] = (pre_e, pre_v, base_e, base_v, cand_e, cand_v)

    write("## 1. The curve: cost per changed byte against how many versions hold the history")
    write("")
    write(f"  {'row':9} {'versions':>8} {'operation pre':>13} {'post':>10} {'changed MB':>11}"
          f" {'ns/MB pre':>12} {'post':>12} {'change':>8}")
    pre_fit, post_fit, states_all = [], [], []
    for case in CASES:
        pre_e, pre_v, _, _, cand_e, cand_v = rows[case]
        changed = sum(cand_v[o].get("changed_bytes", 0) for o in cand_v) / 1e6
        pre_op = sum(pre_e.values()) / 1e9
        cand_op = sum(cand_e.values()) / 1e9
        pre_u, post_u = pre_op / changed * 1e9, cand_op / changed * 1e9
        pre_fit.append(pre_u); post_fit.append(post_u); states_all.append(STATES[case])
        write(f"  {case:9} {STATES[case]:>8} {pre_op:>12.3f} s {cand_op:>9.3f} s {changed:>11.1f}"
              f" {pre_u:>12,.0f} {post_u:>12,.0f} {post_u / pre_u - 1:>7.1%}")
    write("")
    write(f"  per e-fold of version count (least squares over the three rows):"
          f" pre **{slope(states_all, pre_fit):,.0f}** ns/MB,"
          f" post **{slope(states_all, post_fit):,.0f}** ns/MB"
          f"  ({(slope(states_all, post_fit) / slope(states_all, pre_fit)) - 1:+.0%})")
    write(f"  finest against coarsest: {pre_fit[2] / pre_fit[0]:.3f}x before,"
          f" {post_fit[2] / post_fit[0]:.3f}x after")
    write("")

    write("## 2. The depth term inside one chain, at matched changed volume")
    write("")
    write(f"  {'row':17} {'band MB':>9} {'pre (n e/l)':>16} {'post (n e/l)':>16} {'change':>9}")
    for case in CASES:
        pre_e, pre_v, _, _, cand_e, cand_v = rows[case]
        pre_changed = {o: pre_v[o].get("changed_bytes", 0) for o in pre_e}
        cand_changed = {o: cand_v[o].get("changed_bytes", 0) for o in cand_e}
        for (band, pre_r, pe, pl), (_, post_r, ce, cl) in zip(
                bands(pre_e, pre_changed), bands(cand_e, cand_changed)):
            if pre_r is None or post_r is None:
                continue
            write(f"  {case:17} {band:>9} {pre_r:>11.2f}x {f'({pe}/{pl})':>15}"
                  f" {post_r:>11.2f}x {f'({ce}/{cl})':>15} {post_r / pre_r - 1:>8.0%}")
    write("")

    write("## 3. Depth elasticity of the state total against ln(state), all three rows")
    write("")
    for case in CASES:
        pre_e, pre_v, base_e, base_v, cand_e, cand_v = rows[case]
        cells = []
        for label, elapsed, values in (("pre", pre_e, pre_v), ("baseline", base_e, base_v),
                                       ("post", cand_e, cand_v)):
            ords = [o for o in sorted(elapsed) if values[o].get("changed_bytes")]
            ys = [elapsed[o] / (values[o]["changed_bytes"] / 1e6) for o in ords]
            cells.append(f"{label} {slope(ords, ys):>13,.0f}")
        write(f"  {case:17} ns/MB per e-fold: " + " | ".join(cells))
    write("")

    write("## 4. The save path at 157 versions (its own counters, published per state now)")
    write("")
    for case in ("history-stride3", "history-stride1"):
        _, _, _, _, cand_e, cand_v = rows[case]
        ordinals = sorted(cand_v)
        half = max(ordinals) // 2
        early = [o for o in ordinals if 2 <= o <= max(2, half // 4)]
        late = [o for o in ordinals if o > max(ordinals) - max(5, half // 4)]
        ins_e = sum(cand_v[o].get("inserted", 0) for o in early)
        ins_l = sum(cand_v[o].get("inserted", 0) for o in late)
        write(f"  {case} (early n={len(early)}, late n={len(late)};"
              f" {ins_e:,} / {ins_l:,} objects written)")
        for key, label in SAVE:
            e = sum(cand_v[o].get(key, 0) for o in early) / max(1, ins_e)
            l = sum(cand_v[o].get(key, 0) for o in late) / max(1, ins_l)
            write(f"    {label:24s} per inserted object: {e:8.4f} -> {l:8.4f}  ({l / e if e else float('nan'):5.2f}x)")
        write("")

    (CAMPAIGN / "scaling.txt").write_text("\n".join(out) + "\n")
    print("\n".join(out))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
