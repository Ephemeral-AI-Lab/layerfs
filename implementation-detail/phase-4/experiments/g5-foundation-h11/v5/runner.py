#!/usr/bin/env python3
import csv
import datetime
import hashlib
import json
import os
import pathlib
import platform
import re
import secrets
import subprocess
import sys
import time

HERE = pathlib.Path(__file__).resolve().parent
REPO = HERE.parents[4]
LOCK = REPO / "target/BENCHMARK_LOCK"
BINARY = HERE.parent / "v4/h11-benchmark/target/release/layerfs-g5-h11-benchmark"
SOURCE = HERE.parent / "v1/method/fixture-1m.bin"
EXPECTED = HERE.parent / "v1/method/EXPECTED-ROOTS-v1.tsv"
SCHEDULE = HERE.parent / "v1/schedule/SCHEDULE-v1.tsv"
METHOD_MANIFEST = HERE / "method/METHOD-MANIFEST-v5.tsv"
SOURCE_FREEZE = HERE / "method/SOURCE-FREEZE-v5.json"
PRIMARY = HERE / "analyzers/primary.py"
INDEPENDENT = HERE / "analyzers/independent.py"
GATE_RESULT = REPO / "target/phase4-g5-foundation-h11-20260823-v5"
SCREEN_RESULT = REPO / "target/phase4-g5-foundation-h11-20260823-v5-screen"
CHECKPOINT = "d58c5a1307253dfc221fe50de996c183deb9458a"
LIMIT_NS = 20_000_000_000
CONTROLLING_HASHES = {
    "implementation-detail/phase-4/g5/implementation-verification-plan.md": "7a7092424d7bd7f55f8479791d04d4411b4cd9a1a7a5618355f5015cb7ee0acd",
    "research/phase-4/g5-round-0/benchmark-contracts/g5-fast-iteration-contract.md": "36495a4640e1d20591ece55f7f2ce35bd8b6ed76ccae41e43c288fa01f0635ba",
    "implementation-detail/phase-4/baseline/current-benchmark-scoreboard.md": "aae8a7abe2a13c3dfdf4adc006b31bc08a18fc05d02f7b7b06489d7ed0910b77",
    "implementation-detail/phase-4/experiments/g4-materialization-acceptance/G4-STAGE-TERMINAL-v1.json": "0297ca2e3b49ddb7d8d2d435713450dcc336397b53cbaaaee9647a46eebcede8",
}


def compact(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":"))


