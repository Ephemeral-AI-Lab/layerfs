#!/usr/bin/env python3
"""The resolution bucket, split four ways, from the sub-split instrument's own counters.

`split.py` publishes the save's seven top-level buckets. Resolution is 45 % of the
stride10 scope and 76 % of the stride1 scope, and it is not one kind of work: it is
a chain walk that only measures depth, a chain walk that reconstructs a base, a
walk that records the new object's cost, the exact-reuse verification, and the
pooled lane's own lookups. This script reads the **sub-split instrument arm**
(`instrument2`, binary 68f0cc5e...) and publishes those five parts, each read from
the counter charged at the call site that did the work.

It reads `instrument2`'s runs only. The earlier arm's samples predate the sub-split
counters and carry `resolve_ns` alone; they are not a source for this table and are
not rewritten. Nothing here is inferred from counts or from the elapsed/count
correlations: every second is a charge the product took.

The parts sum to `resolve_ns` exactly (the script asserts it per case), so the
top-level seven-bucket split is unchanged by the refinement rather than re-scaled.
"""
import hashlib
import json
import sys
from pathlib import Path

from split import BUCKETS, RECORDED_STORE, RETAINED_SAMPLE, load, scope_ns, inserted, store_sha

CAMPAIGN = Path(__file__).resolve().parent

# The five disjoint parts of `SaveProfile::resolve`, in the product's own charge order.
PARTS = (
    ("resolve.eligible_ns", "eligibility walk (depth_of)"),
    ("resolve.acquire_ns", "base acquisition (resolve_dependency)"),
    ("resolve.cost_ns", "post-trial cost walk"),
    ("resolve.reuse_ns", "exact-reuse verification"),
    ("resolve.pooled_ns", "pooled lane lookup + base"),
)
OUT = []


def write(line=""):
    OUT.append(line)
    print(line)


def part_ns(data, ordinal, part):
    return data["counters"].get(f"history.state.{ordinal}.save.{part}", 0)


