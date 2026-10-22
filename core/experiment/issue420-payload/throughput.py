#!/usr/bin/env python3
"""#420 payload study (question C): single-thread codec throughput on fixture payload bytes.

usage: throughput.py RAWFILE INFLATEDFILE MAXBYTES
zstd level 3 full frame (chunk parameters) on 16 KiB pieces of RAWFILE (today's per-record cost),
zstd decode of those frames, and zlib deflate at levels 1 and 6 / inflate on 16 KiB pieces of
INFLATEDFILE (inflated Git objects). One pass each; microseconds per KiB.
"""
import json
import sys
import time
import zlib

sys.path.insert(0, __file__.rsplit("/", 1)[0])
from common import full_frame, CHUNK_WLOG  # noqa: E402
from compression import zstd  # noqa: E402


def main():
    rawf, inff, mx = sys.argv[1], sys.argv[2], int(sys.argv[3])
    res = {}
    for name, path in (("raw", rawf), ("inflated", inff)):
        data = open(path, "rb").read(mx)
        pieces = [data[i:i + 16384] for i in range(0, len(data) - 16384, 16384)]
        kb = len(pieces) * 16
        if name == "raw":
            t = time.perf_counter()
            frames = [full_frame(p, CHUNK_WLOG) for p in pieces]
            res["zstd3_encode_us_per_kb"] = round((time.perf_counter() - t) * 1e6 / kb, 2)
            t = time.perf_counter()
            for f in frames:
                zstd.decompress(f)
            res["zstd_decode_us_per_kb"] = round((time.perf_counter() - t) * 1e6 / kb, 2)
        else:
            for lvl in (1, 6):
                t = time.perf_counter()
                zs = [zlib.compress(p, lvl) for p in pieces]
                res[f"deflate_l{lvl}_us_per_kb"] = round((time.perf_counter() - t) * 1e6 / kb, 2)
            t = time.perf_counter()
            for z in zs:
                zlib.decompress(z)
            res["inflate_us_per_kb"] = round((time.perf_counter() - t) * 1e6 / kb, 2)
        res[f"{name}_kb"] = kb
    print(json.dumps(res))


if __name__ == "__main__":
    main()
