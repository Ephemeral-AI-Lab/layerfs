#!/usr/bin/env python3
"""V1 base-distance forensic: where does each delta base sit in the corpus?

Read-only, diagnostic. Joins the two Stores' lane-4 records to the corpus oid map
produced by oidtool (ObjectId -> [(checkpoint index, path hex)]).
"""
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


def load_map():
    occ = collections.defaultdict(list)
    with open(OIDMAP) as handle:
        for line in handle:
            oid, ck, path = line.split()
            occ[oid].append((int(ck), path))
    return occ


def main():
    occ = load_map()
    s016 = Store(V016, "016")
    c1 = Store(C1, "c1")
    # v1_lib keys records by the raw 32-byte ObjectId; the map is hex-keyed.
    l016 = {o.hex(): r for o, r in s016.lane4().items()}
    lc1 = {o.hex(): r for o, r in c1.lane4().items()}

    # ---- state membership -------------------------------------------------
    states = {}
    for oid in l016:
        cks = sorted({ck for ck, _ in occ[oid]})
        sel = [ck for ck in cks if ck in ORDINAL]
        states[oid] = (cks, sel)

    missing = [o for o in l016 if not states[o][1]]
    print("lane-4 objects with no selected-state occurrence in the corpus map: %d" % len(missing))

    # ---- pack -> state check ---------------------------------------------
    print()
    print("pack_id -> min selected state (lane 4), to check one-save-per-pack:")
    for label, lane in (("v0.1.6", l016), ("T1", lc1)):
        bypack = collections.defaultdict(set)
        for o, r in lane.items():
            sel = states[o][1]
            bypack[r["pack_id"]].add(sel[0] if sel else -1)
        multi = sum(1 for v in bypack.values() if len(v) > 1)
        print("   %-8s packs %d   packs whose objects span >1 selected state: %d" % (
            label, len(bypack), multi))
        span = collections.Counter()
        for v in bypack.values():
            span[len(v)] += 1
        print("            distinct-state-count histogram over packs: %s" % dict(sorted(span.items())))

    # ---- base distance ----------------------------------------------------
    def analyse(label, lane, gen):
        rows = []
        for o, r in lane.items():
            base = r["record_base"] if gen == "016" else r["column_base"]
            if base is None:
                continue
            base = base.hex()
            xs, xsel = states[o]
            bs, bsel = states[base]
            if not xsel or not bsel:
                rows.append((o, base, None, None, None, None))
                continue
            save_state = xsel[0]
            earlier = [ck for ck in bsel if ck < save_state]
            base_state = max(earlier) if earlier else None
            same_path = None
            if base_state is not None:
                xpaths = {p for ck, p in occ[o] if ck == save_state}
                bpaths = {p for ck, p in occ[base] if ck == base_state}
                same_path = bool(xpaths & bpaths)
            rows.append((o, base, save_state, base_state, same_path, r["body_bytes"]))
        return rows

    for label, lane, gen in (("v0.1.6", l016, "016"), ("T1", lc1, "c1")):
        rows = analyse(label, lane, gen)
        print()
        print("=" * 78)
        print("BASE DISTANCE - %s (lane 4, %d delta records)" % (label, len(rows)))
        print("=" * 78)
        nod = [r for r in rows if r[3] is None]
        print("   base with no earlier selected state: %d" % len(nod))
        ok = [r for r in rows if r[3] is not None]
        gap = collections.Counter()
        bytes_by_gap = collections.Counter()
        obj_by_gap = collections.Counter()
        for o, base, ss, bs, sp, body in ok:
            g = ss - bs
            gap[g] += 1
            bytes_by_gap[g] += body
            obj_by_gap[g] += lane[o]["canonical_length"]
        print("   checkpoint-index gap (full157) -> records / stored B / canonical B:")
        for g in sorted(gap):
            print("      %3d  %6d  %10d  %12d" % (g, gap[g], bytes_by_gap[g], obj_by_gap[g]))
        sp = sum(1 for r in ok if r[4])
        print("   same-path base: %d of %d (%.2f%%)   cross-path: %d (%.2f%%)" % (
            sp, len(ok), 100.0 * sp / len(ok), len(ok) - sp, 100.0 * (len(ok) - sp) / len(ok)))
        spb = sum(r[5] for r in ok if r[4])
        cpb = sum(r[5] for r in ok if not r[4])
        print("   stored B on same-path bases %d   on cross-path bases %d" % (spb, cpb))
        # distance in selected-state ordinals
        og = collections.Counter()
        for o, base, ss, bs, sp2, body in ok:
            og[ORDINAL[ss] - ORDINAL[bs]] += 1
        print("   ordinal (selected-state) gap histogram: %s" % dict(sorted(og.items())))
        # nearest-version availability: for each object, is there ANY earlier selected
        # state where the SAME PATH holds a different object?
        nearest = collections.Counter()
        for o, r in lane.items():
            xs, xsel = states[o]
            if not xsel:
                continue
            ss = xsel[0]
            paths = {p for ck, p in occ[o] if ck == ss}
            found = None
            for ck in reversed([c for c in SELECTION if c < ss]):
                others = [oid2 for oid2 in [] ]
                found = None
                break
            nearest[0] += 0
        # same-path previous selected state (what the T1 caller declares)
        prevsel = {}
        for o, r in lane.items():
            xs, xsel = states[o]
            if not xsel:
                continue
            ss = xsel[0]
            i = ORDINAL[ss]
            prevsel[o] = SELECTION[i - 1] if i > 0 else None
        # Build path -> {checkpoint: oid} index restricted to lane-4 objects
        path_at = collections.defaultdict(dict)
        for o in lane:
            for ck, p in occ[o]:
                if ck in ORDINAL:
                    path_at[(p, ck)] = o
        have_prev_same_path = 0
        for o, r in lane.items():
            xs, xsel = states[o]
            if not xsel:
                continue
            ss = xsel[0]
            ps = prevsel[o]
            if ps is None:
                continue
            for p in {p for ck, p in occ[o] if ck == ss}:
                if (p, ps) in path_at:
                    have_prev_same_path += 1
                    break
        print("   objects whose own path exists at the previous SELECTED state: %d of %d" % (
            have_prev_same_path, len(lane)))

    # ---- Q1/Q4: the objects v0.1.6 deltas and we store FULL ---------------
    print()
    print("=" * 78)
    print("THE 2,594 OBJECTS v0.1.6 DELTAS AND T1 STORES FULL")
    print("=" * 78)
    q1 = [o for o in l016 if l016[o]["tag"] in (1, 2) and lc1[o]["tag"] == 0]
    print("   objects %d   canonical %d   v0.1.6 stored %d   T1 stored %d" % (
        len(q1), sum(l016[o]["canonical_length"] for o in q1),
        sum(l016[o]["body_bytes"] for o in q1), sum(lc1[o]["body_bytes"] for o in q1)))
    # what base did v0.1.6 use, and where is it?
    kind = collections.Counter()
    kbytes = collections.Counter()
    for o in q1:
        base = l016[o]["record_base"].hex()
        xs, xsel = states[o]
        bs, bsel = states[base]
        ss = xsel[0] if xsel else None
        earlier = [ck for ck in bsel if ss is not None and ck < ss]
        bstate = max(earlier) if earlier else None
        if bstate is None:
            kind["base not earlier"] += 1
            continue
        xpaths = {p for ck, p in occ[o] if ck == ss}
        bpaths = {p for ck, p in occ[base] if ck == bstate}
        tag = l016[o]["tag"]
        if xpaths & bpaths:
            # same path: is the base the previous selected state, or nearer?
            key = "tag%d same-path gap=%d" % (tag, ss - bstate)
        else:
            key = "tag%d cross-path" % tag
        kind[key] += 1
        kbytes[key] += lc1[o]["body_bytes"]
    for k in sorted(kind):
        print("      %-32s %6d  T1 stored %10d" % (k, kind[k], kbytes[k]))

    # ---- Q4: does the T1 caller declare what v0.1.6 used? -----------------
    print()
    print("=" * 78)
    print("AVAILABILITY: is v0.1.6's chosen base a same-path earlier selected state?")
    print("=" * 78)
    for label, lane, gen in (("v0.1.6", l016, "016"), ("T1", lc1, "c1")):
        n = 0
        samepath_prev = 0
        samepath_anyearlier = 0
        crosspath = 0
        for o, r in lane.items():
            base = r["record_base"] if gen == "016" else r["column_base"]
            if base is None:
                continue
            base = base.hex()
            xs, xsel = states[o]
            if not xsel:
                continue
            ss = xsel[0]
            bsel = [ck for ck in states[base][1] if ck < ss]
            if not bsel:
                continue
            bstate = max(bsel)
            n += 1
            xpaths = {p for ck, p in occ[o] if ck == ss}
            bpaths = {p for ck, p in occ[base] if ck == bstate}
            if xpaths & bpaths:
                samepath_anyearlier += 1
                i = ORDINAL[ss]
                if i > 0 and bstate == SELECTION[i - 1]:
                    samepath_prev += 1
            else:
                crosspath += 1
        print("   %-8s delta %d   same-path & immediately previous selected state %d (%.2f%%)" % (
            label, n, samepath_prev, 100.0 * samepath_prev / max(1, n)))
        print("            same-path but an earlier state %d (%.2f%%)   cross-path %d (%.2f%%)" % (
            samepath_anyearlier - samepath_prev,
            100.0 * (samepath_anyearlier - samepath_prev) / max(1, n),
            crosspath, 100.0 * crosspath / max(1, n)))


if __name__ == "__main__":
    main()