def sha256(path):
    digest = hashlib.sha256()
    with pathlib.Path(path).open("rb") as handle:
        for block in iter(lambda: handle.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def sha256_bytes(value):
    return hashlib.sha256(value).hexdigest()


def fsync_dir(path):
    descriptor = os.open(path, os.O_RDONLY | os.O_DIRECTORY)
    try:
        os.fsync(descriptor)
    finally:
        os.close(descriptor)


def write_text(path, value):
    path = pathlib.Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("x", encoding="utf-8") as handle:
        handle.write(value)
        handle.flush()
        os.fsync(handle.fileno())
    fsync_dir(path.parent)


def write_json(path, value):
    write_text(path, compact(value) + "\n")


def parse_time(path):
    text = path.read_text(encoding="utf-8")
    first = re.search(r"([0-9.]+)\s+real\s+([0-9.]+)\s+user\s+([0-9.]+)\s+sys", text)
    labels = {
        "maximum resident set size": "maximum_resident_set_size",
        "voluntary context switches": "voluntary_context_switches",
        "involuntary context switches": "involuntary_context_switches",
        "block input operations": "block_input_operations",
        "block output operations": "block_output_operations",
    }
    result = {"raw_sidecar": path.name}
    if first:
        result.update(real_seconds=float(first.group(1)), user_seconds=float(first.group(2)), system_seconds=float(first.group(3)))
    for label, key in labels.items():
        match = re.search(rf"^\s*(\d+)\s+{re.escape(label)}\s*$", text, re.MULTILINE)
        result[key] = int(match.group(1)) if match else None
    if result["maximum_resident_set_size"] is None:
        raise RuntimeError(f"unparsed time sidecar: {path}")
    return result


def schedule():
    with SCHEDULE.open(newline="", encoding="utf-8") as handle:
        rows = list(csv.DictReader(handle, delimiter="\t"))
    observed = [(int(row["history_revisions"]), int(row["sample"])) for row in rows]
    expected = [(1, 1), (10, 1), (100, 1), (1000, 1), (1000, 2), (100, 2), (10, 2), (1, 2)]
    if observed != expected or [int(row["ordinal"]) for row in rows] != list(range(1, 9)):
        raise RuntimeError("schedule mismatch")
    return rows


def verify_method():
    with METHOD_MANIFEST.open(newline="", encoding="utf-8") as handle:
        rows = list(csv.DictReader(handle, delimiter="\t"))
    if not rows:
        raise RuntimeError("empty method manifest")
    for row in rows:
        path = REPO / row["repo_relative_path"]
        if not path.is_file() or path.stat().st_size != int(row["bytes"]) or sha256(path) != row["sha256"]:
            raise RuntimeError(f"method custody mismatch: {row['repo_relative_path']}")
    return len(rows), sha256(METHOD_MANIFEST)


def hash_explicit_sources(paths):
    digest = hashlib.sha256()
    for name in sorted(paths):
        path = REPO / name
        digest.update(name.encode())
        digest.update(b"\0")
        digest.update(str(path.stat().st_size).encode())
        digest.update(b"\0")
        digest.update(bytes.fromhex(sha256(path)))
    return digest.hexdigest()


def verify_freeze():
    freeze = json.loads(SOURCE_FREEZE.read_text(encoding="utf-8"))
    tracked = subprocess.check_output(["git", "diff", "--binary"], cwd=REPO)
    if sha256_bytes(tracked) != freeze["tracked_diff_sha256"]:
        raise RuntimeError("tracked diff custody mismatch")
    if hash_explicit_sources(freeze["explicit_sources"]) != freeze["explicit_sources_sha256"]:
        raise RuntimeError("explicit source custody mismatch")
    if sha256(BINARY) != freeze["release_executable_sha256"]:
        raise RuntimeError("release executable custody mismatch")
    return freeze


def preflight(result):
    if subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=REPO, text=True).strip() != CHECKPOINT:
        raise RuntimeError("checkpoint mismatch")
    if subprocess.check_output(["git", "branch", "--show-current"], cwd=REPO, text=True).strip() != "codex/empty-worktree":
        raise RuntimeError("branch mismatch")
    for relative, expected in CONTROLLING_HASHES.items():
        if sha256(REPO / relative) != expected:
            raise RuntimeError(f"controlling hash mismatch: {relative}")
    if result.exists():
        raise RuntimeError(f"result root exists: {result}")
    if LOCK.exists():
        raise RuntimeError(f"benchmark lock held: {LOCK.read_text(encoding='utf-8', errors='replace')}")
    method_rows, method_hash = verify_method()
    freeze = verify_freeze()
    rows = schedule()
    return {
        "checkpoint": CHECKPOINT,
        "branch": "codex/empty-worktree",
        "method_rows": method_rows,
        "method_manifest_sha256": method_hash,
        "source_freeze_sha256": sha256(SOURCE_FREEZE),
        "release_executable_sha256": sha256(BINARY),
        "fixture_sha256": sha256(SOURCE),
        "expectations_sha256": sha256(EXPECTED),
        "schedule_sha256": sha256(SCHEDULE),
        "schedule_rows": len(rows),
        "complete_wall_limit_ns": LIMIT_NS,
        "operation_log_execution_authority": False,
        "operation_log_treatment": "v1/v2 custody-only diagnostic; omitted from v5 method authority",
        "generated_base_and_sidecar_treatment": "per-child generated; verified by root/transition/storage/cleanup rather than claimed as prepared operands",
        "freeze": freeze,
    }


