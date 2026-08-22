#!/usr/bin/env python3
"""One-shot G3-v1 runner. Measurement requires an explicit execution token."""

import argparse
import hashlib
import json
import os
import platform
import re
import shutil
import signal
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[4]
TARGET = REPO / "target/phase4-g3-incremental-materialization-20260822-v1"
RESULTS = TARGET / "results-v1"
LOCK = REPO / "target/phase4-g3-incremental-materialization-20260822-v1.lock"
WORK = RESULTS / "work-v1"
SOURCE = REPO / "crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs"
BINARY = REPO / "target/release/phase4_create_edit_benchmark"
TIME = Path("/usr/bin/time")
DRY_RUN = HERE / "DRY-RUN-v1.json"
METHODS = (
    HERE / "PROSPECTIVE-G3-INCREMENTAL-MATERIALIZATION-v1.md",
    HERE / "COUNTER-DICTIONARY-v1.md",
    HERE / "run_g3_v1.py",
    HERE / "analyze_g3_v1.py",
    HERE / "recompute_g3_v1.py",
)
HEAD = "d79f0e0e2582d1bc491410224fec2b6cef7482e9"
BRANCH = "codex/empty-worktree"
EXECUTION_TOKEN = "authorized-g3-v1-once"
GLOBAL_LIMIT_NS = 59_000_000_000
OPERATION_LIMIT_NS = 5_000_000_000
OPERATION_SUM_LIMIT_NS = 20_000_000_000
STORAGE_LIMIT = 512 * 1024 * 1024
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
    value = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            value.update(block)
    return value.hexdigest()


def canonical_sha256(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(",", ":")).encode()).hexdigest()


def mode(path):
    return f"{path.stat().st_mode & 0o7777:04o}"


def git_value(*args):
    return subprocess.run(["git", *args], cwd=REPO, check=True, capture_output=True, text=True, timeout=5).stdout.strip()


def schedule_records():
    return [
        {
            "sequence": sequence,
            "label": f"{sequence:02d}-{scenario}",
            "scenario": scenario,
            "size_bytes": size,
            "operation_ceiling_ns": OPERATION_LIMIT_NS,
            "child_ceiling_seconds": child_ceiling,
        }
        for sequence, scenario, size, child_ceiling in SCHEDULE
    ]


def ensure_preflight():
    if Path.cwd().resolve() != REPO:
        raise RuntimeError("run from the repository root")
    if git_value("branch", "--show-current") != BRANCH or git_value("rev-parse", "HEAD") != HEAD:
        raise RuntimeError("repository custody drift")
    for path in (*METHODS, DRY_RUN, SOURCE, BINARY, TIME):
        if not path.is_file():
            raise RuntimeError(f"missing operand: {path}")
    if not os.access(BINARY, os.X_OK) or not os.access(TIME, os.X_OK):
        raise RuntimeError("candidate or /usr/bin/time is not executable")


def ensure_fresh():
    if TARGET.exists() or LOCK.exists():
        raise RuntimeError("G3-v1 result root or lock already exists")


def chronology(event, started_ns, **fields):
    record = {
        "event": event,
        "monotonic_elapsed_ns": time.monotonic_ns() - started_ns,
        "wall_time_ns": time.time_ns(),
        **fields,
    }
    with (RESULTS / "CHRONOLOGY-v1.jsonl").open("a") as handle:
        handle.write(json.dumps(record, separators=(",", ":"), sort_keys=True) + "\n")


def remaining_seconds(started_ns, requested):
    remaining = (GLOBAL_LIMIT_NS - (time.monotonic_ns() - started_ns)) / 1_000_000_000
    if remaining <= 0:
        raise TimeoutError("G3-v1 global 59-second ceiling exhausted")
    return min(requested, remaining)


