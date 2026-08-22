#!/usr/bin/env python3
"""Fresh one-shot G3-v10 runner with complete source/build custody."""

import argparse
import hashlib
import json
import os
import platform
import re
import shutil
import signal
import stat as statmod
import subprocess
import sys
import tempfile
import time
from pathlib import Path, PurePosixPath

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[4]
TARGET = REPO / "target/phase4-g3-incremental-materialization-20260822-v10"
RESULTS = TARGET / "results-v10"
LOCK = REPO / "target/phase4-g3-incremental-materialization-20260822-v10.lock"
WORK = RESULTS / "work-v10"
BINARY = REPO / "target/release/phase4_create_edit_benchmark"
TIME = Path("/usr/bin/time")
DRY_RUN = HERE / "DRY-RUN-v10.json"
SOURCE_PATHS = (
    "Cargo.lock",
    "crates/layerfs-engine/Cargo.toml",
    "crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs",
    "crates/layerfs-engine/src/bin/phase4_g3_materialization.rs",
)
METHOD_NAMES = (
    "PROSPECTIVE-G3-INCREMENTAL-MATERIALIZATION-v10.md",
    "COUNTER-DICTIONARY-v10.md",
    "run_g3_v10.py",
    "analyze_g3_v10.py",
    "recompute_g3_v10.py",
    "finalize_g3_v10.py",
)
BUILD_COMMAND = ["cargo", "build", "--release", "-p", "layerfs-engine", "--bin", "phase4_create_edit_benchmark", "--offline"]
HEAD = "d79f0e0e2582d1bc491410224fec2b6cef7482e9"
BRANCH = "codex/empty-worktree"
TOKEN = "authorized-g3-v10-once"
GLOBAL_NS = 59_000_000_000
OP_NS = 5_000_000_000
SUM_NS = 20_000_000_000
STORAGE = 512 * 1024 * 1024
DIR_FLAGS = os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW | getattr(os, "O_CLOEXEC", 0)
DELETION_METHOD = "descriptor-relative-openat-fstatat-unlinkat-rmdir-no-follow-exact-inventory-v1"
SCHEDULE = (
    (1, "qualified-noop", 10 * 1024 * 1024, 5),
    (2, "qualified-one-byte", 100 * 1024 * 1024, 15),
    (3, "qualified-one-mib", 10 * 1024 * 1024, 5),
    (4, "invalid-authority", 1024 * 1024, 5),
    (5, "external-mutation", 1024 * 1024, 5),
    (6, "symlink-substitution", 1024 * 1024, 5),
    (7, "count-change", 1024 * 1024, 5),
    (8, "before-publication-fault", 1024 * 1024, 5),
    (9, "lost-ack", 1024 * 1024, 5),
)


def sha256(path):
    result = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            result.update(block)
    return result.hexdigest()


def canonical_hash(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(",", ":")).encode()).hexdigest()


def mode(path):
    return f"{path.stat().st_mode & 0o7777:04o}"


def git(*args):
    return subprocess.run(["git", *args], cwd=REPO, check=True, capture_output=True, text=True, timeout=5).stdout.strip()


def rows():
    return [{"sequence": sequence, "label": f"{sequence:02d}-{scenario}", "scenario": scenario, "size_bytes": size, "operation_ceiling_ns": OP_NS, "child_ceiling_seconds": ceiling} for sequence, scenario, size, ceiling in SCHEDULE]


def preflight():
    if Path.cwd().resolve() != REPO or HERE.parents[4] != REPO:
        raise RuntimeError("run from repository root; invalid v10 REPO derivation")
    if git("branch", "--show-current") != BRANCH or git("rev-parse", "HEAD") != HEAD:
        raise RuntimeError("repository custody drift")
    required = [REPO / name for name in SOURCE_PATHS] + [HERE / name for name in METHOD_NAMES] + [DRY_RUN, TIME]
    if any(not path.is_file() for path in required) or shutil.which("cargo") is None or not os.access(TIME, os.X_OK):
        raise RuntimeError("missing source, method, dry-run, cargo, or time operand")


def fresh():
    if TARGET.exists() or LOCK.exists():
        raise RuntimeError("G3-v10 result root or lock already exists")


def fsync_directory(path, markers=None, label="directory"):
    fd = os.open(path, DIR_FLAGS)
    try: os.fsync(fd)
    finally: os.close(fd)
    if markers is not None: markers.append(f"{label}:parent-fsync")


def durable_write_new(path, text, markers=None, label="file"):
    with path.open("x") as handle:
        handle.write(text); handle.flush(); os.fsync(handle.fileno())
    if markers is not None: markers.append(f"{label}:file-fsync")
    fsync_directory(path.parent, markers, label)


def durable_append_text(path, text, markers=None, label="append"):
    new = not path.exists()
    with path.open("a") as handle:
        handle.write(text); handle.flush(); os.fsync(handle.fileno())
    if markers is not None: markers.append(f"{label}:file-fsync")
    if new: fsync_directory(path.parent, markers, label)


def chronology(event, started, **fields):
    record = {"event": event, "monotonic_elapsed_ns": time.monotonic_ns() - started, "wall_time_ns": time.time_ns(), **fields}
    durable_append_text(RESULTS / "CHRONOLOGY-v10.jsonl", json.dumps(record, sort_keys=True, separators=(",", ":")) + "\n", label="chronology")


