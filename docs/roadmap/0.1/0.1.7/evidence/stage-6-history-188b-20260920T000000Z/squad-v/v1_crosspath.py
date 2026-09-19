#!/usr/bin/env python3
"""V1 part 3: what ARE the cross-path earlier-state bases?"""
import collections
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
    l016 = {o.hex(): r for o, r in s016.lane4().items()}
    sel_of = {o: sorted({ck for ck, _ in occ[o] if ck in ORDINAL}) for o in l016}
    first_seen = {o: v[0] for o, v in sel_of.items() if v}
    # corpus path index: (checkpoint, path) -> oid, over ALL 157 checkpoints
    at = collections.defaultdict(dict)
    for o, rows in occ.items():
        for ck, p in rows:
            at[(ck, p)] = o

    def paths_of(o, ck):
        return {p for c, p in occ[o] if c == ck}

    print("=" * 78)
    print("The 2,368 v0.1.6 cross-path bases that sit at an EARLIER selected state")
    print("=" * 78)
    q1 = [o for o in l016 if l016[o]["tag"] in (1, 2) and l016[o]["record_base"] is not None]
    kinds = collections.Counter()
    kb = collections.Counter()
    for o in q1:
        base = l016[o]["record_base"].hex()
        ss = first_seen[o]
        bsel = [ck for ck in sel_of[base] if ck < ss]
        if not bsel:
            continue
        bs = max(bsel)
        xp = paths_of(o, ss)
        bp = paths_of(base, bs)
        if xp & bp:
            continue  # same-path, handled elsewhere
        # rename signature: o's path is absent at bs AND the base's path is absent at ss
        x_absent = all((bs, p) not in at for p in xp)
        b_absent = all((ss, p) not in at for p in bp)
        # is o's path present at bs under a DIFFERENT oid?
        x_present_other = any((bs, p) in at and at[(bs, p)] != o for p in xp)
        if x_absent and b_absent:
            key = "rename signature: neither path exists in the other state"
        elif x_absent:
            key = "object's path is new at this state; base path still exists"
        elif x_present_other:
            key = "object's path existed at the base state with a different object"
        else:
            key = "other"
        kinds[key] += 1
        kb[key] += l016[o]["body_bytes"]
    for k in sorted(kinds):
        print("   %-62s %6d  stored %10d" % (k, kinds[k], kb[k]))
    print("   total cross-path earlier-state bases: %d" % sum(kinds.values()))

    print()
    print("=" * 78)
    print("T1's 5,975 same-save cross-path bases: the product's own per-save index")
    print("=" * 78)
    s1 = Store(C1, "c1")
    lc1 = {o.hex(): r for o, r in s1.lane4().items()}
    n = 0
    saved = collections.Counter()
    for o, r in lc1.items():
        if r["column_base"] is None:
            continue
        base = r["column_base"].hex()
        ss = first_seen[o]
        bsel = [ck for ck in sel_of[base] if ck < ss]
        if bsel:
            continue
        if first_seen[base] != ss:
            continue
        xp = paths_of(o, ss)
        bp = paths_of(base, ss)
        if xp & bp:
            continue
        n += 1
        saved["stored"] += r["body_bytes"]
        saved["canonical"] += r["canonical_length"]
        saved["016_full_alt"] += l016[o]["body_bytes"] if o in l016 and l016[o]["tag"] == 0 else 0
    print("   T1 same-save cross-path deltas: %d objects, canonical %d, stored %d" % (
        n, saved["canonical"], saved["stored"]))
    print("   of those, objects v0.1.6 stored FULL: stored-here %d" % saved["016_full_alt"])


if __name__ == "__main__":
    main()
