#!/usr/bin/env python3
"""#420 format study: binding_hint candidate encodings in fresh SQLite files.

usage: sim_binding_hint.py SRC_DB OUT_DIR VARIANT
variants:
  base       (child, parent, name) WITHOUT ROWID, all columns in the key (today)
  intern     names interned: binding_name(id, name UNIQUE) + (child, parent, name_id) WITHOUT ROWID
  childrow   binding_name + hint(child INTEGER PRIMARY KEY, entries BLOB of varint (parent, name_id) pairs)
  runs-G     binding_name + groups of <=G children (child gap <= 64), body = varint deltas
  runsz-G    same, body zstd level 3
Each prints rows/bytes; 'extra' rows = bindings beyond a child's first (would go to an
overflow row table in a real per-Commit grouping, reported separately).
"""
import json
import os
import sqlite3
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


def zz(n):
    return (n << 1) ^ (n >> 63)


def main():
    src, outdir, variant = sys.argv[1], sys.argv[2], sys.argv[3]
    t0 = time.time()
    path = os.path.join(outdir, f"bh-{variant}.sqlite")
    if os.path.exists(path):
        os.remove(path)
    dst = sqlite3.connect(path)
    for p in ("PRAGMA page_size=4096", "PRAGMA journal_mode=OFF", "PRAGMA synchronous=OFF"):
        dst.execute(p)
    s = sqlite3.connect(f"file:{src}?mode=ro", uri=True)
    rows = s.execute("SELECT child, parent, name FROM binding_hint ORDER BY child, parent, name").fetchall()
    out = {"variant": variant, "bindings": len(rows)}
    if variant == "base":
        dst.execute("CREATE TABLE binding_hint(child INTEGER NOT NULL, parent INTEGER NOT NULL, name BLOB NOT NULL, PRIMARY KEY(child,parent,name)) STRICT, WITHOUT ROWID")
        dst.executemany("INSERT INTO binding_hint VALUES(?,?,?)", rows)
    else:
        names = {}
        for _, _, n in rows:
            if n not in names:
                names[n] = None
        # ids by descending frequency would favour small varints; use first-seen order (writer order is unknown)
        freq = {}
        for _, _, n in rows:
            freq[n] = freq.get(n, 0) + 1
        for i, n in enumerate(sorted(freq, key=lambda k: -freq[k])):
            names[n] = i + 1
        dst.execute("CREATE TABLE binding_name(id INTEGER PRIMARY KEY, name BLOB NOT NULL UNIQUE) STRICT")
        dst.executemany("INSERT INTO binding_name VALUES(?,?)", ((i, n) for n, i in sorted(names.items(), key=lambda x: x[1])))
        if variant == "intern":
            dst.execute("CREATE TABLE binding_hint(child INTEGER NOT NULL, parent INTEGER NOT NULL, name_id INTEGER NOT NULL, PRIMARY KEY(child,parent,name_id)) STRICT, WITHOUT ROWID")
            dst.executemany("INSERT INTO binding_hint VALUES(?,?,?)", ((c, p, names[n]) for c, p, n in rows))
        elif variant == "childrow":
            dst.execute("CREATE TABLE binding_hint(child INTEGER PRIMARY KEY, entries BLOB NOT NULL) STRICT")
            cur, buf, batch = None, b"", []
            for c, p, n in rows:
                if c != cur and cur is not None:
                    batch.append((cur, buf))
                    buf = b""
                cur = c
                buf += varint(p) + varint(names[n])
            batch.append((cur, buf))
            dst.executemany("INSERT INTO binding_hint VALUES(?,?)", batch)
            out["rows"] = len(batch)
        else:
            kind, g = variant.split("-")
            g = int(g)
            dst.execute("CREATE TABLE binding_group(first INTEGER PRIMARY KEY, last INTEGER NOT NULL, body BLOB NOT NULL) STRICT")
            groups, cur = [], []
            prev_child = None
            nchildren = 0
            for c, p, n in rows:
                if c != prev_child:
                    if cur and (nchildren >= g or c - prev_child > 64):
                        groups.append(cur)
                        cur, nchildren = [], 0
                    nchildren += 1
                cur.append((c, p, n))
                prev_child = c
            if cur:
                groups.append(cur)
            batch = []
            for grp in groups:
                body = bytearray()
                pc, pp = grp[0][0], grp[0][0]
                for c, p, n in grp:
                    body += varint(c - pc) + varint(zz(p - pp)) + varint(names[n])
                    pc, pp = c, p
                body = bytes(body)
                if kind == "runsz":
                    body = zstd.compress(body, level=3)
                batch.append((grp[0][0], grp[-1][0], body))
            dst.executemany("INSERT INTO binding_group VALUES(?,?,?)", batch)
            out["rows"] = len(batch)
            out["payload"] = sum(len(b[2]) for b in batch)
    dst.commit()
    out["bytes"] = dst.execute("PRAGMA page_count").fetchone()[0] * 4096
    out["by_table"] = {r[0]: r[1] for r in dst.execute("SELECT name, sum(pgsize) FROM dbstat GROUP BY name")} if False else None
    out["seconds"] = round(time.time() - t0, 1)
    dst.close()
    print(json.dumps(out))


if __name__ == "__main__":
    main()
