#!/usr/bin/env python3
import csv
import hashlib
import json
import os
import platform
import re
import shutil
import stat
import subprocess
import sys
import time
from pathlib import Path

from schedule_g4_v1 import EXPECTED, SCHEDULE, assert_schedule


HERE = Path(__file__).resolve().parent
REPO = HERE.parents[4]
TARGET = REPO / "target/phase4-g4-materialization-acceptance-20260822-v1"
RESULTS = TARGET / "results-v1"
WORK = TARGET / "work-v1"
GLOBAL_LOCK = REPO / "target/BENCHMARK_LOCK"
MANIFEST = HERE / "METHODOLOGY-MANIFEST-v1.tsv"
CANDIDATE = REPO / "target/release/phase4_create_edit_benchmark"
G3_CONTROL = REPO / "target/phase4-g3-incremental-materialization-20260822-v13/results-v13/operands-v13/phase4_create_edit_benchmark"
PROTECTED_CONTROL = REPO / "target/phase4-g2-materialization-decomposition-20260822-v5/results-v5/operands-v5/phase4_create_edit_benchmark-instrumented"
HANDOFF = REPO / "implementation-detail/phase-4/experiments/g4-materialization-acceptance/round-1-research-handoff.md"
EXPECTED_HEAD = "5c342f0ae24ecc69f2bfc03da1c05d1074fe956a"
HASHES = {
    "candidate": "a3573879d55f2fcfb031a334ce208102c7c0c78fa21a99339a8d5585187150c6",
    "g3_control": "535bfa17c01ac227024587d131b44d1decbdd07058e108455952fbe46fa4061e",
    "protected_control": "5d72b46d29a5b77494781f343cc6841a71879b5de426751afe744f27a033e8f5",
    "handoff": "8ca584b9e7958ac57e28e994e1e9bd5638b7d1c703ace1693b1b58706da07d00",
}
SIZE = {1: 1 << 20, 10: 10 << 20, 100: 100 << 20}
CEILING_NS = 120_000_000_000
BUCKET_LIMITS = {
    "lock_and_measured_preflight": 5_000_000_000,
    "private_base_and_shared_preparation": 50_000_000_000,
    "row_dispatch_and_measured_operations": 20_000_000_000,
    "exact_row_verification": 10_000_000_000,
    "primary_and_independent_analysis": 10_000_000_000,
    "cleanup_storage_and_mode_audit": 5_000_000_000,
    "payload_manifest_terminal_and_verification": 10_000_000_000,
}


def sha256(path):
    digest = hashlib.sha256()
    with Path(path).open("rb") as handle:
        for block in iter(lambda: handle.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def fsync_dir(path):
    descriptor = os.open(path, os.O_RDONLY | os.O_DIRECTORY)
    try:
        os.fsync(descriptor)
    finally:
        os.close(descriptor)


def write_text(path, text):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("x") as handle:
        handle.write(text)
        handle.flush()
        os.fsync(handle.fileno())
    fsync_dir(path.parent)


def write_json(path, value):
    write_text(path, json.dumps(value, indent=2, sort_keys=True) + "\n")


def append_jsonl(path, value):
    encoded = (json.dumps(value, separators=(",", ":"), sort_keys=True) + "\n").encode()
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_APPEND, 0o600)
    try:
        os.write(descriptor, encoded)
        os.fsync(descriptor)
    finally:
        os.close(descriptor)


class Buckets:
    def __init__(self, started_ns):
        self.last = started_ns
        self.current = "lock_and_measured_preflight"
        self.values = {name: 0 for name in BUCKET_LIMITS}

    def switch(self, name):
        now = time.monotonic_ns()
        self.values[self.current] += now - self.last
        self.current, self.last = name, now

    def finish(self):
        now = time.monotonic_ns()
        self.values[self.current] += now - self.last
        self.last = now
        return now


