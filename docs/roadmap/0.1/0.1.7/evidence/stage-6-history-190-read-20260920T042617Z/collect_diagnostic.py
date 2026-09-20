#!/usr/bin/env python3
"""Collect one declared #190 read-path diagnostic; never retry or overwrite a sample.

Mechanics are the retained #190 diagnostic protocol: both shared global flocks
(deduplicated by resolved path), the harness's own private `O_CREAT|O_EXCL`
marker via `shared/receipt.py`, a quiet preflight that consumes no sample, fresh
append-only output paths and a hard per-mode ceiling. The harness private marker
is never opened with append+flock.
"""
import argparse
import contextlib
import fcntl
import json
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess
import sys
import time

CAMPAIGN = Path(__file__).resolve().parent
REPO = CAMPAIGN.parents[5]
HARNESS = REPO / "core/benchmark/fs-bench-pro-storage-content"
CORPUS = Path("/Users/yifanxu/Ephemeral-AI-Lab/deepseek-history-data")
sys.path.insert(0, str(HARNESS / "shared"))
import receipt

SWITCHES = ["ADVISORY", "ORDERED_PREDECESSORS", "DECLARED_ONLY", "SIMILARITY_CANDIDATES",
            "FALLBACK_CANDIDATES", "FULL_PRODUCER", "CHUNK_PREDECESSORS", "DEPTH_LIMIT"]


def write(path, value):
    with path.open("x") as handle:
        json.dump(value, handle, indent=2)
        handle.write("\n")


def sha(path):
    return receipt.sha256_file(path)


def observation():
    def command(args):
        return subprocess.check_output(args, text=True)
    processes = command(["ps", "-Ao", "pid,ppid,%cpu,comm"])
    top = command(["top", "-l", "2", "-s", "1", "-n", "0"])
    idle = re.findall(r"([0-9.]+)% idle", top)
    competitors = [line for line in processes.splitlines()[1:]
                   if any(name in line.split()[-1] for name in ("rustc", "cargo", "fs-bench"))]
    return {"utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
            "load": os.getloadavg(), "processes": processes, "top": top,
            "last_cpu_idle_percent": float(idle[-1]) if idle else None,
            "competing_resource_processes": competitors,
            "free_disk_bytes": shutil.disk_usage(CAMPAIGN).free}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("arm")
    parser.add_argument("case", choices=["history-stride10", "history-stride3"])
    parser.add_argument("--mode", choices=["perf", "verify"], default="perf")
    args = parser.parse_args()
    identity_path = CAMPAIGN / f"{args.arm}-identity.json"
    identity = json.loads(identity_path.read_text())
    binary = Path(identity["binary"])
    assert sha(binary) == identity["binary_sha256"], "binary identity changed"
    run = CAMPAIGN / "runs" / f"{args.arm}-{args.case}"
    run.mkdir(parents=True, exist_ok=True)
    raw = run / "raw"
    if args.mode == "perf":
        assert not raw.exists(), "refusing a repeated performance sample"
    else:
        assert (raw / "phases-perf.json").exists(), "performance artifact required"
        assert not (raw / "phases-verify.json").exists(), "refusing repeated verification"
    assert not (run / f"{args.mode}-receipt.json").exists(), "receipt already exists"
    env = os.environ.copy()
    for suffix in SWITCHES:
        env.pop("LAYERFS_HISTORY_" + suffix, None)
    env["LAYERFS_CONSTRUCTION_WORKERS"] = "1"
    env["LAYERFS_HISTORY_PHASES"] = "1"
    limit = 60 if args.mode == "verify" else (120 if args.case.endswith("10") else 240)
    command = [str(binary), "--case", args.case, "--corpus", str(CORPUS),
               "--out", str(raw), "--phase", args.mode]
    with contextlib.ExitStack() as stack:
        for lock in dict.fromkeys([(Path(os.environ.get("TMPDIR", "/tmp")) /
                                   "layerfs-infra-measurement.lock").resolve(),
                                  Path("/tmp/layerfs-infra-measurement.lock").resolve()]):
            handle = stack.enter_context(lock.open("a"))
            fcntl.flock(handle, fcntl.LOCK_EX | fcntl.LOCK_NB)
        stack.enter_context(receipt.measurement_lock(HARNESS / ".measurement.lock"))
        before = observation()
        # A busy observation is not a sample; do not consume the selected arm.
        if before["competing_resource_processes"] or before["last_cpu_idle_percent"] is None or before["last_cpu_idle_percent"] < 70:
            write(run / f"deferred-{time.time_ns()}.json", before)
            raise SystemExit("DEFERRED: competitors or CPU idle below prospective 70% check")
        document = {"schema": "h190-read-diagnostic-v1", "arm": args.arm,
                    "case": args.case, "mode": args.mode, "command": command,
                    "cwd": str(REPO), "limit_ns": limit * 1_000_000_000,
                    "identity_file": str(identity_path), "identity_sha256": sha(identity_path),
                    "binary_sha256": sha(binary), "platform": platform.platform(),
                    "corpus_manifest_sha256": sha(CORPUS / "checkpoint-manifest.json"),
                    "environment": {"LAYERFS_HISTORY_" + key: env.get("LAYERFS_HISTORY_" + key)
                                    for key in SWITCHES + ["PHASES"]},
                    "construction_workers": env["LAYERFS_CONSTRUCTION_WORKERS"],
                    "cache_contract": "fresh-growing-store; OS/intra-chain residency uncontrolled",
                    "setup_reuse": "existing immutable corpus; no prepared Store",
                    "admission_eligible": False, "cold_performance_status": "INELIGIBLE",
                    "pre_observation": before,
                    "quiet_status": "no named competitors and >=70% CPU idle; desktop activity remains"}
        write(run / f"{args.mode}-declaration.json", document)
        start = time.monotonic_ns()
        timed_out = False
        with (run / f"{args.mode}-stdout.txt").open("x") as stdout, (run / f"{args.mode}-stderr.txt").open("x") as stderr:
            try:
                result = subprocess.run(command, cwd=REPO, env=env, stdout=stdout,
                                        stderr=stderr, timeout=limit)
                code = result.returncode
            except subprocess.TimeoutExpired:
                timed_out, code = True, None
        document.update(wall_ns=time.monotonic_ns() - start, exit_code=code, timed_out=timed_out)
        document["post_observation"] = observation()
        document["execution_status"] = "TIMEOUT" if timed_out else ("COMPLETED" if code == 0 else "NONZERO")
        document["artifacts"] = {str(path.relative_to(raw)): sha(path)
                                 for path in sorted(raw.rglob("*")) if path.is_file()}
        if args.mode == "perf" and (raw / "trace.jsonl").exists():
            shutil.copyfile(raw / "trace.jsonl", run / "trace-perf.jsonl")
        write(run / f"{args.mode}-receipt.json", document)
        print(json.dumps({key: document[key] for key in
                          ("arm", "case", "mode", "execution_status", "wall_ns", "exit_code", "timed_out")}))


if __name__ == "__main__":
    main()
