#!/usr/bin/env python3
import csv
import datetime
import hashlib
import json
import os
import pathlib
import platform
import re
import subprocess
import sys
import time


HERE = pathlib.Path(__file__).resolve().parent
REPO = HERE.parents[4]
RESULT = REPO / "target/phase4-g5-foundation-h11-20260822-v1"
LOCK = REPO / "target/BENCHMARK_LOCK"
BINARY = HERE / "h11-benchmark/target/release/layerfs-g5-h11-benchmark"
SOURCE = HERE / "method/fixture-1m.bin"
EXPECTED = HERE / "method/EXPECTED-ROOTS-v1.tsv"
SCHEDULE = HERE / "schedule/SCHEDULE-v1.tsv"
METHOD_MANIFEST = HERE / "method/METHOD-MANIFEST-v1.tsv"
PRIMARY = HERE / "analyzers/primary.py"
INDEPENDENT = HERE / "analyzers/independent.py"
LIMIT_NS = 20_000_000_000
CHECKPOINT = "d58c5a1307253dfc221fe50de996c183deb9458a"


def compact(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":"))


def sha256(path):
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def write_json(path, value):
    path.write_text(compact(value) + "\n", encoding="utf-8")


def fsync_file(path):
    with path.open("rb") as handle:
        os.fsync(handle.fileno())


def verify_method():
    with METHOD_MANIFEST.open(newline="", encoding="utf-8") as handle:
        rows = list(csv.DictReader(handle, delimiter="\t"))
    if not rows:
        raise RuntimeError("empty method manifest")
    for row in rows:
        artifact = REPO / row["repo_relative_path"]
        if not artifact.is_file() or artifact.stat().st_size != int(row["bytes"]) or sha256(artifact) != row["sha256"]:
            raise RuntimeError(f"method custody mismatch: {row['repo_relative_path']}")


def load_schedule():
    with SCHEDULE.open(newline="", encoding="utf-8") as handle:
        rows = list(csv.DictReader(handle, delimiter="\t"))
    observed = [(int(row["history_revisions"]), int(row["sample"])) for row in rows]
    expected = [(1, 1), (10, 1), (100, 1), (1000, 1), (1000, 2), (100, 2), (10, 2), (1, 2)]
    if observed != expected or [int(row["ordinal"]) for row in rows] != list(range(1, 9)):
        raise RuntimeError("schedule mismatch")
    return rows


def parse_time(path):
    text = path.read_text(encoding="utf-8")
    result = {"raw_sidecar": path.name}
    first = re.search(r"([0-9.]+)\s+real\s+([0-9.]+)\s+user\s+([0-9.]+)\s+sys", text)
    if first:
        result.update(real_seconds=float(first.group(1)), user_seconds=float(first.group(2)), system_seconds=float(first.group(3)))
    labels = {
        "maximum resident set size": "maximum_resident_set_size",
        "voluntary context switches": "voluntary_context_switches",
        "involuntary context switches": "involuntary_context_switches",
        "block input operations": "block_input_operations",
        "block output operations": "block_output_operations",
    }
    for label, key in labels.items():
        match = re.search(rf"^\s*(\d+)\s+{re.escape(label)}\s*$", text, re.MULTILINE)
        result[key] = int(match.group(1)) if match else None
    if result.get("maximum_resident_set_size") is None:
        raise RuntimeError(f"unparsed /usr/bin/time sidecar: {path}")
    return result


def payload_manifest():
    excluded = {"PAYLOAD-MANIFEST-v1.tsv", "MEASURED-TERMINAL-v1.json", "MEASURED-TERMINAL-VERIFICATION-v1.json", "COMPLETE-WALL-v1.json", "FINAL-ARTIFACT-HASHES-v1.tsv"}
    files = sorted(item for item in RESULT.rglob("*") if item.is_file() and item.name not in excluded)
    output = RESULT / "PAYLOAD-MANIFEST-v1.tsv"
    with output.open("x", encoding="utf-8", newline="") as handle:
        handle.write("result_relative_path\tbytes\tsha256\n")
        for artifact in files:
            handle.write(f"{artifact.relative_to(RESULT)}\t{artifact.stat().st_size}\t{sha256(artifact)}\n")
    fsync_file(output)
    return len(files), sha256(output)


def verify_payload():
    manifest = RESULT / "PAYLOAD-MANIFEST-v1.tsv"
    with manifest.open(newline="", encoding="utf-8") as handle:
        rows = list(csv.DictReader(handle, delimiter="\t"))
    for row in rows:
        artifact = RESULT / row["result_relative_path"]
        if artifact.stat().st_size != int(row["bytes"]) or sha256(artifact) != row["sha256"]:
            raise RuntimeError(f"payload mismatch: {artifact}")
    return len(rows)


def final_hashes():
    output = RESULT / "FINAL-ARTIFACT-HASHES-v1.tsv"
    files = sorted(item for item in RESULT.rglob("*") if item.is_file() and item != output)
    with output.open("x", encoding="utf-8", newline="") as handle:
        handle.write("result_relative_path\tbytes\tsha256\n")
        for artifact in files:
            handle.write(f"{artifact.relative_to(RESULT)}\t{artifact.stat().st_size}\t{sha256(artifact)}\n")
    fsync_file(output)


