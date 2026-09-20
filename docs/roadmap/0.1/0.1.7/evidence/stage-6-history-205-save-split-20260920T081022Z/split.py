#!/usr/bin/env python3
"""The save path's cost, split in seconds from the product's own seven buckets.

Reads the instrument arm's `timing.json` (the product's span tree) and
`trace.jsonl` (the product's counters) and publishes one table per case.

**The denominator.** `SaveProfile` is accumulated over the whole save operation,
so its charges accrue during `accept_loop` *and* during `finish` - `finish` drains
the last preparation wave through the same `flush_batch`, then seals, flushes the
content index, advances the watermark and commits. The scope that contains every
charge, computed the **same way on every row**, is therefore

    scope = save_ns - storage.begin

where `save_ns` is the harness's own resource counter for `begin_save + accept +
finish`, published per state, and `storage.begin` is the product's acquisition
span. The scope's own composition is reported beside the table: `storage.accept_loop`
and `harness.index` are nodes on the rows that record them (stride10, stride3) and
are omitted on stride1 by the driver's ordinary recording above 53 states, where
the scope overstates accept_loop by exactly those two terms.

Nothing here is inferred from counts. Every second in the table is a charge the
product took at the call site that did the work; the remainder is stated as a
remainder.
"""
import hashlib
import json
import sys
from pathlib import Path

CAMPAIGN = Path(__file__).resolve().parent
REPO = Path("/Users/yifanxu/Ephemeral-AI-Lab/layerfs-190-scope")

BUCKETS = (
    ("resolve_ns", "resolution + exact-reuse verification"),
    ("full_ns", "FULL representation encode"),
    ("delta_ns", "delta representation encode"),
    ("group_ns", "group codec"),
    ("place_ns", "pack placement"),
    ("sql_ns", "SQL statements"),
    ("commit_ns", "transaction cadence"),
)
RECORDED_STORE = {
    "history-stride10": "4af37932aa3391b12269de8130b9c66dc504f64ca78fc3e585f7afddabed8487",
    "history-stride3": "f5c7ff5a6b4f0821aa9a21ac5250335c4c3fb889637a0c5345c5278caadc2a9e",
    "history-stride1": "1635cf7bbbabdc7f9e4af81ac9c6b6a88f45be35b4100dda0f52394c85dcf418",
}
RETAINED_SAMPLE = {
    "history-stride10": ("candidate-history-stride10", 16.908),
    "history-stride3": ("candidate-history-stride3", 36.810),
    "history-stride1": ("candidate-history-stride1", 105.726),
}
OUT = []


def write(line=""):
    OUT.append(line)
    print(line)


def load(run: Path):
    tree = json.loads((run / "raw/timing.json").read_text())
    rows = [json.loads(line) for line in (run / "raw/trace.jsonl").read_text().splitlines()
            if line.strip()]
    phases = json.loads((run / "raw/phases-perf.json").read_text())
    resources = {row["key"]: row["value"] for row in rows
                 if row.get("kind") == "resource" and row.get("unit") == "ns"}
    counters = {row["key"]: row["value"] for row in rows if row.get("kind") == "counter"}
    states = {}
    for node in tree["children"]:
        ordinal = int(node["name"].split(".")[-1])
        states[ordinal] = {"total": node["elapsed_ns"],
                           "kids": {c["name"]: c["elapsed_ns"] for c in node.get("children", [])}}
    return {"phases": phases, "resources": resources, "counters": counters, "states": states}


def scope_ns(data, ordinal):
    """The scope that contains every charge, identical on every row."""
    save = data["resources"].get(f"history.state.{ordinal}.save_ns")
    if save is None:
        return None
    return save - data["states"][ordinal]["kids"].get("storage.begin", 0)


def bucket_ns(data, ordinal, bucket):
    return data["counters"].get(f"history.state.{ordinal}.save.{bucket}", 0)


def inserted(data, ordinal):
    return data["counters"].get(f"history.state.{ordinal}.inserted", 0)