def remaining(started, requested):
    seconds = (GLOBAL_NS - (time.monotonic_ns() - started)) / 1_000_000_000
    if seconds <= 0:
        raise TimeoutError("G3-v10 global ceiling exhausted")
    return min(seconds, requested)


def child(command, label, kind, timeout, output_dir, environment, started):
    command = [str(part) for part in command]
    chronology("child-start", started, label=label, kind=kind, command=command, timeout_seconds=timeout)
    process = subprocess.Popen(command, cwd=REPO, env=environment, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, start_new_session=True)
    timed_out = False
    try:
        stdout, stderr = process.communicate(timeout=remaining(started, timeout))
    except subprocess.TimeoutExpired:
        timed_out = True
        os.killpg(process.pid, signal.SIGKILL)
        stdout, stderr = process.communicate()
    output_new = not output_dir.exists(); output_dir.mkdir(parents=True, exist_ok=True)
    if output_new: fsync_directory(output_dir.parent)
    stdout_path, stderr_path = output_dir / f"{label}.stdout", output_dir / f"{label}.stderr"
    durable_write_new(stdout_path, stdout, label=f"{label}:stdout")
    durable_write_new(stderr_path, stderr, label=f"{label}:stderr")
    chronology("child-timeout" if timed_out else "child-complete", started, label=label, kind=kind, command=command, timeout_seconds=timeout, exit_code=process.returncode)
    if timed_out:
        raise TimeoutError(f"child ceiling exceeded: {label}")
    if process.returncode:
        raise RuntimeError(f"child failed: {label}: exit {process.returncode}")
    return stdout, stderr, stdout_path, stderr_path


def one_object(stdout, label):
    lines = stdout.splitlines()
    if len(lines) != 1:
        raise RuntimeError(f"{label} did not emit exactly one JSON line")
    value = json.loads(lines[0])
    if type(value) is not dict:
        raise RuntimeError(f"{label} stdout is not one JSON object")
    return value


def time_values(stderr):
    elapsed = re.search(r"([0-9.]+)\s+real\s+([0-9.]+)\s+user\s+([0-9.]+)\s+sys", stderr)
    rss = re.search(r"(\d+)\s+maximum resident set size", stderr)
    if not elapsed or not rss:
        raise RuntimeError("incomplete /usr/bin/time -l CPU/RSS output")

    def integer(label):
        found = re.search(rf"(\d+)\s+{label}", stderr)
        return int(found.group(1)) if found else "Unavailable: not emitted by /usr/bin/time -l"

    return {"external_real_seconds": float(elapsed.group(1)), "external_user_seconds": float(elapsed.group(2)), "external_system_seconds": float(elapsed.group(3)), "maximum_resident_set_bytes": int(rss.group(1)), "peak_memory_footprint_bytes": integer("peak memory footprint"), "voluntary_context_switches": integer("voluntary context switches"), "involuntary_context_switches": integer("involuntary context switches"), "block_input_operations": integer("block input operations"), "block_output_operations": integer("block output operations")}


def freeze_sources():
    destination = RESULTS / "source-custody-v10"
    destination.mkdir()
    records = []
    for relative in SOURCE_PATHS:
        source = REPO / relative
        if mode(source) != "0644":
            raise RuntimeError(f"unexpected source mode: {relative}")
        copy = destination / relative
        copy.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, copy)
        copy.chmod(0o400)
        record = {"path": relative, "sha256": sha256(source), "size_bytes": source.stat().st_size, "source_mode": mode(source), "copy_path": str(copy.relative_to(RESULTS)), "copy_sha256": sha256(copy), "copy_size_bytes": copy.stat().st_size, "copy_mode": mode(copy)}
        if record["sha256"] != record["copy_sha256"] or record["size_bytes"] != record["copy_size_bytes"] or record["copy_mode"] != "0400":
            raise RuntimeError(f"source copy mismatch: {relative}")
        records.append(record)
    identity = [{key: row[key] for key in ("path", "sha256", "size_bytes", "source_mode")} for row in records]
    custody = {"schema": "phase4-g3-v10-source-custody-v1", "status": "PASS", "source_set_sha256": canonical_hash(identity), "sources": records}
    (RESULTS / "SOURCE-CUSTODY-v10.json").write_text(json.dumps(custody, indent=2, sort_keys=True) + "\n")
    verify_sources(custody)
    return custody


def verify_sources(custody, result_root=RESULTS):
    if [record.get("path") for record in custody.get("sources", [])] != list(SOURCE_PATHS):
        raise RuntimeError("source custody path set/order changed")
    custody_root = (result_root / "source-custody-v10").resolve()
    for record in custody["sources"]:
        expected_copy = f"source-custody-v10/{record['path']}"
        relative = record.get("copy_path")
        pure = PurePosixPath(relative) if isinstance(relative, str) else None
        if pure is None or pure.is_absolute() or any(part in ("", ".", "..") for part in pure.parts) or relative != expected_copy:
            raise RuntimeError(f"unsafe or inexact source copy path: {record['path']}")
        source, copy = REPO / record["path"], result_root.joinpath(*pure.parts)
        resolved_copy = copy.resolve()
        if custody_root not in resolved_copy.parents or resolved_copy == source.resolve() or copy.is_symlink() or not copy.is_file() or (copy.stat().st_dev, copy.stat().st_ino) == (source.stat().st_dev, source.stat().st_ino):
            raise RuntimeError(f"source copy containment/distinctness mismatch: {record['path']}")
        if record.get("copy_size_bytes") != record.get("size_bytes") or record.get("copy_sha256") != record.get("sha256") or record.get("copy_mode") != "0400" or mode(source) != record["source_mode"] or sha256(source) != record["sha256"] or source.stat().st_size != record["size_bytes"] or mode(copy) != "0400" or sha256(copy) != record["copy_sha256"] or copy.stat().st_size != record["copy_size_bytes"]:
            raise RuntimeError(f"source custody changed: {record['path']}")