def run_child(command, label, output_dir, started_ns, timeout, environment, kind):
    command = [str(item) for item in command]
    chronology("child-start", started_ns, kind=kind, label=label, command=command, timeout_seconds=timeout)
    process = subprocess.Popen(
        command,
        cwd=REPO,
        env=environment,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        start_new_session=True,
    )
    timed_out = False
    try:
        stdout, stderr = process.communicate(timeout=remaining_seconds(started_ns, timeout))
    except subprocess.TimeoutExpired:
        timed_out = True
        os.killpg(process.pid, signal.SIGKILL)
        stdout, stderr = process.communicate()
    output_dir.mkdir(parents=True, exist_ok=True)
    (output_dir / f"{label}.stdout").write_text(stdout)
    (output_dir / f"{label}.stderr").write_text(stderr)
    chronology(
        "child-complete" if not timed_out else "child-timeout",
        started_ns,
        kind=kind,
        label=label,
        command=command,
        exit_code=process.returncode,
        timeout_seconds=timeout,
    )
    if timed_out:
        raise TimeoutError(f"child ceiling exceeded: {label}")
    if process.returncode != 0:
        raise RuntimeError(f"child failed: {label}: exit {process.returncode}")
    return stdout, stderr


def parse_time(stderr):
    elapsed = re.search(r"([0-9.]+)\s+real\s+([0-9.]+)\s+user\s+([0-9.]+)\s+sys", stderr)
    rss = re.search(r"(\d+)\s+maximum resident set size", stderr)
    if not elapsed or not rss:
        raise RuntimeError("incomplete /usr/bin/time -l CPU/RSS output")

    def integer(pattern):
        match = re.search(rf"(\d+)\s+{pattern}", stderr)
        return int(match.group(1)) if match else "Unavailable: not emitted by /usr/bin/time -l"

    return {
        "external_real_seconds": float(elapsed.group(1)),
        "external_user_seconds": float(elapsed.group(2)),
        "external_system_seconds": float(elapsed.group(3)),
        "maximum_resident_set_bytes": int(rss.group(1)),
        "peak_memory_footprint_bytes": integer("peak memory footprint"),
        "voluntary_context_switches": integer("voluntary context switches"),
        "involuntary_context_switches": integer("involuntary context switches"),
        "block_input_operations": integer("block input operations"),
        "block_output_operations": integer("block output operations"),
    }


def one_json_object(stdout, label):
    lines = stdout.splitlines()
    if len(lines) != 1:
        raise RuntimeError(f"{label} did not emit exactly one compact JSON line")
    if any(character.isspace() for character in lines[0][:1]) or "\n" in lines[0]:
        raise RuntimeError(f"{label} stdout is not compact JSON")
    value = json.loads(lines[0])
    if type(value) is not dict:
        raise RuntimeError(f"{label} stdout is not one JSON object")
    return value


def freeze_operands():
    source_hash = sha256(SOURCE)
    binary_hash = sha256(BINARY)
    method_rows = [
        {"path": path.name, "sha256": sha256(path), "size_bytes": path.stat().st_size}
        for path in METHODS
    ]
    method_set_hash = canonical_sha256(method_rows)
    operands = RESULTS / "operands-v1"
    methods = RESULTS / "methodology-v1"
    custody_sources = RESULTS / "custody-v1"
    operands.mkdir()
    methods.mkdir()
    custody_sources.mkdir()
    binary_copy = operands / BINARY.name
    source_copy = custody_sources / "phase4_create_edit_benchmark-source-v1.rs"
    shutil.copyfile(BINARY, binary_copy)
    binary_copy.chmod(0o500)
    shutil.copyfile(SOURCE, source_copy)
    source_copy.chmod(0o400)
    for path in METHODS:
        copy = methods / path.name
        shutil.copyfile(path, copy)
        copy.chmod(0o400)
    shutil.copyfile(DRY_RUN, methods / DRY_RUN.name)
    (methods / DRY_RUN.name).chmod(0o400)
    if sha256(binary_copy) != binary_hash or sha256(source_copy) != source_hash:
        raise RuntimeError("operand snapshot mismatch")
    for row in method_rows:
        if sha256(methods / row["path"]) != row["sha256"]:
            raise RuntimeError(f"method snapshot mismatch: {row['path']}")
    record = {
        "schema": "phase4-g3-v1-custody-v1",
        "source": {"path": str(SOURCE), "snapshot": str(source_copy.relative_to(RESULTS)), "sha256": source_hash, "size_bytes": SOURCE.stat().st_size, "snapshot_mode": mode(source_copy)},
        "executable": {"path": str(BINARY), "snapshot": str(binary_copy.relative_to(RESULTS)), "sha256": binary_hash, "size_bytes": BINARY.stat().st_size, "snapshot_mode": mode(binary_copy)},
        "methodology": method_rows,
        "methodology_set_sha256": method_set_hash,
        "dry_run_sha256": sha256(DRY_RUN),
        "time": {"path": str(TIME), "sha256": sha256(TIME)},
        "branch": BRANCH,
        "head": HEAD,
    }
    (RESULTS / "CUSTODY-v1.json").write_text(json.dumps(record, indent=2, sort_keys=True) + "\n")
    return record, binary_copy, methods


