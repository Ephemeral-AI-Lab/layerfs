#!/usr/bin/env python3
"""#420 payload study: stored-byte share of every payload record (FULL, STORED and PREFIX) by content
class of its file head (WholeFile: own raw payload; Chunk: its file's first chunk). Git classes
are content-identified (no path names).

usage: share.py RECEIPTS_DIR
"""
import json
import os
import pickle
import sys
from collections import defaultdict

sys.path.insert(0, __file__.rsplit("/", 1)[0])
from common import Raw, classify, GIT_HDR  # noqa: E402
import zlib  # noqa: E402


def main():
    R = sys.argv[1]
    raw = Raw(os.path.join(R, "dump"))
    ext = pickle.load(open(os.path.join(R, "dump/extents.pkl"), "rb"))
    pos = {r["id"]: i for i, r in enumerate(raw.rows)}
    t = defaultdict(lambda: [0, 0])
    for i, r in enumerate(raw.rows):
        if r["role"] == 1:
            h = raw.get(r)[:600]
        else:
            h = raw.get(raw.rows[pos[ext[bytes.fromhex(r["id"])][0].hex()]])[:600]
        k = classify(h)
        if k == "zlib":
            try:
                d = zlib.decompressobj().decompress(h, 64)
                k = "git-loose" if GIT_HDR.match(d) else "zlib-other"
            except zlib.error:
                k = "zlib-other"
        key = (("WF" if r["role"] == 1 else "Chunk"), k, "prefix" if r["tag"] == 1 else "full")
        t[key][0] += 1
        t[key][1] += r["rec"]
    tot = sum(v[1] for v in t.values())
    gitb = sum(v[1] for k, v in t.items() if k[1] in ("git-loose", "git-pack", "git-idx", "git-index"))
    for k, v in sorted(t.items(), key=lambda kv: -kv[1][1]):
        print(f"{k[0]:6} {k[1]:22} {k[2]:7} {v[0]:8} {v[1]/1e6:10.2f}")
    print(json.dumps({"payload_record_bytes": tot, "git_content_bytes": gitb, "git_share": gitb / tot}))


if __name__ == "__main__":
    main()
