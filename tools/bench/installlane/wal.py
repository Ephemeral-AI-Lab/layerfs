#!/usr/bin/env python3
"""WAL growth, drain rate, catch-up and backpressure probes.

These model the WAL cost of an install-only lane: a build holds a read snapshot
of the sealed Store (``immutable=1`` in production; a read transaction here),
so no checkpoint can truncate the WAL while it runs.  Writers keep committing
into that WAL.  At install the snapshot has to close and the backlog must be
checkpointed before the swap.

Modes:

``hold``    writers run for ``seconds`` with a reader snapshot open.  Measures
            the WAL growth rate and the final checkpoint.
``drain``   writers and a checkpoint loop run together, no read snapshot.
            Measures whether the checkpointer outruns the writers (it must, for
            a catch-up loop to converge).
``cap``     as ``hold``/``drain`` but a commit gate stalls a writer while the
            WAL is above ``cap_mb``.  Measures what a bounded WAL costs in
            writer availability and latency.
``catchup`` seed ``preset_mb`` of WAL under a snapshot, close it, then run
            writers plus a checkpoint loop.  Measures whether the backlog
            drains while writers continue.
"""

from __future__ import annotations

import argparse
import json
import os
import statistics
import threading
import time

MIB = 1 << 20


def connect(path: str) -> "sqlite3.Connection":
    import sqlite3

    conn = sqlite3.connect(path, timeout=60.0, isolation_level=None)
    conn.execute("PRAGMA busy_timeout=60000")
    # The sealed Store cannot be checkpointed while the build holds its read
    # snapshot (in production the file is attached immutable=1).  No automatic
    # checkpoint means the WAL grows by every committed byte and install pays
    # the whole backlog, which is the cost this prototype measures.
    conn.execute("PRAGMA wal_autocheckpoint=0")
    return conn


def wal_bytes(path: str) -> int:
    try:
        return os.path.getsize(path + "-wal")
    except OSError:
        return 0


def reset(path: str) -> None:
    for suffix in ("", "-wal", "-shm", "-journal"):
        try:
            os.unlink(path + suffix)
        except OSError:
            pass


def _seed(path: str, seed_mb: int) -> None:
    import sqlite3

    conn = connect(path)
    conn.execute("PRAGMA journal_mode=WAL")
    conn.execute("PRAGMA synchronous=NORMAL")
    conn.execute("PRAGMA cache_size=-65536")
    conn.execute("CREATE TABLE t(id INTEGER PRIMARY KEY, body BLOB NOT NULL, w INTEGER NOT NULL)")
    conn.execute("BEGIN")
    conn.executemany("INSERT INTO t(body, w) VALUES(zeroblob(?), 0)", [(MIB,)] * seed_mb)
    conn.execute("COMMIT")
    conn.close()


def _writer(path, stop, deadline, latencies, errors, committed, cap_mb, stalls, salt):
    import sqlite3

    conn = connect(path)
    conn.execute("PRAGMA synchronous=NORMAL")
    conn.execute("PRAGMA cache_size=-65536")
    while not stop.is_set() and time.monotonic() < deadline:
        if cap_mb:
            waited = 0.0
            while wal_bytes(path) > cap_mb * MIB and not stop.is_set():
                time.sleep(0.002)
                waited += 0.002
            if waited:
                stalls.append(waited)
        started = time.perf_counter()
        try:
            conn.execute("INSERT INTO t(body, w) VALUES(zeroblob(?), ?)", (MIB, salt))
            latencies.append(time.perf_counter() - started)
            committed[0] += 1
        except sqlite3.Error as error:  # a refusal is a finding
            errors.append(str(error))
            try:
                conn.execute("ROLLBACK")
            except sqlite3.Error:
                pass
    conn.close()


def _checkpointer(path, stop, results, mode="truncate"):
    conn = connect(path)
    conn.execute("PRAGMA busy_timeout=0")
    while not stop.is_set():
        started = time.perf_counter()
        try:
            row = conn.execute(f"PRAGMA wal_checkpoint({mode.upper()})").fetchone()
            results.append({"result": list(row), "secs": time.perf_counter() - started, "wal": wal_bytes(path)})
        except Exception as error:  # noqa: BLE001 - reported, not raised
            results.append({"error": str(error), "secs": time.perf_counter() - started})
            time.sleep(0.002)
        if results and results[-1].get("result") and results[-1]["result"][0] != 0:
            time.sleep(0.001)
    conn.close()