def freeze_methods():
    destination = RESULTS / "methodology-v10"
    destination.mkdir()
    records = []
    for name in METHOD_NAMES:
        source, copy = HERE / name, destination / name
        record = {"path": name, "sha256": sha256(source), "size_bytes": source.stat().st_size}
        shutil.copyfile(source, copy)
        copy.chmod(0o400)
        if sha256(copy) != record["sha256"]:
            raise RuntimeError(f"method copy mismatch: {name}")
        records.append(record)
    shutil.copyfile(DRY_RUN, destination / DRY_RUN.name)
    (destination / DRY_RUN.name).chmod(0o400)
    custody = {"schema": "phase4-g3-v10-method-custody-v1", "status": "PASS", "methodology_set_sha256": canonical_hash(records), "methods": records, "dry_run_sha256": sha256(DRY_RUN)}
    (RESULTS / "METHODOLOGY-CUSTODY-v10.json").write_text(json.dumps(custody, indent=2, sort_keys=True) + "\n")
    return custody, destination


def verify_methods(custody):
    for record in custody["methods"]:
        if sha256(HERE / record["path"]) != record["sha256"] or sha256(RESULTS / "methodology-v10" / record["path"]) != record["sha256"]:
            raise RuntimeError(f"method custody changed: {record['path']}")


def freeze_binary(source_set, build_stdout, build_stderr):
    if not BINARY.is_file() or not os.access(BINARY, os.X_OK):
        raise RuntimeError("release build did not create candidate binary")
    operands = RESULTS / "operands-v10"
    operands.mkdir()
    copy = operands / BINARY.name
    shutil.copyfile(BINARY, copy)
    copy.chmod(0o500)
    record = {"schema": "phase4-g3-v10-operand-custody-v1", "status": "PASS", "source_set_sha256": source_set, "source_path": str(BINARY), "copy_path": str(copy.relative_to(RESULTS)), "sha256": sha256(copy), "size_bytes": copy.stat().st_size, "source_mode": mode(BINARY), "copy_mode": mode(copy), "build_command": BUILD_COMMAND, "build_invocations": 1, "build_stdout_sha256": sha256(build_stdout), "build_stderr_sha256": sha256(build_stderr)}
    if record["sha256"] != sha256(BINARY) or record["copy_mode"] != "0500":
        raise RuntimeError("binary snapshot mismatch")
    (RESULTS / "OPERAND-CUSTODY-v10.json").write_text(json.dumps(record, indent=2, sort_keys=True) + "\n")
    return record, copy


def environment():
    record = {"schema": "phase4-g3-v10-environment-v1", "cwd": str(REPO), "branch": BRANCH, "head": HEAD, "python": sys.version, "platform": platform.platform(), "uname": list(os.uname()), "selected_environment": {name: os.environ.get(name) for name in ("LANG", "LC_ALL", "PATH", "SHELL", "TZ")}, "unsupported_claims": {"physical_io": "Unavailable: no byte-level VFS or privileged syscall observation", "cache_warmth": "Unavailable: invocation order is not cache-state observation", "stable_media": "Unavailable: sync return does not expose device stable-media completion"}}
    path = RESULTS / "ENVIRONMENT-v10.json"
    path.write_text(json.dumps(record, indent=2, sort_keys=True) + "\n")
    return sha256(path)


def plan(binary, methods):
    result = [{"kind": "build", "label": "release-build", "timeout_seconds": 30, "command": BUILD_COMMAND}]
    result += [
        {"kind": "analyzer-self-check", "label": "primary-self-check", "timeout_seconds": 5, "command": [sys.executable, str(methods / "analyze_g3_v10.py"), "--self-check"]},
        {"kind": "analyzer-self-check", "label": "independent-self-check", "timeout_seconds": 5, "command": [sys.executable, str(methods / "recompute_g3_v10.py"), "--self-check"]},
    ]
    for row in rows():
        result.append({"kind": "measured-row", "label": row["label"], "timeout_seconds": row["child_ceiling_seconds"], "command": [str(TIME), "-l", str(binary), "--g3-row", str(WORK / row["label"]), str(row["size_bytes"]), row["scenario"]]})
    result += [
        {"kind": "analyzer", "label": "primary-analysis", "timeout_seconds": 5, "command": [sys.executable, str(methods / "analyze_g3_v10.py"), str(RESULTS)]},
        {"kind": "analyzer", "label": "independent-recomputation", "timeout_seconds": 5, "command": [sys.executable, str(methods / "recompute_g3_v10.py"), str(RESULTS)]},
    ]
    return result


def disk_usage(path):
    values = {"logical_bytes": 0, "apparent_bytes": 0, "allocated_bytes": 0, "files": 0, "directories": 0, "symlinks": 0}
    if not path.exists(): return values
    for item in (path, *path.rglob("*")):
        stat = item.lstat()
        if item.is_symlink(): values["symlinks"] += 1; values["apparent_bytes"] += stat.st_size; values["allocated_bytes"] += stat.st_blocks * 512
        elif item.is_dir(): values["directories"] += 1
        elif item.is_file(): values["files"] += 1; values["logical_bytes"] += stat.st_size; values["apparent_bytes"] += stat.st_size; values["allocated_bytes"] += stat.st_blocks * 512
    return values


