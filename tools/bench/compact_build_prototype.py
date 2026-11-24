#!/usr/bin/env python3
"""Prototype benchmark: SQLite Store compaction build strategies.

The shipped build copies the sealed Store with ``VACUUM INTO``, deletes the
rows the mark excluded, then runs a full ``VACUUM``. This prototype measures
that path against a clone + delete + ``PRAGMA incremental_vacuum`` path on a
synthetic database whose shape and byte volume match the physical tables a
build rewrites (``pack.body`` dominates). It is a *measurement* tool, not a
production path: it never reads payloads back during verification.

Usage:
    python3 compact_build_prototype.py --suite --dir /tmp/compact-bench
    python3 compact_build_prototype.py --suite --dir ... --small-mb 256 --large-gb 10

Each run is a fresh process so the SQLite page cache starts empty; the input
file is evicted with ``posix_fadvise(DONTNEED)`` after ``sync`` (cold-ish).
"""

from __future__ import annotations

import argparse
import json
import os
import random
import shutil
import sqlite3
import subprocess
import sys
import time

PAGE_SIZE = 4096
CACHE_KIB = 32768
PACK_BYTES = 1 << 20  # one synthetic pack body
OBJ_BYTES = 32

SCHEMA = """
PRAGMA page_size=4096;
PRAGMA auto_vacuum=INCREMENTAL;
CREATE TABLE pack(
  pack_id INTEGER PRIMARY KEY,
  domain INTEGER NOT NULL,
  digest BLOB NOT NULL,
  length INTEGER NOT NULL,
  body BLOB NOT NULL
) STRICT;
CREATE TABLE object_location(
  object_id BLOB PRIMARY KEY,
  role INTEGER NOT NULL,
  canonical_length INTEGER NOT NULL,
  pack_id INTEGER NOT NULL,
  group_number INTEGER NOT NULL,
  record_number INTEGER NOT NULL
) STRICT;
CREATE INDEX object_location_pack ON object_location(pack_id);
CREATE TABLE kept_node(
  kind TEXT NOT NULL, node_id BLOB NOT NULL, stack_id BLOB NOT NULL, serial INTEGER NOT NULL
) STRICT;
CREATE TABLE visited(object_id BLOB PRIMARY KEY) STRICT;
CREATE TABLE store_totals(
  id INTEGER PRIMARY KEY, logical INTEGER NOT NULL, canonical INTEGER NOT NULL,
  stored INTEGER NOT NULL, metadata INTEGER NOT NULL
) STRICT;
"""


def io_write_bytes() -> int:
    try:
        with open("/proc/self/io", encoding="ascii") as handle:
            for line in handle:
                if line.startswith("write_bytes:"):
                    return int(line.split(":", 1)[1])
    except OSError:
        pass
    return 0


def evict(path: str) -> None:
    os.sync()
    try:
        fd = os.open(path, os.O_RDONLY)
    except OSError:
        return
    try:
        os.posix_fadvise(fd, 0, 0, os.POSIX_FADV_DONTNEED)
    finally:
        os.close(fd)


def connect(path: str, writable: bool = True) -> sqlite3.Connection:
    conn = sqlite3.connect(path, timeout=60.0)
    conn.execute(f"PRAGMA cache_size=-{CACHE_KIB}")
    conn.execute("PRAGMA synchronous=OFF")
    conn.execute("PRAGMA journal_mode=DELETE")
    if not writable:
        conn.execute("PRAGMA query_only=1")
    return conn


