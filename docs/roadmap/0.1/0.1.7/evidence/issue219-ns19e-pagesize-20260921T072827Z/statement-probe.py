#!/usr/bin/env python3
"""Why 64 KiB pages cut the commit and did not cut the row.

**A labelled post-hoc diagnostic.** The E1 arm (`ns19-E1-pagesize-20260921T073509Z`)
moved `pipeline.diag_commit_total_ns` 402.7 -> 263.3 ms and `pipeline.diag_insert_objects_ns`
151.8 -> 294.9 ms, and `page-probe.py` measured only the first of those. This probe
measures the second and the third: the same write shape, with the **statement** work
timed separately from the `COMMIT`, across the page-size / page-cache / mmap matrix.

It measures a cause on a count-driven instrument - microseconds per statement, per
append and per dirty page - and it is never a sample of an arm.

    python3 statement-probe.py
"""
import os
import sqlite3
import time

DB = "/tmp/statement-probe.db"
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


def probe(page_size, cache_size, mmap_size=0, label=None):
    if os.path.exists(DB):
        os.remove(DB)
    connection = sqlite3.connect(DB, isolation_level=None)
    connection.execute("PRAGMA journal_mode=MEMORY")
    connection.execute("PRAGMA synchronous=OFF")
    connection.execute("PRAGMA temp_store=MEMORY")
    connection.execute("PRAGMA foreign_keys=ON")
    connection.execute(f"PRAGMA page_size={page_size}")
    connection.execute(f"PRAGMA cache_size={cache_size}")
    if mmap_size:
        connection.execute(f"PRAGMA mmap_size={mmap_size}")
    connection.executescript(SCHEMA)

    body = bytes(range(256)) * (APPEND // 256) + bytes(APPEND % 256)
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
                ((txn * ROWS_PER_TXN + index).to_bytes(24, "little"), APPEND, pack_id, index)
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
                ((txn * ROWS_PER_TXN + index) % 4096, txn, b"s" * 32, b"g" * 32)
                for index in range(4)
            ],
        )
        signature_ns += time.perf_counter() - started
        started = time.perf_counter()
        connection.execute("COMMIT")
        commit_ns += time.perf_counter() - started
    page_count = connection.execute("PRAGMA page_count").fetchone()[0]
    size = os.path.getsize(DB)
    connection.close()
    os.remove(DB)
    return dict(
        label=label or f"page={page_size} cache={cache_size} mmap={mmap_size}",
        pages=page_count, bytes=size, appends=appends, rows=rows,
        commit=commit_ns * 1e3, write=write_ns * 1e3,
        insert=insert_ns * 1e3, signature=signature_ns * 1e3,
    )


def report(r):
    print(
        f"  {r['label']:34s} pages {r['pages']:6d}  "
        f"commit {r['commit']:8.2f}  "
        f"blob {r['write']:7.2f} ({r['write']*1000/r['appends']:5.2f} us/app)  "
        f"insert {r['insert']:7.2f} ({r['insert']*1000/r['rows']:5.2f} us/row)  "
        f"signature {r['signature']:6.2f}"
    )


if __name__ == "__main__":
    print(f"sqlite {sqlite3.sqlite_version}   (ms, summed over 800 transactions)")
    report(probe(4096, -2000, label="page=4096 cache=2MiB (the control)"))
    report(probe(65536, -2000, label="page=64KiB cache=2MiB (E1)"))
    report(probe(65536, -8192, label="page=64KiB cache=8MiB"))
    report(probe(65536, -32768, label="page=64KiB cache=32MiB"))
    report(probe(65536, -131072, label="page=64KiB cache=128MiB"))
    report(probe(65536, -2000, 268435456, label="page=64KiB cache=2MiB mmap=256MiB"))
    report(probe(16384, -8000, label="page=16KiB cache=8MiB"))