def analyse(label, case):
    run = CAMPAIGN / "runs" / f"{label}-{case}"
    data = load(run)
    order = sorted(data["states"])
    scope = {n: scope_ns(data, n) for n in order}
    if any(v is None for v in scope.values()):
        write(f"### {label}/{case}: REFUSED - history.state.<n>.save_ns is absent")
        return None
    total_scope = sum(scope.values())
    total_inserted = sum(inserted(data, n) for n in order)
    nodes = {key: sum(data["states"][n]["kids"].get(key, 0) for n in order)
             for key in ("storage.accept_loop", "harness.index", "storage.finish")}
    has_nodes = nodes["storage.accept_loop"] > 0

    parts = {p: sum(part_ns(data, n, p) for n in order) for p, _ in PARTS}
    resolve = sum(parts.values())
    row_resolve = sum(data["counters"].get(f"history.state.{n}.save.resolve_ns", 0) for n in order)
    charged = sum(sum(data["counters"].get(f"history.state.{n}.save.{b}", 0) for n in order)
                  for b, _ in BUCKETS)
    remainder = total_scope - charged

    write(f"### {label}/{case}")
    write()
    write(f"  states {len(order)}   inserted {total_inserted}   "
          f"operation {data['phases']['operation_ns'] / 1e9:.3f} s   "
          f"invocation {data['phases']['invocation_ns'] / 1e9:.3f} s")
    write(f"  scope (save_ns - storage.begin)   {total_scope / 1e9:8.3f} s")
    if has_nodes:
        write(f"    storage.accept_loop (node)      {nodes['storage.accept_loop'] / 1e9:8.3f} s "
              f"({100 * nodes['storage.accept_loop'] / total_scope:.2f} % of scope)")
    else:
        write("    storage.accept_loop is not recorded on this row (the driver's ordinary")
        write("    recording above 53 states), so the scope overstates it by that node and")
        write(f"    harness.index; storage.finish is recorded and is "
              f"{nodes['storage.finish'] / 1e9:.3f} s ({100 * nodes['storage.finish'] / total_scope:.2f} %)")
    write()

    write(f"  {'resolution part':38} {'seconds':>9} {'of resolve':>11} {'of scope':>9} {'ns/object':>10}")
    for part, name in PARTS:
        ns = parts[part]
        write(f"  {name:38} {ns / 1e9:9.3f} {100 * ns / resolve:10.2f}% "
              f"{100 * ns / total_scope:8.2f}% {ns / max(total_inserted, 1):10.0f}")
    write(f"  {'resolution (all five parts)':38} {resolve / 1e9:9.3f} {100.0:10.2f}% "
          f"{100 * resolve / total_scope:8.2f}% {resolve / max(total_inserted, 1):10.0f}")
    write(f"  {'resolve_ns as the row reports it':38} {row_resolve / 1e9:9.3f} "
          f"{'':>11} {100 * row_resolve / total_scope:8.2f}% "
          f"{row_resolve / max(total_inserted, 1):10.0f}"
          f"   parts==row: {'YES' if resolve == row_resolve else 'NO'}")
    write()

    third = max(len(order) // 3, 1)
    early, late = order[:third], order[-third:]
    write(f"  per inserted object, first {len(early)} states -> last {len(late)} states")
    write(f"  {'resolution part':38} {'early ns/obj':>13} {'late ns/obj':>12} {'growth':>8}")
    for part, name in PARTS:
        e = sum(part_ns(data, n, part) for n in early) / max(sum(inserted(data, n) for n in early), 1)
        l = sum(part_ns(data, n, part) for n in late) / max(sum(inserted(data, n) for n in late), 1)
        write(f"  {name:38} {e:13.0f} {l:12.0f} {l / e if e else float('nan'):7.2f}x")
    e = sum(scope[n] for n in early) / max(sum(inserted(data, n) for n in early), 1)
    l = sum(scope[n] for n in late) / max(sum(inserted(data, n) for n in late), 1)
    write(f"  {'SCOPE TOTAL':38} {e:13.0f} {l:12.0f} {l / e if e else float('nan'):7.2f}x")
    write()

    sample = run / "raw/sample.sqlite"
    digest = store_sha(sample) if sample.is_file() else None
    expected = RECORDED_STORE[case]
    roots = [data["counters"].get(f"history.state.{n}.root") for n in order]
    write(f"  store sha256 {digest}")
    write(f"  recorded     {expected}   equal: {'YES' if digest == expected else 'NO'}")
    write(f"  state roots  {len(roots)} states, first {roots[0]}")
    write(f"               {' ' * 12}last  {roots[-1]}")
    write()

    retained_label, retained_op = RETAINED_SAMPLE[case]
    op = data["phases"]["operation_ns"] / 1e9
    write(f"  instrument overhead bound: operation {op:.3f} s against the retained")
    write(f"  uninstrumented {retained_label} {retained_op:.3f} s -> {op - retained_op:+.3f} s "
          f"({100 * (op - retained_op) / retained_op:+.2f} %)")
    write()
    return {"case": case, "label": label, "scope_ns": total_scope, "resolve_ns": resolve,
            "row_resolve_ns": row_resolve, "parts": parts, "charged": charged,
            "remainder": remainder, "inserted": total_inserted, "nodes": nodes,
            "operation_ns": data["phases"]["operation_ns"], "store": digest,
            "store_equal": digest == expected, "roots": roots,
            "retained_operation_ns": int(retained_op * 1e9)}


def main() -> int:
    label = sys.argv[1] if len(sys.argv) > 1 else "instrument2"
    cases = sys.argv[2:] or ["history-stride10", "history-stride1"]
    summary = [analyse(label, case) for case in cases]
    write("### summary")
    write()
    for entry in summary:
        if entry is None:
            continue
        parts = "  ".join(f"{p.split('.')[1][:-3]} {entry['parts'][p] / 1e9:.3f}"
                          for p, _ in PARTS)
        write(f"  {entry['case']:18} resolve {entry['resolve_ns'] / 1e9:8.3f} s "
              f"of scope {entry['scope_ns'] / 1e9:8.3f} s "
              f"({100 * entry['resolve_ns'] / entry['scope_ns']:5.2f} %)  store "
              f"{'equal' if entry['store_equal'] else 'DIFFERENT'}")
        write(f"  {'':18} {parts}")
    (CAMPAIGN / "split2.txt").write_text("\n".join(OUT) + "\n")
    (CAMPAIGN / "split2.json").write_text(json.dumps(summary, indent=2) + "\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
