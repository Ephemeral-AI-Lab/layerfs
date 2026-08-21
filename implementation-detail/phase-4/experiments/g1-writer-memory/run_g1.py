#!/usr/bin/env python3
"""One-shot Phase-4 G1 writer-memory campaign; dry-run is the default safe path."""

import argparse
import csv
import hashlib
import json
import os
import re
import shutil
import sqlite3
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[3]
TARGET = REPO / "target/phase4-g1-writer-memory-cache-spill-20260821-v1"
RESULTS = TARGET / "results-v1"
LOCK = REPO / "target/phase4-g1-writer-memory-cache-spill-20260821-v1.lock"
MANIFEST = HERE / "METHODOLOGY-MANIFEST-v1.tsv"
DRY_RUN = HERE / "DRY-RUN-v1.json"
ANALYZER = HERE / "analyze_g1.py"
RECOMPUTE = HERE / "recompute_g1.py"
SOURCE = REPO / "crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs"
CDC = REPO / "crates/layerfs-core/src/cdc/mod.rs"
CONTROL = REPO / "target/phase4-fastcdc-contiguous-region-kernel-20260821-v2/operands-v1/phase4_create_edit_benchmark-fastcdc-contiguous-region-kernel-v2"
CANDIDATE = REPO / "target/phase4-g1-writer-memory-build-20260821-v1/release/phase4_create_edit_benchmark"
FIXTURE = REPO / "target/phase4-canonical-v2-complete-validation-20260821-v1/results-v1/work-v1/fixtures/S1-100.source"
BASE = REPO / "target/phase4-fastcdc-contiguous-region-kernel-20260821-v2/results-v1/durable-v2/work-v1/master-B/db-K64-F64-104857600-full-970001.sqlite"
BASE_FILES = {"database": BASE, "authority": Path(str(BASE) + ".authority"), "expectations": Path(str(BASE) + ".expectations")}
HASHES = {
    "control": "454bc2f3deacd8581a3cc352c8b7495215cdc103a85580606246ea12bb25eba8",
    "candidate": "42e3ddeb15df298c978b14639690e366fbb26ee55851524d42c6c3e9c0e8bd55",
    "fixture": "63b3695b8c117b5bc39885e0df0dcd0af1d49e575482bab16577d84b4f40eff4",
    "source": "157699e0cd4cb1e3b5ec631cefb7c967ff7433bdeeb10ee1336e70961b402ad2",
    "control_source": "16e9beedd2fe49d6da65f89f53f488cffbfdcfc71f10477e854cd2d37d00e120",
    "source_diff": "3e167cdcdc267ad18452f03960d6dd45a9ab1e137c0cc6b967722e65990e6a09",
    "cdc": "bc0346eec113914943d046a4ab4742420acfff570d6b00115082c40bdf8e58b6",
    "base_database": "8657363e0f90d61bdb911c138a734b66c6adf4cd2dcd50c63c1ca1dae814e30c",
    "base_authority": "7855ea6096359925f639b91c8d6b9708cfe0bc0df4a3ffd97a280a8e9a9ded48",
    "base_expectations": "a7489b01445e53aa8a0c5824059b8a6b04f92e15a3b6cf953fbb4c83d6b5e18a",
}
SIZES = {"control": 1_372_784, "candidate": 1_372_784, "fixture": 104_857_600, "base_database": 20_480, "base_authority": 32, "base_expectations": 1_096}
COPY_MODES = {"database": "0444", "authority": "0400", "expectations": "0444"}
RUNTIME_MODES = {"database": "0600", "authority": "0600", "expectations": "0400"}
ORDERS = ("AB", "BA", "AB", "BA")
CEILING_NS = 20_000_000_000


def sha256(path):
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def bytes_sha256(data):
    return hashlib.sha256(data).hexdigest()


def mode(path):
    return f"{path.stat().st_mode & 0o7777:04o}"


def verify(path, digest, size=None, expected_mode=None):
    if not path.is_file() or (size is not None and path.stat().st_size != size) or sha256(path) != digest:
        raise RuntimeError(f"custody mismatch: {path}")
    if expected_mode and mode(path) != expected_mode:
        raise RuntimeError(f"mode custody mismatch: {path}")


