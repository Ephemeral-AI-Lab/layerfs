#!/usr/bin/env python3
"""#420 payload study (question A3): Git loose objects against existing canonical file content.

usage: blobmatch.py DUMPDIR CLASSIFY_DIR OUT_JSON OUT_ROWS_JSONL
Content hashes: sha256 of every WholeFile raw payload (position = its index) and of every chunked
file reassembled from its extent leaves (position = largest index among its chunks).
For every inflated Git loose blob: exact match of its content (after the 'blob N\\0' header) to a file
content seen EARLIER in insertion order, or ANYWHERE. For a WholeFile match the delta record is
measured: inflated object compressed with the match's raw payload as zstd prefix (level 3, whole-file
frame) -> width tag + base id + level byte + frame. Duplicate inflated objects (same Git id stored
twice) are counted. Per-blob rows go to OUT_ROWS_JSONL.
"""
import glob
import hashlib
import json
import os
import pickle
import sys
import time
from collections import Counter

sys.path.insert(0, __file__.rsplit("/", 1)[0])
from common import Raw, prefix_frame, WF_WLOG  # noqa: E402


def main():
    d, cdir, outp, rowsp = sys.argv[1:5]
    t0 = time.time()
    raw = Raw(d)
    idx = {r["id"]: i for i, r in enumerate(raw.rows)}
    content = {}  # sha256 -> (pos, kind, ref)
    for i, r in enumerate(raw.rows):
        if r["role"] != 1:
            continue
        h = hashlib.sha256(raw.get(r)).digest()
        if h not in content or content[h][0] > i:
            content[h] = (i, "wf", i)
    t_wf = time.time() - t0
    files = pickle.load(open(os.path.join(d, "extents-files.pkl"), "rb"))
    for root, seq in files.items():
        hs = hashlib.sha256()
        pos = 0
        for cid, so, ln in seq:
            j = idx[cid.hex()]
            pos = max(pos, j)
            hs.update(raw.get(raw.rows[j])[so:so + ln])
        h = hs.digest()
        if h not in content or content[h][0] > pos:
            content[h] = (pos, "chunked", root.hex())
    t_files = time.time() - t0 - t_wf
    rows = []
    for p in sorted(glob.glob(cdir + "/c-*.jsonl")):
        rows += [json.loads(line) for line in open(p) if '"git"' in line]
    c = Counter()
    seen_sha1 = {}
    out = open(rowsp, "w")
    imaps = {}
    for r in rows:
        if not r.get("git"):
            continue
        kind = r["git"]
        c[f"{kind}_n"] += 1
        c[f"{kind}_rec"] += r["rec"]
        if r["isha1"] in seen_sha1:
            c[f"{kind}_dup_inflated_n"] += 1
            c[f"{kind}_dup_inflated_rec"] += r["rec"]
        seen_sha1.setdefault(r["isha1"], r["i"])
        if kind != "blob":
            continue
        m = content.get(bytes.fromhex(r["bsha256"]))
        o = {"i": r["i"], "rec": r["rec"], "ilen": r["ilen"], "iz3": r["iz3"], "lvl": r["lvl"]}
        if m is None:
            c["blob_nomatch_n"] += 1
            c["blob_nomatch_rec"] += r["rec"]
            c["blob_nomatch_iz3"] += r["iz3"]
        else:
            when = "earlier" if m[0] < r["i"] else "later"
            o["match"], o["mkind"], o["mpos"] = when, m[1], m[0]
            c[f"blob_{when}_{m[1]}_n"] += 1
            c[f"blob_{when}_{m[1]}_rec"] += r["rec"]
            if m[1] == "wf":
                f = r["ifile"]
                if f not in imaps:
                    imaps[f] = open(os.path.join(d, f), "rb")
                fh = imaps[f]
                fh.seek(r["ioff"])
                infl = fh.read(r["ilen"])
                base = raw.get(raw.rows[m[2]])
                if len(base) and r["ilen"] <= 131072 + 64:
                    t1 = time.perf_counter()
                    fr = prefix_frame(infl, base, WF_WLOG)
                    o["us"] = round((time.perf_counter() - t1) * 1e6, 1)
                    w = 1 + 32 + 1 + len(fr)
                    o["dw"] = w
                    c[f"blob_{when}_wf_delta_width"] += w
                    c[f"blob_{when}_wf_delta_measured_n"] += 1
                    c[f"blob_{when}_wf_delta_measured_rec"] += r["rec"]
        out.write(json.dumps(o) + "\n")
    out.close()
    res = {"content_hashes": len(content), "seconds_wf_hash": round(t_wf, 1), "seconds_files": round(t_files, 1),
           "seconds": round(time.time() - t0, 1), "c": dict(sorted(c.items()))}
    json.dump(res, open(outp, "w"), indent=1)
    print(json.dumps(res, indent=1))


if __name__ == "__main__":
    main()