def build_source(path: str, total_bytes: int, dead_fraction: float) -> dict:
    if os.path.exists(path):
        os.unlink(path)
    packs = max(8, total_bytes // PACK_BYTES)
    dead = int(packs * dead_fraction)
    live = packs - dead
    pack_bytes = total_bytes // packs
    rng = random.Random(711)
    dead_set = set(rng.sample(range(packs), dead))
    conn = sqlite3.connect(path, timeout=120.0)
    conn.executescript(SCHEMA)
    conn.execute("PRAGMA synchronous=OFF")
    conn.execute("PRAGMA journal_mode=OFF")
    body = bytes(pack_bytes)
    live_ids = []
    dead_ids = []
    conn.execute("BEGIN")
    for pack in range(packs):
        oid = pack.to_bytes(8, "big") + b"\x00" * 24
        conn.execute(
            "INSERT INTO pack(pack_id,domain,digest,length,body) VALUES(?,?,?,?,?)",
            (pack, 0, oid, pack_bytes, body),
        )
        conn.execute(
            "INSERT INTO object_location(object_id,role,canonical_length,pack_id,group_number,record_number) VALUES(?,?,?,?,?,?)",
            (oid, 1, pack_bytes // 2, pack, 0, 0),
        )
        if pack in dead_set:
            dead_ids.append(oid)
        else:
            live_ids.append(oid)
    conn.executemany("INSERT INTO visited(object_id) VALUES(?)", ((x,) for x in live_ids))
    conn.execute(
        "INSERT INTO store_totals(id,logical,canonical,stored,metadata) VALUES(1,?,?,?,?)",
        (packs, packs * (pack_bytes // 2), packs * pack_bytes, 0),
    )
    conn.commit()
    conn.close()
    return {
        "source": path,
        "bytes": os.path.getsize(path),
        "packs": packs,
        "live_packs": live,
        "dead_packs": dead,
        "live_objects": len(live_ids),
        "live_stored": live * pack_bytes,
        "live_canonical": live * (pack_bytes // 2),
        "pack_bytes": pack_bytes,
    }


def delete_dead(conn: sqlite3.Connection) -> None:
    conn.execute("BEGIN IMMEDIATE")
    conn.execute("DELETE FROM object_location WHERE object_id NOT IN (SELECT object_id FROM visited)")
    conn.execute("DELETE FROM pack WHERE pack_id NOT IN (SELECT pack_id FROM object_location)")
    conn.execute(
        "UPDATE store_totals SET canonical=(SELECT COALESCE(SUM(canonical_length),0) FROM object_location),"
        " stored=(SELECT COALESCE(SUM(length),0) FROM pack) WHERE id=1"
    )
    conn.commit()


def verify(path: str, expect: dict) -> dict:
    """Metadata only: no column of ``pack.body`` is ever selected."""
    conn = connect(path, writable=False)
    try:
        objects = conn.execute("SELECT count(*) FROM object_location").fetchone()[0]
        packs = conn.execute("SELECT count(*) FROM pack").fetchone()[0]
        canonical = conn.execute("SELECT COALESCE(SUM(canonical_length),0) FROM object_location").fetchone()[0]
        stored = conn.execute("SELECT COALESCE(SUM(length),0) FROM pack").fetchone()[0]
        totals = conn.execute("SELECT canonical,stored FROM store_totals WHERE id=1").fetchone()
        digests = conn.execute("SELECT count(DISTINCT digest) FROM pack").fetchone()[0]
    finally:
        conn.close()
    ok = (
        objects == expect["live_objects"]
        and packs == expect["live_packs"]
        and canonical == expect["live_canonical"]
        and stored == expect["live_stored"]
        and totals == (expect["live_canonical"], expect["live_stored"])
        and digests == expect["live_packs"]
    )
    return {
        "ok": ok,
        "objects": objects,
        "packs": packs,
        "canonical": canonical,
        "stored": stored,
        "distinct_digests": digests,
        "payload_readback": False,
    }


def strategy_current(source: str, work: str, expect: dict) -> dict:
    """VACUUM INTO, delete, full VACUUM: the shipped build."""
    clone = os.path.join(work, "current.db")
    for suffix in ("", "-journal", "-wal", "-shm"):
        if os.path.exists(clone + suffix):
            os.unlink(clone + suffix)
    evict(source)
    written = io_write_bytes()
    started = time.perf_counter()
    conn = connect(source)
    conn.execute("VACUUM INTO ?", (clone,))
    conn.close()
    copied = time.perf_counter()
    evict(clone)
    conn = connect(clone)
    delete_dead(conn)
    deleted = time.perf_counter()
    evict(clone)
    conn.execute("VACUUM")
    conn.close()
    finished = time.perf_counter()
    return {
        "strategy": "current",
        "steps": ["vacuum_into", "delete", "vacuum"],
        "secs_clone": copied - started,
        "secs_delete": deleted - copied,
        "secs_compact": finished - deleted,
        "secs_total": finished - started,
        "write_bytes": io_write_bytes() - written,
        "source_bytes": os.path.getsize(source),
        "final_bytes": os.path.getsize(clone),
        "verify": verify(clone, expect),
    }


def strategy_clone_incremental(
    source: str, work: str, expect: dict, vacuum: bool, max_steps: int, batch_pages: int
) -> dict:
    """Plain clone, delete, ``incremental_vacuum``: the prototype."""
    clone = os.path.join(work, "prototype.db")
    for suffix in ("", "-journal", "-wal", "-shm"):
        if os.path.exists(clone + suffix):
            os.unlink(clone + suffix)
    evict(source)
    written = io_write_bytes()
    started = time.perf_counter()
    shutil.copyfile(source, clone)
    copied = time.perf_counter()
    evict(clone)
    conn = connect(clone)
    auto = conn.execute("PRAGMA auto_vacuum").fetchone()[0]
    pages_before = conn.execute("PRAGMA page_count").fetchone()[0]
    free_before = conn.execute("PRAGMA freelist_count").fetchone()[0]
    delete_dead(conn)
    pages_deleted = conn.execute("PRAGMA page_count").fetchone()[0]
    free_deleted = conn.execute("PRAGMA freelist_count").fetchone()[0]
    deleted = time.perf_counter()
    steps = 0
    if vacuum:
        # SQLite 3.40's `PRAGMA incremental_vacuum` relocates one page per
        # statement execution and `N` does not fan it out. Stepping it in
        # autocommit costs one journal create/delete per page (measured ~9 ms
        # per page on this medium); the same steps inside a transaction cost
        # ~0.03 ms each. Batches bound the journal to `batch_pages * page_size`
        # while keeping the live body untouched.
        free_initial = free_deleted
        while steps < free_initial and steps < max_steps:
            batch = min(batch_pages, free_initial - steps, max_steps - steps)
            conn.execute("BEGIN IMMEDIATE")
            for _ in range(batch):
                conn.execute("PRAGMA incremental_vacuum")
            conn.execute("COMMIT")
            steps += batch
    pages_after = conn.execute("PRAGMA page_count").fetchone()[0]
    free_after = conn.execute("PRAGMA freelist_count").fetchone()[0]
    compacted = time.perf_counter()
    conn.close()
    return {
        "strategy": "prototype" if vacuum else "prototype_no_vacuum",
        "steps": ["copy", "delete"] + (["incremental_vacuum"] if vacuum else []),
        "auto_vacuum": auto,
        "pages_before": pages_before,
        "pages_deleted": pages_deleted,
        "pages_after": pages_after,
        "free_before": free_before,
        "free_deleted": free_deleted,
        "free_after": free_after,
        "incremental_steps": steps,
        "secs_clone": copied - started,
        "secs_delete": deleted - copied,
        "secs_compact": compacted - deleted,
        "secs_total": compacted - started,
        "write_bytes": io_write_bytes() - written,
        "source_bytes": os.path.getsize(source),
        "final_bytes": os.path.getsize(clone),
        "verify": verify(clone, expect),
    }


def run_one(args: argparse.Namespace) -> dict:
    expect = json.loads(args.expect)
    source = args.source
    work = os.path.dirname(source)
    if args.strategy == "current":
        return strategy_current(source, work, expect)
    if args.strategy == "prototype":
        return strategy_clone_incremental(source, work, expect, True, args.max_steps, args.batch_pages)
    if args.strategy == "prototype_no_vacuum":
        return strategy_clone_incremental(source, work, expect, False, args.max_steps, args.batch_pages)
    raise SystemExit("unknown strategy " + args.strategy)


def rate(secs: float, bytes_: int) -> float:
    return (bytes_ / (1 << 30)) / secs if secs > 0 else 0.0


def suite(args: argparse.Namespace) -> dict:
    os.makedirs(args.dir, exist_ok=True)
    cases = [
        (name, size)
        for name, size in (
            ("small", args.small_mb << 20),
            ("large", args.large_gb << 30),
        )
        if size > 0
    ]
    results = []
    for name, total in cases:
        source = os.path.join(args.dir, f"source-{name}.db")
        expect = build_source(source, total, args.dead)
        case = {"case": name, "setup": expect, "runs": []}
        for strategy in ("current", "prototype", "prototype_no_vacuum"):
            for repeat in range(args.repeat):
                out = subprocess.run(
                    [
                        sys.executable,
                        __file__,
                        "--run",
                        "--source",
                        source,
                        "--strategy",
                        strategy,
                        "--expect",
                        json.dumps(expect),
                        "--max-steps",
                        str(args.max_steps),
                        "--batch-pages",
                        str(args.batch_pages),
                    ],
                    check=True,
                    capture_output=True,
                    text=True,
                    timeout=args.timeout,
                )
                record = json.loads(out.stdout)
                record["repeat"] = repeat
                record["per_gb_total"] = rate(record["secs_total"], record["source_bytes"])
                record["per_gb_delete"] = rate(record["secs_delete"], record["source_bytes"])
                record["per_gb_compact"] = rate(record["secs_compact"], record["source_bytes"])
                case["runs"].append(record)
        results.append(case)
        for path in (source,):
            if os.path.exists(path):
                os.unlink(path)
    return {"dead_fraction": args.dead, "cases": results}


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--run", action="store_true")
    parser.add_argument("--suite", action="store_true")
    parser.add_argument("--dir", default="/tmp/compact-bench")
    parser.add_argument("--source")
    parser.add_argument("--strategy")
    parser.add_argument("--expect")
    parser.add_argument("--small-mb", type=int, default=256)
    parser.add_argument("--large-gb", type=int, default=10)
    parser.add_argument("--dead", type=float, default=0.25)
    parser.add_argument("--repeat", type=int, default=1)
    parser.add_argument("--timeout", type=int, default=3600)
    parser.add_argument("--max-steps", type=int, default=4000000)
    parser.add_argument("--batch-pages", type=int, default=8192)
    parser.add_argument("--out", default=None)
    args = parser.parse_args()
    if args.run:
        result = run_one(args)
    elif args.suite:
        result = suite(args)
    else:
        parser.error("choose --run or --suite")
    text = json.dumps(result, indent=2, sort_keys=True)
    if args.out:
        with open(args.out, "w", encoding="ascii") as handle:
            handle.write(text + "\n")
    print(text)


if __name__ == "__main__":
    main()
