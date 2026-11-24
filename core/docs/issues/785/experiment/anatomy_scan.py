"""Read-only physical inventory of a LayerFS SQLite Store (schema 42).

Streams every pack once (immutable URI, no sidecars), merge-joins the sorted
object_location rows, and tallies per role: record tag (FULL/PREFIX/STORED),
record bytes, canonical bytes, raw payload bytes, delta edges and group codecs.
Writes JSON summary and a binary delta-edge file for chain-depth analysis.
"""

import json
import signal
import sqlite3
import struct
import sys
import time
from collections import defaultdict
from compression import zstd

signal.alarm(115)

DB = sys.argv[1]
OUT = sys.argv[2]
EDGES = sys.argv[3]
WF_OVER = 23  # WHOLE_FILE_CANONICAL_OVERHEAD
ROLE = {1: "WholeFile", 2: "Chunk", 3: "ExtentLeaf", 4: "ExtentBranch", 5: "FileState",
        6: "InodeLeaf", 7: "DirectoryLeaf", 8: "DirectoryBranch", 9: "InodeBranch",
        10: "FilesystemRoot", 11: "AttributeLeaf", 12: "AttributeBranch", 13: "Symlink"}
VER_LANE = {23: "whole", 25: "chunk", 26: "ordinary", 9: "ordinary", 19: "ordinary", 24: "ordinary",
            10: "chunk", 15: "chunk", 20: "chunk", 11: "whole", 14: "whole", 17: "whole", 18: "whole"}

u32 = lambda b, o: struct.unpack_from("<I", b, o)[0]


def groups_of(body, lane):
    n = u32(body, 12)
    used = u32(body, 16)
    out = []
    if lane == "whole":
        off = 24 + 4 * n
        starts = [u32(body, 24 + 4 * i) for i in range(n)]
        for i, s in enumerate(starts):
            e = starts[i + 1] if i + 1 < n else used
            out.append((body[s:e], 0, e - s, e - s))
    else:
        for i in range(n):
            o = 24 + 16 * i
            s, enc, dec, codec = u32(body, o), u32(body, o + 4), u32(body, o + 8), body[o + 12]
            raw = body[s:s + enc]
            if codec == 1:
                raw = zstd.decompress(raw)
                assert len(raw) == dec
            out.append((raw, codec, enc, dec))
    return out


def record(group, k):
    n = u32(group, 0)
    start = 4 + 4 * n + (u32(group, 4 + 4 * (k - 1)) if k else 0)
    end = 4 + 4 * n + u32(group, 4 + 4 * k)
    return group[start:end]


con = sqlite3.connect(f"file:{DB}?immutable=1", uri=True)
locs = con.execute("SELECT pack_id, group_number, record_number, object_id, role, canonical_length "
                   "FROM object_location ORDER BY pack_id, group_number, record_number")
packs = con.execute("SELECT pack_id, domain, length, body FROM pack ORDER BY pack_id")

t0 = time.time()
r = defaultdict(lambda: defaultdict(int))  # role -> counters
lane_stats = defaultdict(lambda: defaultdict(int))
edges = open(EDGES, "wb")
whole_sizes = defaultdict(int)  # log2 bucket -> (count)
nxt = locs.fetchone()
for pack_id, domain, length, body in packs:
    ver = u32(body, 8)
    lane = VER_LANE[ver]
    ls = lane_stats[lane]
    ls["packs"] += 1
    ls["pack_bytes"] += length
    gs = groups_of(body, lane)
    ls["groups"] += len(gs)
    for g, codec, enc, dec in gs:
        ls["group_enc"] += enc
        ls["group_dec"] += dec
        ls["groups_zstd"] += codec == 1
        ls["records_framed"] += u32(g, 0)
    while nxt is not None and nxt[0] == pack_id:
        _, gn, rn, oid, role, canon = nxt
        g, codec, enc, dec = gs[gn]
        rec = record(g, rn)
        tag = rec[0]
        c = r[ROLE[role]]
        c["n"] += 1
        c["canonical"] += canon
        c["record"] += len(rec)
        # share of the group's encoded bytes, by decoded width
        c["encoded_share"] += len(rec) * enc / dec
        key = {0: "full", 1: "prefix", 2: "stored"}.get(tag, f"tag{tag}")
        c[f"n_{key}"] += 1
        c[f"rec_{key}"] += len(rec)
        c[f"canon_{key}"] += canon
        base = None
        if lane == "chunk":
            raw = u32(rec, 1)
            c[f"raw_{key}"] += raw
            if tag == 1:
                base = rec[5:37]
        elif lane == "whole":
            c[f"raw_{key}"] += canon - WF_OVER
            if tag == 1:
                base = rec[1:33]
            whole_sizes[(canon - WF_OVER).bit_length()] += 1
        else:
            if tag == 1:
                base = rec[1:33]
        if base is not None:
            edges.write(oid + base + bytes([role]))
        nxt = locs.fetchone()
edges.close()
json.dump({"roles": r, "lanes": lane_stats, "whole_size_log2": whole_sizes,
           "seconds": time.time() - t0}, open(OUT, "w"), indent=1, default=dict)
print("done", round(time.time() - t0, 1), "s")
