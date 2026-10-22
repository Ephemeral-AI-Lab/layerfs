#!/usr/bin/env python3
"""#420 format study: payload record re-encoding on a sample of packs.

usage: sim_payload.py DB OUT_JSON LO HI MOD
  samples payload packs (domain 1) with pack_id in [LO, HI) and pack_id % MOD == 0.
For FULL (tag 0) chunk and whole-file records: decode the frame, recompress at levels 3, 9, 19.
For STORED (tag 2) records: try level 3 and 19 (is it really incompressible?).
Solid: per whole-file pack, the FULL and STORED raw payloads of the pack concatenated and
compressed as one level-3 / level-19 frame, against the sum of their records today.
PREFIX (delta) records are counted, not re-encoded.
"""
import json
import struct
import sqlite3
import sys
import time
from collections import Counter
from compression import zstd

LANE = {18: "wf", 23: "wf", 20: "chunk", 25: "chunk", 17: "wf", 14: "wf", 11: "wf", 10: "chunk", 15: "chunk"}


def records(body, ver, length):
    gc = struct.unpack_from("<I", body, 12)[0]
    lane = LANE[ver]
    out = []
    if lane == "wf":
        starts = [struct.unpack_from("<I", body, 24 + 4 * i)[0] for i in range(gc)]
        views = [(starts[i], starts[i + 1] if i + 1 < gc else length) for i in range(gc)]
    else:
        views = []
        for i in range(gc):
            s, enc, dec, fl = struct.unpack_from("<IIII", body, 24 + 16 * i)
            views.append((s, s + enc))
    for s, e in views:
        g = body[s:e]
        n = struct.unpack_from("<I", g, 0)[0]
        ends = struct.unpack_from(f"<{n}I", g, 4)
        area, prev = 4 + 4 * n, 0
        for end in ends:
            out.append(g[area + prev: area + end])
            prev = end
    return lane, out


def main():
    db, outp, lo, hi, mod = sys.argv[1], sys.argv[2], int(sys.argv[3]), int(sys.argv[4]), int(sys.argv[5])
    t0 = time.time()
    con = sqlite3.connect(f"file:{db}?mode=ro", uri=True)
    c = Counter()
    for pid, length, body in con.execute(
            "SELECT pack_id, length, body FROM pack WHERE domain=1 AND pack_id>=? AND pack_id<? AND pack_id % ?=0",
            (lo, hi, mod)):
        ver = struct.unpack_from("<I", body, 8)[0]
        lane, recs = records(body, ver, length)
        c[f"{lane}_packs"] += 1
        c[f"{lane}_pack_bytes"] += length
        solid_raw = []
        solid_rec = 0
        for r in recs:
            tag = r[0]
            if lane == "wf":
                # compact whole-file record: tag, [base 32], frame (frame is a zstd frame with content size)
                hdr = 1 + (32 if tag == 1 else 0)
            else:
                hdr = 5 + (32 if tag == 1 else 0)
            frame = r[hdr:]
            k = f"{lane}_t{tag}"
            c[k + "_n"] += 1
            c[k + "_rec"] += len(r)
            if tag == 1:
                continue
            raw = frame if tag == 2 else zstd.decompress(frame)
            c[k + "_raw"] += len(raw)
            for lv in ((3, 9, 19) if tag == 0 else (3, 19)):
                c[f"{k}_L{lv}"] += hdr + len(zstd.compress(raw, level=lv))
            if lane == "wf":
                solid_raw.append(raw)
                solid_rec += len(r)
        if lane == "wf" and len(solid_raw) > 1:
            joined = b"".join(solid_raw)
            c["solid_packs"] += 1
            c["solid_records"] += len(solid_raw)
            c["solid_today"] += solid_rec
            c["solid_L3"] += len(zstd.compress(joined, level=3)) + 4 * len(solid_raw)
            c["solid_L19"] += len(zstd.compress(joined, level=19)) + 4 * len(solid_raw)
    res = {"lo": lo, "hi": hi, "mod": mod, "seconds": round(time.time() - t0, 1), "c": dict(c)}
    json.dump(res, open(outp, "w"), indent=1)
    print(json.dumps({"lo": lo, "seconds": res["seconds"]}))


if __name__ == "__main__":
    main()
