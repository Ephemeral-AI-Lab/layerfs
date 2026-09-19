#!/usr/bin/env python3
"""V1 forensic driver: join the two Stores per object and decompose the lane gap."""
import collections
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from v1_lib import Store, totals, zstd_header  # noqa: E402

V016 = "benchmark-results/repository-history/stride-10/deepseek-stride10/host-runtime/store.sqlite"
C1 = "/tmp/confirm/sample.sqlite"


def section(title):
    print()
    print("=" * 78)
    print(title)
    print("=" * 78)


def main():
    s016 = Store(V016, "016")
    c1 = Store(C1, "c1")

    section("0. Store identity and lane totals")
    for label, s in (("v0.1.6", s016), ("T1 (/tmp/confirm)", c1)):
        by_lane, seen = totals(s)
        lane4_body = by_lane.get("CompactSmall" if s.generation == "016" else "WholeFile", 0)
        lane4 = s.lane4()
        print("%-20s %s" % (label, s.path))
        print("   apparent %d B   object rows %d   packs %d %s" % (
            os.path.getsize(s.path), s.rows, sum(s.versions.values()), dict(sorted(s.versions.items()))))
        print("   pack blob %d   headers+directories %d" % (s.pack_bytes, s.hdr_dir))
        print("   lane bodies: " + " | ".join(
            "%s %d" % (k, by_lane[k]) for k in sorted(by_lane)))
        print("   attributed %d of %d   residual %d" % (
            sum(by_lane.values()), s.pack_bytes, s.pack_bytes - sum(by_lane.values())))
        print("   lane-4 objects %d   lane-4 body %d   canonical %d" % (
            len(lane4), lane4_body, sum(r["canonical_length"] for r in lane4.values())))
        roles = collections.Counter(r["role"] for r in lane4.values())
        if s.has_role:
            print("   lane-4 object_role histogram: %s" % dict(roles))

    l016 = s016.lane4()
    lc1 = c1.lane4()
    common = set(l016) & set(lc1)
    only016 = set(l016) - set(lc1)
    onlyc1 = set(lc1) - set(l016)
    section("1. Lane-4 join on ObjectId")
    print("   common %d   v0.1.6-only %d   T1-only %d" % (len(common), len(only016), len(onlyc1)))
    print("   canonical common  v0.1.6 %d   T1 %d" % (
        sum(l016[o]["canonical_length"] for o in common),
        sum(lc1[o]["canonical_length"] for o in common)))
    mism = [o for o in common if l016[o]["canonical_length"] != lc1[o]["canonical_length"]]
    print("   canonical_length mismatches on the common set: %d" % len(mism))
    print("   v0.1.6-only canonical %d   T1-only canonical %d" % (
        sum(l016[o]["canonical_length"] for o in only016),
        sum(lc1[o]["canonical_length"] for o in onlyc1)))
    print("   v0.1.6-only body %d   T1-only body %d" % (
        sum(l016[o]["body_bytes"] for o in only016),
        sum(lc1[o]["body_bytes"] for o in onlyc1)))

    gap = sum(r["body_bytes"] for r in lc1.values()) - sum(r["body_bytes"] for r in l016.values())
    print("   LANE GAP = %d - %d = %d" % (
        sum(r["body_bytes"] for r in lc1.values()),
        sum(r["body_bytes"] for r in l016.values()), gap))

    section("2. Classification matrix (v0.1.6 tag x T1 representation)")
    classes = collections.defaultdict(lambda: collections.Counter())
    for o in common:
        a, b = l016[o], lc1[o]
        cls = "016tag%d/ours%s" % (a["tag"], "base" if b["tag"] == 1 else "full")
        c = classes[cls]
        c["objects"] += 1
        c["canonical"] += a["canonical_length"]
        c["stored016"] += a["body_bytes"]
        c["storedc1"] += b["body_bytes"]
    print("   %-22s %8s %14s %12s %12s %12s" % (
        "class", "objects", "canonical", "016 stored", "T1 stored", "delta"))
    tot = collections.Counter()
    for cls in sorted(classes):
        c = classes[cls]
        d = c["storedc1"] - c["stored016"]
        print("   %-22s %8d %14d %12d %12d %+12d" % (
            cls, c["objects"], c["canonical"], c["stored016"], c["storedc1"], d))
        for k in c:
            tot[k] += c[k]
    print("   %-22s %8d %14d %12d %12d %+12d" % (
        "COMMON TOTAL", tot["objects"], tot["canonical"], tot["stored016"], tot["storedc1"],
        tot["storedc1"] - tot["stored016"]))
    print("   closure: common delta %+d + T1-only %+d - v0.1.6-only %+d = %+d (lane gap %+d)" % (
        tot["storedc1"] - tot["stored016"],
        sum(lc1[o]["body_bytes"] for o in onlyc1),
        -sum(l016[o]["body_bytes"] for o in only016),
        tot["storedc1"] - tot["stored016"]
        + sum(lc1[o]["body_bytes"] for o in onlyc1)
        - sum(l016[o]["body_bytes"] for o in only016),
        gap))

    section("3. Q1 - DELTA in v0.1.6, FULL in T1")
    q1 = [o for o in common if l016[o]["tag"] in (1, 2) and lc1[o]["tag"] == 0]
    q1b = [o for o in common if l016[o]["tag"] in (1, 2) and lc1[o]["tag"] == 1]
    print("   delta(016) & full(T1): %d objects, canonical %d B" % (
        len(q1), sum(l016[o]["canonical_length"] for o in q1)))
    print("      v0.1.6 stored %d   T1 stored %d   delta %+d" % (
        sum(l016[o]["body_bytes"] for o in q1), sum(lc1[o]["body_bytes"] for o in q1),
        sum(lc1[o]["body_bytes"] for o in q1) - sum(l016[o]["body_bytes"] for o in q1)))
    print("   delta(016) & delta(T1): %d objects, canonical %d B" % (
        len(q1b), sum(l016[o]["canonical_length"] for o in q1b)))
    print("      v0.1.6 stored %d   T1 stored %d   delta %+d" % (
        sum(l016[o]["body_bytes"] for o in q1b), sum(lc1[o]["body_bytes"] for o in q1b),
        sum(lc1[o]["body_bytes"] for o in q1b) - sum(l016[o]["body_bytes"] for o in q1b)))
    q3 = [o for o in common if l016[o]["tag"] == 0 and lc1[o]["tag"] == 1]
    q4 = [o for o in common if l016[o]["tag"] == 0 and lc1[o]["tag"] == 0]
    for name, q in (("full(016) & delta(T1)", q3), ("full(016) & full(T1)", q4)):
        print("   %s: %d objects, canonical %d B, 016 stored %d, T1 stored %d, delta %+d" % (
            name, len(q), sum(l016[o]["canonical_length"] for o in q),
            sum(l016[o]["body_bytes"] for o in q), sum(lc1[o]["body_bytes"] for o in q),
            sum(lc1[o]["body_bytes"] for o in q) - sum(l016[o]["body_bytes"] for o in q)))

    section("4. Q2 - DELTA in BOTH: same base vs different base")
    both = [o for o in common if l016[o]["tag"] in (1, 2) and lc1[o]["tag"] == 1]
    same_base = [o for o in both if l016[o]["record_base"] == lc1[o]["column_base"]]
    diff_base = [o for o in both if l016[o]["record_base"] != lc1[o]["column_base"]]
    for name, q in (("same base", same_base), ("different base", diff_base)):
        if not q:
            continue
        s016b = sum(l016[o]["body_bytes"] for o in q)
        sc1b = sum(lc1[o]["body_bytes"] for o in q)
        print("   %-16s %6d obj  canonical %12d  016 %10d  T1 %10d  delta %+10d  ratio %.4f" % (
            name, len(q), sum(l016[o]["canonical_length"] for o in q), s016b, sc1b, sc1b - s016b,
            sc1b / s016b if s016b else float("nan")))
    ident = sum(1 for o in same_base if l016[o]["frame"] == lc1[o]["frame"])
    samelen = sum(1 for o in same_base if l016[o]["frame_len"] == lc1[o]["frame_len"])
    print("   same-base frames byte-identical: %d of %d (%.2f%%)" % (
        ident, len(same_base), 100.0 * ident / max(1, len(same_base))))
    print("   same-base frames equal length:   %d of %d (%.2f%%)" % (
        samelen, len(same_base), 100.0 * samelen / max(1, len(same_base))))
    if same_base:
        dl = collections.Counter(lc1[o]["frame_len"] - l016[o]["frame_len"] for o in same_base)
        print("   same-base frame-length delta histogram (top 12): %s" % (
            sorted(dl.items(), key=lambda kv: -kv[1])[:12]))
        tot_delta = sum(lc1[o]["frame_len"] - l016[o]["frame_len"] for o in same_base)
        print("   same-base total frame-length delta: %+d over %d objects" % (tot_delta, len(same_base)))
    if diff_base:
        d = collections.Counter()
        for o in diff_base:
            d["c1_bigger"] += lc1[o]["frame_len"] > l016[o]["frame_len"]
            d["equal"] += lc1[o]["frame_len"] == l016[o]["frame_len"]
            d["c1_smaller"] += lc1[o]["frame_len"] < l016[o]["frame_len"]
        print("   different-base frame-length comparison: %s" % dict(d))

    section("5. Q5 - zstd frame header census (lane 4)")
    for label, lane in (("v0.1.6", l016), ("T1", lc1)):
        hdrs = collections.Counter()
        bad = 0
        win = collections.Counter()
        fcs_ok = 0
        for o, r in lane.items():
            h = zstd_header(r["frame"])
            if h is None:
                bad += 1
                continue
            hdrs[(h["fcs_flag"], h["single_segment"], h["checksum"], h["did_flag"])] += 1
            win[h["window_log"]] += 1
            if h["fcs"] is not None and h["fcs"] == r["canonical_length"] - 23:
                fcs_ok += 1
        print("   %-8s bad magic %d" % (label, bad))
        print("            (fcs_flag, single_segment, checksum, dict_id_flag) -> count: %s" % dict(hdrs))
        print("            windowLog histogram: %s" % dict(sorted(win.items(), key=lambda kv: (kv[0] is None, kv[0]))))
        print("            FCS == canonical_length-23: %d of %d" % (fcs_ok, len(lane)))

    section("6. Base membership and chain depth (lane 4)")
    for label, s, lane in (("v0.1.6", s016, l016), ("T1", c1, lc1)):
        own = set(lane)
        allobj = set(s.objects)
        bases = [r["record_base"] if s.generation == "016" else r["column_base"] for r in lane.values()]
        bases = [b for b in bases if b is not None]
        inside_own = sum(1 for b in bases if b in own)
        inside_store = sum(1 for b in bases if b in allobj)
        print("   %-8s bases %d   in lane-4 set %d (%.2f%%)   anywhere in Store %d (%.2f%%)" % (
            label, len(bases), inside_own, 100.0 * inside_own / max(1, len(bases)),
            inside_store, 100.0 * inside_store / max(1, len(bases))))
        # chain depth by following record bases
        depth = {}
        def d(o, seen=()):
            if o in depth:
                return depth[o]
            if o in seen or len(seen) > 12:
                return 0
            r = lane.get(o)
            if r is None:
                return 0
            b = r["record_base"] if s.generation == "016" else r["column_base"]
            if b is None or b not in lane:
                depth[o] = 0
                return 0
            v = 1 + d(b, seen + (o,))
            depth[o] = v
            return v
        hist = collections.Counter(d(o) for o in lane)
        print("            chain depth histogram: %s" % dict(sorted(hist.items())))
        # pack ordering of bases
        same_pack = sum(1 for r in lane.values()
                        if (r["record_base"] if s.generation == "016" else r["column_base"]) is not None
                        and lane.get(r["record_base"] if s.generation == "016" else r["column_base"], {}).get("pack_id") == r["pack_id"])
        earlier_pack = sum(1 for r in lane.values()
                           if (r["record_base"] if s.generation == "016" else r["column_base"]) is not None
                           and (lane.get(r["record_base"] if s.generation == "016" else r["column_base"], {}).get("pack_id") or 10**9) < r["pack_id"])
        print("            base in same pack %d   base in a strictly earlier pack %d" % (same_pack, earlier_pack))

    section("7. Cross-check: C1 column base vs record tag")
    bad = 0
    for r in c1.lane4().values():
        if (r["column_base"] is None) != (r["tag"] == 0):
            bad += 1
        if r["column_base"] is not None and r["column_base"] != r["record_base"]:
            bad += 1
    print("   lane-4 rows where base_object_id and the in-record base disagree: %d" % bad)
    badall = 0
    for r in c1.objects.values():
        if r["lane_version"] == 4:
            continue
        if (r["column_base"] is None) != (r["tag"] == 0):
            badall += 1
    print("   non-lane-4 rows where base_object_id and the in-record tag disagree: %d" % badall)


if __name__ == "__main__":
    main()