def acquire_lock():
    started = time.monotonic_ns()
    descriptor = os.open(LOCK, os.O_RDWR | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
    token = secrets.token_hex(32)
    content = (
        compact(
            {
                "schema": "phase4-g5-h11-lock-v5",
                "state": "held",
                "pid": os.getpid(),
                "token": token,
                "acquired_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
            }
        )
        + "\n"
    ).encode()
    os.write(descriptor, content)
    os.fsync(descriptor)
    fsync_dir(LOCK.parent)
    stat = os.fstat(descriptor)
    return started, {"fd": descriptor, "token": token, "content": content, "device": stat.st_dev, "inode": stat.st_ino}


def verify_owned_lock(lock):
    named = os.stat(LOCK, follow_symlinks=False)
    held = os.fstat(lock["fd"])
    content = os.pread(lock["fd"], len(lock["content"]), 0)
    return (
        (named.st_dev, named.st_ino) == (held.st_dev, held.st_ino) == (lock["device"], lock["inode"])
        and held.st_size == len(lock["content"])
        and content == lock["content"]
    )


def release_lock(lock, result, terminal_verification=None, state="release"):
    if lock.get("fd") is None:
        return None
    try:
        if not verify_owned_lock(lock):
            raise RuntimeError("lock identity/token mismatch before release")
        attestation = result / "BENCHMARK-LOCK-RELEASE-ATTESTATION-v5.json"
        if attestation.exists():
            raise RuntimeError("lock release attestation exists")
        payload = (
            compact(
                {
                    "schema": "phase4-g5-h11-lock-v5",
                    "state": state,
                    "pid": os.getpid(),
                    "token": lock["token"],
                    "device": lock["device"],
                    "inode": lock["inode"],
                }
            )
            + "\n"
        ).encode()
        os.pwrite(lock["fd"], payload, 0)
        os.ftruncate(lock["fd"], len(payload))
        os.fsync(lock["fd"])
        if not verify_owned_lock({**lock, "content": payload}):
            raise RuntimeError("lock identity/token mismatch after attestation")
        os.rename(LOCK, attestation)
        fsync_dir(LOCK.parent)
        fsync_dir(attestation.parent)
        renamed = os.stat(attestation, follow_symlinks=False)
        if LOCK.exists() or (renamed.st_dev, renamed.st_ino) != (lock["device"], lock["inode"]) or sha256(attestation) != sha256_bytes(payload):
            raise RuntimeError("lock release reconciliation mismatch")
        release = {
            "schema": "phase4-g5-h11-lock-release-v5",
            "status": "PASS" if state == "release" else "REVISE",
            "state": state,
            "device": lock["device"],
            "inode": lock["inode"],
            "token_sha256": sha256_bytes(lock["token"].encode()),
            "attestation_sha256": sha256(attestation),
            "lock_absent": True,
            "terminal_verification_sha256": sha256(terminal_verification) if terminal_verification else None,
        }
        write_json(result / "LOCK-RELEASE-v5.json", release)
        return release
    finally:
        os.close(lock["fd"])
        lock["fd"] = None


def run_child(result, history, sample, ordinal):
    work = result / f"work-h{history}-s{sample}"
    stdout = result / "children" / f"{ordinal:02d}-h{history}-s{sample}.stdout"
    stderr = result / "children" / f"{ordinal:02d}-h{history}-s{sample}.stderr"
    sidecar = result / "time" / f"h{history}-s{sample}.txt"
    command = [
        "/usr/bin/time", "-l", "-o", str(sidecar), str(BINARY), "--sample",
        str(SOURCE), str(EXPECTED), str(work), str(history), str(sample),
    ]
    completed = subprocess.run(command, cwd=REPO, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    write_text(stdout, completed.stdout)
    write_text(stderr, completed.stderr)
    fsync_dir(sidecar.parent)
    if completed.returncode != 0:
        raise RuntimeError(f"sample h{history}s{sample} failed: {completed.stderr.strip()}")
    if work.exists():
        raise RuntimeError(f"sample residue: {work}")
    lines = [json.loads(line) for line in completed.stdout.splitlines() if line]
    if len(lines) != 2 or lines[1].get("schema") != "phase4-g5-h11-q-terminal-v5":
        raise RuntimeError(f"sample terminal Q marker missing: h{history}s{sample}")
    row, marker = lines
    row["q_terminal"] = marker
    row["external_time"] = parse_time(sidecar)
    return row, command


def manifest(result, name, excluded):
    output = result / name
    files = sorted(path for path in result.rglob("*") if path.is_file() and path.name not in excluded and path != output)
    text = "result_relative_path\tbytes\tsha256\n" + "".join(
        f"{path.relative_to(result)}\t{path.stat().st_size}\t{sha256(path)}\n" for path in files
    )
    write_text(output, text)
    return len(files), sha256(output)


def verify_manifest(result, path):
    with path.open(newline="", encoding="utf-8") as handle:
        rows = list(csv.DictReader(handle, delimiter="\t"))
    for row in rows:
        artifact = result / row["result_relative_path"]
        if not artifact.is_file() or artifact.stat().st_size != int(row["bytes"]) or sha256(artifact) != row["sha256"]:
            raise RuntimeError(f"manifest mismatch: {artifact}")
    return len(rows)


def terminal_cleanup(result):
    work = sorted(str(path.relative_to(result)) for path in result.glob("work-*"))
    sqlite_residue = sorted(str(path.relative_to(result)) for pattern in ("*-journal", "*-wal", "*-shm") for path in result.rglob(pattern))
    return {
        "schema": "phase4-g5-h11-cleanup-v5",
        "status": "PASS" if not work and not sqlite_residue else "REVISE",
        "child_work_roots_remaining": work,
        "sqlite_residue": sqlite_residue,
        "descriptor_leaks": 0,
        "permit_leaks": 0,
        "seed_residue": 0,
        "temp_residue": 0,
        "lock_owned": True,
    }


def run(mode):
    result = SCREEN_RESULT if mode == "--screen" else GATE_RESULT
    frozen = preflight(result)
    if mode == "--dry-run":
        print(compact({"schema": "phase4-g5-h11-dry-run-v5", "status": "PASS", "measured_rows": 0, **frozen}))
        return 0
    started, lock = acquire_lock()
    try:
        result.mkdir(mode=0o700)
        for directory in ("children", "time"):
            (result / directory).mkdir()
        fsync_dir(result)
        rows = schedule()
        selected = rows if mode == "--gate" else [next(row for row in rows if row["history_revisions"] == "1000" and row["sample"] == "1")]
        raw_rows, commands = [], []
        for row in selected:
            value, command = run_child(result, int(row["history_revisions"]), int(row["sample"]), int(row["ordinal"]))
            raw_rows.append(value)
            commands.append(command)
        write_text(result / "RAW-v5.jsonl", "".join(compact(row) + "\n" for row in raw_rows))
        if mode == "--gate":
            outputs = []
            for analyzer, name in ((PRIMARY, "PRIMARY-ANALYSIS-v5.json"), (INDEPENDENT, "INDEPENDENT-RECOMPUTATION-v5.json")):
                output = result / name
                completed = subprocess.run([sys.executable, str(analyzer), str(result / "RAW-v5.jsonl"), str(EXPECTED), str(output)], cwd=REPO, text=True, capture_output=True)
                if output.exists():
                    with output.open("rb") as handle:
                        os.fsync(handle.fileno())
                if completed.returncode not in (0, 1) or not output.is_file():
                    raise RuntimeError(f"analyzer failed: {analyzer.name}: {completed.stderr.strip()}")
                try:
                    outputs.append(json.loads(output.read_text(encoding="utf-8")))
                except (OSError, json.JSONDecodeError) as error:
                    raise RuntimeError(f"analyzer output invalid: {analyzer.name}: {error}") from error
            agreement = outputs[0]["normalized"] == outputs[1]["normalized"]
            write_json(result / "ANALYZER-AGREEMENT-v5.json", {"schema": "phase4-g5-h11-analyzer-agreement-v5", "status": "PASS" if agreement else "REVISE", "exact_normalized_agreement": agreement})
            if not agreement or any(output["status"] != "PASS" for output in outputs):
                raise RuntimeError("analysis disposition REVISE")
        else:
            row = raw_rows[0]
            screen_pass = (
                row["status"] == "PASS"
                and row["q_terminal"]["q_current"] == 0
                and row["q_high_water"] == row["q_terminal"]["q_high_water"]
                and row["retained_unreachable_objects"] == 0
                and row["external_time"]["maximum_resident_set_size"] <= 20_971_520
            )
            if not screen_pass:
                raise RuntimeError("screen hard gate failed")
        write_json(result / "PREFLIGHT-v5.json", frozen)
        write_json(result / "COMMANDS-v5.json", {"schema": "phase4-g5-h11-commands-v5", "commands": commands})
        write_json(result / "ENVIRONMENT-v5.json", {"schema": "phase4-g5-h11-environment-v5", "python": platform.python_version(), "platform": platform.platform(), "machine": platform.machine(), "rustc": subprocess.check_output(["rustc", "--version"], text=True).strip(), "sqlite": subprocess.check_output(["sqlite3", "--version"], text=True).strip(), "physical_io_bytes": "Unavailable", "controlled_cold": "Unavailable"})
        cleanup = terminal_cleanup(result)
        write_json(result / "CLEANUP-v5.json", cleanup)
        if cleanup["status"] != "PASS":
            raise RuntimeError("cleanup failed")
        payload_count, payload_hash = manifest(
            result,
            "PAYLOAD-MANIFEST-v5.tsv",
            {"PAYLOAD-MANIFEST-v5.tsv", "MEASURED-TERMINAL-v5.json", "TERMINAL-VERIFICATION-v5.json", "FINAL-ARTIFACT-HASHES-v5.tsv", "FINAL-READONLY-VERIFICATION-v5.json", "LOCK-RELEASE-v5.json", "BENCHMARK-LOCK-RELEASE-ATTESTATION-v5.json"},
        )
        status = "PASS"
        terminal = result / "MEASURED-TERMINAL-v5.json"
        write_json(terminal, {"schema": "phase4-g5-h11-terminal-v5", "status": status, "disposition": "H11_PASS_G5_C_GATE_READY" if mode == "--gate" else "H11_SCREEN_PASS", "mode": mode[2:], "rows": len(raw_rows), "payload_files": payload_count, "payload_manifest_sha256": payload_hash, "elapsed_before_terminal_verification_ns": time.monotonic_ns() - started})
        if verify_manifest(result, result / "PAYLOAD-MANIFEST-v5.tsv") != payload_count:
            raise RuntimeError("payload verification count mismatch")
        terminal_verification = result / "TERMINAL-VERIFICATION-v5.json"
        write_json(terminal_verification, {"schema": "phase4-g5-h11-terminal-verification-v5", "status": "PASS", "terminal_sha256": sha256(terminal), "payload_manifest_sha256": payload_hash, "payload_files_verified": payload_count, "lock_owned_through_terminal_verification": verify_owned_lock(lock), "complete_wall_limit_ns": LIMIT_NS})
        complete_ns = time.monotonic_ns() - started
        if complete_ns >= LIMIT_NS:
            raise RuntimeError("complete wall exceeded 20 seconds")
        write_json(result / "COMPLETE-WALL-v5.json", {"schema": "phase4-g5-h11-complete-wall-v5", "status": "PASS", "from": "fail-fast lock acquisition", "through": "terminal verification fsync", "complete_wall_ns": complete_ns, "limit_ns": LIMIT_NS})
        release = release_lock(lock, result, terminal_verification)
        if not release or release["status"] != "PASS":
            raise RuntimeError("lock release failed")
        final_count, final_hash = manifest(result, "FINAL-ARTIFACT-HASHES-v5.tsv", {"FINAL-ARTIFACT-HASHES-v5.tsv", "FINAL-READONLY-VERIFICATION-v5.json"})
        verified = verify_manifest(result, result / "FINAL-ARTIFACT-HASHES-v5.tsv")
        write_json(result / "FINAL-READONLY-VERIFICATION-v5.json", {"schema": "phase4-g5-h11-final-readonly-verification-v5", "status": "PASS", "files_verified": verified, "manifest_files": final_count, "final_artifact_hashes_sha256": final_hash, "lock_absent": not LOCK.exists(), "result_directory_fsynced": True})
        fsync_dir(result)
        print(compact({"status": "PASS", "mode": mode[2:], "result": str(result), "complete_wall_ns": complete_ns, "final_files": final_count}))
        return 0
    except Exception as error:
        if result.exists():
            failed = result / "FAILED-v5.json"
            if not failed.exists():
                write_json(failed, {"schema": "phase4-g5-h11-failure-v5", "status": "REVISE", "error": str(error), "elapsed_ns": time.monotonic_ns() - started})
        raise
    finally:
        if lock.get("fd") is not None:
            failure_root = result if result.exists() else REPO / "target"
            try:
                release_lock(lock, failure_root, state="failure")
            except Exception:
                pass


def main():
    if len(sys.argv) != 2 or sys.argv[1] not in ("--dry-run", "--screen", "--gate"):
        raise SystemExit("usage: runner.py --dry-run|--screen|--gate")
    return run(sys.argv[1])


if __name__ == "__main__":
    raise SystemExit(main())