def identity(stat):
    kind = "directory" if statmod.S_ISDIR(stat.st_mode) else "symlink" if statmod.S_ISLNK(stat.st_mode) else "regular" if statmod.S_ISREG(stat.st_mode) else "other"
    return {"kind": kind, "device": stat.st_dev, "inode": stat.st_ino, "mode": stat.st_mode & 0o7777, "nlink": stat.st_nlink, "size_bytes": stat.st_size, "mtime_ns": stat.st_mtime_ns, "ctime_ns": stat.st_ctime_ns, "allocated_bytes": stat.st_blocks * 512}


def same_identity(left, right):
    return all(left[key] == right[key] for key in ("kind", "device", "inode", "mode", "nlink", "size_bytes", "mtime_ns", "ctime_ns"))


def inventory(fd, prefix=""):
    entries = []
    for name in sorted(os.listdir(fd)):
        if not name or name in (".", "..") or "/" in name or "\0" in name:
            raise RuntimeError("unsafe anchored inventory name")
        relative = f"{prefix}/{name}" if prefix else name
        observed = identity(os.stat(name, dir_fd=fd, follow_symlinks=False))
        record = {"path": relative, **observed}
        entries.append(record)
        if observed["kind"] == "directory":
            child = os.open(name, DIR_FLAGS, dir_fd=fd)
            try:
                if not same_identity(identity(os.fstat(child)), observed):
                    raise RuntimeError("directory identity changed during inventory")
                entries.extend(inventory(child, relative))
            finally:
                os.close(child)
    return entries


def inventory_usage(entries):
    regular = [row for row in entries if row["kind"] == "regular"]
    symlinks = [row for row in entries if row["kind"] == "symlink"]
    return {"logical_bytes": sum(row["size_bytes"] for row in regular), "apparent_bytes": sum(row["size_bytes"] for row in (*regular, *symlinks)), "allocated_bytes": sum(row["allocated_bytes"] for row in (*regular, *symlinks)), "files": len(regular), "directories": sum(row["kind"] == "directory" for row in entries), "symlinks": len(symlinks)}


def durable_append(path, record):
    durable_append_text(path, json.dumps(record, sort_keys=True, separators=(",", ":")) + "\n", label="jsonl")


def expected_children(entries, prefix):
    depth = 0 if not prefix else prefix.count("/") + 1
    return [row for row in entries if row["path"].count("/") == depth and (not prefix or row["path"].startswith(prefix + "/"))]


def delete_inventory(fd, prefix, frozen, deleted):
    children = expected_children(frozen, prefix)
    if sorted(os.listdir(fd)) != sorted(row["path"].rsplit("/", 1)[-1] for row in children):
        raise RuntimeError("late addition/removal before anchored deletion")
    for row in children:
        name = row["path"].rsplit("/", 1)[-1]
        if not same_identity(identity(os.stat(name, dir_fd=fd, follow_symlinks=False)), row):
            raise RuntimeError("entry substitution before anchored deletion")
    for row in children:
        name = row["path"].rsplit("/", 1)[-1]
        if row["kind"] == "directory":
            child = os.open(name, DIR_FLAGS, dir_fd=fd)
            try:
                if not same_identity(identity(os.fstat(child)), row):
                    raise RuntimeError("directory substitution before recursion")
                delete_inventory(child, row["path"], frozen, deleted)
                current_child_identity = identity(os.fstat(child))
            finally:
                os.close(child)
            if not same_identity(identity(os.stat(name, dir_fd=fd, follow_symlinks=False)), current_child_identity):
                raise RuntimeError("directory substitution before rmdir")
            os.rmdir(name, dir_fd=fd)
        else:
            if not same_identity(identity(os.stat(name, dir_fd=fd, follow_symlinks=False)), row):
                raise RuntimeError("leaf substitution before unlink")
            os.unlink(name, dir_fd=fd)
        deleted.append(row)