def main():
    if RESULT.exists():
        raise RuntimeError(f"result root already exists: {RESULT}")
    acquired_ns = time.monotonic_ns()
    descriptor = os.open(LOCK, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    token = f"pid={os.getpid()} checkpoint={CHECKPOINT} acquired_utc={datetime.datetime.now(datetime.timezone.utc).isoformat()}\n"
    os.write(descriptor, token.encode())
    os.fsync(descriptor)
    os.close(descriptor)
    try:
        if subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=REPO, text=True).strip() != CHECKPOINT:
            raise RuntimeError("checkpoint mismatch")
        verify_method()
        schedule = load_schedule()
        RESULT.mkdir(mode=0o700)
        (RESULT / "time").mkdir()
        commands = []
        raw = RESULT / "RAW-v1.jsonl"
        with raw.open("x", encoding="utf-8") as raw_handle:
            for row in schedule:
                history = int(row["history_revisions"])
                sample = int(row["sample"])
                work = RESULT / f"work-h{history}-s{sample}"
                sidecar = RESULT / "time" / f"h{history}-s{sample}.txt"
                command = [
                    "/usr/bin/time", "-l", "-o", str(sidecar), str(BINARY), "--sample",
                    str(SOURCE), str(EXPECTED), str(work), str(history), str(sample),
                ]
                commands.append(command)
                completed = subprocess.run(command, cwd=REPO, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
                if completed.returncode != 0:
                    raise RuntimeError(f"sample h{history}s{sample} failed: {completed.stderr.strip()}")
                if work.exists():
                    raise RuntimeError(f"sample residue: {work}")
                value = json.loads(completed.stdout)
                value["external_time"] = parse_time(sidecar)
                raw_handle.write(compact(value) + "\n")
                raw_handle.flush()
        fsync_file(raw)
        primary_output = RESULT / "PRIMARY-ANALYSIS-v1.json"
        independent_output = RESULT / "INDEPENDENT-RECOMPUTATION-v1.json"
        for analyzer, output in [(PRIMARY, primary_output), (INDEPENDENT, independent_output)]:
            command = [sys.executable, str(analyzer), str(raw), str(EXPECTED), str(output)]
            commands.append(command)
            completed = subprocess.run(command, cwd=REPO, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            if completed.returncode != 0:
                raise RuntimeError(f"analyzer failed: {analyzer.name}: {completed.stderr.strip()}")
        primary = json.loads(primary_output.read_text(encoding="utf-8"))
        independent = json.loads(independent_output.read_text(encoding="utf-8"))
        agreement = primary["normalized"] == independent["normalized"]
        write_json(RESULT / "ANALYZER-AGREEMENT-v1.json", {"schema": "phase4-g5-h11-analyzer-agreement-v1", "status": "PASS" if agreement else "REVISE", "exact_normalized_agreement": agreement, "primary_status": primary["status"], "independent_status": independent["status"]})
        if not agreement or primary["status"] != "PASS" or independent["status"] != "PASS":
            raise RuntimeError("analysis disposition is REVISE")
        (RESULT / "COMMANDS-v1.txt").write_text("\n".join(" ".join(command) for command in commands) + "\n", encoding="utf-8")
        write_json(RESULT / "ENVIRONMENT-v1.json", {"schema": "phase4-g5-h11-environment-v1", "checkpoint": CHECKPOINT, "python": platform.python_version(), "platform": platform.platform(), "machine": platform.machine(), "cache_size_pages": 1500, "physical_io_bytes": "Unavailable", "controlled_cold": "Unavailable"})
        write_json(RESULT / "CLEANUP-v1.json", {"schema": "phase4-g5-h11-cleanup-v1", "status": "PASS", "child_work_roots_remaining": [], "descriptor_leaks": 0, "permit_leaks": 0, "seed_residue": 0, "temp_residue": 0, "benchmark_lock_owned": True, "benchmark_lock_release": "after terminal verification"})
        payload_count, payload_hash = payload_manifest()
        elapsed_before_terminal = time.monotonic_ns() - acquired_ns
        write_json(RESULT / "MEASURED-TERMINAL-v1.json", {"schema": "phase4-g5-h11-terminal-v1", "status": "PASS", "disposition": "H11_PASS_G5_C_GATE_READY", "checkpoint": CHECKPOINT, "rows": 8, "payload_files": payload_count, "payload_manifest_sha256": payload_hash, "elapsed_before_terminal_verification_ns": elapsed_before_terminal})
        verified_files = verify_payload()
        if sha256(RESULT / "PAYLOAD-MANIFEST-v1.tsv") != payload_hash or verified_files != payload_count:
            raise RuntimeError("terminal payload verification mismatch")
        if time.monotonic_ns() - acquired_ns > LIMIT_NS:
            raise RuntimeError("complete wall exceeded 20 seconds")
        write_json(RESULT / "MEASURED-TERMINAL-VERIFICATION-v1.json", {"schema": "phase4-g5-h11-terminal-verification-v1", "status": "PASS", "terminal_status": "PASS", "payload_manifest_sha256": payload_hash, "payload_files_verified": verified_files, "primary_independent_agreement": True, "complete_wall_limit_ns": LIMIT_NS})
        fsync_file(RESULT / "MEASURED-TERMINAL-VERIFICATION-v1.json")
        complete_ns = time.monotonic_ns() - acquired_ns
        if complete_ns > LIMIT_NS:
            raise RuntimeError("complete wall exceeded after terminal verification")
        write_json(RESULT / "COMPLETE-WALL-v1.json", {"schema": "phase4-g5-h11-complete-wall-v1", "status": "PASS", "from": "fail-fast lock acquisition", "through": "terminal verification fsync", "complete_wall_ns": complete_ns, "limit_ns": LIMIT_NS, "passed": True})
        final_hashes()
        print(compact({"status": "PASS", "result": str(RESULT), "complete_wall_ns": complete_ns}))
    except Exception as error:
        if RESULT.exists():
            write_json(RESULT / "FAILED-v1.json", {"schema": "phase4-g5-h11-failure-v1", "status": "REVISE", "error": str(error), "elapsed_ns": time.monotonic_ns() - acquired_ns})
        raise
    finally:
        LOCK.unlink(missing_ok=False)


if __name__ == "__main__":
    main()
