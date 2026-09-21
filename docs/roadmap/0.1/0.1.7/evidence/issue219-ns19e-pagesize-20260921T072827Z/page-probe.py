#!/usr/bin/env python3
"""What the page size costs, measured with no product code at all.

**This is a labelled diagnostic, not an arm.** It measures one cause on a
count-driven instrument: it reproduces the `pipeline-namespace-10000` row's write
shape (800 transactions, 16,800 blob appends of 18,000 bytes into 256 KiB packs,
24,800 `WITHOUT ROWID` row inserts) under the product's own pragma profile, at
four page sizes, and reports **µs per dirty page** beside the page counts. It is
never a second sample of an arm and it decides nothing by itself.

The product's own row, for comparison, records `pipeline.diag_commit_total_ns`
(the 800 `COMMIT`s) and `PRAGMA page_count` = 82,129 on a 336,400,384-byte Store.

    python3 page-probe.py
"""
import os
import sqlite3
import time

DB = "/tmp/page-probe.db"
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
"""


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
    pack_id = 0
    used = PACK_LIMIT
    appends = 0
    rows = 0
    commit_ns = 0
    write_ns = 0
    started_all = time.perf_counter()
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
        connection.executemany(
            "INSERT INTO objects(object_id,save_id,object_role,canonical_length,"
            "pack_id,group_number,record_number) VALUES (?,1,1,?,?,0,?)",
            [
                ((txn * ROWS_PER_TXN + index).to_bytes(24, "little"), APPEND,
                 pack_id, index)
                for index in range(ROWS_PER_TXN)
            ],
        )
        rows += ROWS_PER_TXN
        started = time.perf_counter()
        connection.execute("COMMIT")
        commit_ns += time.perf_counter() - started
    total = time.perf_counter() - started_all
    page_count = connection.execute("PRAGMA page_count").fetchone()[0]
    size = os.path.getsize(DB)
    started = time.perf_counter()
    connection.close()
    close_ms = (time.perf_counter() - started) * 1000
    os.remove(DB)
    return {
        "label": label or f"page_size={page_size} cache_size={cache_size}",
        "page_size": page_size,
        "cache_size": cache_size,
        "appends": appends,
        "rows": rows,
        "packs": pack_id,
        "page_count": page_count,
        "bytes": size,
        "commit_ms": commit_ns * 1000,
        "write_ms": write_ns * 1000,
        "total_ms": total * 1000,
        "close_ms": close_ms,
    }


def report(row):
    pages = row["page_count"]
    print(
        f"  {row['label']:38s} "
        f"pages {pages:7d}  file {row['bytes']/1e6:7.1f} MB  "
        f"commit {row['commit_ms']:8.2f} ms ({row['commit_ms']*1000/pages:6.2f} us/page)  "
        f"blob-open+write {row['write_ms']:8.2f} ms ({row['write_ms']*1000/row['appends']:6.2f} us/append)  "
        f"close {row['close_ms']:7.2f} ms"
    )


if __name__ == "__main__":
    print(f"sqlite {sqlite3.sqlite_version}")
    for page_size in (4096, 16384, 65536):
        report(probe(page_size, -2000))
    # Page size and cache size interact: a 2 MiB cache holds 32 pages of 64 KiB.
    # This row exists only to say whether the next round has anything to register.
    for cache in (-8000, -32768):
        report(probe(65536, cache, f"page_size=65536 cache_size={cache} (diagnostic)"))