def verify_file(path, expected):
    path = Path(path)
    if not path.is_file() or sha256(path) != expected:
        raise RuntimeError(f"custody mismatch: {path}")


def verify_methodology():
    rows = list(csv.DictReader(MANIFEST.open(), delimiter="\t"))
    for row in rows:
        path = HERE / row["path"]
        if not path.is_file() or path.stat().st_size != int(row["size_bytes"]) or sha256(path) != row["sha256"]:
            raise RuntimeError(f"methodology drift: {row['path']}")
    return rows


def parse_time(stderr):
    timing = re.search(r"([0-9.]+) real\s+([0-9.]+) user\s+([0-9.]+) sys", stderr)
    rss = re.search(r"(\d+)\s+maximum resident set size", stderr)
    switches = re.search(r"(\d+)\s+voluntary context switches\s+(\d+)\s+involuntary context switches", stderr)
    if not timing or not rss:
        raise RuntimeError("incomplete /usr/bin/time -l observation")
    return {
        "external_real_seconds": float(timing.group(1)),
        "external_user_seconds": float(timing.group(2)),
        "external_system_seconds": float(timing.group(3)),
        "maximum_resident_set_bytes": int(rss.group(1)),
        "voluntary_context_switches": int(switches.group(1)) if switches else "Unavailable",
        "involuntary_context_switches": int(switches.group(2)) if switches else "Unavailable",
    }


def run_capture(command, label, directory, env=None, timed=True, allow_nonzero=False):
    directory.mkdir(parents=True, exist_ok=True)
    command = [str(item) for item in command]
    executed = ["/usr/bin/time", "-l", *command] if timed else command
    completed = subprocess.run(executed, cwd=REPO, env=env, capture_output=True, text=True, timeout=30)
    write_text(directory / f"{label}.stdout", completed.stdout)
    write_text(directory / f"{label}.stderr", completed.stderr)
    if completed.returncode and not allow_nonzero:
        raise RuntimeError(f"child failed: {label}: {completed.stderr[-400:]}")
    return completed, parse_time(completed.stderr) if timed else None


def snapshot(source, destination, expected):
    shutil.copyfile(source, destination)
    destination.chmod(0o500)
    with destination.open("rb") as handle:
        os.fsync(handle.fileno())
    verify_file(destination, expected)


def child_json(completed):
    values = [line for line in completed.stdout.splitlines() if line.strip()]
    if not values:
        raise RuntimeError("child produced no JSON")
    return json.loads(values[-1])


def storage_usage(root):
    logical = allocated = files = 0
    if root.exists():
        for path in root.rglob("*"):
            if path.is_file() and not path.is_symlink():
                metadata = path.stat()
                logical += metadata.st_size
                allocated += metadata.st_blocks * 512
                files += 1
    return {"logical_bytes": logical, "apparent_bytes": logical, "allocated_bytes": allocated, "files": files}


