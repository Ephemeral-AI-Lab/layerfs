#!/usr/bin/env python3
"""#420 payload study: summarise classify.py rows (question A tables).

usage: sum_a.py CLASSIFY_DIR
"""
import glob
import json
import sys
from collections import defaultdict


def main():
    rows = []
    for p in sorted(glob.glob(sys.argv[1] + "/c-*.jsonl")):
        rows += [json.loads(line) for line in open(p)]
    t = defaultdict(lambda: [0, 0, 0])
    for r in rows:
        for key in ((r["role"], r["sub"], r["cls"]), (r["role"], r["sub"], "ALL"), (r["role"], "ALL", r["cls"])):
            a = t[key]
            a[0] += 1
            a[1] += r["rec"]
            a[2] += r["raw"]
    role = {1: "WholeFile", 2: "Chunk"}
    print("A1 class table: records, stored MB, raw MB (sub: stored=tag2, poor=FULL rec/raw>0.9, good=other FULL)")
    for k in sorted(t, key=lambda k: (k[0], k[1], -t[k][1])):
        n, s, w = t[k]
        print(f"{role[k[0]]:9} {k[1]:6} {k[2]:22} {n:8} {s/1e6:10.2f} {w/1e6:10.2f}")
    # zlib / gzip anatomy (WholeFile only)
    print("\nA2 zlib/gzip WholeFile records (all subs)")
    z = defaultdict(lambda: defaultdict(int))
    for r in rows:
        if r["role"] != 1 or r["cls"] not in ("zlib", "gzip"):
            continue
        g = z[(r["cls"], r.get("git") or "-")]
        g["n"] += 1
        g["rec"] += r["rec"]
        g["raw"] += r["raw"]
        if r.get("complete"):
            g["complete"] += 1
            g["ilen"] += r["ilen"]
            g["iz3"] += r["iz3"]
            if r["lvl"]:
                g["repro_n"] += 1
                g["repro_rec"] += r["rec"]
                g["repro_iz3"] += r["iz3"]
                g[f"lvl{r['lvl']}_{r['how']}"] += 1
    for k, g in sorted(z.items()):
        print(k, json.dumps(dict(g)))


if __name__ == "__main__":
    main()