def environment_record():
    selected = {name: os.environ.get(name) for name in ("LANG", "LC_ALL", "PATH", "SHELL", "TZ")}
    record = {
        "schema": "phase4-g3-v1-environment-v1",
        "cwd": str(REPO),
        "branch": BRANCH,
        "head": HEAD,
        "python": sys.version,
        "platform": platform.platform(),
        "uname": list(os.uname()),
        "selected_environment": selected,
        "unsupported_claims": {
            "physical_io": "Unavailable: no byte-level VFS or privileged syscall observation",
            "cache_warmth": "Unavailable: logical invocation order is not an OS cache-state observation",
            "stable_media": "Unavailable: sync dispatch/return does not expose device stable-media completion",
        },
    }
    path = RESULTS / "ENVIRONMENT-v1.json"
    path.write_text(json.dumps(record, indent=2, sort_keys=True) + "\n")
    return sha256(path)


def command_plan(binary_copy, methods):
    children = [
        {"kind": "analyzer-self-check", "label": "primary-self-check", "timeout_seconds": 5, "command": [sys.executable, str(methods / "analyze_g3_v1.py"), "--self-check"]},
        {"kind": "analyzer-self-check", "label": "independent-self-check", "timeout_seconds": 5, "command": [sys.executable, str(methods / "recompute_g3_v1.py"), "--self-check"]},
    ]
    for row in schedule_records():
        row_root = WORK / row["label"]
        children.append({
            "kind": "measured-row",
            "label": row["label"],
            "timeout_seconds": row["child_ceiling_seconds"],
            "command": [str(TIME), "-l", str(binary_copy), "--g3-row", str(row_root), str(row["size_bytes"]), row["scenario"]],
        })
    children.extend((
        {"kind": "analyzer", "label": "primary-analysis", "timeout_seconds": 5, "command": [sys.executable, str(methods / "analyze_g3_v1.py"), str(RESULTS)]},
        {"kind": "analyzer", "label": "independent-recomputation", "timeout_seconds": 5, "command": [sys.executable, str(methods / "recompute_g3_v1.py"), str(RESULTS)]},
    ))
    return children


def usage(path):
    logical = apparent = allocated = files = directories = symlinks = 0
    if not path.exists():
        return {"logical_bytes": 0, "apparent_bytes": 0, "allocated_bytes": 0, "files": 0, "directories": 0, "symlinks": 0}
    for item in (path, *path.rglob("*")):
        stat = item.lstat()
        if item.is_symlink():
            symlinks += 1
            apparent += stat.st_size
            allocated += stat.st_blocks * 512
        elif item.is_dir():
            directories += 1
        elif item.is_file():
            files += 1
            logical += stat.st_size
            apparent += stat.st_size
            allocated += stat.st_blocks * 512
    return {"logical_bytes": logical, "apparent_bytes": apparent, "allocated_bytes": allocated, "files": files, "directories": directories, "symlinks": symlinks}


