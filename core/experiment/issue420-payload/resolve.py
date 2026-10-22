#!/usr/bin/env python3
"""#420 payload study: resolve PREFIX records into raw payloads and build one index.

usage: resolve.py DUMPDIR
Reads meta-*.jsonl.gz / raw-*.bin / delta-*.bin written by dump.py and writes, into DUMPDIR:
  resolved.bin  raw payload of every PREFIX record (decoded with its base raw payload as a zstd prefix)
  index.pkl     list of records in insertion order (pack, group, record), each a dict with
                id, role, tag, base, rec, raw, depth, src ('raw-LO.bin' or 'resolved.bin'), off
Also prints the base-order check: PREFIX records whose base sits at a later (pack, group, record).
"""
import glob
import gzip
import json
import mmap
import os
import pickle
import sys
import time
from collections import Counter
from compression import zstd
from compression.zstd import ZstdDict


def main():
    d = sys.argv[1]
    t0 = time.time()
    rows = []
    maps = {}
    for mp in sorted(glob.glob(os.path.join(d, "meta-*.jsonl.gz")), key=lambda p: int(p.split("-")[-1].split(".")[0])):
        lo = mp.split("-")[-1].split(".")[0]
        rp = os.path.join(d, f"raw-{lo}.bin")
        dp = os.path.join(d, f"delta-{lo}.bin")
        for f in (rp, dp):
            if os.path.getsize(f):
                fh = open(f, "rb")
                maps[os.path.basename(f)] = mmap.mmap(fh.fileno(), 0, access=mmap.ACCESS_READ)
        with gzip.open(mp, "rt") as fh:
            for line in fh:
                r = json.loads(line)
                r["k"] = tuple(r["k"])
                if r["tag"] == 1:
                    r["dsrc"] = f"delta-{lo}.bin"
                else:
                    r["src"] = f"raw-{lo}.bin"
                rows.append(r)
    rows.sort(key=lambda r: r["k"])
    by = {r["id"]: r for r in rows}
    out = open(os.path.join(d, "resolved.bin"), "wb")
    roff = 0
    cache = {}
    stats = Counter()

    rd = open(os.path.join(d, "resolved.bin"), "rb")

    def raw_of(r):
        if r["id"] in cache:
            return cache[r["id"]]
        if r["src"] == "resolved.bin":
            out.flush()
            rd.seek(r["off"])
            return rd.read(r["raw"])
        m = maps[r["src"]]
        return m[r["off"]: r["off"] + r["raw"]]

    def resolve(r):
        nonlocal roff
        stack = []
        cur = r
        while cur["tag"] == 1 and cur["off"] < 0 and cur["id"] not in cache:
            stack.append(cur)
            cur = by[cur["base"]]
        for x in reversed(stack):
            b = by[x["base"]]
            base = raw_of(b)
            frame = maps[x["dsrc"]][x["doff"]: x["doff"] + x["dlen"]]
            raw = zstd.decompress(frame, zstd_dict=ZstdDict(base, is_raw=True).as_prefix)
            assert len(raw) == x["raw"], (x["id"], len(raw), x["raw"])
            x["depth"] = b.get("depth", 0) + 1
            cache[x["id"]] = raw
            out.write(raw)
            x["off"] = roff
            x["src"] = "resolved.bin"
            roff += len(raw)
            stats[f"role{x['role']}_resolved"] += 1
            if b["k"] > x["k"]:
                stats[f"role{x['role']}_base_later"] += 1
            if b["role"] != x["role"]:
                stats["cross_role_base"] += 1
        # keep the cache bounded: raws are now in resolved.bin
        if len(cache) > 20000:
            cache.clear()

    for r in rows:
        if r["tag"] != 1:
            r["depth"] = 0
    # cache entries hold raw for in-flight chains; after write, re-read through resolved map
    for r in rows:
        if r["tag"] == 1 and r["off"] < 0:
            resolve(r)
    out.close()
    for r in rows:
        r.pop("dsrc", None)
        r.pop("doff", None)
        r.pop("dlen", None)
    depth = Counter((r["role"], r["depth"]) for r in rows)
    pickle.dump(rows, open(os.path.join(d, "index.pkl"), "wb"))
    res = {"records": len(rows), "resolved_bytes": roff, "stats": dict(stats),
           "depth_hist": {f"{k[0]}:{k[1]}": v for k, v in sorted(depth.items())},
           "seconds": round(time.time() - t0, 1)}
    print(json.dumps(res))


if __name__ == "__main__":
    main()
