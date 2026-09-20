#!/usr/bin/env python3
"""Run one `history.*` performance invocation as a **diagnostic** and receipt it.

This round declares the harness's corpus reading as a phase and publishes two
cache diagnostics; this script produces the evidence that the declaration works and
that the diagnostics read something. It is not an admission run: the complete
command cannot fit the frozen 15/25 s budgets, so the established #190 diagnostic
caps apply (120 s stride10, 240 s stride3) and are neither enlarged nor promoted.

Protocol, unchanged from the retained #190 campaign:
  * fresh output directory; the collector refuses an existing one;
  * quiet preflight: no named `cargo`/`rustc`/`fs-bench` competitor and >=70% CPU
    idle on the second of two one-second observations; a busy preflight writes a
    deferral and consumes no sample;
  * the two global locks are held by `with_locks.py` around this whole process;
  * one sample per case; no repeat, no best-of, no re-run for a better number.
"""
import argparse
import json
import os
import platform
import re
import shutil
import subprocess
import sys
import time
from hashlib import sha256
from pathlib import Path

CAMPAIGN = Path(__file__).resolve().parent
REPO = Path("/Users/yifanxu/Ephemeral-AI-Lab/layerfs-190-qual")
HARNESS = REPO / "core/benchmark/fs-bench-pro-storage-content"
BINARY = HARNESS / "target/release/fs-bench-storage-content"
CORPUS = Path("/Users/yifanxu/Ephemeral-AI-Lab/deepseek-history-data")
SWITCHES = (
    "ADVISORY",
    "ORDERED_PREDECESSORS",
    "DECLARED_ONLY",
    "SIMILARITY_CANDIDATES",
    "FALLBACK_CANDIDATES",
    "FULL_PRODUCER",
    "CHUNK_PREDECESSORS",
    "DEPTH_LIMIT",
)
# Frozen #190 diagnostic complete-command caps. stride10/stride3 are the retained
# campaign's (120 s / 240 s). history-stride1 has never had one: it was never run.
# Its cap is declared here, before the first sample, by the campaign's own convention
# (120 s for 17 states, 240 s for 53 is ~4.5 s of complete command per state, which
# is the row's corpus reading plus its replay), i.e. 157 x 4.5 s = 706.5 s -> 720 s.
# It is a diagnostic ceiling, not an admission budget, and it is not enlarged after
# a miss: a run that cannot finish inside it is recorded NOT_RUN with its wall.
LIMITS = {"history-stride10": 120, "history-stride3": 240, "history-stride1": 720}
# The driver refuses the per-state phase diagnostics above 53 states (timer node
# bound), so stride1 runs the ordinary recording: its named children are measured,
# the extra per-state phase nodes and the read-work instrumentation are not.
PHASES = {"history-stride10": "1", "history-stride3": "1", "history-stride1": ""}


def sha(path: Path) -> str:
    digest = sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def observation() -> dict:
    top = subprocess.run(
        ["top", "-l", "2", "-n", "0", "-s", "1", "-stats", "cpu"],
        capture_output=True, text=True).stdout
    idle = re.findall(r"([0-9.]+)% idle", top)
    processes = subprocess.run(
        ["ps", "-A", "-o", "pid,ppid,%cpu,comm"], capture_output=True, text=True).stdout
    competitors = [line for line in processes.splitlines()[1:]
                   if any(name in line.split()[-1] for name in ("rustc", "cargo", "fs-bench"))]
    return {
        "utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "load": os.getloadavg(),
        "processes": processes,
        "last_cpu_idle_percent": float(idle[-1]) if idle else None,
        "competing_resource_processes": competitors,
        "free_disk_bytes": shutil.disk_usage(CAMPAIGN).free,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("case", choices=sorted(LIMITS))
    args = parser.parse_args()
    run = CAMPAIGN / "runs" / f"diagnostic-{args.case}"
    raw = run / "raw"
    if run.exists():
        print(f"REFUSED: {run} exists; a run never overwrites evidence")
        return 2
    # Only the *parent* is created here. The child creates its own  directory
    # and refuses an existing one, so the collector must not pre-create : a
    # collector that did was refused once in this round, and the refusal is retained
    # in .
    run.mkdir(parents=True)
    env = {key: value for key, value in os.environ.items()
           if not key.startswith("LAYERFS_")}
    env.update({f"LAYERFS_HISTORY_{key}": os.environ.get(f"LAYERFS_HISTORY_{key}", "")
                for key in SWITCHES})
    env["LAYERFS_HISTORY_PHASES"] = PHASES[args.case]
    env["LAYERFS_CONSTRUCTION_WORKERS"] = "1"
    command = [str(BINARY), "--case", args.case, "--corpus", str(CORPUS),
               "--out", str(raw), "--phase", "perf"]
    limit = LIMITS[args.case]

    before = observation()
    if (before["competing_resource_processes"] or before["last_cpu_idle_percent"] is None
            or before["last_cpu_idle_percent"] < 70):
        (run / f"deferred-{time.time_ns()}.json").write_text(json.dumps(before, indent=2) + "\n")
        print("DEFERRED: competitors or CPU idle below the prospective 70% check")
        return 3
    document = {
        "schema": "h190-corpus-phase-diagnostic-v1",
        "case": args.case,
        "command": command,
        "cwd": str(REPO),
        "limit_ns": limit * 1_000_000_000,
        "binary": str(BINARY),
        "binary_sha256": sha(BINARY),
        "platform": platform.platform(),
        "corpus_manifest_sha256": sha(CORPUS / "checkpoint-manifest.json"),
        "environment": {f"LAYERFS_HISTORY_{key}": env[f"LAYERFS_HISTORY_{key}"]
                        for key in SWITCHES + ("PHASES",)},
        "construction_workers": env["LAYERFS_CONSTRUCTION_WORKERS"],
        "cache_contract": "fresh-growing-store; corpus residency measured at first read, not enforced",
        "setup_reuse": "existing immutable corpus; no prepared Store",
        "admission_eligible": False,
        "cold_performance_status": "INELIGIBLE",
        "purpose": ("diagnostic: verify that the declared phases now reconcile and read the "
                    "corpus residency and device-read diagnostics"),
        "quiet_status": "no named competitors and >=70% CPU idle; desktop activity remains",
        "pre_observation": before,
    }
    (run / "perf-declaration.json").write_text(json.dumps(document, indent=2) + "\n")

    start = time.monotonic_ns()
    timed_out = False
    with (run / "perf-stdout.txt").open("x") as stdout, \
            (run / "perf-stderr.txt").open("x") as stderr:
        try:
            result = subprocess.run(command, cwd=REPO, env=env, stdout=stdout,
                                    stderr=stderr, timeout=limit)
            code = result.returncode
        except subprocess.TimeoutExpired:
            timed_out, code = True, None
    document.update(wall_ns=time.monotonic_ns() - start, exit_code=code, timed_out=timed_out)
    document["post_observation"] = observation()
    (run / "perf-receipt.json").write_text(json.dumps(document, indent=2) + "\n")
    print(f"{args.case}: exit {code}, wall {document['wall_ns'] / 1e9:.3f} s, timed_out {timed_out}")
    return 0 if code == 0 and not timed_out else 1


if __name__ == "__main__":
    raise SystemExit(main())
