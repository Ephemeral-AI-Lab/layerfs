#!/usr/bin/env python3
"""#420 payload study (question C): per-object CPU cost of the Git loose-blob lever (Python bound).

usage: cost.py DUMPDIR CLASSIFY_DIR BLOB_ROWS_JSONL OUT_JSON MOD
Deterministic sample: loose blobs with an earlier WholeFile content match and index % MOD == 0.
Save side: detect + inflate + deflate at the recorded level + compare (reproduction proof) +
sha256 of the body + zstd-3 prefix frame against the matched raw payload.
Read side: zstd prefix decode + deflate at the recorded level (rebuild of the stored bytes).
Also the base read cost already paid today (stored record = copy) is noted as zero.
One sample per object, timings by time.perf_counter, single thread.
"""
import glob
import hashlib
import json
import os
import sys
import time
import zlib

sys.path.insert(0, __file__.rsplit("/", 1)[0])
from common import Raw, prefix_frame, WF_WLOG  # noqa: E402
from compression import zstd  # noqa: E402
from compression.zstd import ZstdDict  # noqa: E402


def main():
    d, cdir, browsp, outp, mod = sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4], int(sys.argv[5])
    raw = Raw(d)
    meta = {}
    for p in glob.glob(cdir + "/c-*.jsonl"):
        for line in open(p):
            if '"git":"blob"' in line:
                r = json.loads(line)
                meta[r["i"]] = r
    fhs = {}
    tot = {"n": 0, "kb_stored": 0.0, "kb_inflated": 0.0, "save_us": 0.0, "read_us": 0.0,
           "inflate_us": 0.0, "deflate_us": 0.0, "hash_us": 0.0, "penc_us": 0.0, "pdec_us": 0.0}
    sizes = []
    for line in open(browsp):
        b = json.loads(line)
        if b.get("match") != "earlier" or b.get("mkind") != "wf" or b["i"] % mod:
            continue
        r = meta[b["i"]]
        stored = raw.get(raw.rows[b["i"]])
        base = raw.get(raw.rows[b["mpos"]])
        if len(base) < 8:
            continue
        t0 = time.perf_counter()
        infl = zlib.decompress(stored)
        t1 = time.perf_counter()
        again = zlib.compress(infl, r["lvl"])
        t2 = time.perf_counter()
        assert again == stored
        h = hashlib.sha256(infl[r["hl"]:]).digest()
        t3 = time.perf_counter()
        fr = prefix_frame(infl, base, WF_WLOG)
        t4 = time.perf_counter()
        back = zstd.decompress(fr, zstd_dict=ZstdDict(base, is_raw=True).as_prefix)
        t5 = time.perf_counter()
        rebuilt = zlib.compress(back, r["lvl"])
        t6 = time.perf_counter()
        assert rebuilt == stored and h
        tot["n"] += 1
        tot["kb_stored"] += len(stored) / 1024
        tot["kb_inflated"] += len(infl) / 1024
        tot["inflate_us"] += (t1 - t0) * 1e6
        tot["deflate_us"] += (t2 - t1) * 1e6
        tot["hash_us"] += (t3 - t2) * 1e6
        tot["penc_us"] += (t4 - t3) * 1e6
        tot["pdec_us"] += (t5 - t4) * 1e6
        tot["save_us"] += (t4 - t0) * 1e6
        tot["read_us"] += (t6 - t4) * 1e6
        sizes.append(((t6 - t4) * 1e6, len(infl)))
    n = max(1, tot["n"])
    res = {k: round(v, 1) for k, v in tot.items()}
    res.update({"mean_save_us": round(tot["save_us"] / n, 1), "mean_read_us": round(tot["read_us"] / n, 1),
                "read_us_per_inflated_kb": round(tot["read_us"] / max(1, tot["kb_inflated"]), 2),
                "save_us_per_inflated_kb": round(tot["save_us"] / max(1, tot["kb_inflated"]), 2),
                "deflate_l1_us_per_kb": round(tot["deflate_us"] / max(1, tot["kb_inflated"]), 2),
                "max_read_us": round(max(s[0] for s in sizes), 1) if sizes else 0,
                "p50_inflated_bytes": sorted(s[1] for s in sizes)[len(sizes) // 2] if sizes else 0})
    json.dump(res, open(outp, "w"), indent=1)
    print(json.dumps(res, indent=1))


if __name__ == "__main__":
    main()
