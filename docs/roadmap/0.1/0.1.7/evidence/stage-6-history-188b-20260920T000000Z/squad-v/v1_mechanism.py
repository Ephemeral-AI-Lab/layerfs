#!/usr/bin/env python3
"""V1 part 5: mechanism of the four-slot loss + concrete cross-path examples."""
import collections
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from v1_lib import Store  # noqa: E402

V016 = "benchmark-results/repository-history/stride-10/deepseek-stride10/host-runtime/store.sqlite"
OIDMAP = "/tmp/oidtool/oidmap.txt"
SELECTION = [1 + 10 * i for i in range(16)] + [157]


def main():
    occ = collections.defaultdict(list)
    with open(OIDMAP) as handle:
        for line in handle:
            oid, ck, path = line.split()
            occ[oid].append((int(ck), path))
    default = Store("/tmp/r2/sample.sqlite", "c1")
    r4 = Store("/tmp/r4/sample.sqlite", "c1")
    b = {o.hex(): r for o, r in default.lane4().items()}
    a = {o.hex(): r for o, r in r4.lane4().items()}

    def first_seen(o):
        sel = sorted({ck for ck, _ in occ[o] if ck in SELECTION})
        return sel[0] if sel else None

    def same_path(o, base):
        ss = first_seen(o)
        bs = [ck for ck in {ck for ck, _ in occ[base]} if ck < ss]
        if not bs:
            return None
        bs = max(bs)
        return bool({p for ck, p in occ[o] if ck == ss} & {p for ck, p in occ[base] if ck == bs})

    print("=" * 78)
    print("Why the four-slot measured-order arm lost: what replaced what")
    print("=" * 78)
    table = collections.Counter()
    for o in set(a) & set(b):
        x, y = b[o], a[o]
        if x["column_base"] == y["column_base"]:
            continue
        d = y["body_bytes"] - x["body_bytes"]
        xb = x["column_base"].hex() if x["column_base"] else None
        yb = y["column_base"].hex() if y["column_base"] else None
        xs = same_path(o, xb) if xb else None
        ys = same_path(o, yb) if yb else None
        key = "default=%s -> arm=%s" % (
            "same-path" if xs else ("cross-path" if xs is False else "none"),
            "same-path" if ys else ("cross-path" if ys is False else "none"))
        table[key + " | objects"] += 1
        table[key + " | bytes"] += d
    for k in sorted(k for k in table if k.endswith("objects")):
        print("   %-46s %6d  %+d B" % (k[:-len(" | objects")], table[k], table[k[:-len(" | objects")] + " | bytes"]))

    print()
    print("=" * 78)
    print("Concrete examples of v0.1.6 cross-path bases")
    print("=" * 78)
    s016 = Store(V016, "016")
    l016 = {o.hex(): r for o, r in s016.lane4().items()}
    shown = 0
    for o, r in l016.items():
        base = r["record_base"].hex() if r["record_base"] else None
        if base is None:
            continue
        sp = same_path(o, base)
        if sp is not False:
            continue
        ss = first_seen(o)
        bsel = [ck for ck in {ck for ck, _ in occ[base]} if ck < ss]
        if not bsel:
            continue
        bs = max(bsel)
        xp = sorted(p for ck, p in occ[o] if ck == ss)
        bp = sorted(p for ck, p in occ[base] if ck == bs)
        print("   object %s  canonical %d  tag %d  frame %d B" % (
            o[:16], r["canonical_length"], r["tag"], r["frame_len"]))
        print("      version at checkpoint %d: %s" % (ss, bytes.fromhex(xp[0]).decode("utf-8", "replace")))
        print("      base    at checkpoint %d: %s   (canonical %d, frame %d B)" % (
            bs, bytes.fromhex(bp[0]).decode("utf-8", "replace"),
            l016[base]["canonical_length"], l016[base]["frame_len"]))
        shown += 1
        if shown == 4:
            break

    print()
    print("=" * 78)
    print("Final arithmetic")
    print("=" * 78)
    lc1 = {o.hex(): r for o, r in Store("/tmp/confirm/sample.sqlite", "c1").lane4().items()}
    l016b = l016
    common = set(lc1) & set(l016b)
    classes = collections.defaultdict(lambda: collections.Counter())
    for o in common:
        x, y = l016b[o], lc1[o]
        cls = "v0.1.6 %s / T1 %s" % ("delta" if x["tag"] else "full", "delta" if y["tag"] else "full")
        c = classes[cls]
        c["objects"] += 1
        c["canonical"] += x["canonical_length"]
        c["v016"] += x["body_bytes"]
        c["t1"] += y["body_bytes"]
    tot = collections.Counter()
    for k in sorted(classes):
        c = classes[k]
        print("   %-28s %6d obj  %12d canon  v0.1.6 %10d  T1 %10d  %+10d" % (
            k, c["objects"], c["canonical"], c["v016"], c["t1"], c["t1"] - c["v016"]))
        for f in c:
            tot[f] += c[f]
    print("   %-28s %6d obj  %12d canon  v0.1.6 %10d  T1 %10d  %+10d" % (
        "TOTAL", tot["objects"], tot["canonical"], tot["v016"], tot["t1"], tot["t1"] - tot["v016"]))


if __name__ == "__main__":
    main()