def git(*args):
    completed = subprocess.run(["git", *args], cwd=REPO, capture_output=True, check=True)
    return completed.stdout


def schedule():
    rows = []
    for position, arm in enumerate("AB", 1):
        rows.append({"kind": "warmup", "pair": 0, "order": "AB", "position": position, "arm": arm})
    for pair, order in enumerate(ORDERS, 1):
        for position, arm in enumerate(order, 1):
            rows.append({"kind": "measured", "pair": pair, "order": order, "position": position, "arm": arm})
    for sequence, row in enumerate(rows, 1):
        row["sequence"] = sequence
        row["label"] = f"{sequence:02d}-{row['kind']}-p{row['pair']}-{row['order']}-pos{row['position']}-{row['arm']}"
        row["iteration"] = 981_000 + sequence
        row["command"] = ["/usr/bin/time", "-l", "{control|candidate}", "--fast-row", "{fresh-row-root}", "104857600", "write", str(row["iteration"]), str(row["kind"] == "warmup").lower(), "capture-only"]
    return rows


def ensure_fresh():
    if TARGET.exists() or LOCK.exists():
        raise RuntimeError("G1 result root or execution lock already exists")


def verify_methodology():
    expected = os.environ.get("G1_METHODOLOGY_SHA256")
    if not expected or sha256(MANIFEST) != expected:
        raise RuntimeError("methodology manifest custody mismatch")
    for row in csv.DictReader(MANIFEST.open(), delimiter="\t"):
        path = HERE / row["path"]
        verify(path, row["sha256"], int(row["size_bytes"]))


def runtime_preflight():
    connection = sqlite3.connect(f"file:{BASE}?mode=ro", uri=True)
    base_pragmas = {
        name: connection.execute(f"PRAGMA {name}").fetchone()[0]
        for name in ("page_size", "journal_mode", "synchronous", "mmap_size")
    }
    meta = connection.execute("SELECT hex(profile_id),schema_version,journal_mode,synchronous,temp_store,mmap_size FROM wp4m_meta WHERE id=1").fetchone()
    connection.close()
    expected_meta = ("94A03BA7B6C97B5FF37C0EC62EF1D801B9896494B45456BD3DF23E2CB278D13B", 5, "delete", 2, 1, 0)
    if base_pragmas != {"page_size": 4096, "journal_mode": "delete", "synchronous": 2, "mmap_size": 0} or meta != expected_meta:
        raise RuntimeError("shared base runtime/format preflight mismatch")
    candidate_source = SOURCE.read_text()
    control_source = git("show", "HEAD:crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs").decode()
    if candidate_source.count("PRAGMA cache_spill=2000;") != 1 or "PRAGMA cache_spill=2000;" in control_source or "fn g1_writer_memory_runtime_policy_is_connection_local_and_format_preserving()" not in candidate_source:
        raise RuntimeError("G1 source/runtime policy preflight mismatch")
    return {
        "base_catalog_connection": base_pragmas,
        "format_profile": list(meta),
        "control": {"cache_size": 2000, "cache_spill": 20000, "source_sha256": HASHES["control_source"]},
        "candidate": {"cache_size": 2000, "cache_spill": 2000, "source_sha256": HASHES["source"]},
        "focused_test": "tests::g1_writer_memory_runtime_policy_is_connection_local_and_format_preserving PASS",
    }


def verify_inputs():
    if git("branch", "--show-current").decode().strip() != "codex/empty-worktree" or git("rev-parse", "HEAD").decode().strip() != "286eb7a456165f5417ff0dfcfb603aed07f2e074":
        raise RuntimeError("repository branch or HEAD drift")
    verify(CONTROL, HASHES["control"], SIZES["control"], "0555")
    verify(CANDIDATE, HASHES["candidate"], SIZES["candidate"], "0555")
    verify(FIXTURE, HASHES["fixture"], SIZES["fixture"])
    verify(SOURCE, HASHES["source"])
    verify(CDC, HASHES["cdc"])
    for name, path in BASE_FILES.items():
        verify(path, HASHES[f"base_{name}"], SIZES[f"base_{name}"], COPY_MODES[name])
    control_source = git("show", "HEAD:crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs")
    if bytes_sha256(control_source) != HASHES["control_source"]:
        raise RuntimeError("control source custody mismatch")
    diff = git("diff", "--binary", "--", "crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs")
    if bytes_sha256(diff) != HASHES["source_diff"]:
        raise RuntimeError("candidate source-only diff custody mismatch")
    verify_methodology()
    return runtime_preflight()


