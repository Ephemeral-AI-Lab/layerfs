#!/usr/bin/env python3
"""The teardown price, measured with no product code at all.

Writes N transactions of P pages into a scratch store under the product's pragma
profile, then times `sqlite3_close`. The point is that the cost is a property of
the *pages written*, not of the product: it reproduces here, it is the same in
every journal mode, and it moves by ~7x between runs minutes apart.

    python3 close-probe.py
"""
import os
import sqlite3
import time


def probe(label, journal="MEMORY", txns=800, pages=125, sync="OFF"):
    path = "/tmp/close-probe.db"
    if os.path.exists(path):
        os.remove(path)
    connection = sqlite3.connect(path, isolation_level=None)
    connection.execute(f"PRAGMA journal_mode={journal}")
    connection.execute(f"PRAGMA synchronous={sync}")
    connection.execute("PRAGMA temp_store=MEMORY")
    connection.execute("CREATE TABLE packs(pack_id INTEGER PRIMARY KEY, data BLOB)")
    body = b"z" * (pages * 4096)
    for index in range(txns):
        connection.execute("BEGIN IMMEDIATE")
        connection.execute(
            "INSERT INTO packs(pack_id,data) VALUES (?,?)", (index + 1, body)
        )
        connection.execute("COMMIT")
    started = time.perf_counter()
    connection.close()
    elapsed = (time.perf_counter() - started) * 1000
    size = os.path.getsize(path) / 1e6
    print(f"  {label:46s} close {elapsed:8.2f} ms   written {size:7.1f} MB")
    os.remove(path)


if __name__ == "__main__":
    print(f"sqlite {sqlite3.sqlite_version}")
    probe("no transactions, empty file", txns=0, pages=0)
    probe("800 x 125 pages")
    probe("17378 x 1 page", txns=17378, pages=1)
    for journal in ("MEMORY", "OFF", "DELETE", "WAL"):
        probe(f"800 x 125 pages, journal={journal}", journal=journal)
