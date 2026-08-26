#!/usr/bin/env python3
"""Capture one source-bound local Cloudflare authoritative-SQLite durability proof."""

from __future__ import annotations

import argparse
import hashlib
import json
import shutil
import subprocess
import time
from pathlib import Path


EXPECTED_COMMIT = "510b4850385c90311a7a12fcd6a5469812ef5fa0"
EXPECTED_TREE = "21ab7d1e269b3543d11a10068c15e74015929ee8"


def run(argv: list[str], cwd: Path) -> subprocess.CompletedProcess[bytes]:
    return subprocess.run(argv, cwd=cwd, stdout=subprocess.PIPE, stderr=subprocess.PIPE)


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("output", type=Path)
    parser.add_argument("cloudflare_repo", type=Path)
    args = parser.parse_args()
    output = args.output.resolve()
    repo = args.cloudflare_repo.resolve()
    harness = repo / "script/local-durable-fs-bench.mjs"
    test = repo / "script/local-durable-fs-bench.test.mjs"
    if output.exists():
        raise SystemExit(f"evidence root already exists: {output}")
    commit = run(["git", "rev-parse", "HEAD^{commit}"], repo).stdout.decode().strip()
    tree = run(["git", "rev-parse", "HEAD^{tree}"], repo).stdout.decode().strip()
    status = run(["git", "status", "--porcelain"], repo)
    if (commit, tree, status.stdout) != (EXPECTED_COMMIT, EXPECTED_TREE, b""):
        raise SystemExit("Cloudflare wrapper source is not the admitted clean commit/tree")

    output.mkdir(parents=True)
    shutil.copyfile(harness, output / harness.name)
    shutil.copyfile(test, output / test.name)
    argv = ["node", "--experimental-sqlite", "--no-warnings", str(harness)]
    plan = {
        "schema": "layerfs-stage2-015-cloudflare-local-authority-plan-v1",
        "classification": "DIAGNOSTIC_ONLY",
        "wrapper_commit": commit,
        "wrapper_tree": tree,
        "harness_sha256": sha256(harness),
        "test_sha256": sha256(test),
        "argv": argv,
        "cwd": str(repo),
    }
    (output / "plan.json").write_text(json.dumps(plan, indent=2, sort_keys=True) + "\n")
    (output / "git-status.stdout").write_bytes(status.stdout)
    (output / "git-status.stderr").write_bytes(status.stderr)
    started = time.time_ns()
    result = run(argv, repo)
    completed = time.time_ns()
    (output / "harness.stdout").write_bytes(result.stdout)
    (output / "harness.stderr").write_bytes(result.stderr)
    (output / "exit.txt").write_text(f"{result.returncode}\n")
    (output / "wall.json").write_text(
        json.dumps(
            {
                "started_unix_ns": started,
                "completed_unix_ns": completed,
                "wall_ns": completed - started,
            },
            indent=2,
            sort_keys=True,
        )
        + "\n"
    )
    try:
        receipt = json.loads(result.stdout)
    except json.JSONDecodeError as error:
        raise SystemExit(f"harness did not emit JSON: {error}") from error
    receipt["classification"] = "DIAGNOSTIC_ONLY"
    receipt["cloudflareDurableObjectPresent"] = False
    receipt["cloudflareDeploymentPresent"] = False
    receipt["terminalEligible"] = False
    (output / "receipt.json").write_text(json.dumps(receipt, indent=2, sort_keys=True) + "\n")
    if result.returncode or receipt.get("status") != "PASS":
        raise SystemExit("Cloudflare local authoritative durability proof failed")


if __name__ == "__main__":
    main()