def remove_exact_work(storage):
    if WORK != RESULTS / "work-v1" or WORK.parent.resolve() != RESULTS.resolve() or not WORK.is_dir() or WORK.is_symlink():
        raise RuntimeError("unsafe or missing exact G3-v1 work root")
    relative_paths = sorted(str(path.relative_to(WORK)) for path in WORK.rglob("*"))
    for current, directory_names, file_names in os.walk(WORK, topdown=False, followlinks=False):
        current_path = Path(current)
        for name in file_names:
            (current_path / name).unlink()
        for name in directory_names:
            child = current_path / name
            if child.is_symlink():
                child.unlink()
            else:
                child.rmdir()
    WORK.rmdir()
    peak = {
        dimension: max((sample[dimension] for sample in storage["samples"]), default=0)
        for dimension in ("logical_bytes", "apparent_bytes", "allocated_bytes")
    }
    cleanup = {
        "schema": "phase4-g3-v1-cleanup-v1",
        "status": "PASS" if not WORK.exists() and max(peak.values()) <= STORAGE_LIMIT else "REVISE",
        "declared_root": "work-v1",
        "broad_deletion": False,
        "deletion_method": "enumerated-no-follow-unlink-and-rmdir",
        "deleted_entries": len(relative_paths),
        "deleted_path_set_sha256": canonical_sha256(relative_paths),
        "work_root_absent": not WORK.exists(),
        "storage_ceiling_bytes": STORAGE_LIMIT,
        "peak_logical_bytes": peak["logical_bytes"],
        "peak_apparent_bytes": peak["apparent_bytes"],
        "peak_allocated_bytes": peak["allocated_bytes"],
    }
    (RESULTS / "CLEANUP-v1.json").write_text(json.dumps(cleanup, indent=2, sort_keys=True) + "\n")
    return cleanup