def retire_row(specification, row, storage, work_fd):
    label = specification["label"]
    row_stat = identity(os.stat(label, dir_fd=work_fd, follow_symlinks=False))
    if row_stat["kind"] != "directory":
        raise RuntimeError("row root is not a directory")
    row_fd = os.open(label, DIR_FLAGS, dir_fd=work_fd)
    try:
        if not same_identity(identity(os.fstat(row_fd)), row_stat):
            raise RuntimeError("row root identity changed on open")
        frozen = sorted(inventory(row_fd), key=lambda entry: entry["path"])
        pre_row = inventory_usage(frozen)
        work_inventory = inventory(work_fd)
        pre_work = inventory_usage(work_inventory)
        if max(pre_row[name] for name in ("logical_bytes", "apparent_bytes", "allocated_bytes")) > STORAGE:
            raise RuntimeError("isolated row transient storage ceiling")
        prepare = {"schema": "phase4-g3-v10-row-cleanup-v1", "event": "PREPARE", "sequence": specification["sequence"], "label": label, "row_root": f"work-v10/{label}", "row_identity": row_stat, "inventory": frozen, "inventory_count": len(frozen), "inventory_sha256": canonical_hash(frozen), "pre_delete_row": pre_row, "pre_delete_work": pre_work, "deletion_method": DELETION_METHOD, "anchored_work_dirfd": True, "anchored_row_dirfd": True, "row_fd_retained_prepare_through_delete": True, "enumeration_followed_symlinks": False, "private_namespace_process_custody": True, "candidate_exactness": {name: row[name] for name in ("byte_exact", "mode_exact", "temp_residue_count", "seed_residue_count", "old_or_new", "output_digest", "expected_output_digest")}}
        durable_append(RESULTS / "ROW-CLEANUP-v10.jsonl", prepare)
        deleted = []
        if not same_identity(identity(os.fstat(row_fd)), row_stat) or sorted(inventory(row_fd), key=lambda entry: entry["path"]) != frozen:
            raise RuntimeError("row inventory changed after durable PREPARE")
        delete_inventory(row_fd, "", frozen, deleted)
        if os.listdir(row_fd):
            raise RuntimeError("row root not empty after exact delete set")
        deleted = sorted(deleted, key=lambda entry: entry["path"])
        if deleted != frozen or canonical_hash(deleted) != prepare["inventory_sha256"]:
            raise RuntimeError("actual delete set differs from frozen inventory")
        current_row_identity = identity(os.fstat(row_fd))
        if not same_identity(identity(os.stat(label, dir_fd=work_fd, follow_symlinks=False)), current_row_identity):
            raise RuntimeError("row root substituted before anchored rmdir")
        os.rmdir(label, dir_fd=work_fd)
        if label in os.listdir(work_fd):
            raise RuntimeError("row root survived anchored rmdir")
        os.fsync(work_fd)
        complete = {"schema": "phase4-g3-v10-row-cleanup-v1", "event": "COMPLETE", "sequence": specification["sequence"], "label": label, "row_root": prepare["row_root"], "prepare_sha256": canonical_hash(prepare), "inventory_count": prepare["inventory_count"], "inventory_sha256": prepare["inventory_sha256"], "deleted_count": len(deleted), "deleted_sha256": canonical_hash(deleted), "deletion_method": DELETION_METHOD, "row_root_absent": True, "post_delete_work": inventory_usage(inventory(work_fd))}
        durable_append(RESULTS / "ROW-CLEANUP-v10.jsonl", complete)
        storage["samples"].append({"sequence": specification["sequence"], "label": label, "pre_delete_row": pre_row, "pre_delete_work": pre_work})
        return prepare, complete
    finally:
        os.close(row_fd)


def finalize_cleanup(storage, results_fd, work_fd):
    records = [json.loads(line) for line in (RESULTS / "ROW-CLEANUP-v10.jsonl").read_text().splitlines() if line]
    expected = [(row["sequence"], row["label"]) for row in rows() for _ in (0, 1)]
    expected_events = [event for _ in rows() for event in ("PREPARE", "COMPLETE")]
    if [(row.get("sequence"), row.get("label")) for row in records] != expected or [row.get("event") for row in records] != expected_events or os.listdir(work_fd):
        raise RuntimeError("row cleanup record/order or empty WORK mismatch")
    prepares, completes = records[::2], records[1::2]
    os.rmdir("work-v10", dir_fd=results_fd)
    peaks = {name: max(record["pre_delete_row"][name] for record in prepares) for name in ("logical_bytes", "apparent_bytes", "allocated_bytes")}
    cumulative = {name: sum(record["pre_delete_row"][name] for record in prepares) for name in ("logical_bytes", "apparent_bytes", "allocated_bytes")}
    record = {"schema": "phase4-g3-v10-cleanup-v1", "status": "PASS" if not WORK.exists() and max(peaks.values()) <= STORAGE else "REVISE", "declared_root": "work-v10", "broad_deletion": False, "deletion_method": DELETION_METHOD, "prepare_records": len(prepares), "complete_records": len(completes), "row_cleanup_records": len(records), "row_cleanup_labels": [row["label"] for row in prepares], "row_cleanup_sha256": sha256(RESULTS / "ROW-CLEANUP-v10.jsonl"), "all_row_roots_absent": all(row["row_root_absent"] for row in completes), "work_root_absent": not WORK.exists(), "durable_prepare_complete": True, "storage_ceiling_bytes": STORAGE, "peak_equation": "max_individual_PREPARE_pre_delete_row_not_cumulative_sum", "peak_logical_bytes": peaks["logical_bytes"], "peak_apparent_bytes": peaks["apparent_bytes"], "peak_allocated_bytes": peaks["allocated_bytes"], "cumulative_logical_bytes": cumulative["logical_bytes"], "cumulative_apparent_bytes": cumulative["apparent_bytes"], "cumulative_allocated_bytes": cumulative["allocated_bytes"]}
    (RESULTS / "CLEANUP-v10.json").write_text(json.dumps(record, indent=2, sort_keys=True) + "\n")
    return record