def store_sha(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def analyse(label, case):
    run = CAMPAIGN / "runs" / f"{label}-{case}"
    data = load(run)
    order = sorted(data["states"])
    scope = {n: scope_ns(data, n) for n in order}
    total_scope = sum(scope.values())
    total_inserted = sum(inserted(data, n) for n in order)
    nodes = {key: sum(data["states"][n]["kids"].get(key, 0) for n in order)
             for key in ("storage.accept_loop", "harness.index", "storage.finish")}
    has_nodes = nodes["storage.accept_loop"] > 0

    write(f"### {label}/{case}")
    write()
    write(f"  states {len(order)}   inserted {total_inserted}   "
          f"operation {data['phases']['operation_ns'] / 1e9:.3f} s   "
          f"invocation {data['phases']['invocation_ns'] / 1e9:.3f} s")
    write(f"  scope (save_ns - storage.begin)   {total_scope / 1e9:8.3f} s")
    if has_nodes:
        write(f"    storage.accept_loop (node)      {nodes['storage.accept_loop'] / 1e9:8.3f} s "
              f"({100 * nodes['storage.accept_loop'] / total_scope:.2f} % of scope)")
        write(f"    harness.index (node)            {nodes['harness.index'] / 1e9:8.3f} s "
              f"({100 * nodes['harness.index'] / total_scope:.2f} %)")
        write(f"    storage.finish (node)           {nodes['storage.finish'] / 1e9:8.3f} s "
              f"({100 * nodes['storage.finish'] / total_scope:.2f} %)")
        rest = total_scope - sum(nodes.values())
        write(f"    unrecorded bookkeeping          {rest / 1e9:8.3f} s "
              f"({100 * rest / total_scope:.2f} %)")
    else:
        write("    storage.accept_loop and harness.index are not recorded on this row")
        write("    (the driver's ordinary recording above 53 states), so the scope")
        write("    overstates accept_loop by exactly those two terms; storage.finish")
        write(f"    is recorded and is {nodes['storage.finish'] / 1e9:.3f} s "
              f"({100 * nodes['storage.finish'] / total_scope:.2f} % of scope)")
    write()

    write(f"  {'bucket':38} {'seconds':>9} {'share':>8} {'ns/object':>10}")
    charged = 0
    per_bucket = {}
    for bucket, name in BUCKETS:
        ns = sum(bucket_ns(data, n, bucket) for n in order)
        per_bucket[bucket] = ns
        charged += ns
        write(f"  {name:38} {ns / 1e9:9.3f} {100 * ns / total_scope:7.2f}% "
              f"{ns / max(total_inserted, 1):10.0f}")
    remainder = total_scope - charged
    write(f"  {'remainder (uncharged)':38} {remainder / 1e9:9.3f} "
          f"{100 * remainder / total_scope:7.2f}% {remainder / max(total_inserted, 1):10.0f}")
    write(f"  {'charged total':38} {charged / 1e9:9.3f} "
          f"{100 * charged / total_scope:7.2f}%")
    write()

    third = max(len(order) // 3, 1)
    early, late = order[:third], order[-third:]
    write(f"  per inserted object, first {len(early)} states -> last {len(late)} states")
    write(f"  {'bucket':38} {'early ns/obj':>13} {'late ns/obj':>12} {'growth':>8}")
    for bucket, name in BUCKETS:
        e = sum(bucket_ns(data, n, bucket) for n in early) / max(sum(inserted(data, n) for n in early), 1)
        l = sum(bucket_ns(data, n, bucket) for n in late) / max(sum(inserted(data, n) for n in late), 1)
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
    return {"case": case, "scope_ns": total_scope, "charged": charged, "remainder": remainder,
            "buckets": per_bucket, "inserted": total_inserted, "nodes": nodes,
            "operation_ns": data["phases"]["operation_ns"], "store": digest,
            "store_equal": digest == expected, "roots": roots,
            "retained_operation_ns": int(retained_op * 1e9)}


def main() -> int:
    cases = sys.argv[1:] or ["history-stride10", "history-stride1"]
    summary = [analyse("instrument", case) for case in cases]
    write("### summary")
    write()
    for entry in summary:
        write(f"  {entry['case']:20} scope {entry['scope_ns'] / 1e9:8.3f} s  "
              f"charged {entry['charged'] / 1e9:8.3f} s  "
              f"remainder {entry['remainder'] / 1e9:6.3f} s "
              f"({100 * entry['remainder'] / entry['scope_ns']:5.2f} %)  "
              f"store {'equal' if entry['store_equal'] else 'DIFFERENT'}")
    (CAMPAIGN / "split.txt").write_text("\n".join(OUT) + "\n")
    (CAMPAIGN / "split.json").write_text(json.dumps(summary, indent=2) + "\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