def dry_run(preflight):
    ensure_fresh()
    rows = schedule()
    centers = {
        arm: sum(row["sequence"] for row in rows if row["kind"] == "measured" and row["arm"] == arm) / 4
        for arm in "AB"
    }
    if centers != {"A": 6.5, "B": 6.5}:
        raise RuntimeError("planned temporal centers are not equal")
    record = {
        "schema": "phase4-g1-writer-memory-dry-run-v1",
        "status": "PASS",
        "mode": "dry-run",
        "schedule": rows,
        "planned_invocations": 10,
        "planned_measured_rows": 8,
        "actual_measured_rows": 0,
        "database_copies_created": 0,
        "benchmark_children_invoked": 0,
        "temporal_centers": centers,
        "result_root_exists": TARGET.exists(),
        "lock_exists": LOCK.exists(),
        "ceiling_ns": CEILING_NS,
        "hashes": HASHES,
        "runtime_preflight": preflight,
    }
    DRY_RUN.write_text(json.dumps(record, indent=2, sort_keys=True) + "\n")
    print(json.dumps({"status": "PASS", "actual_measured_rows": 0, "temporal_centers": centers}, sort_keys=True))


def parse_time(stderr):
    timing = re.search(r"([0-9.]+) real\s+([0-9.]+) user\s+([0-9.]+) sys", stderr)
    rss = re.search(r"(\d+)\s+maximum resident set size", stderr)
    footprint = re.search(r"(\d+)\s+peak memory footprint", stderr)
    if not timing or not rss or not footprint:
        raise RuntimeError("/usr/bin/time -l output lacks CPU, RSS, or footprint")
    return {"external_real_seconds": float(timing.group(1)), "user_seconds": float(timing.group(2)), "system_seconds": float(timing.group(3)), "maximum_resident_set_bytes": int(rss.group(1)), "peak_memory_footprint_bytes": int(footprint.group(1))}


def remaining(started):
    return (CEILING_NS - (time.monotonic_ns() - started)) / 1_000_000_000


def run_process(command, label, output_root, started, env=None, allow_nonzero=False):
    budget = remaining(started)
    if budget <= 0.5:
        raise TimeoutError(f"20-second global ceiling exhausted before {label}")
    completed = subprocess.run([str(value) for value in command], cwd=REPO, env=env, capture_output=True, text=True, timeout=max(0.25, budget - 0.25))
    (output_root / f"{label}.stdout").write_text(completed.stdout)
    (output_root / f"{label}.stderr").write_text(completed.stderr)
    if completed.returncode and not allow_nonzero:
        raise RuntimeError(f"irreversible command failure: {label}")
    return completed


def copy_methodology():
    destination = RESULTS / "methodology-v1"
    destination.mkdir()
    for row in csv.DictReader(MANIFEST.open(), delimiter="\t"):
        source = HERE / row["path"]
        target = destination / row["path"]
        shutil.copyfile(source, target)
        verify(target, row["sha256"], int(row["size_bytes"]))
    shutil.copyfile(MANIFEST, destination / MANIFEST.name)