def main():
    if Path.cwd().resolve() != REPO:
        raise RuntimeError("run from repository root")
    if TARGET.exists():
        raise RuntimeError(f"result root already exists: {TARGET}")
    if GLOBAL_LOCK.exists():
        raise RuntimeError(f"global benchmark lock occupied: {GLOBAL_LOCK.read_text(errors='replace')}")
    started_ns = time.monotonic_ns()
    lock_fd = os.open(GLOBAL_LOCK, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    os.write(lock_fd, f"phase4-g4-v1 pid={os.getpid()}\n".encode())
    os.fsync(lock_fd)
    os.close(lock_fd)
    buckets = Buckets(started_ns)
    lock_released = False
    try:
        if TARGET.exists():
            raise RuntimeError("result root appeared after lock acquisition")
        branch = subprocess.run(["git", "branch", "--show-current"], cwd=REPO, capture_output=True, text=True, check=True).stdout.strip()
        head = subprocess.run(["git", "rev-parse", "HEAD"], cwd=REPO, capture_output=True, text=True, check=True).stdout.strip()
        if branch != "codex/empty-worktree" or head != EXPECTED_HEAD:
            raise RuntimeError("repository custody drift")
        methodology = verify_methodology()
        verify_file(CANDIDATE, HASHES["candidate"])
        verify_file(G3_CONTROL, HASHES["g3_control"])
        verify_file(PROTECTED_CONTROL, HASHES["protected_control"])
        verify_file(HANDOFF, HASHES["handoff"])
        dry = assert_schedule()
        if dry["expected"] != EXPECTED or dry["actual_rows"] != 0:
            raise RuntimeError("schedule dry-run drift")
        TARGET.mkdir()
        RESULTS.mkdir()
        WORK.mkdir()
        for name in ("arm-raw-v1", "preparation-v1", "analysis-v1", "operands-v1", "methodology-v1"):
            (RESULTS / name).mkdir()
        copy = RESULTS / "methodology-v1"
        for row in methodology:
            shutil.copyfile(HERE / row["path"], copy / row["path"])
        shutil.copyfile(MANIFEST, copy / MANIFEST.name)
        current = RESULTS / "operands-v1/phase4_create_edit_benchmark-g4"
        g3_control = RESULTS / "operands-v1/phase4_create_edit_benchmark-g3-control"
        protected_control = RESULTS / "operands-v1/phase4_create_edit_benchmark-protected-control"
        snapshot(CANDIDATE, current, HASHES["candidate"])
        snapshot(G3_CONTROL, g3_control, HASHES["g3_control"])
        snapshot(PROTECTED_CONTROL, protected_control, HASHES["protected_control"])
        write_json(RESULTS / "OPERAND-CUSTODY-v1.json", {
            "candidate": {"path": str(current), "sha256": sha256(current), "mode": "0500"},
            "g3_control": {"path": str(g3_control), "sha256": sha256(g3_control), "mode": "0500"},
            "protected_control": {"path": str(protected_control), "sha256": sha256(protected_control), "mode": "0500"},
        })
        write_json(RESULTS / "ENVIRONMENT-v1.json", {
            "platform": platform.platform(), "python": platform.python_version(),
            "branch": branch, "head": head, "filesystem": "APFS observations via stat",
            "sqlite_profile": {"journal_mode": "DELETE", "synchronous": "FULL", "temp_store": "FILE", "mmap_size": 0, "cache_spill": 2000},
            "true_device_controller_cold": "Unavailable",
            "host_physical_io_bytes": "Unavailable",
        })
        append_jsonl(RESULTS / "CHRONOLOGY-v1.jsonl", {"event": "campaign-start", "elapsed_ns": time.monotonic_ns() - started_ns, "wall_time_ns": time.time_ns()})

        buckets.switch("private_base_and_shared_preparation")
        fixtures = {}
        for mib in (1, 10, 100):
            fixture = WORK / f"fixture-{mib}m"
            completed, _ = run_capture([current, "--g4-prepare", fixture, SIZE[mib]], f"g4-fixture-{mib}m", RESULTS / "preparation-v1", timed=False)
            fixture_record = child_json(completed)
            fixtures[mib] = fixture
            write_json(RESULTS / f"FIXTURE-CUSTODY-{mib}m-v1.json", {
                "reported": fixture_record,
                "database_sha256": sha256(fixture / "g3-qualified-noop/store.sqlite"),
                "authority_sha256": sha256(fixture / "g3-qualified-noop/store.sqlite.authority"),
                "source_sha256": sha256(fixture / "g3-qualified-noop/target.source"),
                "source_size": (fixture / "g3-qualified-noop/target.source").stat().st_size,
            })
        fast_root = WORK / "fast-guards"
        run_capture([current, "--fast-fixture", fast_root, SIZE[100]], "fast-guard-fixture-100m", RESULTS / "preparation-v1", timed=False)
        storage_samples = [{"stage": "fixtures-ready", **storage_usage(WORK)}]
        commands = []

        def record_arm(sequence, record_name, role, command, binary_hash, env=None, category="row_dispatch_and_measured_operations"):
            buckets.switch(category)
            label = f"{sequence:02d}-{record_name}-{role}"
            commands.append({"label": label, "command": [str(item) for item in command], "binary_sha256": binary_hash})
            completed, external = run_capture(command, label, RESULTS / "arm-raw-v1", env=env)
            payload = child_json(completed)
            buckets.switch("exact_row_verification")
            if payload.get("status") != "PASS":
                raise RuntimeError(f"non-PASS arm: {label}")
            arm = {"sequence": sequence, "record": record_name, "role": role, "payload": payload, "external": external, "binary_sha256": binary_hash, "stdout_sha256": sha256(RESULTS / f"arm-raw-v1/{label}.stdout"), "stderr_sha256": sha256(RESULTS / f"arm-raw-v1/{label}.stderr")}
            append_jsonl(RESULTS / "ARM-RAW-v1.jsonl", arm)
            return arm

        def g4_arm(sequence, name, role, mode, mib):
            output = WORK / "native" / f"{sequence:02d}-{role}"
            output.parent.mkdir(parents=True, exist_ok=True)
            arm = record_arm(sequence, name, role, [current, "--g4-row", fixtures[mib], SIZE[mib], mode, output], HASHES["candidate"])
            if output.exists():
                output.rmdir()
            return arm

        def g3_arm(sequence, name, role, scenario, mib, executable, digest):
            root = WORK / "g3" / f"{sequence:02d}-{role}"
            root.parent.mkdir(parents=True, exist_ok=True)
            env = os.environ.copy()
            env["WP4M_EXECUTABLE_SHA256"] = digest
            return record_arm(sequence, name, role, [executable, "--g3-row", root, SIZE[mib], scenario], digest, env, "private_base_and_shared_preparation")

        internal_operation = {"read-range-1m": "read-range-1m", "write": "full", "edit-same": "same-middle", "reopen": "reopen"}

        def fast_arm(sequence, name, role, operation, arm_index, executable, digest):
            iteration = 940_000 + sequence * 10 + arm_index
            buckets.switch("private_base_and_shared_preparation")
            run_capture([current, "--fast-prepare", fast_root, SIZE[100], operation, iteration], f"prepare-{sequence:02d}-{role}", RESULTS / "preparation-v1", timed=False)
            database = fast_root / f"db-K64-F64-{SIZE[100]}-{internal_operation[operation]}-{iteration}.sqlite"
            authority = Path(str(database) + ".authority")
            expectations = Path(str(database) + ".expectations")
            hashes = {"database": sha256(database), "authority": sha256(authority), "expectations": sha256(expectations)}
            env = os.environ.copy()
            env.update({
                "LAYERFS_FAST_LANE": "1", "WP4M_EXECUTABLE_SHA256": digest,
                "WP4M_BASE_COPY_METHOD": "fast-lane-isolated-prepared-row",
                "WP4M_BASE_DATABASE_SHA256": hashes["database"],
                "WP4M_BASE_AUTHORITY_SHA256": hashes["authority"],
                "WP4M_BASE_EXPECTATIONS_SHA256": hashes["expectations"],
            })
            return record_arm(sequence, name, role, [executable, "--fast-row", fast_root, SIZE[100], operation, iteration, "false", "complete-roundtrip"], digest, env)

        g3_scenario = {
            16: "qualified-noop", 17: "qualified-noop", 18: "qualified-one-byte",
            19: "qualified-one-mib", 20: "count-change", 21: "count-change",
            22: "invalid-authority", 23: "invalid-authority", 24: "external-mutation",
            25: "symlink-substitution", 26: "before-publication-fault", 27: "lost-ack",
        }
        guard_operation = {8: "read-range-1m", 28: "write", 29: "edit-same", 30: "reopen"}

        for sequence, name, kind, mib in SCHEDULE:
            arms = []
            if kind == "r01":
                arms = [
                    g4_arm(sequence, name, "r0-control", "r0-control", mib),
                    g4_arm(sequence, name, "r1-attribution-control", "r1-closure-on", mib),
                    g4_arm(sequence, name, "r1-candidate", "r1-closure-off", mib),
                ]
            elif kind == "r1-fresh":
                arms = [g4_arm(sequence, name, "r1-candidate", "r1-fresh", mib)]
            elif kind == "cold-unavailable":
                record = {"sequence": sequence, "record": name, "kind": kind, "size_mib": mib, "status": "Unavailable", "reason": "exclusive-host custody and privileged global host-buffer purge cannot be established; true device/controller cold is unsupported"}
                append_jsonl(RESULTS / "G4-RAW-v1.jsonl", record)
                continue
            elif kind == "seed-read":
                arms = [g4_arm(sequence, name, "s1-candidate", "seed-read", mib)]
            elif kind in {"m0-control", "m0-candidate"}:
                arms = [g4_arm(sequence, name, kind, kind, mib)]
            elif kind == "g3":
                scenario = g3_scenario[sequence]
                arms = [
                    g3_arm(sequence, name, "g3-control", scenario, mib, g3_control, HASHES["g3_control"]),
                    g3_arm(sequence, name, "s1-candidate", scenario, mib, current, HASHES["candidate"]),
                ]
            elif kind == "fast-guard":
                operation = guard_operation[sequence]
                arms = [
                    fast_arm(sequence, name, "protected-control", operation, 0, protected_control, HASHES["protected_control"]),
                    fast_arm(sequence, name, "protected-candidate", operation, 1, current, HASHES["candidate"]),
                ]
            else:
                raise RuntimeError(f"unknown schedule kind {kind}")
            append_jsonl(RESULTS / "G4-RAW-v1.jsonl", {"sequence": sequence, "record": name, "kind": kind, "size_mib": mib, "status": "PASS", "arm_roles": [arm["role"] for arm in arms]})
            storage_samples.append({"stage": f"record-{sequence:02d}-complete", **storage_usage(WORK)})
            if time.monotonic_ns() - started_ns >= CEILING_NS:
                raise TimeoutError("G4 complete-wall ceiling exhausted")

        write_json(RESULTS / "COMMANDS-v1.json", commands)
        write_json(RESULTS / "STORAGE-v1.json", {
            "schema": "phase4-g4-sampled-storage-v1", "classification": "sampled_storage_max_not_continuous_peak",
            "samples": storage_samples,
            "sampled_max_logical_bytes": max(item["logical_bytes"] for item in storage_samples),
            "sampled_max_allocated_bytes": max(item["allocated_bytes"] for item in storage_samples),
            "physical_io_bytes": "Unavailable",
        })

        buckets.switch("primary_and_independent_analysis")
        primary, _ = run_capture([sys.executable, RESULTS / "methodology-v1/analyze_g4_v1.py", RESULTS], "primary-analysis", RESULTS / "analysis-v1", timed=False, allow_nonzero=True)
        independent, _ = run_capture([sys.executable, RESULTS / "methodology-v1/recompute_g4_v1.py", RESULTS], "independent-recomputation", RESULTS / "analysis-v1", timed=False, allow_nonzero=True)
        primary_report = json.loads((RESULTS / "PRIMARY-ANALYSIS-v1.json").read_text())
        independent_report = json.loads((RESULTS / "INDEPENDENT-RECOMPUTATION-v1.json").read_text())
        if primary_report["normalized_ledger"] != independent_report["normalized_ledger"]:
            raise RuntimeError("independent normalized ledger mismatch")

        buckets.switch("cleanup_storage_and_mode_audit")
        shutil.rmtree(WORK)
        if WORK.exists():
            raise RuntimeError("work root survived cleanup")
        residue = sorted(str(path.relative_to(TARGET)) for path in TARGET.rglob("*") if path.name.endswith(("-journal", "-wal", "-shm")) or ".g3-tmp-" in path.name or ".g3-seed-" in path.name)
        cleanup = {"status": "PASS" if not residue else "REVISE", "work_root_absent": True, "residue": residue, "declared_deleted_root": "work-v1"}
        write_json(RESULTS / "CLEANUP-v1.json", cleanup)
        source_files = [
            REPO / "crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs",
            REPO / "crates/layerfs-engine/src/bin/phase4_g3_materialization.rs",
            REPO / "crates/layerfs-core/src/canonical_v2.rs",
            REPO / "Cargo.lock",
        ]
        write_json(RESULTS / "SOURCE-CUSTODY-v1.json", {
            "branch": branch, "head": head, "files": [{"path": str(path.relative_to(REPO)), "sha256": sha256(path), "size_bytes": path.stat().st_size} for path in source_files],
            "git_diff_sha256": hashlib.sha256(subprocess.run(["git", "diff", "--binary"], cwd=REPO, capture_output=True, check=True).stdout).hexdigest(),
            "protected_handoff_sha256": sha256(HANDOFF),
        })
        append_jsonl(RESULTS / "CHRONOLOGY-v1.jsonl", {"event": "rows-analysis-cleanup-complete", "elapsed_ns": time.monotonic_ns() - started_ns, "records": 30, "arms": 50, "wall_time_ns": time.time_ns()})

        buckets.switch("payload_manifest_terminal_and_verification")
        manifest_path = RESULTS / "PAYLOAD-MANIFEST-v1.tsv"
        excluded = {manifest_path, RESULTS / "MEASURED-TERMINAL-v1.json", RESULTS / "MEASURED-TERMINAL-VERIFICATION-v1.json", RESULTS / "COMPLETE-WALL-v1.json", RESULTS / "FINAL-ARTIFACT-HASHES-v1.tsv"}
        payloads = sorted(path for path in RESULTS.rglob("*") if path.is_file() and not path.is_symlink() and path not in excluded)
        write_text(manifest_path, "path\tsha256\tsize_bytes\n" + "".join(f"{path.relative_to(RESULTS)}\t{sha256(path)}\t{path.stat().st_size}\n" for path in payloads))
        manifest_rows = list(csv.DictReader(manifest_path.open(), delimiter="\t"))
        mismatches = [row["path"] for row in manifest_rows if not (RESULTS / row["path"]).is_file() or (RESULTS / row["path"]).stat().st_size != int(row["size_bytes"]) or sha256(RESULTS / row["path"]) != row["sha256"]]
        elapsed_before_terminal = time.monotonic_ns() - started_ns
        issues = sorted(set(primary_report["issues"] + independent_report["issues"] + residue + mismatches))
        if elapsed_before_terminal >= CEILING_NS - 500_000_000:
            issues.append("complete-wall-reserve-before-terminal")
        status = "PASS" if not issues else "REVISE"
        terminal = {
            "schema": "phase4-g4-measured-terminal-v1", "status": status, "disposition": f"G4 {status}",
            "issues": issues, "record_count": 30, "arm_count": 50,
            "normalized_ledger_sha256": primary_report["normalized_ledger_sha256"],
            "primary_analysis_sha256": sha256(RESULTS / "PRIMARY-ANALYSIS-v1.json"),
            "independent_recomputation_sha256": sha256(RESULTS / "INDEPENDENT-RECOMPUTATION-v1.json"),
            "payload_manifest_sha256": sha256(manifest_path), "payload_manifest_entries": len(manifest_rows),
            "payload_mismatches": mismatches, "cleanup_sha256": sha256(RESULTS / "CLEANUP-v1.json"),
            "candidate_executable_sha256": sha256(current), "g3_control_executable_sha256": sha256(g3_control),
            "protected_control_executable_sha256": sha256(protected_control),
            "complete_wall_ceiling_ns": CEILING_NS, "g5_eligible_after_static_and_final_audits": status == "PASS",
        }
        write_json(RESULTS / "MEASURED-TERMINAL-v1.json", terminal)
        if GLOBAL_LOCK.exists():
            GLOBAL_LOCK.unlink()
            fsync_dir(GLOBAL_LOCK.parent)
        lock_released = True
        verification = {
            "schema": "phase4-g4-terminal-verification-v1", "status": status,
            "terminal_sha256": sha256(RESULTS / "MEASURED-TERMINAL-v1.json"),
            "manifest_sha256": sha256(manifest_path), "manifest_entries": len(manifest_rows),
            "manifest_mismatches": mismatches, "record_count": len([line for line in (RESULTS / "G4-RAW-v1.jsonl").read_text().splitlines() if line]),
            "arm_count": len([line for line in (RESULTS / "ARM-RAW-v1.jsonl").read_text().splitlines() if line]),
            "normalized_ledgers_equal": primary_report["normalized_ledger"] == independent_report["normalized_ledger"],
            "lock_absent": not GLOBAL_LOCK.exists(), "work_root_absent": not WORK.exists(), "residue_count": len(residue),
        }
        write_json(RESULTS / "MEASURED-TERMINAL-VERIFICATION-v1.json", verification)
        terminal_verification_fsynced_ns = buckets.finish()
        complete_ns = terminal_verification_fsynced_ns - started_ns
        if complete_ns >= CEILING_NS or sum(buckets.values.values()) != complete_ns:
            raise RuntimeError(f"complete-wall failure after terminal verification: {complete_ns}")
        complete_wall = {
            "schema": "phase4-g4-complete-wall-v1", "status": status,
            "t0_global_lock_attempt_monotonic_ns": started_ns,
            "t_terminal_verification_fsynced_monotonic_ns": terminal_verification_fsynced_ns,
            "complete_wall_ns": complete_ns, "ceiling_ns": CEILING_NS, "reserve_ns": CEILING_NS - complete_ns,
            "buckets_ns": buckets.values, "bucket_sum_ns": sum(buckets.values.values()),
            "bucket_sum_matches_complete": sum(buckets.values.values()) == complete_ns,
            "bucket_limits_ns": BUCKET_LIMITS,
            "terminal_verification_sha256": sha256(RESULTS / "MEASURED-TERMINAL-VERIFICATION-v1.json"),
        }
        write_json(RESULTS / "COMPLETE-WALL-v1.json", complete_wall)
        artifact_files = sorted(path for path in RESULTS.rglob("*") if path.is_file() and path.name != "FINAL-ARTIFACT-HASHES-v1.tsv")
        write_text(RESULTS / "FINAL-ARTIFACT-HASHES-v1.tsv", "path\tsha256\tsize_bytes\n" + "".join(f"{path.relative_to(RESULTS)}\t{sha256(path)}\t{path.stat().st_size}\n" for path in artifact_files))
        for path in sorted((item for item in RESULTS.rglob("*") if item.is_file()), key=lambda item: len(item.parts), reverse=True):
            path.chmod(0o444)
        for path in sorted((item for item in RESULTS.rglob("*") if item.is_dir()), key=lambda item: len(item.parts), reverse=True):
            path.chmod(0o555)
        RESULTS.chmod(0o555)
        fsync_dir(RESULTS)
        print(json.dumps({"status": status, "records": 30, "arms": 50, "complete_wall_ns": complete_ns, "result": str(RESULTS)}, sort_keys=True))
        return 0 if status == "PASS" else 2
    finally:
        if not lock_released and GLOBAL_LOCK.exists():
            GLOBAL_LOCK.unlink()
            fsync_dir(GLOBAL_LOCK.parent)


if __name__ == "__main__":
    raise SystemExit(main())
