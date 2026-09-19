#!/usr/bin/env python3
"""V1 part 2: attribute the gap to the candidate class, and price the counterfactual."""
import collections
import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from v1_lib import Store  # noqa: E402

V016 = "benchmark-results/repository-history/stride-10/deepseek-stride10/host-runtime/store.sqlite"
C1 = "/tmp/confirm/sample.sqlite"
OIDMAP = "/tmp/oidtool/oidmap.txt"
SELECTION = [1 + 10 * i for i in range(16)] + [157]
ORDINAL = {ck: i for i, ck in enumerate(SELECTION)}


def main():
    occ = collections.defaultdict(list)
    with open(OIDMAP) as handle:
        for line in handle:
            oid, ck, path = line.split()
            occ[oid].append((int(ck), path))
    s016 = Store(V016, "016")
    c1 = Store(C1, "c1")
    l016 = {o.hex(): r for o, r in s016.lane4().items()}
    lc1 = {o.hex(): r for o, r in c1.lane4().items()}
    sel_of = {o: sorted({ck for ck, _ in occ[o] if ck in ORDINAL}) for o in l016}
    first_seen = {o: v[0] for o, v in sel_of.items() if v}

    def relation(o, base):
        """('same-save'|'earlier', gap, same-path?) for object o against base."""
        ss = first_seen[o]
        bsel = [ck for ck in sel_of[base] if ck < ss]
        if bsel:
            bs = max(bsel)
        else:
            bs = first_seen[base]
            if bs != ss:
                return ("later-or-equal", None, None)
        xp = {p for ck, p in occ[o] if ck == ss}
        bp = {p for ck, p in occ[base] if ck == bs}
        return ("same-save" if bs == ss else "earlier", ss - bs, bool(xp & bp))

    print("=" * 78)
    print("A. v0.1.6 tag semantics, checked against the corpus (same save vs earlier state)")
    print("=" * 78)
    for tag in (1, 2):
        recs = [o for o in l016 if l016[o]["tag"] == tag]
        rel = collections.Counter()
        path = collections.Counter()
        for o in recs:
            kind, gap, sp = relation(o, l016[o]["record_base"].hex())
            rel[kind] += 1
            if kind == "earlier":
                path["same-path gap=%d" % gap if sp else "cross-path gap=%d" % gap] += 1
            else:
                path["same-save " + ("same-path" if sp else "cross-path")] += 1
        print("   tag %d: %d records" % (tag, len(recs)))
        print("      base state: %s" % dict(rel))
        for k in sorted(path):
            print("      %-28s %6d" % (k, path[k]))

    print()
    print("=" * 78)
    print("B. T1 base classes (what our caller offered)")
    print("=" * 78)
    rel = collections.Counter()
    for o in lc1:
        r = lc1[o]
        if r["column_base"] is None:
            rel["full, no base"] += 1
            continue
        kind, gap, sp = relation(o, r["column_base"].hex())
        rel["delta %s %s" % (kind, "same-path" if sp else "cross-path")] += 1
    print("   %s" % dict(rel))

    print()
    print("=" * 78)
    print("C. The 2,594 objects v0.1.6 deltas and T1 stores FULL, by candidate class")
    print("=" * 78)
    q1 = [o for o in l016 if l016[o]["tag"] in (1, 2) and lc1[o]["tag"] == 0]
    cls = collections.defaultdict(lambda: collections.Counter())
    for o in q1:
        kind, gap, sp = relation(o, l016[o]["record_base"].hex())
        if kind == "same-save":
            key = "base written in the SAME save (%s)" % ("same-path" if sp else "cross-path")
        elif kind == "earlier":
            key = "base at an earlier state (%s, gap %s)" % ("same-path" if sp else "cross-path", gap)
        else:
            key = "base not earlier (%s)" % kind
        c = cls[key]
        c["objects"] += 1
        c["canonical"] += l016[o]["canonical_length"]
        c["stored016"] += l016[o]["body_bytes"]
        c["storedc1"] += lc1[o]["body_bytes"]
    print("   %-46s %6s %12s %11s %11s %11s" % ("class", "obj", "canonical", "016 B", "T1 B", "T1-016"))
    tot = collections.Counter()
    for k in sorted(cls):
        c = cls[k]
        print("   %-46s %6d %12d %11d %11d %+11d" % (
            k, c["objects"], c["canonical"], c["stored016"], c["storedc1"], c["storedc1"] - c["stored016"]))
        for f in c:
            tot[f] += c[f]
    print("   %-46s %6d %12d %11d %11d %+11d" % (
        "TOTAL", tot["objects"], tot["canonical"], tot["stored016"], tot["storedc1"],
        tot["storedc1"] - tot["stored016"]))

    print()
    print("=" * 78)
    print("D. The 1,129 objects T1 deltas and v0.1.6 stores FULL")
    print("=" * 78)
    q3 = [o for o in l016 if l016[o]["tag"] == 0 and lc1[o]["tag"] == 1]
    rel = collections.Counter()
    for o in q3:
        kind, gap, sp = relation(o, lc1[o]["column_base"].hex())
        key = "%s %s" % (kind, "same-path" if sp else "cross-path")
        rel[key] += 1
        rel[key + " bytes"] += lc1[o]["body_bytes"]
    print("   %d objects, canonical %d, v0.1.6 stored %d, T1 stored %d" % (
        len(q3), sum(l016[o]["canonical_length"] for o in q3),
        sum(l016[o]["body_bytes"] for o in q3), sum(lc1[o]["body_bytes"] for o in q3)))
    for k in sorted(rel):
        print("      %-28s %d" % (k, rel[k]))

    print()
    print("=" * 78)
    print("E. The 91 same-path bases v0.1.6 used and T1 did not")
    print("=" * 78)
    n = 0
    for o in q1:
        kind, gap, sp = relation(o, l016[o]["record_base"].hex())
        if kind == "earlier" and sp:
            n += 1
            if n <= 10:
                print("      %s tag%d gap=%s canonical=%d 016=%d T1=%d" % (
                    o[:16], l016[o]["tag"], gap, l016[o]["canonical_length"],
                    l016[o]["body_bytes"], lc1[o]["body_bytes"]))
    print("   total same-path earlier-state bases among the 2,594: %d" % n)

    print()
    print("=" * 78)
    print("F. Counterfactual: T1 lane at v0.1.6's base assignment")
    print("=" * 78)
    # record grammar is identical: tag u8 [+ 32 B base] + frame.  Same-base frames are
    # byte-identical (section 4 of v1_forensic), so the counterfactual is exact where
    # the base matches and is priced by substitution elsewhere.
    same = [o for o in l016 if l016[o]["tag"] in (1, 2) and lc1[o]["tag"] == 1
            and l016[o]["record_base"] == lc1[o]["column_base"]]
    print("   measured: %d objects already share v0.1.6's base; their stored bytes are" % len(same))
    print("             %d in v0.1.6 and %d in T1 -> delta %+d" % (
        sum(l016[o]["body_bytes"] for o in same), sum(lc1[o]["body_bytes"] for o in same),
        sum(lc1[o]["body_bytes"] for o in same) - sum(l016[o]["body_bytes"] for o in same)))
    print("   T1 lane total            %d" % sum(r["body_bytes"] for r in lc1.values()))
    print("   v0.1.6 lane total        %d" % sum(r["body_bytes"] for r in l016.values()))
    print("   gap                      %+d" % (sum(r["body_bytes"] for r in lc1.values())
                                               - sum(r["body_bytes"] for r in l016.values())))
    print("   of which NOT explained by base choice (same-base objects): %+d" % (
        sum(lc1[o]["body_bytes"] for o in same) - sum(l016[o]["body_bytes"] for o in same)))

    print()
    print("=" * 78)
    print("G. T1 save counters from the run's own trace")
    print("=" * 78)
    counters = {}
    with open("/tmp/confirm/trace.jsonl") as handle:
        for line in handle:
            rec = json.loads(line)
            if rec.get("kind") == "counter" and str(rec.get("key", "")).startswith("delta."):
                counters[rec["key"]] = rec["value"]
    for k in sorted(counters):
        print("      %-28s %s" % (k, counters[k]))


if __name__ == "__main__":
    main()
