#!/usr/bin/env python3
"""Correctness probes for an install-only lane.

Three cases, all on the store-shaped model:

``naive_vs_closure``
    A commit after the mark dedupes a file onto an object that lived in a pack
    the mark skipped.  A delta that copies only packs above the mark's
    ``next_pack_id`` loses that object; a re-mark that walks the closure of the
    post-mark roots copies the old pack whole.  The loss is measured.

``adversarial_delta``
    A post-mark commit references every object the mark skipped (a re-save of
    a deleted tree that content dedupe maps onto old objects).  The delta is
    then the whole dead set, not the bytes written after the mark, so the delta
    copy has no bound in the post-mark write volume.

``concurrent_fence``
    Writers keep committing while the delta copy runs.  A delta watermark is
    always behind the newest commit, so install needs a fence: stop writers,
    catch up the delta, verify, rename.  The probe measures the loss without a
    fence and the fence window with one.
"""
from __future__ import annotations

import os
import threading
import time

import store


def _reset_dirs(directory: str) -> str:
    os.makedirs(directory, exist_ok=True)
    return directory


def naive_vs_closure(directory: str, size_mib: int = 64) -> dict:
    directory = _reset_dirs(directory)
    source = os.path.join(directory, "source.db")
    info = store.make_fixture(source, size_mib)
    marked = store.mark(source)
    naive = os.path.join(directory, "naive.db")
    closure = os.path.join(directory, "closure.db")
    naive_build = store.build(source, naive, marked)
    closure_build = store.build(source, closure, marked)

    # The post-mark commit: one 1 MiB pack plus a dedupe reference to an object
    # in a pack the mark skipped (and its seven chained refs).
    stop = threading.Event()
    dead_object = store.oid(info["dead_packs"][0], 0)
    writer = store.Writer(source, stop, deadline=time.monotonic() + 30, dedupe_every=1,
                          dead_pool=[dead_object])
    writer.start()
    time.sleep(0.3)
    stop.set()
    writer.join()
    post_mark = {"commits": writer.commits, "dedupe_refs": writer.dedupe_refs}

    naive_delta = store.remark_delta(source, naive, marked, "new_packs_only")
    naive_verify = store.verify(source, naive)
    closure_delta = store.remark_delta(source, closure, marked, "closure")
    closure_verify = store.verify(source, closure)
    return {
        "fixture": {"size_mib": size_mib, "dead_packs": len(info["dead_packs"])},
        "post_mark": post_mark,
        "naive_delta": {k: naive_delta[k] for k in
                        ("delta_objects", "delta_packs", "delta_bytes", "delta_seconds")},
        "naive_verify": naive_verify,
        "closure_delta": {k: closure_delta[k] for k in
                          ("delta_objects", "delta_packs", "delta_bytes", "delta_seconds")},
        "closure_verify": closure_verify,
        "data_lost_without_remark": naive_verify["missing"] > 0,
        "recovered_by_closure_remark": closure_verify["ok"],
        "build": {"naive_seconds": naive_build["seconds"], "closure_seconds": closure_build["seconds"]},
    }


def adversarial_delta(directory: str, size_mib: int = 256) -> dict:
    directory = _reset_dirs(directory)
    source = os.path.join(directory, "source.db")
    target = os.path.join(directory, "target.db")
    info = store.make_fixture(source, size_mib)
    marked = store.mark(source)
    build = store.build(source, target, marked)

    # A post-mark commit (or a set of them) references every skipped object:
    # the shape of re-saving a deleted tree whose content the store dedupes.
    conn = store.connect(source)
    conn.execute("BEGIN IMMEDIATE")
    hw = store.read_high_water(conn)
    dead_objects = [store.oid(pack_id, index)
                    for pack_id in info["dead_packs"]
                    for index in range(store.OBJECTS_PER_PACK)]
    serial = hw[0]
    roots = []
    root_objects = []
    kept = []
    for index, object_id in enumerate(dead_objects):
        serial += 1
        root_id = store.new_root_id(serial, 1)
        roots.append((root_id, "commit", serial))
        root_objects.append((root_id, object_id))
        kept.append(("commit", root_id, serial))
    conn.executemany("INSERT INTO root VALUES(?, ?, ?)", roots)
    conn.executemany("INSERT INTO root_object VALUES(?, ?)", root_objects)
    conn.executemany("INSERT INTO kept_node VALUES(?, ?, ?)", kept)
    conn.execute("UPDATE high_water SET serial=? WHERE 1", (serial,))
    conn.execute("COMMIT")
    conn.close()

    delta = store.remark_delta(source, target, marked, "closure")
    verified = store.verify(source, target)
    return {
        "fixture": {"size_mib": size_mib, "dead_packs": len(info["dead_packs"]),
                    "dead_bytes": info["dead_bytes"]},
        "post_mark_new_bytes": 0,
        "delta_bytes": delta["delta_bytes"],
        "delta_packs": delta["delta_packs"],
        "delta_seconds": delta["delta_seconds"],
        "delta_over_new_writes": None,
        "delta_over_dead_set": delta["delta_bytes"] / info["dead_bytes"] if info["dead_bytes"] else None,
        "verify": verified,
        "build_seconds": build["seconds"],
    }