def prepare_row(spec, fixture_copy, seen_inodes):
    row_root = RESULTS / "rows-v1/work-v1" / spec["label"]
    row_root.mkdir(parents=True)
    os.symlink(os.path.relpath(fixture_copy, row_root), row_root / FIXTURE.name)
    target = row_root / f"db-K64-F64-104857600-full-{spec['iteration']}.sqlite"
    targets = {"database": target, "authority": Path(str(target) + ".authority"), "expectations": Path(str(target) + ".expectations")}
    custody = {}
    for name, destination in targets.items():
        source = BASE_FILES[name]
        shutil.copy2(source, destination)
        source_stat, copy_stat = source.stat(), destination.stat()
        digest = sha256(destination)
        if digest != HASHES[f"base_{name}"] or mode(destination) != COPY_MODES[name] or copy_stat.st_ino == source_stat.st_ino or (copy_stat.st_dev, copy_stat.st_ino) in seen_inodes:
            raise RuntimeError(f"fresh common-base copy custody mismatch: {spec['label']} {name}")
        seen_inodes.add((copy_stat.st_dev, copy_stat.st_ino))
        destination.chmod(int(RUNTIME_MODES[name], 8))
        after_stat = destination.stat()
        if mode(destination) != RUNTIME_MODES[name] or sha256(destination) != digest:
            raise RuntimeError(f"runtime mode changed bytes: {spec['label']} {name}")
        custody[name] = {
            "source_sha256": HASHES[f"base_{name}"], "post_copy_sha256": digest, "post_mode_sha256": sha256(destination),
            "source_mode_octal": mode(source), "post_copy_mode_octal": COPY_MODES[name], "runtime_mode_octal": mode(destination),
            "source_device": source_stat.st_dev, "source_inode": source_stat.st_ino, "copy_device": after_stat.st_dev, "copy_inode": after_stat.st_ino,
            "distinct_inode": after_stat.st_ino != source_stat.st_ino, "bytes_unchanged": sha256(destination) == HASHES[f"base_{name}"],
        }
    return row_root, target, custody


def prepare_campaign(preflight):
    RESULTS.mkdir(parents=True)
    copy_methodology()
    shutil.copyfile(DRY_RUN, RESULTS / "DRY-RUN-v1.json")
    fixture_copy = RESULTS / "input-v1/S1-100.source"
    fixture_copy.parent.mkdir()
    shutil.copyfile(FIXTURE, fixture_copy)
    fixture_copy.chmod(0o444)
    verify(fixture_copy, HASHES["fixture"], SIZES["fixture"], "0444")
    rows = schedule()
    schedule_path = RESULTS / "G1-SCHEDULE-v1.tsv"
    with schedule_path.open("w", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=list(rows[0]), delimiter="\t", lineterminator="\n")
        writer.writeheader()
        writer.writerows(rows)
    (RESULTS / "RUNTIME-PREFLIGHT-v1.json").write_text(json.dumps(preflight, indent=2, sort_keys=True) + "\n")
    seen_inodes = set()
    prepared = {}
    for spec in rows:
        prepared[spec["label"]] = prepare_row(spec, fixture_copy, seen_inodes)
    binding = {
        "schema": "phase4-g1-writer-memory-input-bindings-v1", "hashes": HASHES,
        "methodology_manifest_sha256": sha256(MANIFEST), "dry_run_sha256": sha256(DRY_RUN),
        "runtime_preflight_sha256": sha256(RESULTS / "RUNTIME-PREFLIGHT-v1.json"), "schedule_sha256": sha256(schedule_path),
        "common_base_modes": {"copy": COPY_MODES, "runtime": RUNTIME_MODES},
    }
    (RESULTS / "INPUT-BINDINGS-v1.json").write_text(json.dumps(binding, indent=2, sort_keys=True) + "\n")
    return rows, prepared


def acquire(rows, prepared, started):
    raw = RESULTS / "rows-v1/G1-RAW-v1.jsonl"
    raw.write_text("")
    binaries = {"A": CONTROL, "B": CANDIDATE}
    for spec in rows:
        row_root, target, custody = prepared[spec["label"]]
        executable = binaries[spec["arm"]]
        env = os.environ.copy()
        env.update({
            "LAYERFS_FAST_LANE": "1", "WP4M_EXECUTABLE_SHA256": HASHES["control" if spec["arm"] == "A" else "candidate"],
            "WP4M_BASE_COPY_METHOD": "physical-byte-copy-identical-database-authority-expectations",
            "WP4M_BASE_DATABASE_SHA256": HASHES["base_database"], "WP4M_BASE_AUTHORITY_SHA256": HASHES["base_authority"],
            "WP4M_BASE_EXPECTATIONS_SHA256": HASHES["base_expectations"],
        })
        command = ["/usr/bin/time", "-l", executable, "--fast-row", row_root, "104857600", "write", spec["iteration"], str(spec["kind"] == "warmup").lower(), "capture-only"]
        completed = run_process(command, spec["label"], RESULTS / "rows-v1", started, env)
        row = json.loads(completed.stdout)
        row.update({key: value for key, value in spec.items() if key != "command"})
        row.update(parse_time(completed.stderr))
        row["binary_sha256"] = sha256(executable)
        row["common_base_sha256"] = {name: HASHES[f"base_{name}"] for name in BASE_FILES}
        row["common_base_custody"] = custody
        row["post_run_file_modes_octal"] = {"database": mode(target), "authority": mode(Path(str(target) + ".authority")), "expectations": mode(Path(str(target) + ".expectations"))}
        row["residue_files"] = sorted(str(path.relative_to(row_root)) for path in row_root.rglob("*") if path.is_file() and path.name.endswith(("-journal", "-wal", "-shm")))
        with raw.open("a") as handle:
            handle.write(json.dumps(row, separators=(",", ":"), sort_keys=True) + "\n")