def probe(
    path: str,
    seed_mb: int,
    seconds: float,
    cap_mb: int | None = None,
    drain: bool = False,
    preset_mb: int = 0,
    writers: int = 2,
    preset_from_reader: bool = True,
) -> dict:
    reset(path)
    _seed(path, seed_mb)
    stop = threading.Event()
    latencies: list[float] = []
    errors: list[str] = []
    committed = [0]
    stalls: list[float] = []
    checkpoints: list[dict] = []
    samples: list[tuple[float, int]] = []
    sampler_stop = threading.Event()

    def sampler() -> None:
        started = time.monotonic()
        while not sampler_stop.is_set():
            samples.append((time.monotonic() - started, wal_bytes(path)))
            time.sleep(0.02)

    def start_sampler() -> None:
        sampler_stop.clear()
        thread = threading.Thread(target=sampler, daemon=True)
        thread.start()
        return thread

    def stop_sampler(thread) -> None:
        sampler_stop.set()
        thread.join()

    backlog = 0
    snapshot = snapshot_after = None
    started_at = time.monotonic()
    deadline = started_at + seconds

    def open_keeper() -> "sqlite3.Connection":
        conn = connect(path)
        conn.execute("SELECT count(*) FROM t").fetchone()  # open the pager
        return conn

    keeper = None
    if preset_mb:
        # A build snapshot holds the checkpoint back while the WAL fills; this
        # is the backlog install inherits.
        reader = connect(path)
        reader.execute("BEGIN")
        snapshot = reader.execute("SELECT count(*), COALESCE(SUM(length(body)), 0) FROM t").fetchone()
        pre = connect(path)
        pre.execute("PRAGMA synchronous=NORMAL")
        pre.execute("BEGIN")
        pre.executemany("INSERT INTO t(body, w) VALUES(zeroblob(?), -1)", [(MIB,)] * preset_mb)
        pre.execute("COMMIT")
        pre.close()
        keeper = open_keeper()
        if not preset_from_reader:
            # Install starts: the build snapshot closes, the backlog stays.
            reader.execute("COMMIT")
            reader.close()
            reader = None
    elif drain:
        reader = None
        keeper = open_keeper()
    else:
        reader = connect(path)
        reader.execute("BEGIN")
        snapshot = reader.execute("SELECT count(*), COALESCE(SUM(length(body)), 0) FROM t").fetchone()
        keeper = open_keeper()
    backlog = wal_bytes(path)

    # WAL mode is set by the seed; a writer connection is opened per thread.
    check_thread = (
        threading.Thread(target=_checkpointer, args=(path, stop, checkpoints)) if drain or preset_mb else None
    )
    sample_thread = start_sampler()
    threads = [
        threading.Thread(
            target=_writer, args=(path, stop, deadline, latencies, errors, committed, cap_mb, stalls, salt)
        )
        for salt in range(writers)
    ]
    for thread in threads:
        thread.start()
    if check_thread:
        check_thread.start()
    time.sleep(seconds)
    # Stop sampling before the writers close: the last writer close can delete
    # the WAL and would read as a drain that never happened.
    stop_sampler(sample_thread)
    stop.set()
    for thread in threads:
        thread.join()
    if check_thread:
        check_thread.join()
    elapsed = time.monotonic() - started_at
    if reader is not None:
        snapshot_after = reader.execute("SELECT count(*), COALESCE(SUM(length(body)), 0) FROM t").fetchone()
    held_checkpoint = None
    if reader is not None and not preset_mb:
        attempt = connect(path)
        attempt.execute("PRAGMA busy_timeout=5000")
        attempt_started = time.perf_counter()
        held_checkpoint = {
            "result": list(attempt.execute("PRAGMA wal_checkpoint(TRUNCATE)").fetchone()),
            "secs": time.perf_counter() - attempt_started,
            "wal": wal_bytes(path),
        }
        attempt.close()
    if reader is not None:
        reader.execute("COMMIT")
        reader.close()

    keeper_started = time.perf_counter()
    final_row = keeper.execute("PRAGMA wal_checkpoint(TRUNCATE)").fetchone()
    final_secs = time.perf_counter() - keeper_started
    final_wal = wal_bytes(path)
    keeper.close()

    ordered = sorted(latencies)
    wal_at = [value for _, value in samples]
    peak = max(wal_at) if wal_at else 0
    end_wal = wal_at[-1] if wal_at else 0
    # Growth over the sampled window, not start/end: the last writer close can
    # delete the WAL, which would read as a drain that never happened.
    if len(samples) > 1:
        first_t, first_wal = samples[0]
        last_t, last_wal = samples[-1]
        growth = (last_wal - first_wal) / (last_t - first_t) if last_t > first_t else 0.0
    else:
        growth = 0.0
    if cap_mb:
        converged = peak <= cap_mb * MIB * 1.15
    elif preset_mb:
        converged = end_wal <= backlog
    elif drain:
        converged = peak <= max(64 * MIB, backlog + 32 * MIB)
    else:
        converged = None
    return {
        "mode": "catchup" if preset_mb else ("drain" if drain else "hold"),
        "seed_mb": seed_mb,
        "seconds": round(elapsed, 3),
        "writers": writers,
        "cap_mb": cap_mb,
        "snapshot": list(snapshot) if snapshot else None,
        "snapshot_after": list(snapshot_after) if snapshot_after else None,
        "snapshot_stable": snapshot == snapshot_after if snapshot else None,
        "committed_mib": committed[0],
        "committed_bytes": committed[0] * MIB,
        "write_rate_mib_s": round(committed[0] / elapsed, 2) if elapsed else 0,
        "commit_ms_p50": round((statistics.median(ordered) * 1000), 3) if ordered else 0.0,
        "commit_ms_p99": round((ordered[min(len(ordered) - 1, int(len(ordered) * 0.99))] * 1000), 3)
        if ordered
        else 0.0,
        "commit_ms_max": round((ordered[-1] * 1000), 3) if ordered else 0.0,
        "writer_errors": errors,
        "cap_stalls": len(stalls),
        "cap_stalled_seconds": round(sum(stalls), 3),
        "start_wal_bytes": backlog,
        "peak_wal_bytes": peak,
        "end_wal_bytes": end_wal,
        "wal_growth_mib_s": round(growth / MIB, 2),
        "wal_min_bytes": min(wal_at) if wal_at else 0,
        "wal_max_bytes": max(wal_at) if wal_at else 0,
        "wal_samples_tail": [(round(t, 2), b) for t, b in samples[-10:]],
        "wal_samples": [(round(t, 3), b) for t, b in samples[::5]],
        "checkpoint_held_by_reader": held_checkpoint,
        "checkpoint_loop": {
            "calls": len(checkpoints),
            "truncate_ok": sum(1 for c in checkpoints if c.get("result") and c["result"][0] == 0),
            "busy": sum(1 for c in checkpoints if c.get("result") and c["result"][0] != 0),
            "errors": sum(1 for c in checkpoints if "error" in c),
            "total_secs": round(sum(c.get("secs", 0) for c in checkpoints), 3),
        },
        "drained_while_writing": converged,
        "final_checkpoint": {"result": list(final_row), "secs": round(final_secs, 3)},
        "final_checkpoint_mib_s": round((peak / MIB) / final_secs, 2) if final_secs else None,
        "wal_after_final_checkpoint": final_wal,
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--dir", default="/var/tmp/installlane-wal")
    parser.add_argument("--mode", choices=["hold", "drain", "cap", "cap_drain", "catchup"], default="hold")
    parser.add_argument("--seed-mb", type=int, default=256)
    parser.add_argument("--seconds", type=float, default=10.0)
    parser.add_argument("--cap-mb", type=int, default=128)
    parser.add_argument("--preset-mb", type=int, default=1024)
    parser.add_argument("--writers", type=int, default=2)
    parser.add_argument("--out", default=None)
    args = parser.parse_args()
    os.makedirs(args.dir, exist_ok=True)
    path = os.path.join(args.dir, "wal.db")
    if args.mode == "hold":
        result = probe(path, args.seed_mb, args.seconds, writers=args.writers)
    elif args.mode == "drain":
        result = probe(path, args.seed_mb, args.seconds, drain=True, writers=args.writers)
    elif args.mode == "cap":
        result = probe(path, args.seed_mb, args.seconds, cap_mb=args.cap_mb, writers=args.writers)
    elif args.mode == "cap_drain":
        result = probe(path, args.seed_mb, args.seconds, cap_mb=args.cap_mb, drain=True, writers=args.writers)
    else:
        result = probe(
            path, args.seed_mb, args.seconds, preset_mb=args.preset_mb, writers=args.writers, preset_from_reader=False
        )
    text = json.dumps(result, indent=2, sort_keys=True)
    if args.out:
        with open(args.out, "w", encoding="ascii") as handle:
            handle.write(text + "\n")
    print(text)


if __name__ == "__main__":
    main()
