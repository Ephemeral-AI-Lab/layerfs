#!/usr/bin/env python3
"""Squad A1 accounting: v0.1.6 vs v0.1.7 retained-history storage, stride-10.

Read-only. Diagnostic only; NOT admission evidence.

Inputs (both on disk, both read-only):
  A = /tmp/base187/sample.sqlite                       (v0.1.7, history-stride10)
  B = benchmark-results/repository-history/stride-10/
        deepseek-stride10/host-runtime/store.sqlite    (v0.1.6, history-stride10)

The v0.1.6 Store is the retained host-runtime Store of the #153 campaign
(ledger L31); the report records apparent 49,315,940 B, this file is
49,315,840 B -- a 100 B difference, reported rather than smoothed.
"""
import os
import sqlite3
import struct
import sys
from collections import defaultdict

MAGIC = b"LFPACK\x00\x00"
A = "/tmp/base187/sample.sqlite"
B = ("benchmark-results/repository-history/stride-10/deepseek-stride10/"
     "host-runtime/store.sqlite")
LANE = {1: "Ordinary", 2: "Native", 4: "WholeFile", 6: "PooledMetadata",
        3: "Small(v3)", 5: "Metadata(v5)", 7: "Singleton"}


def decode(pack_id, blob):
    if blob[:8] != MAGIC or len(blob) < 16:
        raise ValueError("pack %d: bad magic" % pack_id)
    version, count = struct.unpack_from("<II", blob, 8)
    out = []
    if version == 4:
        for i in range(count):
            s = struct.unpack_from("<I", blob, 16 + 4 * i)[0]
            e = (len(blob) if i + 1 == count
                 else struct.unpack_from("<I", blob, 16 + 4 * (i + 1))[0])
            out.append((s, e))
    else:
        for i in range(count):
            s, enc, dec = struct.unpack_from("<III", blob, 16 + 16 * i)
            out.append((s, s + enc))
    return version, count, out


def load(path):
    c = sqlite3.connect("file:%s?mode=ro" % path, uri=True)
    groups, pack_bytes, pack_count, framing = {}, 0, 0, 0
    for pid, blob in c.execute("SELECT pack_id, data FROM object_packs"):
        version, count, parts = decode(pid, blob)
        pack_bytes += len(blob)
        pack_count += 1
        framing += 16 + (4 if version == 4 else 16) * count
        for i, (s, e) in enumerate(parts):
            groups[(pid, i)] = (version, s, e, blob)
    objs = {}
    for oid, canon, pid, gn in c.execute(
            "SELECT object_id, canonical_length, pack_id, group_number FROM objects"):
        version, s, e, blob = groups[(pid, gn)]
        objs[oid] = {"canon": canon, "lane": version,
                     "tag": (blob[s] if version == 4 else None),
                     "stored": e - s, "group": (pid, gn)}
    return {"conn": c, "groups": groups, "objs": objs, "pack_bytes": pack_bytes,
            "pack_count": pack_count, "framing": framing, "path": path,
            "size": os.path.getsize(path), "blocks": os.stat(path).st_blocks * 512}


def lane_table(st):
    agg = defaultdict(lambda: [0, 0, set()])
    for o in st["objs"].values():
        a = agg[o["lane"]]
        a[0] += 1
        a[1] += o["canon"]
        a[2].add(o["group"])
    # pooled value groups are named by metadata_value_groups, not by objects
    pool = agg[6]
    for pid, gn in st["conn"].execute("SELECT pack_id, group_number FROM metadata_value_groups"):
        pool[2].add((pid, gn))
    rows = []
    for lane, (n, canon, gs) in sorted(agg.items()):
        stored = sum(st["groups"][g][2] - st["groups"][g][1] for g in gs)
        rows.append((LANE[lane], n, canon, stored))
    return rows


def whole_file(st):
    """Per-object classification of the v4 lane; tag 0 = FULL, else DELTA."""
    out = {}
    for oid, o in st["objs"].items():
        if o["lane"] == 4:
            out[oid] = ("DELTA" if o["tag"] != 0 else "FULL", o["canon"], o["tag"])
    return out


