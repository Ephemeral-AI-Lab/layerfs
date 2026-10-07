"""Single-attempt external resource observations; no qualified phase/peak claim.

Each call retains its actual read interval and process incarnation. Linux proc
and cgroup counters have their declared kernel scope. macOS ps supplies RSS only.
Unsupported observations are unavailable, never zero. These helpers perform no
cache reset, peak substitution, automatic retry or calibration/admission.
"""
import json
import math
import os
from pathlib import Path
import subprocess
import sys
import time

WINDOW = 65536
STATUS_BYTES = {"VmRSS", "RssAnon", "RssFile", "RssShmem", "VmSwap"}
ROLLUP_BYTES = {"Rss", "Pss", "Anonymous", "Shared_Clean", "Shared_Dirty",
                "Private_Clean", "Private_Dirty", "Swap"}
IO_BYTES = {"rchar", "wchar", "read_bytes", "write_bytes", "cancelled_write_bytes"}
IO_CALLS = {"syscr", "syscw"}
CGROUP_BYTES = {"anon", "file", "shmem", "sock", "slab", "pagetables",
                "file_dirty", "file_writeback", "kernel", "kernel_stack"}


def unsigned(text):
    if not isinstance(text, str) or not text or not text.isascii() or not text.isdecimal():
        raise ValueError("noncanonical unsigned counter")
    return int(text)


def read_once(path):
    # FileIO issues one read: buffered reads can span several procfs walks.
    stream = Path(path).open("rb", buffering=0)
    try:
        data = stream.read(WINDOW + 1)
    except BaseException as original:
        try:
            stream.close()
        except BaseException as secondary:
            original.close_failure = secondary
        raise
    stream.close()
    if len(data) > WINDOW:
        raise ValueError("observation exceeds one bounded read window")
    return data.decode("ascii")


def unique_rows(text, separator):
    result = {}
    for line in text.splitlines():
        if not line:
            continue
        key, found, value = line.partition(separator)
        if not found or not key or key in result:
            raise ValueError("duplicate or malformed counter row")
        result[key] = value.strip()
    return result


def proc_status(text):
    rows = unique_rows(text, ":")
    result = {}
    for key in STATUS_BYTES:
        if key not in rows:
            raise ValueError("required process memory field unavailable: " + key)
        fields = rows[key].split()
        if len(fields) != 2 or fields[1] != "kB":
            raise ValueError("process memory unit")
        result[key] = unsigned(fields[0]) * 1024
    return result


def proc_io(text):
    rows = unique_rows(text, ":")
    if set(rows) != IO_BYTES | IO_CALLS:
        raise ValueError("process I/O field inventory")
    return {key: unsigned(value) for key, value in rows.items()}


def proc_rollup(text):
    lines = text.splitlines()
    if not lines or not lines[0].endswith("[rollup]"):
        raise ValueError("smaps rollup header")
    rows = unique_rows("\n".join(lines[1:]), ":")
    result = {}
    for key in ROLLUP_BYTES:
        if key not in rows:
            raise ValueError("required smaps rollup field unavailable: " + key)
        fields = rows[key].split()
        if len(fields) != 2 or fields[1] != "kB":
            raise ValueError("smaps rollup unit")
        result[key] = unsigned(fields[0]) * 1024
    return result


def proc_birth(text, pid):
    prefix, found, suffix = text.rpartition(")")
    fields = suffix.split()
    if not found or prefix.partition(" (")[0] != str(pid) or len(fields) < 20:
        raise ValueError("process stat identity")
    return unsigned(fields[19])


def cgroup_memory(text):
    rows = unique_rows(text, " ")
    # Preserve the declared byte fields separately: many are overlapping subsets.
    return {key: unsigned(rows[key]) for key in CGROUP_BYTES if key in rows}


def cgroup_io(text):
    result = {}
    for line in text.splitlines():
        fields = line.split()
        if not fields:
            continue
        device = fields[0]
        major, found, minor = device.partition(":")
        if not found or device in result:
            raise ValueError("cgroup I/O device identity")
        unsigned(major)
        unsigned(minor)
        row = {}
        for item in fields[1:]:
            key, found, value = item.partition("=")
            if not found or not key or key in row:
                raise ValueError("cgroup I/O counter identity")
            row[key] = unsigned(value)
        if not {"rbytes", "wbytes", "rios", "wios"} <= row.keys():
            raise ValueError("cgroup I/O required fields")
        result[device] = row
    return result


def clock_identity():
    clock = time.get_clock_info("monotonic")
    return {"api": "python.time.monotonic_ns", "platform": sys.platform,
            "implementation": clock.implementation,
            "resolution_ns": math.ceil(clock.resolution * 1_000_000_000),
            "monotonic": clock.monotonic, "adjustable": clock.adjustable,
            "cross_clock_calibration": "UNAVAILABLE"}


def linux_process(pid, proc_root=Path("/proc")):
    if type(pid) is not int or pid <= 0:
        raise ValueError("explicit positive process identity required")
    folder = Path(proc_root) / str(pid)
    opened = time.monotonic_ns()
    before = proc_birth(read_once(folder / "stat"), pid)
    memory = proc_status(read_once(folder / "status"))
    rollup = proc_rollup(read_once(folder / "smaps_rollup"))
    io = proc_io(read_once(folder / "io"))
    after = proc_birth(read_once(folder / "stat"), pid)
    closed = time.monotonic_ns()
    if before != after:
        raise ValueError("process incarnation changed during observation")
    return {"schema": "cluster-two-resource-observation-v1", "clock": clock_identity(),
            "read_open_ns": opened, "read_closed_ns": closed,
            "scope": "linux-process", "pid": pid, "birth_start_ticks": before,
            "memory_status_reported_bytes": memory, "smaps_rollup_bytes": rollup,
            "io_bytes": {k: io[k] for k in IO_BYTES},
            "io_calls": {k: io[k] for k in IO_CALLS},
            "limitations": ["memory fields overlap; no summed resident total",
                            "single bounded reads do not prove EOF or complete inventories",
                            "status counters retain kernel reporting precision; smaps walk cost is observed",
                            "process counters do not establish exclusive Workspace or device I/O",
                            "two identity reads exclude observed PID reuse, not process liveness after close",
                            "one observed sample supplies no continuous peak or phase coverage"]}


