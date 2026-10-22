#!/usr/bin/env python3
"""#420 format study: object_location key and locator encodings in fresh SQLite files.

usage: sim_object_location.py SRC_DB OUT_DIR VARIANT
variants:
  base     object_id BLOB(32) PK WITHOUT ROWID, role, canonical_length, pack_id, group, record (today)
  key16    first 16 bytes of the id as the key (collision side table not modelled: expected 0 rows)
  key8     first 8 bytes as a signed INTEGER PRIMARY KEY (rowid table)
  key8loc  key8 plus one packed locator integer (pack_id<<21 | group<<13 | record) and role folded into length
  ord      ordinal INTEGER PRIMARY KEY (save order), full object_id in the row, UNIQUE index on object_id
  ord16    as ord, the lookup index on the first 16 bytes only (expression index; the row holds the full id)
"""
import json
import os
import sqlite3
import sys
import time


def main():
    src, outdir, variant = sys.argv[1], sys.argv[2], sys.argv[3]
    t0 = time.time()
    path = os.path.join(outdir, f"ol-{variant}.sqlite")
    if os.path.exists(path):
        os.remove(path)
    dst = sqlite3.connect(path)
    for p in ("PRAGMA page_size=4096", "PRAGMA journal_mode=OFF", "PRAGMA synchronous=OFF"):
        dst.execute(p)
    s = sqlite3.connect(f"file:{src}?mode=ro", uri=True)
    rows = s.execute("SELECT object_id, role, canonical_length, pack_id, group_number, record_number FROM object_location ORDER BY object_id").fetchall()
    out = {"variant": variant, "rows": len(rows)}
    if variant == "base":
        dst.execute("CREATE TABLE object_location(object_id BLOB PRIMARY KEY, role INTEGER NOT NULL, canonical_length INTEGER NOT NULL, pack_id INTEGER NOT NULL, group_number INTEGER NOT NULL, record_number INTEGER NOT NULL) STRICT, WITHOUT ROWID")
        dst.executemany("INSERT INTO object_location VALUES(?,?,?,?,?,?)", rows)
    elif variant == "key16":
        keys = {r[0][:16] for r in rows}
        out["collisions"] = len(rows) - len(keys)
        dst.execute("CREATE TABLE object_location(key BLOB PRIMARY KEY, role INTEGER NOT NULL, canonical_length INTEGER NOT NULL, pack_id INTEGER NOT NULL, group_number INTEGER NOT NULL, record_number INTEGER NOT NULL) STRICT, WITHOUT ROWID")
        dst.executemany("INSERT INTO object_location VALUES(?,?,?,?,?,?)", ((r[0][:16],) + tuple(r[1:]) for r in rows))
    elif variant in ("ord", "ord16"):
        rows = s.execute("SELECT object_id, role, canonical_length, pack_id, group_number, record_number FROM object_location ORDER BY pack_id, group_number, record_number").fetchall()
        dst.execute("CREATE TABLE object_location(ordinal INTEGER PRIMARY KEY, object_id BLOB NOT NULL, sized INTEGER NOT NULL, locator INTEGER NOT NULL) STRICT")
        dst.executemany("INSERT INTO object_location VALUES(?,?,?,?)",
                        ((i + 1, oid, (ln << 4) | role, (p << 21) | (g << 13) | rec) for i, (oid, role, ln, p, g, rec) in enumerate(rows)))
        if variant == "ord":
            dst.execute("CREATE UNIQUE INDEX object_ids ON object_location(object_id)")
        else:
            dst.execute("CREATE INDEX object_ids ON object_location(substr(object_id,1,16))")
    else:
        def k8(b):
            return int.from_bytes(b[:8], "big", signed=True)
        keys = {k8(r[0]) for r in rows}
        out["collisions"] = len(rows) - len(keys)
        conv = sorted(((k8(r[0]),) + tuple(r[1:]) for r in rows))
        if variant == "key8":
            dst.execute("CREATE TABLE object_location(key INTEGER PRIMARY KEY, role INTEGER NOT NULL, canonical_length INTEGER NOT NULL, pack_id INTEGER NOT NULL, group_number INTEGER NOT NULL, record_number INTEGER NOT NULL) STRICT")
            dst.executemany("INSERT INTO object_location VALUES(?,?,?,?,?,?)", conv)
        else:
            dst.execute("CREATE TABLE object_location(key INTEGER PRIMARY KEY, sized INTEGER NOT NULL, locator INTEGER NOT NULL) STRICT")
            dst.executemany("INSERT INTO object_location VALUES(?,?,?)",
                            ((k, (ln << 4) | role, (p << 21) | (g << 13) | rec) for k, role, ln, p, g, rec in conv))
    dst.commit()
    out["bytes"] = dst.execute("PRAGMA page_count").fetchone()[0] * 4096
    out["seconds"] = round(time.time() - t0, 1)
    dst.close()
    print(json.dumps(out))


if __name__ == "__main__":
    main()
