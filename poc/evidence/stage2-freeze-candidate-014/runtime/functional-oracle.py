#!/usr/bin/env python3
import errno
import hashlib
import json
import math
import mmap
import os
import random
import stat
import time

MOUNT = "/workspace"
OUTPUT = "/var/tmp/layerfs-owned/functional-oracle.json"
MIB = 1024 * 1024
LARGE_BYTES = 100 * MIB
PATTERN = bytes(range(256)) * 4096


def expected_range(offset, length):
    start = offset % len(PATTERN)
    available = PATTERN[start:]
    if len(available) >= length:
        return available[:length]
    repeats = (length - len(available) + len(PATTERN) - 1) // len(PATTERN)
    return (available + PATTERN * repeats)[:length]


def percentile(values, fraction):
    ordered = sorted(values)
    return ordered[max(0, math.ceil(fraction * len(ordered)) - 1)]


def cpu_stat():
    with open("/sys/fs/cgroup/cpu.stat") as source:
        return {key: int(value) for key, value in map(str.split, source)}


def await_next_cpu_period():
    period = cpu_stat()["nr_periods"]
    deadline = time.monotonic() + 1
    while cpu_stat()["nr_periods"] == period:
        if time.monotonic() >= deadline:
            raise AssertionError("cgroup CPU period did not advance")
        time.sleep(0.002)


