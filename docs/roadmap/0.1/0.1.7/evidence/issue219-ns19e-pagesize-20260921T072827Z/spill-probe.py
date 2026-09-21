#!/usr/bin/env python3
"""Does an uncached random-key page explain E1's insert regression?

**A labelled post-hoc diagnostic.** `statement-probe.py` inserted object ids in
ascending order, so every statement touched the last leaf page and the page size
made the inserts *cheaper*. The product inserts content-derived ids, which land on
a random leaf, and there `pipeline.diag_insert_objects_ns` went 151.8 -> 294.9 ms
in `ns19-E1-pagesize-20260921T073509Z`. This probe is the same write shape with
**random 24-byte keys** and the product's full table set, so the b-tree seek is
cold; it reports microseconds per statement, per append and per commit, which is
the count-driven price of one page.

    python3 spill-probe.py
"""
import os
import random
import sqlite3
import time

DB = "/tmp/spill-probe.db"
PACK_LIMIT = 262_144
APPEND = 18_000
TXNS = 800
APPENDS_PER_TXN = 21
ROWS_PER_TXN = 31

SCHEMA = """
CREATE TABLE store_policy (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    next_pack_id INTEGER NOT NULL DEFAULT 1
) STRICT;
CREATE TABLE saves (save_id INTEGER PRIMARY KEY, pack_ceiling INTEGER NOT NULL DEFAULT 0) STRICT;
INSERT INTO saves(save_id) VALUES (1);
INSERT INTO store_policy(id) VALUES (1);
CREATE TABLE object_packs (
    pack_id INTEGER PRIMARY KEY,
    save_id INTEGER NOT NULL,
    data BLOB NOT NULL
) STRICT;
CREATE INDEX packs_save ON object_packs(save_id, pack_id);
CREATE TABLE objects (
    object_id BLOB NOT NULL,
    save_id INTEGER NOT NULL,
    object_role INTEGER NOT NULL,
    canonical_length INTEGER NOT NULL,
    pack_id INTEGER NOT NULL,
    group_number INTEGER NOT NULL,
    record_number INTEGER NOT NULL,
    PRIMARY KEY (object_id, save_id)
) STRICT, WITHOUT ROWID;
CREATE INDEX objects_save ON objects(save_id, object_id);
CREATE TABLE content_signatures (
    slot INTEGER PRIMARY KEY,
    stamp INTEGER NOT NULL,
    object_id BLOB NOT NULL,
    signature BLOB NOT NULL,
    save_id INTEGER NOT NULL
) STRICT;
CREATE INDEX signatures_save ON content_signatures(save_id, slot);
"""


def keys(count, seed):
    rng = random.Random(seed)
    return [rng.randbytes(24) for _ in range(count)]


def probe(page_size, cache_size, label=None):
    if os.path.exists(DB):
        os.remove(DB)
    connection = sqlite3.connect(DB, isolation_level=None)
    connection.execute("PRAGMA journal_mode=MEMORY")
    connection.execute("PRAGMA synchronous=OFF")
    connection.execute("PRAGMA temp_store=MEMORY")
    connection.execute("PRAGMA foreign_keys=ON")
    connection.execute(f"PRAGMA page_size={page_size}")
    connection.execute(f"PRAGMA cache_size={cache_size}")
    connection.executescript(SCHEMA)

    body = bytes(range(256)) * (APPEND // 256) + bytes(APPEND % 256)
    ids = keys(TXNS * ROWS_PER_TXN, 219)
    slots = keys(TXNS * 4, 220)
    pack_id = 0
    used = PACK_LIMIT
    appends = rows = 0
    commit_ns = write_ns = insert_ns = signature_ns = 0
    for txn in range(TXNS):
        connection.execute("BEGIN IMMEDIATE")
        for _ in range(APPENDS_PER_TXN):
            if used + APPEND > PACK_LIMIT:
                pack_id += 1
                used = 0
                connection.execute(
                    "INSERT INTO object_packs(pack_id,save_id,data) VALUES (?,1,zeroblob(?))",
                    (pack_id, PACK_LIMIT),
                )
                connection.execute(
                    "UPDATE store_policy SET next_pack_id=? WHERE id=1", (pack_id + 1,)
                )
            started = time.perf_counter()
            blob = connection.blobopen("object_packs", "data", pack_id, readonly=False)
            try:
                blob[used:used + APPEND] = body
            finally:
                blob.close()
            write_ns += time.perf_counter() - started
            used += APPEND
            appends += 1
        started = time.perf_counter()
        connection.executemany(
            "INSERT INTO objects(object_id,save_id,object_role,canonical_length,"
            "pack_id,group_number,record_number) VALUES (?,1,1,?,?,0,?)",
            [
                (ids[txn * ROWS_PER_TXN + index], APPEND, pack_id, index)
                for index in range(ROWS_PER_TXN)
            ],
        )
        insert_ns += time.perf_counter() - started
        rows += ROWS_PER_TXN
        started = time.perf_counter()
        connection.executemany(
            "INSERT OR REPLACE INTO content_signatures(slot,stamp,object_id,signature,save_id)"
            " VALUES (?,?,?,?,1)",
            [
                (
                    int.from_bytes(slots[txn * 4 + index][:4], "little") % 4096,
                    txn, b"s" * 32, b"g" * 32,
                )
                for index in range(4)
            ],
        )
        signature_ns += time.perf_counter() - started
        started = time.perf_counter()
        connection.execute("COMMIT")
        commit_ns += time.perf_counter() - started
    page_count = connection.execute("PRAGMA page_count").fetchone()[0]
    connection.close()
    os.remove(DB)
    return dict(
        label=label or f"page={page_size} cache={cache_size}",
        pages=page_count, appends=appends, rows=rows,
        commit=commit_ns * 1e3, write=write_ns * 1e3,
        insert=insert_ns * 1e3, signature=signature_ns * 1e3,
    )


def report(r):
    print(
        f"  {r['label']:36s} pages {r['pages']:6d}  "
        f"commit {r['commit']:8.2f}  "
        f"blob {r['write']:7.2f}  "
        f"insert {r['insert']:8.2f} ({r['insert']*1000/r['rows']:6.2f} us/row)  "
        f"signature {r['signature']:6.2f}"
    )


if __name__ == "__main__":
    print(f"sqlite {sqlite3.sqlite_version}   (ms, summed over 800 transactions; random keys)")
    report(probe(4096, -2000, "page=4096 cache=2MiB (the control)"))
    report(probe(65536, -2000, "page=64KiB cache=2MiB (E1 as landed)"))
    report(probe(65536, -8192, "page=64KiB cache=8MiB"))
    report(probe(65536, -32768, "page=64KiB cache=32MiB"))
    report(probe(16384, -2000, "page=16KiB cache=2MiB"))
    report(probe(16384, -8192, "page=16KiB cache=8MiB"))