def macos_process(pid):
    if sys.platform != "darwin" or type(pid) is not int or pid <= 0:
        raise ValueError("explicit macOS process identity required")
    opened = time.monotonic_ns()
    # One subprocess, with an explicit stop. Refusal raises its original error.
    command = ["/bin/ps", "-p", str(pid), "-o", "pid=,lstart=,rss="]
    raw = subprocess.check_output(command, timeout=2, env={**os.environ, "LC_ALL": "C"})
    closed = time.monotonic_ns()
    fields = raw.decode("ascii").split()
    if len(fields) != 7 or fields[0] != str(pid):
        raise ValueError("macOS process identity/field inventory")
    return {"schema": "cluster-two-resource-observation-v1", "clock": clock_identity(),
            "read_open_ns": opened, "read_closed_ns": closed,
            "scope": "macos-process", "pid": pid, "birth_lstart": " ".join(fields[1:6]),
            "memory_bytes": {"rss": unsigned(fields[6]) * 1024},
            "io_bytes": None, "io_calls": None, "command": command,
            "limitations": ["RSS excludes separate processes and supplies no anon/file/swap decomposition",
                            "ps child/observer overhead requires calibration and inclusive accounting",
                            "physical I/O is UNAVAILABLE from this source",
                            "birth time has ps precision; no stronger incarnation proof",
                            "one observed sample supplies no continuous peak or phase coverage"]}


def linux_cgroup(folder):
    folder = Path(folder)
    if not folder.is_absolute():
        raise ValueError("explicit absolute owned cgroup path required")
    opened = time.monotonic_ns()
    before = folder.stat()
    current = unsigned(read_once(folder / "memory.current").strip())
    memory = cgroup_memory(read_once(folder / "memory.stat"))
    io = cgroup_io(read_once(folder / "io.stat"))
    after = folder.stat()
    closed = time.monotonic_ns()
    if (before.st_dev, before.st_ino) != (after.st_dev, after.st_ino):
        raise ValueError("cgroup incarnation changed during observation")
    return {"schema": "cluster-two-resource-observation-v1", "clock": clock_identity(),
            "read_open_ns": opened, "read_closed_ns": closed,
            "scope": "explicit-cgroup-v2-hierarchy", "path": str(folder),
            "device": before.st_dev, "inode": before.st_ino,
            "memory_current_bytes": current, "memory_stat_bytes": memory, "io": io,
            "memory_stat_unavailable_fields": sorted(CGROUP_BYTES - memory.keys()),
            "limitations": ["hierarchy includes descendants and observer when present",
                            "single bounded reads do not prove EOF or complete inventories",
                            "absent memory.stat fields are unavailable, never zero",
                            "memory.stat fields overlap and cannot be summed as current",
                            "io.stat is cgroup block accounting, not exclusive Workspace physical I/O",
                            "no lifetime memory.peak substitutes this sample or a phase peak",
                            "ownership/topology, clocks, precision and observer overhead require attestation"]}


def artifact_allocation(path):
    path = Path(path)
    if not path.is_absolute():
        raise ValueError("explicit absolute artifact path required")
    opened = time.monotonic_ns()
    state = path.stat()
    closed = time.monotonic_ns()
    return {"schema": "cluster-two-resource-observation-v1", "clock": clock_identity(),
            "read_open_ns": opened, "read_closed_ns": closed, "scope": "one-stat-artifact",
            "path": str(path), "device": state.st_dev, "inode": state.st_ino,
            "logical_bytes": state.st_size, "allocated_bytes": state.st_blocks * 512,
            "limitations": ["stat block allocation is not VFS/device I/O or phase residency",
                            "snapshot neither pins lifetime nor includes other artifact/WAL/journal paths",
                            "reflink/compression/shared-filesystem physical attribution remains unqualified"]}


class Stream:
    """Fresh synchronous JSONL output; one observation retained at a time.

    A failed read or write ends this original stream. The caller retains that
    exception and the acknowledged output prefix; no restart/truncation is allowed.
    """
    def __init__(self, path):
        self.output = Path(path).open("xb", buffering=0)
        self.failed = False
        self.failure = None
        self.close_failure = None
        self.close_attempted = False
        self.records = 0
        self.acknowledged_bytes = 0

    def append(self, observe):
        if self.failed or self.output.closed:
            raise ValueError("observation stream is terminal")
        try:
            value = observe()
            data = json.dumps(value, sort_keys=True, separators=(",", ":")).encode() + b"\n"
            if len(data) > WINDOW:
                raise ValueError("serialized observation exceeds bounded window")
            written = self.output.write(data)
            if type(written) is int and 0 <= written <= len(data):
                self.acknowledged_bytes += written
            if written != len(data):
                raise OSError("short observation write")
            self.output.flush()
            self.records += 1
            return value
        except BaseException as original:
            self.failed = True
            self.failure = original
            raise

    def close(self):
        if self.close_attempted:
            return
        self.close_attempted = True
        try:
            # Raw FileIO owns no buffered bytes to flush again after failure.
            self.output.close()
        except BaseException as original:
            self.close_failure = original
            self.failed = True
            if self.failure is None:
                self.failure = original
            raise
