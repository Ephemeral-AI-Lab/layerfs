#!/usr/bin/env python3
"""Run one fresh, locked stride1 diagnostic and retain exact command custody."""

import fcntl
import hashlib
import json
import os
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[6]
HARNESS = ROOT / "core/benchmark/fs-bench-pro-storage-content"
CORPUS = Path("/Users/yifanxu/Ephemeral-AI-Lab/deepseek-history-data")
SOURCE = [
    "core/crates/layerfs-storage/src/cas/lifecycle.rs",
    "core/crates/layerfs-storage/src/cas/owner.rs",
    "core/crates/layerfs-storage/src/cas/placement.rs",
    "core/crates/layerfs-storage/src/cas/save.rs",
    "core/crates/layerfs-storage/src/cas/selection.rs",
    "core/benchmark/fs-bench-pro-storage-content/src/ops/history.rs",
    "core/docs/issues/237/evidence/sparse-c2-guard/run_arm.py",
]
FIXED = {
    "LAYERFS_HISTORY_ADVISORY": "1",
    "LAYERFS_HISTORY_DEPTH_LIMIT": "7",
    "LAYERFS_HISTORY_FULL_PRODUCER": "0",
    "LAYERFS_HISTORY_CHUNK_PREDECESSORS": "1",
    "LAYERFS_CONSTRUCTION_WORKERS": "1",
}
UNSET = [
    "LAYERFS_HISTORY_ORDERED_PREDECESSORS",
    "LAYERFS_HISTORY_DECLARED_ONLY",
    "LAYERFS_HISTORY_SIMILARITY_CANDIDATES",
    "LAYERFS_HISTORY_FALLBACK_CANDIDATES",
    "LAYERFS_HISTORY_PHASES",
]


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def run(arm: str) -> int:
    if arm not in {"control", "candidate"}:
        raise ValueError("arm must be control or candidate")
    output = ROOT / f"benchmark-results/fs-bench-pro-storage-content/issue237-sparse-c2-{arm}"
    if output.exists():
        raise FileExistsError(output)
    if subprocess.check_output(["git", "status", "--porcelain"], cwd=ROOT).strip():
        raise RuntimeError("source tree is dirty")
    binary = HARNESS / "target/release/fs-bench-storage-content"
    command = [
        str(binary), "--case", "history-stride1", "--phase", "perf",
        "--out", str(output), "--corpus", str(CORPUS),
    ]
    environment = os.environ.copy()
    for name in UNSET:
        environment.pop(name, None)
    environment.update(FIXED)
    manifest = CORPUS / "checkpoint-manifest.json"
    if sha256(manifest) != "03f21acfb415907f521217e7a972ed512265c8d0c2da0f8034e2ff3014334271":
        raise RuntimeError("corpus manifest changed")
    custody = {
        "arm": arm,
        "source_commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
        "command": command,
        "cwd": str(ROOT),
        "environment_fixed": FIXED,
        "environment_unset": UNSET,
        "binary_sha256": sha256(binary),
        "corpus_manifest_sha256": sha256(manifest),
        "source_sha256": {name: sha256(ROOT / name) for name in SOURCE},
        "cache_contract": "sparse-space-diagnostic; corpus cache uncontrolled; no speed claim",
    }
    output.parent.mkdir(parents=True, exist_ok=True)
    lock = HARNESS / ".measurement.lock"
    with lock.open("a") as handle:
        fcntl.flock(handle, fcntl.LOCK_EX | fcntl.LOCK_NB)
        started = time.monotonic()
        with output.with_suffix(".stdout").open("x") as stdout, output.with_suffix(".stderr").open("x") as stderr:
            result = subprocess.run(command, cwd=ROOT, env=environment, stdout=stdout, stderr=stderr, check=False)
        custody["complete_command_wall_s"] = time.monotonic() - started
        custody["exit_code"] = result.returncode
    with output.with_suffix(".custody.json").open("x") as file:
        json.dump(custody, file, indent=2, sort_keys=True)
        file.write("\n")
    return result.returncode


if __name__ == "__main__":
    if len(sys.argv) != 2:
        raise SystemExit("usage: run_arm.py control|candidate")
    raise SystemExit(run(sys.argv[1]))
