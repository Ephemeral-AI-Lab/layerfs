#!/usr/bin/env python3
"""Prepare and cache one v0.1.6 M1 host Store master.

A selected invocation acquires its writable Store by byte-copying a protected
pristine master (`shared/runner.py::_host_acquire`). Initialising an L100/L500
fixture is explicit first-use fixture preparation, so the master is built once
here and cached under the runner's own selected-input identity. This path takes
no measurement lock and produces no performance or verification receipt.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time

BENCH = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(BENCH))

from shared import runner, runtime  # noqa: E402


def digest(value):
    return hashlib.sha256(
        json.dumps(value, sort_keys=True, separators=(",", ":")).encode()
    ).hexdigest()


def fixture_row(binary, family, case, seed, image):
    output = subprocess.run(
        [str(binary), "infra-fixture-info", family, case, str(seed)],
        check=True,
        capture_output=True,
        text=True,
        env={**os.environ, "LAYERFS_V013_IMAGE": image},
    )
    rows = runner.records(output.stdout)
    if not rows:
        raise SystemExit(f"fixture info produced no row: {output.stderr[-2000:]}")
    return rows[-1]


def list_row(binary, family, case, image, extended=False):
    output = subprocess.run(
        [str(binary), "infra-list", family, case],
        check=True,
        capture_output=True,
        text=True,
        env={**os.environ, "LAYERFS_V013_IMAGE": image},
    )
    rows = [r for r in runner.records(output.stdout) if r.get("scenario_id") == case]
    rows = [r for r in rows if bool(r.get("extended")) == bool(extended)]
    if len(rows) != 1:
        raise SystemExit(f"select exactly one registered case: {case}")
    return rows[0]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--family", default="mixed_load_bearing")
    parser.add_argument("--case", action="append", required=True)
    parser.add_argument("--seed", type=int, default=1)
    parser.add_argument(
        "--host-binary", default=str(runner.REPO / "target/release/fs-benchmark-pro")
    )
    parser.add_argument("--image", default=os.environ.get("LAYERFS_BENCH_IMAGE"))
    parser.add_argument("--timeout", type=float, default=7200)
    parser.add_argument("--extended", action="store_true")
    args = parser.parse_args()
    binary = Path(args.host_binary)
    identity_path = Path(str(binary) + ".identity.json")
    if not identity_path.is_file():
        raise SystemExit("host binary identity is missing; build the host first")
    host_identity = json.loads(identity_path.read_text())
    if runtime.file_sha256(binary) != host_identity["binary_sha256"]:
        raise SystemExit("host binary seal mismatch; rebuild with --build-host")
    if not args.image:
        raise SystemExit("--image or LAYERFS_BENCH_IMAGE is required")
    deadline = runtime.Deadline.after(args.timeout)
    for case in args.case:
        recipe = list_row(binary, args.family, case, args.image, args.extended)
        if recipe.get("extended") and not args.extended:
            raise SystemExit(f"{case} is an explicit extended case; pass --extended")
        fixture = fixture_row(binary, args.family, case, args.seed, args.image)
        source = host_identity["LAYERFS_SOURCE_SEAL"]
        key = digest(
            {
                "family": args.family,
                "case": case,
                "seed": args.seed,
                "source": source,
                "recipe": recipe,
            }
        )
        root = runner.HOST_ROOT / "prepared" / key
        if root.exists():
            print(f"{case}: cache hit {root}")
            continue
        stage = runner.HOST_ROOT / "samples" / f"prepare-{os.getpid()}-{case}"
        if stage.exists():
            raise SystemExit(f"staging already exists: {stage}")
        stage.parent.mkdir(parents=True, exist_ok=True)
        env = {**os.environ, "LAYERFS_V013_IMAGE": args.image}
        started = time.monotonic()
        result = subprocess.run(
            [str(binary), "infra-prepare", args.family, case, str(args.seed), str(stage)],
            capture_output=True,
            text=True,
            env=env,
        )
        print(
            f"{case}: key {key} rc={result.returncode} wall={time.monotonic()-started:.1f}s"
        )
        if result.returncode != 0:
            print(result.stdout[-4000:])
            print(result.stderr[-4000:])
            raise SystemExit(1)
        master = stage / "payload" / "store.sqlite"
        if not master.is_file():
            raise SystemExit(f"prepared master absent: {master}")
        checked = stage / "checked.sqlite"
        runtime.closed_store_copy(master, checked, deadline=deadline)
        checked.unlink()
        master.rename(stage / "store.sqlite")
        (stage / "store.sqlite").chmod(0o444)
        files = runtime.host_tree_identity(stage, deadline)
        manifest = {
            "compatibility": {
                "contract": "layerfs-canonical-v5-workspace-fixture-v1",
                "fixture": fixture,
                "schema_sha256": host_identity["schema_sha256"],
                "seed": args.seed,
            },
            "producer": source,
            "created_ns": time.time_ns(),
            "files": files,
            "data_bytes": sum(item.get("bytes", 0) for item in files.values()),
        }
        (stage / "host-cache.json").write_text(json.dumps(manifest, sort_keys=True))
        (stage / "host-owner.json").write_text(json.dumps({"owner": runtime.OWNER}))
        for path in stage.rglob("*"):
            if path.is_file() and not path.is_symlink():
                path.chmod(path.stat().st_mode & ~0o222)
        root.parent.mkdir(parents=True, exist_ok=True)
        stage.rename(root)
        print(f"{case}: cached {root} data_bytes={manifest['data_bytes']}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