def analyze(started):
    primary = run_process([sys.executable, ANALYZER, RESULTS], "primary-analyzer", RESULTS, started, allow_nonzero=True)
    independent = run_process([sys.executable, RECOMPUTE, RESULTS], "independent-recompute", RESULTS, started, allow_nonzero=True)
    if not primary.stdout.strip() or not independent.stdout.strip():
        raise RuntimeError("analyzer returned no disposition")
    first = json.loads((RESULTS / "G1-ANALYSIS-v1.json").read_text())
    second = json.loads((RESULTS / "INDEPENDENT-RECOMPUTATION-v1.json").read_text())
    if (first["status"], first["disposition"]) != (second["status"], second["disposition"]):
        raise RuntimeError("primary and independent dispositions disagree")
    for key, value in first.get("statistics", {}).items():
        if abs(value - second.get("statistics", {}).get(key, float("inf"))) > 1e-12:
            raise RuntimeError(f"independent statistic disagreement: {key}")
    return first


def payload_digest(path):
    if path.is_symlink():
        data = os.readlink(path).encode()
        return "symlink", bytes_sha256(data), len(data)
    return "file", sha256(path), path.stat().st_size


def write_payload_manifest():
    path = RESULTS / "PAYLOAD-MANIFEST-v1.tsv"
    excluded = {path, RESULTS / "TERMINAL-v1.json", RESULTS / "TERMINAL-VERIFICATION-v1.txt"}
    files = sorted(item for item in RESULTS.rglob("*") if (item.is_file() or item.is_symlink()) and item not in excluded)
    with path.open("w") as handle:
        handle.write("kind\tpath\tsha256\tsize_bytes\n")
        for item in files:
            kind, digest, size = payload_digest(item)
            handle.write(f"{kind}\t{item.relative_to(RESULTS)}\t{digest}\t{size}\n")
    rows = list(csv.DictReader(path.open(), delimiter="\t"))
    for row in rows:
        kind, digest, size = payload_digest(RESULTS / row["path"])
        if (kind, digest, size) != (row["kind"], row["sha256"], int(row["size_bytes"])):
            raise RuntimeError(f"payload manifest verification failed: {row['path']}")
    return path, len(rows)


def seal():
    for path in sorted((item for item in TARGET.rglob("*") if item.is_file() and not item.is_symlink()), key=lambda item: len(item.parts), reverse=True):
        path.chmod(0o444)
    for path in sorted((item for item in TARGET.rglob("*") if item.is_dir()), key=lambda item: len(item.parts), reverse=True):
        path.chmod(0o555)
    TARGET.chmod(0o555)