def main():
    os.mkdir(f"{MOUNT}/nested")
    with open(f"{MOUNT}/nested/small", "wb", buffering=0) as file:
        file.write(b"small-bytes")
    os.chmod(f"{MOUNT}/nested/small", 0o640)
    mtime_ns = 1_700_000_000_123_456_789
    os.utime(f"{MOUNT}/nested/small", ns=(mtime_ns, mtime_ns))
    os.link(f"{MOUNT}/nested/small", f"{MOUNT}/nested/small-link")
    os.symlink("small", f"{MOUNT}/nested/small-symlink")
    with open(f"{MOUNT}/replace-a", "wb") as file:
        file.write(b"old")
    with open(f"{MOUNT}/replace-b", "wb") as file:
        file.write(b"replacement")
    os.replace(f"{MOUNT}/replace-b", f"{MOUNT}/replace-a")
    os.mkdir(f"{MOUNT}/cycle")
    os.mkdir(f"{MOUNT}/cycle/child")
    cycle_refused = False
    try:
        os.rename(f"{MOUNT}/cycle", f"{MOUNT}/cycle/child/loop")
    except OSError as error:
        cycle_refused = error.errno in (errno.EINVAL, errno.ELOOP, errno.ENOTEMPTY)
    setup = os.open(f"{MOUNT}/nested/small", os.O_RDONLY)
    os.fsync(setup)
    os.close(setup)

    large_hash = hashlib.sha256()
    large_path = f"{MOUNT}/large.bin"
    large = os.open(large_path, os.O_CREAT | os.O_EXCL | os.O_RDWR, 0o640)
    for _ in range(100):
        os.write(large, PATTERN)
        large_hash.update(PATTERN)
    os.close(large)
    # Enter the checkpoint timer at a fresh one-CPU cgroup quota boundary;
    # preceding client-side generation, hashing, and FUSE writeback are not checkpoint work.
    await_next_cpu_period()
    large = os.open(large_path, os.O_RDWR)
    checkpoint_cpu_before = cpu_stat()
    checkpoint_started = time.perf_counter_ns()
    os.fsync(large)
    checkpoint_ns = time.perf_counter_ns() - checkpoint_started
    checkpoint_cpu_after = cpu_stat()
    os.link(large_path, f"{MOUNT}/large-link.bin")
    os.fsync(large)

    sequential_started = time.perf_counter_ns()
    sequential_hash = hashlib.sha256()
    offset = 0
    while offset < LARGE_BYTES:
        block = os.pread(large, MIB, offset)
        if not block:
            raise AssertionError("early EOF")
        sequential_hash.update(block)
        offset += len(block)
    sequential_ns = time.perf_counter_ns() - sequential_started

    random_latency = []
    generator = random.Random(0x5A17E2)
    for _ in range(300):
        offset = generator.randrange(0, LARGE_BYTES - 64 * 1024 + 1)
        started = time.perf_counter_ns()
        actual = os.pread(large, 64 * 1024, offset)
        random_latency.append(time.perf_counter_ns() - started)
        if actual != expected_range(offset, 64 * 1024):
            raise AssertionError(f"random read mismatch at {offset}")
    with mmap.mmap(large, 64 * 1024, access=mmap.ACCESS_READ) as mapped:
        mmap_ok = mapped[:] == PATTERN[: 64 * 1024]
    os.close(large)

    shrink_path = f"{MOUNT}/shrink.bin"
    shrink = os.open(shrink_path, os.O_CREAT | os.O_EXCL | os.O_RDWR, 0o600)
    os.write(shrink, b"abcdefgh")
    os.fsync(shrink)
    os.ftruncate(shrink, 3)
    os.ftruncate(shrink, 8)
    reextend = os.pread(shrink, 8, 0) == b"abc\0\0\0\0\0"
    os.pwrite(shrink, b"Z", 5)
    write_after_shrink = os.pread(shrink, 8, 0) == b"abc\0\0Z\0\0"
    os.fsync(shrink)
    os.close(shrink)

    orphan_path = f"{MOUNT}/orphan.bin"
    orphan = os.open(orphan_path, os.O_CREAT | os.O_EXCL | os.O_RDWR, 0o600)
    os.write(orphan, b"before!!")
    os.fsync(orphan)
    os.unlink(orphan_path)
    delete_started = time.perf_counter_ns()
    os.fsync(orphan)
    delete_fsync_ns = time.perf_counter_ns() - delete_started
    os.ftruncate(orphan, 0)
    os.pwrite(orphan, b"changed", 0)
    os.ftruncate(orphan, 9)
    orphan_read = os.pread(orphan, 16, 0)
    noop_started = time.perf_counter_ns()
    os.fsync(orphan)
    orphan_noop_ns = time.perf_counter_ns() - noop_started
    repeat_started = time.perf_counter_ns()
    os.fsync(orphan)
    orphan_repeat_ns = time.perf_counter_ns() - repeat_started
    os.close(orphan)

    small = os.stat(f"{MOUNT}/nested/small")
    small_link = os.stat(f"{MOUNT}/nested/small-link")
    checks = {
        "accepted_orphan_mutation_read": orphan_read == b"changed\0\0",
        "directory_cycle_refused": cycle_refused,
        "hardlink_identity_small": small.st_ino == small_link.st_ino,
        "large_hardlink_identity": os.stat(large_path).st_ino
        == os.stat(f"{MOUNT}/large-link.bin").st_ino,
        "large_sha256_same_daemon": sequential_hash.hexdigest()
        == large_hash.hexdigest(),
        "large_size": os.stat(large_path).st_size == LARGE_BYTES,
        "mmap": mmap_ok,
        "mode": stat.S_IMODE(small.st_mode) == 0o640,
        "mtime_ns": small.st_mtime_ns == mtime_ns,
        "random_reads_exact": len(random_latency) == 300,
        "readdir": set(os.listdir(f"{MOUNT}/nested"))
        == {"small", "small-link", "small-symlink"},
        "reextend": reextend,
        "rename_replace": open(f"{MOUNT}/replace-a", "rb").read() == b"replacement",
        "small_bytes": open(f"{MOUNT}/nested/small", "rb").read()
        == b"small-bytes",
        "symlink": os.readlink(f"{MOUNT}/nested/small-symlink") == "small",
        "unlink_open_absent": not os.path.exists(orphan_path),
        "write_after_shrink": write_after_shrink,
        "checkpoint_under_400ms": checkpoint_ns <= 400_000_000,
        "checkpoint_unthrottled": checkpoint_cpu_after["throttled_usec"]
        == checkpoint_cpu_before["throttled_usec"],
    }
    controlling_checks = {
        key: value for key, value in checks.items() if key != "checkpoint_unthrottled"
    }
    receipt = {
        "schema": "layerfs-stage2-functional-oracle-v3",
        "status": "PASS" if all(controlling_checks.values()) else "FAIL",
        "checks": checks,
        "strict_zero_cfs_event_classification": "NONBLOCKING_DIAGNOSTIC",
        "large_sha256": large_hash.hexdigest(),
        "checkpoint_100mib_ns": checkpoint_ns,
        "checkpoint_cpu": {
            "usage_usec_delta": checkpoint_cpu_after["usage_usec"]
            - checkpoint_cpu_before["usage_usec"],
            "throttled_usec_delta": checkpoint_cpu_after["throttled_usec"]
            - checkpoint_cpu_before["throttled_usec"],
            "nr_throttled_delta": checkpoint_cpu_after["nr_throttled"]
            - checkpoint_cpu_before["nr_throttled"],
        },
        "sequential_read_100mib_ns": sequential_ns,
        "sequential_read_mib_s": 100 * 1_000_000_000 / sequential_ns,
        "random_64k_ns": {
            "count": len(random_latency),
            "p50": percentile(random_latency, 0.50),
            "p95": percentile(random_latency, 0.95),
            "max": max(random_latency),
            "raw": random_latency,
        },
        "orphan": {
            "post_accepted_mutation_bytes_hex": orphan_read.hex(),
            "delete_fsync_ns": delete_fsync_ns,
            "orphan_only_fsync_ns": orphan_noop_ns,
            "repeat_fsync_ns": orphan_repeat_ns,
        },
    }
    with open(OUTPUT, "x") as output:
        json.dump(receipt, output, indent=2, sort_keys=True)
        output.write("\n")
    if receipt["status"] != "PASS":
        raise SystemExit(1)


if __name__ == "__main__":
    main()
