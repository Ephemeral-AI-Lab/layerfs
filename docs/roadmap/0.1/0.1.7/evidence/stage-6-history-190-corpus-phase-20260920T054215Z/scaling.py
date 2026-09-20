#!/usr/bin/env python3
"""Is the depth term more work per byte, or the same work costing more?

The matched-volume comparison says a late-chain state costs 1.2-2.3x an early-chain
state at equal changed bytes. That leaves two very different mechanisms:

* **algorithmic**: the read/update path does more work per byte as the chain grows -
  deeper delta chains, more candidate records, more waves per byte;
* **contextual**: the same work costs more - a bigger Store, a colder working set, a
  larger page-cache footprint.

`history-stride3` and `history-stride10` publish the read-side work counters per state
(`filesystem.provider.*`, `...pooled.*`, `...references.*`, `...validation.*`), so the
two are separable: work per changed byte against nanoseconds per unit of that work.
Stride1 ran the ordinary recording and publishes only its per-state totals, so it
contributes the per-state cost curve and not this split.
"""
import json
import math
import sys
from pathlib import Path

CAMPAIGN = Path(__file__).resolve().parent
DETAILED = ("history-stride3", "history-stride10")
WORK = (
    "filesystem.provider.read_waves",
    "filesystem.provider.returned_canonical_bytes",
    "filesystem.provider.pooled.leaf_requests",
    "filesystem.provider.pooled.pack_fetches",
    "filesystem.provider.pooled.pack_bytes",
    "filesystem.provider.pooled.physical_record_calls",
    "filesystem.provider.pooled.physical_group_decodes",
    "filesystem.provider.pooled.value_group_decodes",
    "filesystem.references.serials_scanned",
    "filesystem.references.base_records_read",
    "filesystem.validation.inode_pages_read",
)
BANDS = ((4_000_000, 7_000_000), (7_000_000, 10_000_000),
         (10_000_000, 14_000_000), (14_000_000, 22_000_000))


def load(case: str):
    raw = CAMPAIGN / "runs" / f"diagnostic-{case}" / "raw"
    tree = json.loads((raw / "timing.json").read_text())
    phases: dict[int, dict[str, int]] = {}
    for child in tree["children"]:
        ordinal = int(child["name"].rsplit(".", 1)[-1])
        phases[ordinal] = {"total": int(child["elapsed_ns"])}
        for node in child.get("children", []):
            phases[ordinal][node["name"]] = int(node["elapsed_ns"])
    values: dict[int, dict[str, int]] = {}
    for line in (raw / "trace.jsonl").read_text(encoding="utf-8").splitlines():
        if not line.strip():
            continue
        record = json.loads(line)
        key = str(record.get("key", ""))
        if not key.startswith("history.state."):
            continue
        parts = key.split(".")
        ordinal = int(parts[2])
        suffix = ".".join(parts[3:])
        if record.get("kind") in ("counter", "resource") and isinstance(record.get("value"), int):
            values.setdefault(ordinal, {})[suffix] = int(record["value"])
    return phases, values


def slope_vs_log(xs, ys):
    """Least-squares slope of y against ln(x): the elasticity in e-folds."""
    n = len(xs)
    lx = [math.log(x) for x in xs]
    mx, my = sum(lx) / n, sum(ys) / n
    num = sum((x - mx) * (y - my) for x, y in zip(lx, ys))
    den = sum((x - mx) ** 2 for x in lx)
    return num / den if den else float("nan")


