#!/usr/bin/env python3
"""#420 format study: byte anatomy of the pack table by object role.

Read-only on a clone of the range-60 fixture. Parses every pack (header,
directory, groups, record framing), decodes zstd ordinary groups and joins
records to object_location by (pack, group, record) to attribute bytes to roles.

usage: anatomy.py DB OUT_JSON LO HI [META_DUMP]
  processes pack_id in [LO, HI); optional META_DUMP appends decoded ordinary
  group bodies as (u32 pack, u8 group, u32 len, body) for later experiments.
"""
import json
import sqlite3
import struct
import sys
import time
from collections import defaultdict
from compression import zstd

ROLE = {1: "WholeFile", 2: "Chunk", 3: "ExtentLeaf", 4: "ExtentBranch", 5: "FileState",
        6: "InodeLeaf", 7: "DirectoryLeaf", 8: "DirectoryBranch", 9: "InodeBranch",
        10: "FilesystemRoot", 11: "AttributeLeaf", 12: "AttributeBranch", 13: "Symlink"}
LANE = {9: "ord", 19: "ord", 24: "ord", 10: "chunk", 15: "chunk", 20: "chunk", 25: "chunk",
        11: "wf", 14: "wf", 17: "wf", 18: "wf", 23: "wf", 13: "single", 16: "single"}
SIZED = {23, 24, 25}
TIGHT = {18, 19, 20}


def main():
    db, out, lo, hi = sys.argv[1], sys.argv[2], int(sys.argv[3]), int(sys.argv[4])
    dump = open(sys.argv[5], "ab") if len(sys.argv) > 5 else None
    t0 = time.time()
    con = sqlite3.connect(f"file:{db}?mode=ro", uri=True)
    loc = {}
    for oid, role, clen, p, g, r in con.execute(
            "select object_id, role, canonical_length, pack_id, group_number, record_number "
            "from object_location where pack_id>=? and pack_id<?", (lo, hi)):
        loc[(p, g, r)] = (role, clen)
    agg = defaultdict(lambda: defaultdict(int))
    tot = defaultdict(int)
    for pid, domain, length, body in con.execute(
            "select pack_id, domain, length, body from pack where pack_id>=? and pack_id<? order by pack_id",
            (lo, hi)):
        ver = struct.unpack_from("<I", body, 8)[0]
        gc = struct.unpack_from("<I", body, 12)[0]
        lane = LANE[ver]
        tot["packs"] += 1
        tot[f"packs_v{ver}"] += 1
        tot["pack_bytes"] += length
        tot[f"pack_bytes_d{domain}"] += length
        tot["header_bytes"] += 24
        ent = 4 if lane == "wf" else 16
        if ver in SIZED:
            slots = gc
        elif ver in TIGHT:
            slots = 16
        elif lane == "single":
            slots = 1
        else:
            slots = 256
        tot["dir_used_bytes"] += ent * gc
        tot["dir_reserved_bytes"] += ent * (slots - gc)
        off = 24 + ent * slots
        views = []
        if lane == "wf":
            starts = [struct.unpack_from("<I", body, 24 + 4 * i)[0] for i in range(gc)]
            for i in range(gc):
                end = starts[i + 1] if i + 1 < gc else length
                views.append((starts[i], end, end - starts[i], 0))
        else:
            for i in range(gc):
                s, enc, dec, flags = struct.unpack_from("<IIII", body, 24 + 16 * i)
                views.append((s, s + enc, dec, flags & 0xff))
        for gi, (s, e, dec, codec) in enumerate(views):
            raw = body[s:e]
            tot["groups"] += 1
            tot[f"group_enc_{lane}"] += len(raw)
            if codec == 1:
                g = zstd.decompress(raw)
                assert len(g) == dec, (pid, gi)
                tot["zstd_groups"] += 1
                tot["zstd_enc"] += len(raw)
                tot["zstd_dec"] += dec
                if dump is not None:
                    dump.write(struct.pack("<IBI", pid, gi, len(g)))
                    dump.write(g)
            else:
                g = raw
            n = struct.unpack_from("<I", g, 0)[0]
            ends = struct.unpack_from(f"<{n}I", g, 4)
            area = 4 + 4 * n
            prev = 0
            scale = len(raw) / len(g)
            framing = area
            tot["record_framing_dec"] += framing
            tot["record_framing_alloc"] += framing * scale
            for ri, end in enumerate(ends):
                rec = g[area + prev: area + end]
                prev = end
                role, clen = loc.get((pid, gi, ri), (0, 0))
                rn = ROLE.get(role, "unlocated")
                a = agg[rn]
                a["records"] += 1
                a["canonical"] += clen
                a["record_bytes"] += len(rec)
                a["stored_alloc"] += len(rec) * scale
                tag = rec[0]
                a[f"tag{tag}"] += 1
                base = 0
                if lane == "ord":
                    if role in (6, 9) and tag == 1:
                        base = 32
                elif lane == "chunk":
                    if tag == 1:
                        base = 32
                    a["frame"] += len(rec) - 5 - base
                else:
                    if tag == 1:
                        base = 32
                a["base_id_bytes"] += base
                a["base_id_alloc"] += base * scale
                if tag == 1:
                    a["delta_record_bytes"] += len(rec)
                    a["delta_canonical"] += clen
                else:
                    a["full_record_bytes"] += len(rec)
                    a["full_canonical"] += clen
            assert area + prev == len(g), (pid, gi)
    res = {"lo": lo, "hi": hi, "seconds": round(time.time() - t0, 2), "totals": tot,
           "roles": {k: dict(v) for k, v in agg.items()}}
    json.dump(res, open(out, "w"), indent=1)
    print(json.dumps({"lo": lo, "hi": hi, "seconds": res["seconds"], "packs": tot["packs"]}))


if __name__ == "__main__":
    main()
