#!/usr/bin/env python3
"""Run one `history.*` arm/case as a **diagnostic** and receipt it, once.

Two arms, one sample each, identical instrumentation: the harness source is the
retained one and is not modified by this round, so the only difference between the
arms is which product crates the binary linked. The protocol is the retained #190
campaign's, unchanged:

  * fresh output directory; the collector refuses an existing one, and it must not
    pre-create the child's own `--out` directory (the child refuses an existing one);
  * quiet preflight: no named `cargo`/`rustc`/`fs-bench` competitor and >=70% CPU
    idle on the second of two one-second observations; a busy preflight writes a
    deferral and consumes no sample;
  * the two global locks are held by `with_locks.py` around this whole process;
  * one sample per case per arm; no repeat, no best-of, no re-run for a better
    number; failures and deferrals stay on disk.
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
HARNESS = "core/benchmark/fs-bench-pro-storage-content/target/release/fs-bench-storage-content"
# Both arms carry the **same** harness source, which now publishes the save's own
# counters per state; only the product crates differ. The harness seal below is
# computed over that source and must match between the arms.
ARMS = {
    "baseline": Path("/Users/yifanxu/Ephemeral-AI-Lab/layerfs-190-baseline"),
    "candidate": Path("/Users/yifanxu/Ephemeral-AI-Lab/layerfs-190-scope"),
}
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
# The established #190 diagnostic complete-command caps, neither enlarged nor
# promoted: 120 s for stride10 and 240 s for stride3 are the retained campaign's.
LIMITS = {"history-stride10": 120, "history-stride3": 240, "history-stride1": 720}
# stride10/stride3 publish the per-state phase nodes and the read-work
# instrumentation. The driver **refuses** them above 53 states ("timer node bound"),
# so stride1 runs the ordinary recording: its state total is measured and its named
# children are fewer, which is a recording difference and not a smaller workload.
# The 720 s cap is the retained campaign's, declared there by its own convention
# (120 s for 17 states and 240 for 53 is ~4.5 s of complete command per state, so
# 157 x 4.5 s = 706.5 -> 720 s) before its first stride1 sample. It is reused
# unchanged here - not raised for a retry, not lowered to look tighter - and the
# retained stride1 run finished in 199.0 s, 28% of it.
PHASES = {"history-stride10": "1", "history-stride3": "1", "history-stride1": ""}


def sha(path: Path) -> str:
    digest = sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def harness_seal(repo: Path) -> str:
    """Content hash of the harness source both arms must share."""
    base = repo / "core/benchmark/fs-bench-pro-storage-content/src"
    digest = sha256()
    for path in sorted(base.rglob("*.rs")):
        digest.update(str(path.relative_to(base)).encode())
        digest.update(b"\0")
        digest.update(path.read_bytes())
        digest.update(b"\0")
    return digest.hexdigest()


def git(repo: Path, *args: str) -> str:
    return subprocess.run(["git", "-C", str(repo), *args],
                          capture_output=True, text=True).stdout.strip()


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
    parser.add_argument("arm", choices=sorted(ARMS))
    parser.add_argument("case", choices=sorted(LIMITS))
    args = parser.parse_args()
    repo = ARMS[args.arm]
    binary = repo / HARNESS
    run = CAMPAIGN / "runs" / f"{args.arm}-{args.case}"
    raw = run / "raw"
    if run.exists():
        print(f"REFUSED: {run} exists; a run never overwrites evidence")
        return 2
    # Only the *parent* is created here: the child creates its own `raw` directory
    # and refuses an existing one.
    run.mkdir(parents=True)
    env = {key: value for key, value in os.environ.items() if not key.startswith("LAYERFS_")}
    env.update({f"LAYERFS_HISTORY_{key}": os.environ.get(f"LAYERFS_HISTORY_{key}", "")
                for key in SWITCHES})
    env["LAYERFS_HISTORY_PHASES"] = PHASES[args.case]
    env["LAYERFS_CONSTRUCTION_WORKERS"] = "1"
    command = [str(binary), "--case", args.case, "--corpus", str(CORPUS),
               "--out", str(raw), "--phase", "perf"]
    limit = LIMITS[args.case]

    before = observation()
    if (before["competing_resource_processes"] or before["last_cpu_idle_percent"] is None
            or before["last_cpu_idle_percent"] < 70):
        (run / f"deferred-{time.time_ns()}.json").write_text(json.dumps(before, indent=2) + "\n")
        print("DEFERRED: competitors or CPU idle below the prospective 70% check")
        return 3
    document = {
        "schema": "h190-save-attribution-diagnostic-v1",
        "arm": args.arm,
        "case": args.case,
        "command": command,
        "cwd": str(repo),
        "limit_ns": limit * 1_000_000_000,
        "binary": str(binary),
        "binary_sha256": sha(binary),
        "source_head": git(repo, "rev-parse", "HEAD"),
        "source_subject": git(repo, "log", "-1", "--pretty=%s"),
        "source_clean_over_product_and_reference": git(
            repo, "status", "--porcelain", "--", "core/crates", "crates") == "",
        "harness_dirty_files": git(repo, "status", "--porcelain", "--",
                                   "core/benchmark/fs-bench-pro-storage-content"),
        "platform": platform.platform(),
        "corpus_manifest_sha256": sha(CORPUS / "checkpoint-manifest.json"),
        "environment": {f"LAYERFS_HISTORY_{key}": env[f"LAYERFS_HISTORY_{key}"]
                        for key in SWITCHES + ("PHASES",)},
        "construction_workers": env["LAYERFS_CONSTRUCTION_WORKERS"],
        "cache_contract": "fresh-growing-store; corpus residency measured at first read, not enforced",
        "setup_reuse": "existing immutable corpus; no prepared Store",
        "admission_eligible": False,
        "cold_performance_status": "INELIGIBLE",
        "harness_source_seal": harness_seal(repo),
        "purpose": ("diagnostic: attribute storage.accept_loop per state from the save's own "
                    "counters, matched arms, identical harness source in both"),
        "quiet_status": "no named competitors and >=70% CPU idle; desktop activity remains",
        "pre_observation": before,
    }
    (run / "perf-declaration.json").write_text(json.dumps(document, indent=2) + "\n")

    start = time.monotonic_ns()
    timed_out = False
    with (run / "perf-stdout.txt").open("x") as stdout, \
            (run / "perf-stderr.txt").open("x") as stderr:
        try:
            result = subprocess.run(command, cwd=repo, env=env, stdout=stdout,
                                    stderr=stderr, timeout=limit)
            code = result.returncode
        except subprocess.TimeoutExpired:
            timed_out, code = True, None
    document.update(wall_ns=time.monotonic_ns() - start, exit_code=code, timed_out=timed_out)
    document["post_observation"] = observation()
    (run / "perf-receipt.json").write_text(json.dumps(document, indent=2) + "\n")
    print(f"{args.arm}/{args.case}: exit {code}, wall {document['wall_ns'] / 1e9:.3f} s, "
          f"timed_out {timed_out}")
    return 0 if code == 0 and not timed_out else 1


if __name__ == "__main__":
    raise SystemExit(main())