def run_campaign():
    ensure_fresh()
    if os.environ.get("G3_V1_EXECUTE") != EXECUTION_TOKEN:
        raise RuntimeError("set exact G3_V1_EXECUTE token to execute the one-shot campaign")
    lock_fd = os.open(LOCK, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o400)
    os.write(lock_fd, f"pid={os.getpid()}\n".encode())
    os.close(lock_fd)
    started_ns = time.monotonic_ns()
    try:
        TARGET.mkdir()
        RESULTS.mkdir()
        chronology("campaign-start", started_ns, global_ceiling_ns=GLOBAL_LIMIT_NS)
        custody, binary_copy, methods = freeze_operands()
        environment_hash = environment_record()
        plan = command_plan(binary_copy, methods)
        (RESULTS / "COMMANDS-v1.json").write_text(json.dumps(plan, indent=2, sort_keys=True) + "\n")
        environment = os.environ.copy()
        environment.update({
            "LANG": "C", "LC_ALL": "C", "TZ": "UTC", "RUST_BACKTRACE": "0",
            "G3_EXECUTABLE_SHA256": custody["executable"]["sha256"],
            "G3_SOURCE_SHA256": custody["source"]["sha256"],
            "G3_METHODOLOGY_SET_SHA256": custody["methodology_set_sha256"],
        })
        for child in plan[:2]:
            stdout, _ = run_child(child["command"], child["label"], RESULTS / "self-check-v1", started_ns, child["timeout_seconds"], environment, child["kind"])
            result = one_json_object(stdout, child["label"])
            if result.get("status") != "PASS" or result.get("mutations_rejected", 0) < 10:
                raise RuntimeError(f"analyzer synthetic mutation self-check failed: {child['label']}")

        WORK.mkdir()
        rows_dir = RESULTS / "rows-v1"
        rows_dir.mkdir()
        raw = rows_dir / "G3-V1-RAW.jsonl"
        raw.open("x").close()
        storage = {"schema": "phase4-g3-v1-runner-storage-v1", "ceiling_bytes": STORAGE_LIMIT, "samples": []}
        operation_sum = 0
        for spec, child in zip(schedule_records(), plan[2:11]):
            row_root = WORK / spec["label"]
            if row_root.exists():
                raise RuntimeError(f"row root is not fresh: {row_root}")
            stdout, stderr = run_child(child["command"], child["label"], rows_dir, started_ns, child["timeout_seconds"], environment, child["kind"])
            row = one_json_object(stdout, child["label"])
            if row.get("scenario") != spec["scenario"] or row.get("size_bytes") != spec["size_bytes"]:
                raise RuntimeError(f"candidate row identity mismatch: {child['label']}")
            if type(row.get("operation_total_ns")) is not int or row["operation_total_ns"] >= OPERATION_LIMIT_NS:
                raise RuntimeError(f"measured operation ceiling failed: {child['label']}")
            operation_sum += row["operation_total_ns"]
            row.update({
                "sequence": spec["sequence"], "label": spec["label"],
                "command": child["command"], "child_timeout_seconds": child["timeout_seconds"],
                "child_exit_code": 0, "executable_sha256": custody["executable"]["sha256"],
                "source_sha256": custody["source"]["sha256"],
                "methodology_set_sha256": custody["methodology_set_sha256"],
                "environment_sha256": environment_hash, **parse_time(stderr),
            })
            with raw.open("a") as handle:
                handle.write(json.dumps(row, separators=(",", ":"), sort_keys=True) + "\n")
            sample = {"after_sequence": spec["sequence"], **usage(WORK)}
            storage["samples"].append(sample)
            if max(sample["logical_bytes"], sample["apparent_bytes"], sample["allocated_bytes"]) > STORAGE_LIMIT:
                raise RuntimeError("runner-observed transient storage ceiling exceeded")
        if operation_sum >= OPERATION_SUM_LIMIT_NS:
            raise RuntimeError("G3-v1 summed operation wall ceiling exceeded")
        (RESULTS / "STORAGE-v1.json").write_text(json.dumps(storage, indent=2, sort_keys=True) + "\n")
        cleanup = remove_exact_work(storage)
        if cleanup["status"] != "PASS":
            raise RuntimeError("exact transient cleanup failed")

        reports = []
        for child, filename in zip(plan[11:], ("G3-PRIMARY-ANALYSIS-v1.json", "G3-INDEPENDENT-RECOMPUTATION-v1.json")):
            stdout, _ = run_child(child["command"], child["label"], RESULTS / "analysis-children-v1", started_ns, child["timeout_seconds"], environment, child["kind"])
            report = one_json_object(stdout, child["label"])
            (RESULTS / filename).write_text(json.dumps(report, separators=(",", ":"), sort_keys=True) + "\n")
            reports.append(report)
        if any(report.get("status") != "PASS" for report in reports) or reports[0].get("normalized_ledger") != reports[1].get("normalized_ledger") or reports[0].get("normalized_ledger_sha256") != reports[1].get("normalized_ledger_sha256"):
            raise RuntimeError("primary/independent analysis gate or exact agreement failed")

        current_methods = [{"path": path.name, "sha256": sha256(path), "size_bytes": path.stat().st_size} for path in METHODS]
        if current_methods != custody["methodology"] or sha256(SOURCE) != custody["source"]["sha256"] or sha256(BINARY) != custody["executable"]["sha256"]:
            raise RuntimeError("source, executable, or method custody changed during campaign")
        elapsed_ns = time.monotonic_ns() - started_ns
        if elapsed_ns >= GLOBAL_LIMIT_NS:
            raise RuntimeError("G3-v1 global 59-second ceiling exceeded")
        campaign = {
            "schema": "phase4-g3-v1-campaign-v1", "status": "PASS",
            "rows": len(SCHEDULE), "rows_rerun": 0, "operation_total_ns": operation_sum,
            "operation_sum_ceiling_ns": OPERATION_SUM_LIMIT_NS,
            "per_operation_ceiling_ns": OPERATION_LIMIT_NS,
            "global_elapsed_ns": elapsed_ns, "global_ceiling_ns": GLOBAL_LIMIT_NS,
            "raw_sha256": sha256(raw),
            "primary_sha256": sha256(RESULTS / "G3-PRIMARY-ANALYSIS-v1.json"),
            "independent_sha256": sha256(RESULTS / "G3-INDEPENDENT-RECOMPUTATION-v1.json"),
            "normalized_ledger_sha256": reports[0]["normalized_ledger_sha256"],
            "cleanup_sha256": sha256(RESULTS / "CLEANUP-v1.json"),
            "custody_sha256": sha256(RESULTS / "CUSTODY-v1.json"),
            "methodology_set_sha256": custody["methodology_set_sha256"],
        }
        (RESULTS / "CAMPAIGN-v1.json").write_text(json.dumps(campaign, indent=2, sort_keys=True) + "\n")
        chronology("campaign-complete", started_ns, status="PASS", rows=len(SCHEDULE))
        print(json.dumps(campaign, separators=(",", ":"), sort_keys=True))
    except Exception as error:
        if RESULTS.is_dir():
            failure = {"schema": "phase4-g3-v1-failure-v1", "status": "REVISE", "reason": f"{type(error).__name__}: {error}", "global_elapsed_ns": time.monotonic_ns() - started_ns, "result_root_preserved": True}
            (RESULTS / "FAILURE-v1.json").write_text(json.dumps(failure, indent=2, sort_keys=True) + "\n")
        raise
    finally:
        if LOCK.exists():
            LOCK.unlink()