def concurrent_fence(directory: str, size_mib: int = 64, seconds: float = 3.0,
                     writers: int = 1) -> dict:
    directory = _reset_dirs(directory)
    source = os.path.join(directory, "source.db")
    target = os.path.join(directory, "target.db")
    info = store.make_fixture(source, size_mib)
    marked = store.mark(source)
    store.build(source, target, marked)

    stop = threading.Event()
    pool = store.dead_object_pool(source, info["dead_packs"][:16])
    threads = [
        store.Writer(source, stop, deadline=time.monotonic() + seconds + 60,
                     dedupe_every=97, dead_pool=pool, salt=salt, max_commits=400)
        for salt in range(writers)
    ]
    for thread in threads:
        thread.start()
    time.sleep(0.5)

    # Watermark w1: the delta copies roots committed up to here, while the
    # writers keep going.
    first = store.remark_delta(source, target, marked, "closure")
    w1 = first["install_serial"]
    time.sleep(0.5)
    verified_at_w1 = store.verify(source, target, fence_serial=w1)
    conn = store.connect(source)
    w2 = store.read_high_water(conn)[0]
    conn.close()
    verified_at_w2 = store.verify(source, target, fence_serial=w2)

    # The fence: quiesce writers, catch up the delta, verify, then install.
    fence_started = time.perf_counter()
    stop.set()
    for thread in threads:
        thread.join()
    conn = store.connect(source)
    w3 = store.read_high_water(conn)[0]
    conn.close()
    catch_up = store.remark_delta(source, target, marked, "closure")
    verified_at_w3 = store.verify(source, target, fence_serial=w3)
    fence_seconds = time.perf_counter() - fence_started
    commits = sum(thread.commits for thread in threads)
    return {
        "fixture": {"size_mib": size_mib, "dead_packs": len(info["dead_packs"])},
        "writers": writers,
        "writer_commits": commits,
        "writer_errors": [error for thread in threads for error in thread.errors],
        "first_delta": {k: first[k] for k in ("new_roots", "delta_packs", "delta_bytes", "delta_seconds")},
        "watermarks": {"mark": marked["hw"][0], "w1": w1, "w2": w2, "w3": w3},
        "verify_at_w1": verified_at_w1,
        "verify_at_w2_without_fence": verified_at_w2,
        "commits_after_first_delta": commits - max(0, w1 - marked["hw"][0]),
        "catch_up_delta": {k: catch_up[k] for k in ("new_roots", "delta_packs", "delta_bytes", "delta_seconds")},
        "verify_at_w3": verified_at_w3,
        "fence_seconds": fence_seconds,
        "needs_fence": (not verified_at_w2["ok"]) and verified_at_w3["ok"],
    }


def main() -> None:
    import argparse
    import json

    parser = argparse.ArgumentParser()
    parser.add_argument("--dir", default="/var/tmp/installlane-hazard")
    parser.add_argument("--size-mib", type=int, default=64)
    parser.add_argument("--out", default=None)
    args = parser.parse_args()
    result = {
        "naive_vs_closure": naive_vs_closure(os.path.join(args.dir, "naive"), args.size_mib),
        "adversarial_delta": adversarial_delta(os.path.join(args.dir, "adversarial"), args.size_mib * 4),
        "concurrent_fence": concurrent_fence(os.path.join(args.dir, "fence"), args.size_mib),
    }
    text = json.dumps(result, indent=2, sort_keys=True)
    if args.out:
        with open(args.out, "w", encoding="ascii") as handle:
            handle.write(text + "\n")
    print(text)


if __name__ == "__main__":
    main()
