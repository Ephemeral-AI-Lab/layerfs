#!/usr/bin/env python3
"""#420 format study: re-encode decoded ordinary (metadata) group bodies.

usage: sim_meta_codec.py MODE OUT_JSON DUMP...
  MODE level:L          every group at zstd level L (python compression.zstd, content size on)
  MODE dict:SIZE:L      dictionary of SIZE bytes trained on TRAIN groups (every 8th group of the
                        first dump, used for training only), every group of the other dumps compressed at level L with it
Dumps are anatomy.py META_DUMP files: (u32 pack, u8 group, u32 len, body)*.
Reports total encoded bytes, decoded bytes, groups, and compress / decompress ns per byte.
"""
import json
import struct
import sys
import time
from compression import zstd


def groups(path):
    with open(path, "rb") as f:
        data = f.read()
    off = 0
    while off < len(data):
        pid, gi, n = struct.unpack_from("<IBI", data, off)
        off += 9
        yield data[off:off + n]
        off += n


def main():
    mode, out = sys.argv[1], sys.argv[2]
    dumps = sys.argv[3:]
    parts = mode.split(":")
    zd = None
    t_train = 0.0
    if parts[0] == "dict":
        size, level = int(parts[1]), int(parts[2])
        train = [g for i, g in enumerate(groups(dumps[0])) if i % 8 == 0]
        t = time.perf_counter()
        zd = zstd.train_dict(train, size)
        t_train = time.perf_counter() - t
    else:
        level = int(parts[1])
    enc = dec = n = 0
    tc = td = 0.0
    for d in (dumps[1:] if zd else dumps):
        for g in groups(d):
            t = time.perf_counter()
            c = zstd.compress(g, level=level, zstd_dict=zd) if zd else zstd.compress(g, level=level)
            tc += time.perf_counter() - t
            t = time.perf_counter()
            r = zstd.decompress(c, zstd_dict=zd) if zd else zstd.decompress(c)
            td += time.perf_counter() - t
            assert len(r) == len(g)
            enc += len(c)
            dec += len(g)
            n += 1
    res = {"mode": mode, "groups": n, "decoded": dec, "encoded": enc, "ratio": round(dec / enc, 3),
           "compress_ns_per_byte": round(tc * 1e9 / dec, 2), "decompress_ns_per_byte": round(td * 1e9 / dec, 2),
           "train_seconds": round(t_train, 2), "dumps": dumps}
    json.dump(res, open(out, "w"))
    print(json.dumps({k: v for k, v in res.items() if k != "dumps"}))


if __name__ == "__main__":
    main()