def campaign():
    fresh()
    if os.environ.get("G3_V10_EXECUTE") != TOKEN:
        raise RuntimeError("set exact G3_V10_EXECUTE token")
    fd = os.open(LOCK, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o400)
    os.write(fd, f"pid={os.getpid()}\n".encode()); os.close(fd)
    started = time.monotonic_ns()
    source_custody = None
    results_fd = work_fd = None
    try:
        TARGET.mkdir(); fsync_directory(TARGET.parent); RESULTS.mkdir(); fsync_directory(TARGET); fsync_directory(RESULTS)
        chronology("campaign-start", started, global_ceiling_ns=GLOBAL_NS)
        frozen_dry_run = json.loads(DRY_RUN.read_text())
        if frozen_dry_run.get("status") != "PASS" or frozen_dry_run.get("actual_rows") != 0 or frozen_dry_run.get("build_children_invoked") != 0 or frozen_dry_run.get("benchmark_children_invoked") != 0 or frozen_dry_run.get("analyzer_children_invoked") != 0:
            raise RuntimeError("invalid nonzero or non-PASS frozen dry run")
        source_custody = freeze_sources()
        method_custody, methods = freeze_methods()
        if source_custody["source_set_sha256"] != frozen_dry_run.get("source_set_sha256") or method_custody["methodology_set_sha256"] != frozen_dry_run.get("methodology_set_sha256"):
            raise RuntimeError("source or method set differs from frozen zero-row dry run")
        env_hash = environment()
        env = os.environ.copy()
        env.update({"LANG": "C", "LC_ALL": "C", "TZ": "UTC", "RUST_BACKTRACE": "0", "G3_SOURCE_SET_SHA256": source_custody["source_set_sha256"], "G3_METHODOLOGY_SET_SHA256": method_custody["methodology_set_sha256"]})
        planned_binary = RESULTS / "operands-v10" / BINARY.name
        commands = plan(planned_binary, methods)
        (RESULTS / "COMMANDS-v10.json").write_text(json.dumps(commands, indent=2, sort_keys=True) + "\n")
        _, _, build_stdout, build_stderr = child(commands[0]["command"], commands[0]["label"], commands[0]["kind"], 30, RESULTS / "build-v10", env, started)
        verify_sources(source_custody)
        binary_custody, binary = freeze_binary(source_custody["source_set_sha256"], build_stdout, build_stderr)
        env["G3_EXECUTABLE_SHA256"] = binary_custody["sha256"]
        for command in commands[1:3]:
            stdout, _, _, _ = child(command["command"], command["label"], command["kind"], command["timeout_seconds"], RESULTS / "self-check-v10", env, started)
            result = one_object(stdout, command["label"])
            if result.get("status") != "PASS" or result.get("mutations_rejected", 0) < 10: raise RuntimeError("analyzer self-check failed")
        results_fd = os.open(RESULTS, DIR_FLAGS); os.mkdir("work-v10", dir_fd=results_fd); work_fd = os.open("work-v10", DIR_FLAGS, dir_fd=results_fd)
        row_dir = RESULTS / "rows-v10"; row_dir.mkdir(); fsync_directory(RESULTS); raw = row_dir / "G3-V10-RAW.jsonl"; durable_write_new(raw, "", label="raw-create"); durable_write_new(RESULTS / "ROW-CLEANUP-v10.jsonl", "", label="cleanup-log-create")
        storage = {"schema": "phase4-g3-v10-runner-storage-v1", "ceiling_bytes": STORAGE, "samples": []}
        operation_sum = 0
        for specification, command in zip(rows(), commands[3:12]):
            if os.listdir(work_fd): raise RuntimeError("prior row not retired or row root is not fresh")
            stdout, stderr, _, _ = child(command["command"], command["label"], command["kind"], command["timeout_seconds"], row_dir, env, started)
            row = one_object(stdout, command["label"])
            if row.get("scenario") != specification["scenario"] or row.get("size_bytes") != specification["size_bytes"] or type(row.get("operation_total_ns")) is not int or row["operation_total_ns"] >= OP_NS: raise RuntimeError(f"row identity/operation ceiling: {command['label']}")
            operation_sum += row["operation_total_ns"]
            row.update({"sequence": specification["sequence"], "label": specification["label"], "command": command["command"], "child_timeout_seconds": command["timeout_seconds"], "child_exit_code": 0, "executable_sha256": binary_custody["sha256"], "source_set_sha256": source_custody["source_set_sha256"], "methodology_set_sha256": method_custody["methodology_set_sha256"], "environment_sha256": env_hash, **time_values(stderr)})
            durable_append(raw, row)
            retire_row(specification, row, storage, work_fd)
            if os.listdir(work_fd): raise RuntimeError("row root survived retirement before next row")
        if operation_sum >= SUM_NS: raise RuntimeError("summed operation ceiling")
        (RESULTS / "STORAGE-v10.json").write_text(json.dumps(storage, indent=2, sort_keys=True) + "\n")
        if finalize_cleanup(storage, results_fd, work_fd)["status"] != "PASS": raise RuntimeError("cleanup failed")
        os.close(work_fd); work_fd = None
        reports = []
        for command, filename in zip(commands[12:], ("G3-PRIMARY-ANALYSIS-v10.json", "G3-INDEPENDENT-RECOMPUTATION-v10.json")):
            stdout, _, _, _ = child(command["command"], command["label"], command["kind"], command["timeout_seconds"], RESULTS / "analysis-children-v10", env, started)
            report = one_object(stdout, command["label"]); (RESULTS / filename).write_text(json.dumps(report, sort_keys=True, separators=(",", ":")) + "\n"); reports.append(report)
        if any(report.get("status") != "PASS" for report in reports) or reports[0].get("normalized_ledger") != reports[1].get("normalized_ledger") or reports[0].get("normalized_ledger_sha256") != reports[1].get("normalized_ledger_sha256"): raise RuntimeError("analysis or exact agreement failed")
        verify_sources(source_custody); verify_methods(method_custody)
        if sha256(binary) != binary_custody["sha256"]: raise RuntimeError("binary snapshot changed")
        elapsed = time.monotonic_ns() - started
        if elapsed >= GLOBAL_NS: raise RuntimeError("global ceiling")
        record = {"schema": "phase4-g3-v10-campaign-v1", "status": "PASS", "disposition": "G3_V10_CAMPAIGN_PASS_STATIC_CLOSURE_REQUIRED", "rows": 9, "rows_rerun": 0, "build_command": BUILD_COMMAND, "build_invocations": 1, "build_ceiling_seconds": 30, "operation_total_ns": operation_sum, "operation_sum_ceiling_ns": SUM_NS, "per_operation_ceiling_ns": OP_NS, "global_elapsed_ns": elapsed, "global_ceiling_ns": GLOBAL_NS, "source_set_sha256": source_custody["source_set_sha256"], "executable_sha256": binary_custody["sha256"], "methodology_set_sha256": method_custody["methodology_set_sha256"], "raw_sha256": sha256(raw), "row_cleanup_sha256": sha256(RESULTS / "ROW-CLEANUP-v10.jsonl"), "primary_sha256": sha256(RESULTS / "G3-PRIMARY-ANALYSIS-v10.json"), "independent_sha256": sha256(RESULTS / "G3-INDEPENDENT-RECOMPUTATION-v10.json"), "normalized_ledger_sha256": reports[0]["normalized_ledger_sha256"], "cleanup_sha256": sha256(RESULTS / "CLEANUP-v10.json"), "source_custody_sha256": sha256(RESULTS / "SOURCE-CUSTODY-v10.json"), "binary_custody_sha256": sha256(RESULTS / "OPERAND-CUSTODY-v10.json"), "static_closure_required": True, "finalizer_required": True, "lock_removed_after_campaign": True}
        (RESULTS / "CAMPAIGN-v10.json").write_text(json.dumps(record, indent=2, sort_keys=True) + "\n")
        chronology("campaign-complete", started, status="PASS", rows=9)
        print(json.dumps(record, sort_keys=True, separators=(",", ":")))
    except Exception as error:
        if RESULTS.is_dir():
            failure = {"schema": "phase4-g3-v10-failure-v1", "status": "REVISE", "reason": f"{type(error).__name__}: {error}", "source_set_sha256": source_custody.get("source_set_sha256") if source_custody else None, "global_elapsed_ns": time.monotonic_ns() - started, "result_root_preserved": True}
            durable_write_new(RESULTS / "FAILURE-v10.json", json.dumps(failure, indent=2, sort_keys=True) + "\n", label="failure")
        raise
    finally:
        if work_fd is not None: os.close(work_fd)
        if results_fd is not None: os.close(results_fd)
        if LOCK.exists(): LOCK.unlink()