def main() -> int:
    lines = []
    for case in DETAILED:
        phases, values = load(case)
        ordinals = sorted(phases)
        changed = {o: values[o].get("changed_bytes", 0) for o in ordinals}
        half = max(ordinals) // 2
        lines.append(f"=== {case}: work per changed byte, early half against late half, by changed-bytes band")
        lines.append(f"  {'band (MB)':>11} {'fs ns/MB early':>15} {'late':>10} {'x':>5} | "
                     f"{'waves/MB e':>10} {'l':>7} {'x':>5} | {'packfetch/MB e':>14} {'l':>7} {'x':>5} | "
                     f"{'ns/wave e':>10} {'l':>8} {'x':>5}")
        for lo, hi in BANDS:
            early = [o for o in ordinals if lo <= changed[o] < hi and o <= half]
            late = [o for o in ordinals if lo <= changed[o] < hi and o > half]
            if not early or not late:
                continue
            def agg(group, suffix, scale=1):
                return sum(values[o].get(suffix, 0) for o in group) / (sum(changed[o] for o in group) / scale)
            def fs_ns_per_mb(group):
                return sum(phases[o].get("filesystem", 0) for o in group) / (sum(changed[o] for o in group) / 1e6)
            def ns_per_wave(group):
                waves = sum(values[o].get("filesystem.provider.read_waves", 0) for o in group)
                return sum(phases[o].get("filesystem", 0) for o in group) / waves if waves else float("nan")
            e_fs, l_fs = fs_ns_per_mb(early), fs_ns_per_mb(late)
            e_w, l_w = agg(early, "filesystem.provider.read_waves", 1e6), agg(late, "filesystem.provider.read_waves", 1e6)
            e_p, l_p = agg(early, "filesystem.provider.pooled.pack_fetches", 1e6), agg(late, "filesystem.provider.pooled.pack_fetches", 1e6)
            e_nw, l_nw = ns_per_wave(early), ns_per_wave(late)
            lines.append(f"  {lo / 1e6:4.0f}-{hi / 1e6:<6.0f} {e_fs:>15,.0f} {l_fs:>10,.0f} {l_fs / e_fs:>5.2f} | "
                         f"{e_w:>10.1f} {l_w:>7.1f} {l_w / e_w:>5.2f} | {e_p:>14.1f} {l_p:>7.1f} {l_p / e_p:>5.2f} | "
                         f"{e_nw:>10,.0f} {l_nw:>8,.0f} {l_nw / e_nw:>5.2f}")
        # Elasticity: per-state filesystem ns per changed byte against ln(ordinal).
        xs = [o for o in ordinals if changed[o] > 0]
        ys = [phases[o].get("filesystem", 0) / changed[o] * 1e6 for o in xs]
        lines.append(f"  elasticity of filesystem ns/MB against ln(state): {slope_vs_log(xs, ys):+,.0f} ns/MB per e-fold"
                     f"  (from {ys[0]:,.0f} at state {xs[0]} to {ys[-1]:,.0f} at state {xs[-1]})")
        lines.append("")

    lines.append("=== the same history, split into more states (all three rows cover the same 157 checkpoints)")
    lines.append(f"  {'row':18s} {'states':>7} {'operation s':>12} {'s/state':>9} {'changed MB':>11} {'ns/MB':>10} {'MB/state':>9}")
    for case in ("history-stride10", "history-stride3", "history-stride1"):
        phases, values = load(case)
        ordinals = sorted(phases)
        op = sum(phases[o]["total"] for o in ordinals)
        changed = sum(values[o].get("changed_bytes", 0) for o in ordinals)
        lines.append(f"  {case:18s} {len(ordinals):>7} {op / 1e9:>12.3f} {op / len(ordinals) / 1e6:>9.1f}"
                     f" {changed / 1e6:>11.1f} {op / changed * 1e6:>10.1f} {changed / len(ordinals) / 1e6:>9.1f}")
    lines.append("=== which mechanism: per-byte work against per-unit cost (detailed rows only)")
    for case in DETAILED:
        phases, values = load(case)
        ordinals = sorted(phases)
        changed = {o: values[o].get("changed_bytes", 0) for o in ordinals}
        half = max(ordinals) // 2
        lines.append(f"  {case}")
        lines.append(f"    {'band MB':>9} {'hit rate e/l':>15} {'fetch/wave e/l':>16} {'KB/fetch e/l':>15}"
                     f" {'rec/leaf e/l':>14} {'edges/rec e/l':>14} {'pack bytes/MB e/l':>19}")
        for lo, hi in BANDS:
            early = [o for o in ordinals if lo <= changed[o] < hi and o <= half]
            late = [o for o in ordinals if lo <= changed[o] < hi and o > half]
            if not early or not late:
                continue
            def total(group, suffix):
                return sum(values[o].get(suffix, 0) for o in group)
            def per(group, suffix):
                return total(group, suffix) / len(group)
            def hit_rate(group):
                hits = total(group, "filesystem.provider.pooled.physical_group_cache_hits")
                decodes = total(group, "filesystem.provider.pooled.physical_group_decodes")
                return hits / (hits + decodes) if hits + decodes else float("nan")
            def ratio(group, a, b):
                return per(group, a) / per(group, b) if per(group, b) else float("nan")
            def pack_bytes_per_mb(group):
                return total(group, "filesystem.provider.pooled.pack_bytes") / (sum(changed[o] for o in group) / 1e6)
            lines.append(f"    {lo / 1e6:3.0f}-{hi / 1e6:<5.0f} {hit_rate(early):7.3f}/{hit_rate(late):<7.3f}"
                         f" {ratio(early, 'filesystem.provider.pooled.pack_fetches', 'filesystem.provider.read_waves'):7.1f}/"
                         f"{ratio(late, 'filesystem.provider.pooled.pack_fetches', 'filesystem.provider.read_waves'):<8.1f}"
                         f" {per(early, 'filesystem.provider.pooled.pack_bytes') / max(1, per(early, 'filesystem.provider.pooled.pack_fetches')) / 1024:7.1f}/"
                         f"{per(late, 'filesystem.provider.pooled.pack_bytes') / max(1, per(late, 'filesystem.provider.pooled.pack_fetches')) / 1024:<7.1f}"
                         f" {ratio(early, 'filesystem.provider.pooled.physical_record_calls', 'filesystem.provider.pooled.leaf_requests'):6.2f}/"
                         f"{ratio(late, 'filesystem.provider.pooled.physical_record_calls', 'filesystem.provider.pooled.leaf_requests'):<7.2f}"
                         f" {ratio(early, 'filesystem.provider.pooled.chain_edges', 'filesystem.provider.pooled.physical_record_calls'):6.2f}/"
                         f"{ratio(late, 'filesystem.provider.pooled.chain_edges', 'filesystem.provider.pooled.physical_record_calls'):<7.2f}"
                         f" {pack_bytes_per_mb(early):9,.0f}/{pack_bytes_per_mb(late):<9,.0f}")
    lines.append("")
    lines.append("Reading: bytes copied per fetch and fetches per wave both rise, while the decoded-group")
    lines.append("hit rate falls, and chain edges per record call stay flat. That is a bounded-cache")
    lines.append("against a growing working set, not a deeper-chain algorithm.")
    (CAMPAIGN / "scaling.txt").write_text("\n".join(lines) + "\n")
    print("\n".join(lines))
    return 0


if __name__ == "__main__":
    sys.exit(main())