def main():
    a, b = load(A), load(B)
    print("=" * 78)
    print("A  v0.1.7  %s" % a["path"])
    print("B  v0.1.6  %s" % b["path"])
    print("=" * 78)
    for lbl, s in (("A v0.1.7", a), ("B v0.1.6", b)):
        print("%-9s apparent %12d   st_blocks*512 %12d   packs %4d   pack BLOBs %12d"
              % (lbl, s["size"], s["blocks"], s["pack_count"], s["pack_bytes"]))
    print("%-9s non-pack %12d   pack framing %12d   group bodies %12d"
          % ("A v0.1.7", a["size"] - a["pack_bytes"], a["framing"],
             a["pack_bytes"] - a["framing"]))
    print("%-9s non-pack %12d   pack framing %12d   group bodies %12d"
          % ("B v0.1.6", b["size"] - b["pack_bytes"], b["framing"],
             b["pack_bytes"] - b["framing"]))
    print()
    print("--- lane tables (stored = distinct group bodies; the v4 lanes are one")
    print("    record per group, so a group body belongs to one object) ---")
    for lbl, s in (("A v0.1.7", a), ("B v0.1.6", b)):
        print("%s:" % lbl)
        tot = [0, 0, 0]
        for lane, n, canon, stored in lane_table(s):
            print("   %-15s %6d obj %12d canonical %12d stored  %6.2fx"
                  % (lane, n, canon, stored, canon / stored if stored else 0))
            tot[0] += n; tot[1] += canon; tot[2] += stored
        print("   %-15s %6d obj %12d canonical %12d stored  %6.2fx"
              % ("TOTAL", tot[0], tot[1], tot[2], tot[1] / tot[2]))
    print()
    wa, wb = whole_file(a), whole_file(b)
    print("--- whole-file (v4) lane ---")
    for lbl, w in (("A v0.1.7", wa), ("B v0.1.6", wb)):
        d = [v for v in w.values() if v[0] == "DELTA"]
        f = [v for v in w.values() if v[0] == "FULL"]
        print("   %s  DELTA %6d obj %12d canon  (%.2f%% of bytes)   FULL %6d obj %12d canon"
              % (lbl, len(d), sum(x[1] for x in d),
                 100.0 * sum(x[1] for x in d) / sum(x[1] for x in w.values()),
                 len(f), sum(x[1] for x in f)))
    common = set(wa) & set(wb)
    sa = {o["group"]: o["stored"] for o in a["objs"].values() if o["lane"] == 4}
    sb = {o["group"]: o["stored"] for o in b["objs"].values() if o["lane"] == 4}
    ca = sum(a["objs"][k]["canon"] for k in common)
    cd6 = sum(wb[k][1] for k in common if wb[k][0] == "DELTA")
    cf6 = sum(wb[k][1] for k in common if wb[k][0] == "FULL")
    cd7 = sum(wa[k][1] for k in common if wa[k][0] == "DELTA")
    cf7 = sum(wa[k][1] for k in common if wa[k][0] == "FULL")
    sd6 = sum(sb[b["objs"][k]["group"]] for k in common if wb[k][0] == "DELTA")
    sf6 = sum(sb[b["objs"][k]["group"]] for k in common if wb[k][0] == "FULL")
    sd7 = sum(sa[a["objs"][k]["group"]] for k in common if wa[k][0] == "DELTA")
    sf7 = sum(sa[a["objs"][k]["group"]] for k in common if wa[k][0] == "FULL")
    print()
    print("--- per-class rates over the %d objects both Stores hold ---" % len(common))
    print("   v0.1.6 DELTA %12d canon -> %10d stored = %.9f B/B (%.2fx)"
          % (cd6, sd6, sd6 / cd6, cd6 / sd6))
    print("   v0.1.6 FULL  %12d canon -> %10d stored = %.9f B/B (%.2fx)"
          % (cf6, sf6, sf6 / cf6, cf6 / sf6))
    print("   v0.1.7 DELTA %12d canon -> %10d stored = %.9f B/B (%.2fx)"
          % (cd7, sd7, sd7 / cd7, cd7 / sd7))
    print("   v0.1.7 FULL  %12d canon -> %10d stored = %.9f B/B (%.2fx)"
          % (cf7, sf7, sf7 / cf7, cf7 / sf7))
    print("   FULL rate ratio v0.1.7/v0.1.6 = %.6f   (below 1.0 means v0.1.7 is BETTER)"
          % ((sf7 / cf7) / (sf6 / cf6)))
    print()
    print("--- confusion: v0.1.6 class (kind) x v0.1.7 class ---")
    tab = defaultdict(lambda: [0, 0])
    for k in common:
        key = ("kind%d" % wb[k][2], wa[k][0])
        tab[key][0] += 1
        tab[key][1] += wa[k][1]
    for key in sorted(tab):
        print("   %-8s -> %-6s %6d obj %12d canonical" % (key[0], key[1], tab[key][0], tab[key][1]))
    print()
    print("--- counterfactual: v0.1.7's own objects priced at v0.1.6's per-class rate ---")
    pred = sum((sd6 / cd6) if wb[k][0] == "DELTA" else (sf6 / cf6) for k in common
               for _ in [0]) if False else 0.0
    pred = 0.0
    for k in common:
        pred += wa[k][1] * ((sd6 / cd6) if wb[k][0] == "DELTA" else (sf6 / cf6))
    actual = sa and sum(sa[a["objs"][k]["group"]] for k in common)
    print("   predicted stored %12.0f   actual v0.1.7 %12d   residual %12.0f"
          % (pred, actual, actual - pred))
    print()
    print("--- reconciliation of the apparent-byte difference ---")
    ta = {r[0]: r for r in lane_table(a)}
    tb = {r[0]: r for r in lane_table(b)}
    da = a["size"] - b["size"]
    print("   apparent   A %12d   B %12d   delta %12d  (%.4fx)"
          % (a["size"], b["size"], da, a["size"] / b["size"]))
    parts = []
    for lane in sorted(set(ta) | set(tb)):
        x = ta.get(lane, (lane, 0, 0, 0))[3]
        y = tb.get(lane, (lane, 0, 0, 0))[3]
        parts.append((lane, x - y))
    parts.append(("pack framing", a["framing"] - b["framing"]))
    parts.append(("non-pack (SQLite)", (a["size"] - a["pack_bytes"]) - (b["size"] - b["pack_bytes"])))
    tot = 0
    for lane, d in parts:
        tot += d
        print("   %-18s %12d" % (lane, d))
    print("   %-18s %12d   (check against %d)" % ("SUM", tot, da))
    print()
    wf = [d for lane, d in parts if lane == "WholeFile"][0]
    print("   whole-file share of the excess = %d / %d = %.4f"
          % (wf, da, wf / da))


if __name__ == "__main__":
    main()
