#!/usr/bin/env python3
"""Install-only-lane measurement suite.

Two arms:

``small``    WAL/backpressure probes plus the three correctness probes, on a
             64-256 MiB store-shaped model.
``ten_gib``  one 10 GiB store-shaped fixture: mark, fresh-copy build, then a
             build snapshot held open while writers commit 6.2 GiB of WAL
             (32.5 s x 190 MiB/s, the measured writer rate), an incremental
             re-mark, the delta copy, the checkpoint and the verification.
             The 50 GB arm is never run.

Run: ``python3 suite.py --arm small`` or ``--arm ten_gib``.
"""
from __future__ import annotations

import argparse
import json
import os
import threading
import time

import hazard
import store
import wal

MIB = 1 << 20
GIB = 1 << 30


def small(directory: str) -> dict:
    os.makedirs(directory, exist_ok=True)
    result: dict = {"arm": "small", "host": {"python": os.sys.version.split()[0]}}
    result["wal_hold"] = wal.probe(os.path.join(directory, "wal-hold.db"), 256, 10.0, writers=2)
    result["wal_cap_hold"] = wal.probe(os.path.join(directory, "wal-cap.db"), 256, 10.0,
                                       cap_mb=256, writers=2)
    result["wal_cap_drain"] = wal.probe(os.path.join(directory, "wal-capdrain.db"), 64, 10.0,
                                        cap_mb=128, drain=True, writers=2)
    result["wal_catchup"] = wal.probe(os.path.join(directory, "wal-catchup.db"), 64, 15.0,
                                      preset_mb=1024, writers=2)
    result["naive_vs_closure"] = hazard.naive_vs_closure(os.path.join(directory, "naive"), 64)
    result["adversarial_delta"] = hazard.adversarial_delta(os.path.join(directory, "adversarial"), 256)
    result["concurrent_fence"] = hazard.concurrent_fence(os.path.join(directory, "fence"), 64)
    return result