def finalize(started, status, disposition, exit_code, error=None):
    final_error = None
    try:
        raw = RESULTS / "rows-v1/G1-RAW-v1.jsonl"
        rows = [json.loads(line) for line in raw.read_text().splitlines() if line] if raw.is_file() else []
        status_path = RESULTS / "STATUS-v1.json"
        status_path.write_text(json.dumps({
            "schema": "phase4-g1-writer-memory-status-v1", "status": status, "disposition": disposition, "exit_code": exit_code,
            "error_type": type(error).__name__ if error else None, "error": str(error) if error else None,
            "rows": len(rows), "measured_rows": sum(row.get("kind") == "measured" for row in rows),
            "elapsed_ns_before_manifest": time.monotonic_ns() - started, "ceiling_ns": CEILING_NS,
        }, indent=2, sort_keys=True) + "\n")
        manifest, entries = write_payload_manifest()
        manifest_hash = sha256(manifest)
        terminal_status, terminal_disposition, terminal_exit = status, disposition, exit_code
        post_manifest = time.monotonic_ns() - started
        if post_manifest >= CEILING_NS:
            terminal_status, terminal_disposition, terminal_exit = "FAIL", "G1 TIMEOUT", 1
        terminal_path = RESULTS / "TERMINAL-v1.json"
        terminal_path.write_text(json.dumps({
            "schema": "phase4-g1-writer-memory-terminal-v1", "status": terminal_status, "disposition": terminal_disposition,
            "exit_code": terminal_exit, "payload_manifest_sha256": manifest_hash, "payload_manifest_entries": entries,
            "status_sha256": sha256(status_path), "input_bindings_sha256": sha256(RESULTS / "INPUT-BINDINGS-v1.json"),
            "post_manifest_elapsed_ns": post_manifest, "ceiling_ns": CEILING_NS, "within_ceiling_post_manifest": post_manifest < CEILING_NS,
        }, indent=2, sort_keys=True) + "\n")
        verification_elapsed = time.monotonic_ns() - started
        verification_pass = sha256(manifest) == manifest_hash and len(list(csv.DictReader(manifest.open(), delimiter="\t"))) == entries and verification_elapsed < CEILING_NS
        (RESULTS / "TERMINAL-VERIFICATION-v1.txt").write_text(
            f"status={'PASS' if verification_pass else 'FAIL'}\n"
            f"disposition={terminal_disposition}\nexit_code={terminal_exit}\n"
            f"payload_manifest_sha256={manifest_hash}\npayload_manifest_entries={entries}\n"
            f"terminal_sha256={sha256(terminal_path)}\nverification_elapsed_ns={verification_elapsed}\n"
            f"ceiling_ns={CEILING_NS}\nwithin_ceiling={str(verification_elapsed < CEILING_NS).lower()}\n"
            f"manifest_verification_pass={str(verification_pass).lower()}\n"
        )
        if not verification_pass:
            final_error = RuntimeError("terminal manifest verification or 20-second clock failed")
    except Exception as failure:
        final_error = failure
        try:
            (RESULTS / "FINALIZATION-FAILURE-v1.json").write_text(json.dumps({"status": "FAIL", "error": str(failure)}, indent=2) + "\n")
        except Exception:
            pass
    finally:
        try:
            seal()
        except Exception as failure:
            final_error = final_error or failure
    return final_error


def execute(preflight):
    ensure_fresh()
    expected_dry_run = os.environ.get("G1_DRY_RUN_SHA256")
    if not DRY_RUN.is_file() or not expected_dry_run or sha256(DRY_RUN) != expected_dry_run:
        raise RuntimeError("dry-run custody mismatch")
    LOCK.mkdir()
    started = time.monotonic_ns()
    status, disposition, exit_code, error = "FAIL", "G1 FAILURE", 1, None
    final_error = None
    try:
        rows, prepared = prepare_campaign(preflight)
        acquire(rows, prepared, started)
        result = analyze(started)
        status, disposition = result["status"], result["disposition"]
        exit_code = 0 if disposition == "G1 MEASURED PASS / STATIC CLOSURE REQUIRED" else (2 if status == "PASS" else 1)
    except BaseException as failure:
        error = failure
        disposition = "G1 TIMEOUT" if isinstance(failure, (TimeoutError, subprocess.TimeoutExpired)) else "G1 FAILURE"
    finally:
        if RESULTS.exists():
            final_error = finalize(started, status, disposition, exit_code, error)
        if LOCK.exists():
            LOCK.rmdir()
    if final_error:
        print(f"G1 finalization failure: {final_error}", file=sys.stderr)
        return 1
    print(json.dumps({"status": status, "disposition": disposition, "exit_code": exit_code, "elapsed_ns": time.monotonic_ns() - started}, sort_keys=True))
    return exit_code


def main():
    parser = argparse.ArgumentParser()
    modes = parser.add_mutually_exclusive_group(required=True)
    modes.add_argument("--dry-run", action="store_true")
    modes.add_argument("--execute", action="store_true")
    args = parser.parse_args()
    ensure_fresh()
    preflight = verify_inputs()
    if args.dry_run:
        dry_run(preflight)
        return 0
    return execute(preflight)


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as error:
        print(f"G1 premeasurement failure: {type(error).__name__}: {error}", file=sys.stderr)
        raise SystemExit(1)
