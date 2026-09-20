#!/usr/bin/env python3
"""Run one `history.*` arm/case as a **diagnostic** and receipt it, once.

The protocol is the retained #190 campaign's, unchanged, with one difference this
round needs: the arm is an explicit **binary**, not a worktree. The instrument arm,
the retained uninstrumented control (an archived immutable executable) and the
treatment arm are three different builds, and only the first is profiled.

  * fresh output directory; the collector refuses an existing one, and it must not
    pre-create the child's own `--out` directory (the child refuses an existing one);
  * quiet preflight: no named `cargo`/`rustc`/`fs-bench` competitor and >=70% CPU
    idle on the second of two one-second observations; a busy preflight writes a
    deferral and consumes no sample;
  * the two global locks are held by `with_locks.py` around this whole process;
  * one sample per case per arm; no repeat, no best-of, no re-run for a better
    number; failures and deferrals stay on disk;
  * `--pre-execute` runs the freshly built binary once, outside the sample, before
    the sample is taken. A freshly created executable's first run costs 0.7-2.0 s
    on this machine before `main` (L50), which lands outside the child's clock and
    is one measured cause of an `INCOMPLETE` stride10 reconciliation. The
    pre-execution is declared in the receipt with its own wall time; it never
    touches the corpus, the Store or the timed region.
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
# promoted: 120 s stride10 / 240 s stride3 / 720 s stride1.
LIMITS = {"history-stride10": 120, "history-stride3": 240, "history-stride1": 720}
# stride10/stride3 publish the per-state phase nodes. The driver **refuses** them
# above 53 states ("timer node bound"), so stride1 runs the ordinary recording:
# `storage.accept_loop` and `harness.index` are not recorded as nodes there, and
# the row's accept total must be derived the same way on both sides.
PHASES = {"history-stride10": "1", "history-stride3": "1", "history-stride1": ""}
RECORDED_STORE = {
    "history-stride10": "4af37932aa3391b12269de8130b9c66dc504f64ca78fc3e585f7afddabed8487",
    "history-stride3": "f5c7ff5a6b4f0821aa9a21ac5250335c4c3fb889637a0c5345c5278caadc2a9e",
    "history-stride1": "1635cf7bbbabdc7f9e4af81ac9c6b6a88f45be35b4100dda0f52394c85dcf418",
}


def sha(path: Path) -> str:
    digest = sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def harness_seal(repo: Path) -> str:
    """Content hash of the harness source this binary was built from."""
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
    parser.add_argument("label", help="arm label, e.g. instrument or control")
    parser.add_argument("case", choices=sorted(LIMITS))
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--cwd", required=True, type=Path,
                        help="directory the child runs in (its own source worktree)")
    parser.add_argument("--seal-repo", type=Path,
                        help="worktree whose harness source this binary was built from")
    parser.add_argument("--pre-execute", action="store_true",
                        help="run the binary once outside the sample and declare it")
    parser.add_argument("--env", action="append", default=[], metavar="KEY=VALUE",
                        help="extra child environment, recorded in the declaration; "
                             "used to enable a measurement-only probe for one arm")
    args = parser.parse_args()
    extra_env = {}
    for item in args.env:
        key, _, value = item.partition("=")
        if not key or not _:
            print(f"REFUSED: --env {item!r} is not KEY=VALUE")
            return 2
        extra_env[key] = value

    binary = args.binary.resolve()
    if not binary.is_file():
        print(f"REFUSED: {binary} is not a file")
        return 2
    run = CAMPAIGN / "runs" / f"{args.label}-{args.case}"
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
    env.update(extra_env)
    command = [str(binary), "--case", args.case, "--corpus", str(CORPUS),
               "--out", str(raw), "--phase", "perf"]
    limit = LIMITS[args.case]

    pre_execution = None
    if args.pre_execute:
        started = time.monotonic_ns()
        pre = subprocess.run([str(binary), "--list"], cwd=str(args.cwd), env=env,
                             capture_output=True, text=True, timeout=60)
        pre_execution = {
            "command": [str(binary), "--list"],
            "exit_code": pre.returncode,
            "wall_ns": time.monotonic_ns() - started,
            "stdout_head": pre.stdout[:400],
            "stderr_head": pre.stderr[:400],
            "reason": ("a freshly created executable's first run costs 0.7-2.0 s before "
                       "main on this machine (L50); it is executed once here so the "
                       "sample's own wall can reconcile. It reads no corpus and no Store."),
        }

    before = observation()
    if (before["competing_resource_processes"] or before["last_cpu_idle_percent"] is None
            or before["last_cpu_idle_percent"] < 70):
        (run / f"deferred-{time.time_ns()}.json").write_text(json.dumps(before, indent=2) + "\n")
        print("DEFERRED: competitors or CPU idle below the 70% check")
        return 3
    document = {
        "schema": "h205-save-split-diagnostic-v1",
        "label": args.label,
        "case": args.case,
        "command": command,
        "cwd": str(args.cwd),
        "limit_ns": limit * 1_000_000_000,
        "binary": str(binary),
        "binary_sha256": sha(binary),
        "source_head": git(args.cwd, "rev-parse", "HEAD") if (args.cwd / ".git").exists()
                       or git(args.cwd, "rev-parse", "HEAD") else None,
        "source_subject": git(args.cwd, "log", "-1", "--pretty=%s"),
        "source_clean_over_product_and_reference": git(
            args.cwd, "status", "--porcelain", "--", "core/crates", "crates") == "",
        "harness_dirty_files": git(args.cwd, "status", "--porcelain", "--",
                                   "core/benchmark/fs-bench-pro-storage-content"),
        "platform": platform.platform(),
        "corpus_manifest_sha256": sha(CORPUS / "checkpoint-manifest.json"),
        "recorded_store_sha256": RECORDED_STORE[args.case],
        "environment": {f"LAYERFS_HISTORY_{key}": env[f"LAYERFS_HISTORY_{key}"]
                        for key in SWITCHES + ("PHASES",)},
        "construction_workers": env["LAYERFS_CONSTRUCTION_WORKERS"],
        "extra_environment": extra_env,
        "cache_contract": "fresh-growing-store; corpus residency measured at first read, not enforced",
        "setup_reuse": "existing immutable corpus; no prepared Store",
        "admission_eligible": False,
        "cold_performance_status": "INELIGIBLE",
        "harness_source_seal": harness_seal(args.seal_repo or args.cwd),
        "pre_execution": pre_execution,
        "quiet_status": "no named competitors and >=70% CPU idle; desktop activity remains",
        "pre_observation": before,
    }
    (run / "perf-declaration.json").write_text(json.dumps(document, indent=2) + "\n")

    start = time.monotonic_ns()
    timed_out = False
    with (run / "perf-stdout.txt").open("x") as stdout, \
            (run / "perf-stderr.txt").open("x") as stderr:
        try:
            result = subprocess.run(command, cwd=str(args.cwd), env=env, stdout=stdout,
                                    stderr=stderr, timeout=limit)
            code = result.returncode
        except subprocess.TimeoutExpired:
            timed_out, code = True, None
    document.update(wall_ns=time.monotonic_ns() - start, exit_code=code, timed_out=timed_out)
    document["post_observation"] = observation()
    (run / "perf-receipt.json").write_text(json.dumps(document, indent=2) + "\n")
    print(f"{args.label}/{args.case}: exit {code}, wall {document['wall_ns'] / 1e9:.3f} s, "
          f"timed_out {timed_out}")
    return 0 if code == 0 and not timed_out else 1


if __name__ == "__main__":
    raise SystemExit(main())