def dry_record():
    source_rows = [{"path": relative, "sha256": sha256(REPO / relative), "size_bytes": (REPO / relative).stat().st_size, "source_mode": mode(REPO / relative)} for relative in SOURCE_PATHS]
    method_rows = [{"path": name, "sha256": sha256(HERE / name), "size_bytes": (HERE / name).stat().st_size} for name in METHOD_NAMES]
    return {"schema": "phase4-g3-v10-dry-run-v1", "status": "PASS", "actual_rows": 0, "build_children_invoked": 0, "benchmark_children_invoked": 0, "analyzer_children_invoked": 0, "result_root_created": False, "lock_created": False, "result_root_absent": not TARGET.exists(), "lock_absent": not LOCK.exists(), "source_paths": list(SOURCE_PATHS), "source_at_dry_run": source_rows, "source_set_sha256": canonical_hash(source_rows), "source_hashes_frozen_before_build": True, "source_set_sha256_frozen_before_build": True, "build_plan": {"command": BUILD_COMMAND, "planned_invocations": 1, "child_ceiling_seconds": 30, "inside_global_timer": True, "before_binary_freeze": True}, "schedule": rows(), "planned_measured_rows": 9, "planned_row_reruns": 0, "per_operation_ceiling_ns": OP_NS, "operation_sum_ceiling_ns": SUM_NS, "global_ceiling_ns": GLOBAL_NS, "transient_storage_ceiling_bytes": STORAGE, "declared_cleanup_root": "results-v10/work-v10", "broad_deletion": False, "row_evidence_durability": {"stdout_stderr": "flush_file-fsync_parent-fsync", "raw_file_precreated_file_and_parent_fsynced": True, "raw_entry": "append_flush_file-fsync", "chronology": "append_flush_file-fsync_parent-fsync-on-create", "failure": "exclusive-create_flush_file-fsync_parent-fsync", "all_before_prepare": True}, "row_cleanup_plan": {"artifact": "results-v10/ROW-CLEANUP-v10.jsonl", "records": 18, "events_per_row": ["PREPARE", "COMPLETE"], "durable_append_flush_fsync": True, "parent_directory_fsynced_before_first_prepare": True, "retire_exact_row_before_next": True, "row_fd_retained_prepare_through_delete": True, "enumeration_followed_symlinks": False, "deletion_method": DELETION_METHOD, "peak_equation": "max_individual_PREPARE_pre_delete_row_not_cumulative_sum"}, "method_names": list(METHOD_NAMES), "methodology_at_dry_run": method_rows, "methodology_set_sha256": canonical_hash(method_rows), "execute_flag_required": "--execute", "execute_environment_required": {"G3_V10_EXECUTE": TOKEN}, "static_closure_and_finalizer_after_campaign": True}