def dry_record():
    rows = schedule_records()
    return {
        "schema": "phase4-g3-v1-dry-run-v1", "status": "PASS",
        "actual_rows": 0, "benchmark_children_invoked": 0,
        "analyzer_children_invoked": 0, "result_root_created": False,
        "lock_created": False, "result_root_absent": not TARGET.exists(),
        "lock_absent": not LOCK.exists(), "schedule": rows,
        "planned_measured_rows": len(rows), "planned_row_reruns": 0,
        "per_operation_ceiling_ns": OPERATION_LIMIT_NS,
        "operation_sum_ceiling_ns": OPERATION_SUM_LIMIT_NS,
        "global_ceiling_ns": GLOBAL_LIMIT_NS,
        "planned_analyzer_self_checks": 2, "planned_analyzers": 2,
        "transient_storage_ceiling_bytes": STORAGE_LIMIT,
        "declared_cleanup_root": "results-v1/work-v1",
        "broad_deletion": False,
        "execute_flag_required": "--execute",
        "execute_environment_required": {"G3_V1_EXECUTE": EXECUTION_TOKEN},
        "source_hash_frozen_at_execute": str(SOURCE),
        "executable_hash_frozen_at_execute": str(BINARY),
        "method_hashes_frozen_at_execute": [path.name for path in METHODS],
    }


def self_check():
    rows = schedule_records()
    assert len(rows) == 9 and [row["sequence"] for row in rows] == list(range(1, 10))
    assert len({row["label"] for row in rows}) == 9
    assert rows[1]["child_ceiling_seconds"] == 15 and all(row["child_ceiling_seconds"] == 5 for row in rows[:1] + rows[2:])
    sample = "0.12 real 0.01 user 0.02 sys\n123 maximum resident set size\n4 peak memory footprint\n"
    parsed = parse_time(sample)
    assert parsed["maximum_resident_set_bytes"] == 123 and parsed["external_real_seconds"] == 0.12
    record = dry_record()
    assert record["actual_rows"] == record["benchmark_children_invoked"] == record["analyzer_children_invoked"] == 0
    print(json.dumps({"status": "PASS", "schedule_rows": len(rows), "measured_children_invoked": 0}, sort_keys=True))


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--execute", action="store_true")
    parser.add_argument("--self-check", action="store_true")
    arguments = parser.parse_args()
    if arguments.self_check:
        self_check()
    elif arguments.execute:
        ensure_preflight()
        run_campaign()
    else:
        ensure_fresh()
        print(json.dumps(dry_record(), indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
