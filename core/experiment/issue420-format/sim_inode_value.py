#!/usr/bin/env python3
"""#420 format study: inode_value candidate encodings, built in fresh SQLite files.

usage: sim_inode_value.py SRC_DB OUT_DIR VARIANT
variants:
  base      ordinal INTEGER PRIMARY KEY, value 73 B (fresh copy of today's table)
  trim      inline rows drop their 16 zero padding bytes (57 B)
  compact   kind+flags 1 B, nref varint, content root 32 B, inline mode/mtime 16 B (or root 32 B)
  group-raw-G     runs of consecutive ordinals, <=G values per row, compact values concatenated
  group-z1-G      same, zstd level 1 per row
  group-z3-G      same, zstd level 3 per row
  group-col-z3-G  same, values transposed by field before zstd level 3
  group-colord-z3-G  as group-col-z3-G, content roots replaced by object ordinals (objects numbered
                  in save order: pack_id, group, record), zigzag-delta varints
Prints one JSON line: variant, rows, bytes(page_count*4096), payload bytes, seconds.
"""
import json
import os
import sqlite3
import struct
import sys
import time
from compression import zstd


def varint(n):
    out = bytearray()
    while True:
        b = n & 0x7F
        n >>= 7
        if n:
            out.append(b | 0x80)
        else:
            out.append(b)
            return bytes(out)


def compact(v):
    kind = v[0]
    nref = int.from_bytes(v[1:9], "big")
    if kind & 0x80:
        return bytes([kind]) + varint(nref) + v[9:41] + v[41:57]
    return bytes([kind]) + varint(nref) + v[9:41] + v[41:73]


def columnar(vals):
    kinds = bytes(v[0] for v in vals)
    nrefs = b"".join(varint(int.from_bytes(v[1:9], "big")) for v in vals)
    roots = b"".join(v[9:41] for v in vals)
    modes = b"".join(v[41:45] if v[0] & 0x80 else b"" for v in vals)
    secs = b"".join(v[45:53] if v[0] & 0x80 else b"" for v in vals)
    nsec = b"".join(v[53:57] if v[0] & 0x80 else b"" for v in vals)
    attr = b"".join(v[41:73] for v in vals if not v[0] & 0x80)
    return kinds + nrefs + roots + modes + secs + nsec + attr


def zz(n):
    return (n << 1) ^ (n >> 63)


def columnar_ord(vals, ordmap):
    kinds = bytes(v[0] for v in vals)
    nrefs = b"".join(varint(int.from_bytes(v[1:9], "big")) for v in vals)
    prev, roots = 0, bytearray()
    for v in vals:
        o = ordmap[v[9:41]]
        roots += varint(zz(o - prev))
        prev = o
    modes = b"".join(v[41:45] if v[0] & 0x80 else b"" for v in vals)
    secs = b"".join(v[45:53] if v[0] & 0x80 else b"" for v in vals)
    nsec = b"".join(v[53:57] if v[0] & 0x80 else b"" for v in vals)
    attr = b"".join(v[41:73] for v in vals if not v[0] & 0x80)
    return kinds + nrefs + bytes(roots) + modes + secs + nsec + attr


def main():
    src, outdir, variant = sys.argv[1], sys.argv[2], sys.argv[3]
    t0 = time.time()
    path = os.path.join(outdir, f"iv-{variant}.sqlite")
    if os.path.exists(path):
        os.remove(path)
    dst = sqlite3.connect(path)
    dst.execute("PRAGMA page_size=4096")
    dst.execute("PRAGMA journal_mode=OFF")
    dst.execute("PRAGMA synchronous=OFF")
    s = sqlite3.connect(f"file:{src}?mode=ro", uri=True)
    rows = s.execute("SELECT ordinal, value FROM inode_value ORDER BY ordinal")
    payload = 0
    nrows = 0
    if variant in ("base", "trim", "compact"):
        dst.execute("CREATE TABLE inode_value(ordinal INTEGER PRIMARY KEY, value BLOB NOT NULL) STRICT")
        batch = []
        for o, v in rows:
            if variant == "trim":
                v = v[:57] if v[0] & 0x80 else v
            elif variant == "compact":
                v = compact(v)
            payload += len(v)
            batch.append((o, v))
            if len(batch) >= 50000:
                dst.executemany("INSERT INTO inode_value VALUES(?,?)", batch)
                nrows += len(batch)
                batch = []
        dst.executemany("INSERT INTO inode_value VALUES(?,?)", batch)
        nrows += len(batch)
    else:
        kind, g = variant.rsplit("-", 1)
        g = int(g)
        dst.execute("CREATE TABLE inode_value_group(first INTEGER PRIMARY KEY, count INTEGER NOT NULL, body BLOB NOT NULL) STRICT")
        level = 1 if "z1" in kind else 3
        ordmap = {}
        if kind == "group-colord-z3":
            for i, (oid,) in enumerate(s.execute("SELECT object_id FROM object_location ORDER BY pack_id, group_number, record_number")):
                ordmap[oid] = i + 1
        batch = []
        run = []
        last = None

        def flush():
            nonlocal payload, nrows
            if not run:
                return
            vals = [v for _, v in run]
            if kind == "group-raw":
                body = b"".join(compact(v) for v in vals)
            elif kind == "group-col-z3":
                body = zstd.compress(columnar(vals), level=3)
            elif kind == "group-colord-z3":
                body = zstd.compress(columnar_ord(vals, ordmap), level=3)
            else:
                body = zstd.compress(b"".join(compact(v) for v in vals), level=level)
            payload += len(body)
            batch.append((run[0][0], len(run), body))
            nrows += 1
            run.clear()

        for o, v in rows:
            if last is not None and (o != last + 1 or len(run) >= g):
                flush()
            run.append((o, v))
            last = o
            if len(batch) >= 20000:
                dst.executemany("INSERT INTO inode_value_group VALUES(?,?,?)", batch)
                batch.clear()
        flush()
        dst.executemany("INSERT INTO inode_value_group VALUES(?,?,?)", batch)
    dst.commit()
    pages = dst.execute("PRAGMA page_count").fetchone()[0]
    dst.close()
    print(json.dumps({"variant": variant, "rows": nrows, "bytes": pages * 4096,
                      "payload": payload, "seconds": round(time.time() - t0, 1)}))


if __name__ == "__main__":
    main()
