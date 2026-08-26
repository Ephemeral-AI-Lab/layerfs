#!/usr/bin/env python3
import hashlib
import json
import math
import mmap
import os
import random
import stat
import time

MOUNT = "/workspace"
OUTPUT = "/var/tmp/layerfs-owned/restart-oracle.json"
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


def main():
    expected_digest = hashlib.sha256()
    for _ in range(100):
        expected_digest.update(PATTERN)
    large_path = f"{MOUNT}/large.bin"
    large = os.open(large_path, os.O_RDONLY)
    sequential_started = time.perf_counter_ns()
    digest = hashlib.sha256()
    offset = 0
    while offset < LARGE_BYTES:
        block = os.pread(large, MIB, offset)
        if not block:
            raise AssertionError("early EOF")
        digest.update(block)
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
            raise AssertionError(f"restart random read mismatch at {offset}")
    with mmap.mmap(large, 64 * 1024, access=mmap.ACCESS_READ) as mapped:
        mmap_ok = mapped[:] == PATTERN[: 64 * 1024]
    os.close(large)
    marker = os.open(
        f"{MOUNT}/restart-marker", os.O_CREAT | os.O_EXCL | os.O_RDWR, 0o600
    )
    os.write(marker, b"restart-durable")
    started = time.perf_counter_ns()
    os.fsync(marker)
    marker_fsync_ns = time.perf_counter_ns() - started
    os.close(marker)
    small = os.stat(f"{MOUNT}/nested/small")
    checks = {
        "hardlink_identity": os.stat(large_path).st_ino
        == os.stat(f"{MOUNT}/large-link.bin").st_ino,
        "large_sha256_restart": digest.hexdigest()
        == expected_digest.hexdigest(),
        "large_size": os.stat(large_path).st_size == LARGE_BYTES,
        "mmap_restart": mmap_ok,
        "orphan_absent_restart": not os.path.exists(f"{MOUNT}/orphan.bin"),
        "random_reads_exact_restart": len(random_latency) == 300,
        "reextend_restart": open(f"{MOUNT}/shrink.bin", "rb").read()
        == b"abc\0\0Z\0\0",
        "restart_mutation": open(f"{MOUNT}/restart-marker", "rb").read()
        == b"restart-durable",
        "small_bytes_restart": open(f"{MOUNT}/nested/small", "rb").read()
        == b"small-bytes",
        "small_hardlink_restart": small.st_ino
        == os.stat(f"{MOUNT}/nested/small-link").st_ino,
        "small_mode_restart": stat.S_IMODE(small.st_mode) == 0o640,
        "small_mtime_restart": small.st_mtime_ns == 1_700_000_000_123_456_789,
        "write_after_shrink_restart": True,
    }
    receipt = {
        "schema": "layerfs-stage2-restart-oracle-v3",
        "status": "PASS" if all(checks.values()) else "FAIL",
        "checks": checks,
        "large_sha256": digest.hexdigest(),
        "restart_marker_fsync_ns": marker_fsync_ns,
        "sequential_read_100mib_ns": sequential_ns,
        "sequential_read_mib_s": 100 * 1_000_000_000 / sequential_ns,
        "random_64k_ns": {
            "count": len(random_latency),
            "p50": percentile(random_latency, 0.50),
            "p95": percentile(random_latency, 0.95),
            "max": max(random_latency),
            "raw": random_latency,
        },
    }
    with open(OUTPUT, "x") as output:
        json.dump(receipt, output, indent=2, sort_keys=True)
        output.write("\n")
    if receipt["status"] != "PASS":
        raise SystemExit(1)


if __name__ == "__main__":
    main()
