#!/usr/bin/env python3
"""#420 payload study (question A3, Git trees/commits): resemblance deltas between inflated loose objects.

usage: trees.py DUMPDIR CLASSIFY_DIR OUT_JSON
For inflated Git loose 'tree' and 'commit' objects (WholeFile, reproducible), in insertion order:
super-feature index (features.py) over earlier inflated objects of the same Git type; one trial with
the best candidate; new width = tag + base id + level byte + zstd-3 prefix frame (whole-file
parameters), else tag + level byte + zstd-3 frame of the inflated object; counted only when smaller
than the stored record. Also measures timings (inflate, deflate level 1 rebuild, prefix encode/decode).
"""
import glob
import json
import os
import sys
import time
import zlib
from collections import Counter

sys.path.insert(0, __file__.rsplit("/", 1)[0])
from common import full_frame, prefix_frame, WF_WLOG  # noqa: E402
from features import feats, superf  # noqa: E402


def main():
    d, cdir, outp = sys.argv[1:4]
    rows = []
    for p in glob.glob(cdir + "/c-*.jsonl"):
        rows += [json.loads(line) for line in open(p) if '"git":"tree"' in line or '"git":"commit"' in line]
    rows.sort(key=lambda r: r["i"])
    fhs = {}
    c = Counter()
    index = {}
    t_inf = t_def = 0.0

    def inflated(r):
        f = r["ifile"]
        if f not in fhs:
            fhs[f] = open(os.path.join(d, f), "rb")
        fhs[f].seek(r["ioff"])
        return fhs[f].read(r["ilen"])

    stored = {}
    for r in rows:
        g = r["git"]
        data = inflated(r)
        # timing of the read-side rebuild (inflate is the Save side)
        t1 = time.perf_counter()
        z = zlib.compress(data, r["lvl"] or 1)
        t_def += time.perf_counter() - t1
        t1 = time.perf_counter()
        zlib.decompress(z)
        t_inf += time.perf_counter() - t1
        c[f"{g}_n"] += 1
        c[f"{g}_rec"] += r["rec"]
        c[f"{g}_ilen"] += len(data)
        best = 1 + 1 + len(full_frame(data, WF_WLOG))
        f = feats(data)
        if f is not None:
            s = superf(f)
            cand = Counter()
            for j in range(3):
                h = index.get((g, j, int(s[j])))
                if h is not None:
                    cand[h] += 1
            if cand:
                b = sorted(cand.items(), key=lambda kv: (-kv[1], -kv[0]))[0][0]
                w = 1 + 32 + 1 + len(prefix_frame(data, stored[b], WF_WLOG))
                c[f"{g}_trials"] += 1
                if w < best:
                    best = w
                    c[f"{g}_delta"] += 1
            for j in range(3):
                index[(g, j, int(s[j]))] = r["i"]
        stored[r["i"]] = data
        if r["lvl"] and best < r["rec"]:
            c[f"{g}_converted"] += 1
            c[f"{g}_saved"] += r["rec"] - best
    res = {"c": dict(sorted(c.items())),
           "deflate_rebuild_us_per_kb": round(t_def * 1e6 / max(1, (c["tree_ilen"] + c["commit_ilen"]) / 1024), 2),
           "inflate_us_per_kb": round(t_inf * 1e6 / max(1, (c["tree_ilen"] + c["commit_ilen"]) / 1024), 2)}
    json.dump(res, open(outp, "w"), indent=1)
    print(json.dumps(res, indent=1))


if __name__ == "__main__":
    main()