def ten_gib(directory: str, size_gib: float = 10.0, delta_gib: float = 6.2,
            writers: int = 2) -> dict:
    os.makedirs(directory, exist_ok=True)
    source = os.path.join(directory, "source.db")
    target = os.path.join(directory, "target.db")
    result: dict = {
        "arm": "ten_gib",
        "size_gib": size_gib,
        "delta_gib": delta_gib,
        "writers": writers,
    }

    started = time.perf_counter()
    info = store.make_fixture(source, int(size_gib * 1024))
    result["fixture"] = {
        "seconds": time.perf_counter() - started,
        "packs": info["packs"],
        "bytes": info["bytes"],
        "dead_packs": len(info["dead_packs"]),
        "dead_bytes": info["dead_bytes"],
        "objects": info["objects"],
    }

    marked = store.mark(source)
    result["mark"] = {
        "seconds": marked["seconds"],
        "packs": marked["packs"],
        "objects": marked["objects"],
        "bytes": marked["bytes"],
        "high_water": list(marked["hw"]),
    }
    built = store.build(source, target, marked)
    result["build"] = {"seconds": built["seconds"], "packs": built["packs"], "bytes": built["bytes"]}

    # The build's read snapshot: in production this is the immutable=1 attached
    # sealed file, which keeps the source file byte-identical and therefore
    # blocks every checkpoint.  Here a read transaction holds the same line.
    reader = store.connect(source)
    reader.execute("BEGIN")
    reader.execute("SELECT count(*) FROM pack").fetchone()
    wal_stop = threading.Event()
    samples: list[tuple[float, int]] = []

    def sample() -> None:
        started = time.monotonic()
        while not wal_stop.is_set():
            samples.append((time.monotonic() - started, store.wal_bytes(source)))
            time.sleep(0.05)

    wal_thread = threading.Thread(target=sample)
    wal_thread.start()

    deadline = time.monotonic() + 180.0
    per_writer = max(1, int(delta_gib * 1024 / writers))
    stop = threading.Event()
    pool = store.dead_object_pool(source, info["dead_packs"][:64])
    threads = [
        store.Writer(source, stop, deadline=deadline, dedupe_every=50, dead_pool=pool,
                     salt=salt, max_commits=per_writer)
        for salt in range(writers)
    ]
    write_started = time.perf_counter()
    for thread in threads:
        thread.start()
    for thread in threads:
        thread.join()
    write_seconds = time.perf_counter() - write_started
    wal_stop.set()
    wal_thread.join()
    commits = sum(thread.commits for thread in threads)
    latencies = [latency for thread in threads for latency in thread.latencies]
    result["writer"] = {
        "commits": commits,
        "mib": commits,
        "seconds": write_seconds,
        "rate_mib_s": commits / write_seconds,
        "dedupe_refs": sum(thread.dedupe_refs for thread in threads),
        "errors": [error for thread in threads for error in thread.errors],
        **store.summarise_latencies(latencies),
        "wal_peak_bytes": max((value for _, value in samples), default=0),
        "wal_growth_mib_s": ((samples[-1][1] - samples[0][1]) / MIB / (samples[-1][0] - samples[0][0]))
        if len(samples) > 1 and samples[-1][0] > samples[0][0] else 0.0,
        "wal_samples": [(round(t, 2), value) for t, value in samples[::int(max(1, len(samples) / 20))]],
    }
    wal_before_checkpoint = store.wal_bytes(source)

    # Install: the frozen fence would compare the mark's tuple with the current
    # one; writers moved every field, so it refuses Conflict.
    probe_conn = store.connect(source)
    current = store.read_high_water(probe_conn)
    probe_conn.close()
    result["frozen_fence"] = {
        "mark_tuple": list(marked["hw"]),
        "current_tuple": list(current),
        "same_tuple": tuple(marked["hw"]) == tuple(current),
        "would_refuse_conflict": tuple(marked["hw"]) != tuple(current),
    }

    delta = store.remark_delta(source, target, marked, "closure")
    result["remark_delta"] = {
        key: delta[key]
        for key in ("scan_seconds", "copy_seconds", "delta_seconds", "delta_objects",
                    "delta_packs", "delta_bytes", "new_roots", "install_serial")
    }
    result["remark_delta"]["delta_packs_over_writer_commits"] = (
        delta["delta_packs"] / commits if commits else None)
    result["remark_delta"]["delta_over_wal_bytes"] = (
        delta["delta_bytes"] / wal_before_checkpoint if wal_before_checkpoint else None)

    # Close the build snapshot, then checkpoint the backlog.  The keeper holds
    # the pager open so the close does not run its own checkpoint.
    keeper = store.connect(source)
    keeper.execute("SELECT count(*) FROM pack").fetchone()
    reader.execute("COMMIT")
    reader.close()
    checkpoint_started = time.perf_counter()
    checkpoint_row = keeper.execute("PRAGMA wal_checkpoint(TRUNCATE)").fetchone()
    checkpoint_seconds = time.perf_counter() - checkpoint_started
    result["checkpoint"] = {
        "wal_bytes_before": wal_before_checkpoint,
        "seconds": checkpoint_seconds,
        "mib_s": (wal_before_checkpoint / MIB / checkpoint_seconds) if checkpoint_seconds else None,
        "result": list(checkpoint_row),
        "wal_bytes_after": store.wal_bytes(source),
    }
    keeper.close()

    verify_started = time.perf_counter()
    verified = store.verify(source, target)
    verify_seconds = time.perf_counter() - verify_started
    result["verify"] = {**verified, "seconds": verify_seconds}

    install_only = checkpoint_seconds + delta["delta_seconds"] + verify_seconds
    result["comparison"] = {
        "current_path_exclusion_seconds": built["seconds"],
        "install_only_fence_seconds": install_only,
        "install_only_shorter": install_only < built["seconds"],
        "install_over_current_ratio": install_only / built["seconds"] if built["seconds"] else None,
        "delta_bytes": delta["delta_bytes"],
        "wal_bytes": wal_before_checkpoint,
        "build_bytes": built["bytes"],
    }
    return result


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--arm", choices=["small", "ten_gib"], required=True)
    parser.add_argument("--dir", default=None)
    parser.add_argument("--out", default=None)
    parser.add_argument("--size-gib", type=float, default=10.0)
    parser.add_argument("--delta-gib", type=float, default=6.2)
    parser.add_argument("--writers", type=int, default=2)
    args = parser.parse_args()
    directory = args.dir or os.path.join("/var/tmp", "installlane", args.arm)
    if args.arm == "small":
        result = small(directory)
    else:
        result = ten_gib(directory, args.size_gib, args.delta_gib, args.writers)
    text = json.dumps(result, indent=2, sort_keys=True, default=str)
    out = args.out or os.path.join(directory, "results.json")
    with open(out, "w", encoding="ascii") as handle:
        handle.write(text + "\n")
    print(text)


if __name__ == "__main__":
    main()
