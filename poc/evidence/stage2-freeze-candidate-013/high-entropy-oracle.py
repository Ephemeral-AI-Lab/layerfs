#!/usr/bin/env python3
import hashlib
import json
import os
import time

MIB = 1024 * 1024
COUNT = 100
PATH = "/workspace/high-entropy.bin"
OUTPUT = "/var/tmp/layerfs-owned/high-entropy-oracle.json"


def cpu_stat():
    with open("/sys/fs/cgroup/cpu.stat") as source:
        return {key: int(value) for key, value in map(str.split, source)}


digest = hashlib.sha256()
file = os.open(PATH, os.O_CREAT | os.O_EXCL | os.O_RDWR, 0o600)
for index in range(COUNT):
    block = hashlib.shake_256(
        b"layerfs-stage2-high-entropy-v1" + index.to_bytes(4, "big")
    ).digest(MIB)
    digest.update(block)
    view = memoryview(block)
    while view:
        written = os.write(file, view)
        if written <= 0:
            raise AssertionError("short write")
        view = view[written:]
os.close(file)

file = os.open(PATH, os.O_RDONLY)
before = cpu_stat()
started = time.perf_counter_ns()
os.fsync(file)
checkpoint_ns = time.perf_counter_ns() - started
after = cpu_stat()
os.close(file)

checks = {
    "checkpoint_completed": checkpoint_ns > 0,
    "exact_size": os.stat(PATH).st_size == COUNT * MIB,
}
receipt = {
    "schema": "layerfs-stage2-high-entropy-checkpoint-v1",
    "status": "PASS" if all(checks.values()) else "FAIL",
    "checks": checks,
    "bytes": COUNT * MIB,
    "sha256": digest.hexdigest(),
    "checkpoint_ns": checkpoint_ns,
    "checkpoint_cpu": {
        "usage_usec_delta": after["usage_usec"] - before["usage_usec"],
        "nr_throttled_delta": after["nr_throttled"] - before["nr_throttled"],
        "throttled_usec_delta": after["throttled_usec"] - before["throttled_usec"],
    },
}
with open(OUTPUT, "x") as output:
    json.dump(receipt, output, indent=2, sort_keys=True)
    output.write("\n")
if receipt["status"] != "PASS":
    raise SystemExit(1)