def self_check():
    assert HERE.parents[4] == REPO
    schedule = rows(); assert len(schedule) == 9 and [row["sequence"] for row in schedule] == list(range(1, 10)) and len(set(row["label"] for row in schedule)) == 9
    assert schedule[1]["child_ceiling_seconds"] == 15 and all(row["child_ceiling_seconds"] == 5 for row in schedule[:1] + schedule[2:])
    assert BUILD_COMMAND == ["cargo", "build", "--release", "-p", "layerfs-engine", "--bin", "phase4_create_edit_benchmark", "--offline"]
    parsed = time_values("0.1 real 0.2 user 0.3 sys\n123 maximum resident set size\n")
    assert parsed["maximum_resident_set_bytes"] == 123
    dry = dry_record(); assert dry["actual_rows"] == dry["build_children_invoked"] == dry["benchmark_children_invoked"] == dry["analyzer_children_invoked"] == 0
    assert "finalize_g3_v10.py" in METHOD_NAMES
    forbidden = ("G2-" + "v10", "G2-" + "V10", "decomposition-20260822-" + "v10")
    forbidden_hits = {name: token for name in METHOD_NAMES for token in forbidden if token in (HERE / name).read_text()}
    assert not forbidden_hits
    with tempfile.TemporaryDirectory() as directory:
        result_root = Path(directory) / "results-v10"; records = []
        for relative in SOURCE_PATHS:
            source, copy = REPO / relative, result_root / "source-custody-v10" / relative
            copy.parent.mkdir(parents=True, exist_ok=True); shutil.copyfile(source, copy); copy.chmod(0o400)
            records.append({"path": relative, "sha256": sha256(source), "size_bytes": source.stat().st_size, "source_mode": mode(source), "copy_path": f"source-custody-v10/{relative}", "copy_sha256": sha256(copy), "copy_size_bytes": copy.stat().st_size, "copy_mode": mode(copy)})
        custody = {"sources": records}; verify_sources(custody, result_root)
        mutations = []
        for field, value in (("copy_path", None), ("copy_path", "source-custody-v10/../../outside"), ("copy_size_bytes", records[0]["copy_size_bytes"] + 1)):
            changed = json.loads(json.dumps(custody))
            if value is None: changed["sources"][0].pop(field)
            else: changed["sources"][0][field] = value
            mutations.append(changed)
        for changed in mutations:
            try: verify_sources(changed, result_root)
            except RuntimeError: continue
            raise AssertionError("source-copy mutation accepted")
    with tempfile.TemporaryDirectory() as directory:
        work = Path(directory) / "work"; row = work / "row"; row.mkdir(parents=True); (row / "expected").write_bytes(b"x")
        work_fd = os.open(work, DIR_FLAGS); row_fd = os.open("row", DIR_FLAGS, dir_fd=work_fd)
        try:
            frozen = sorted(inventory(row_fd), key=lambda entry: entry["path"]); (row / "late").write_bytes(b"late")
            try: delete_inventory(row_fd, "", frozen, [])
            except RuntimeError: pass
            else: raise AssertionError("late addition accepted")
            assert (row / "late").is_file() and (row / "expected").is_file()
        finally:
            os.close(row_fd); os.close(work_fd)
        safe = work / "safe"; safe.mkdir(); (safe / "file").write_bytes(b"x"); os.symlink("file", safe / "link")
        safe_fd = os.open(safe, DIR_FLAGS)
        try:
            frozen = sorted(inventory(safe_fd), key=lambda entry: entry["path"]); deleted = []; delete_inventory(safe_fd, "", frozen, deleted); assert sorted(deleted, key=lambda entry: entry["path"]) == frozen and not os.listdir(safe_fd)
        finally: os.close(safe_fd)
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory); markers = []
        durable_write_new(root / "row.stdout", "out", markers, "stdout")
        durable_write_new(root / "row.stderr", "err", markers, "stderr")
        durable_write_new(root / "raw.jsonl", "", markers, "raw-create")
        durable_append_text(root / "raw.jsonl", "{}\n", markers, "raw-entry")
        durable_append_text(root / "chronology.jsonl", "{}\n", markers, "chronology")
        durable_write_new(root / "FAILURE.json", "{}\n", markers, "failure")
        assert markers == ["stdout:file-fsync", "stdout:parent-fsync", "stderr:file-fsync", "stderr:parent-fsync", "raw-create:file-fsync", "raw-create:parent-fsync", "raw-entry:file-fsync", "chronology:file-fsync", "chronology:parent-fsync", "failure:file-fsync", "failure:parent-fsync"]
    print(json.dumps({"status": "PASS", "schedule_rows": 9, "forbidden_version_matched_g2_hits": forbidden_hits, "source_copy_mutations_rejected": 3, "cleanup_late_addition_rejected": True, "cleanup_symlink_unlinked_not_followed": True, "durable_evidence_order_markers": markers, "build_children_invoked": 0, "measured_children_invoked": 0, "repo_derivation": str(REPO)}, sort_keys=True))


def main():
    parser = argparse.ArgumentParser(); parser.add_argument("--execute", action="store_true"); parser.add_argument("--self-check", action="store_true"); arguments = parser.parse_args()
    if arguments.self_check: self_check()
    elif arguments.execute: preflight(); campaign()
    else: fresh(); print(json.dumps(dry_record(), indent=2, sort_keys=True))


if __name__ == "__main__": main()
