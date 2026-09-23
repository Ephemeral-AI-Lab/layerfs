#!/usr/bin/env python3
"""Run one #231 diagnostic with declared whole-source residency evidence."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import sys
import time

ROOT = Path(__file__).resolve().parents[6]
sys.path.insert(0, str(ROOT / "core/benchmark/fs-bench-pro"))
sys.path.insert(0, str(ROOT / "benchmark/fs-bench-pro/shared"))
import runner  # noqa: E402
import cold  # noqa: E402


def cold_source(source, manifest):
    backend = cold.Residency()
    started = time.monotonic_ns()
    result = {"method": cold.METHOD, "self_check": backend.self_check(),
              "source": str(source), "manifest_sha256": hashlib.sha256(manifest.read_bytes()).hexdigest(),
              "files": 0, "bytes": 0, "pages": 0, "resident_pages": 0}
    rows = [line.split("\t") for line in manifest.read_text().splitlines()]
    files = [(source / row[0], int(row[4])) for row in rows if row[1] == "f"]
    for path, expected in files:
        with path.open("rb") as file:
            metadata = os.fstat(file.fileno())
            if metadata.st_size != expected:
                raise ValueError(f"source size drift: {path}")
            backend.check(file.fileno(), expected, evict=True)
    for path, expected in files:
        with path.open("rb") as file:
            pages, resident = backend.check(file.fileno(), expected)
            result["files"] += 1
            result["bytes"] += expected
            result["pages"] += pages
            result["resident_pages"] += resident
    result["finished_ns"] = time.monotonic_ns()
    result["wall_ns"] = result["finished_ns"] - started
    result["status"] = "VERIFIED_COLD" if not result["resident_pages"] else "INELIGIBLE"
    return result


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--case", choices=("namespace-1000-compact-v3", "namespace-10000"), required=True)
    parser.add_argument("--out", required=True)
    parser.add_argument("--verify", action="store_true")
    args = parser.parse_args()
    out = runner.owned(args.out)
    original_prepare = runner.init.prepare
    original_import = runner.init.public_import
    state = {}

    def prepare(case, root):
        state["fixture"] = original_prepare(case, root)
        return state["fixture"]

    def public_import(daemon, case, stack, scope_seed):
        fixture = state["fixture"]
        cold_result = cold_source(Path(fixture["source"]), Path(fixture["manifest"]))
        folder = out / "daemon-host/init_namespace" / case.id
        (folder / "cold-preflight.json").write_text(json.dumps(cold_result, sort_keys=True, indent=2) + "\n")
        if (cold_result["status"] != "VERIFIED_COLD" or cold_result["files"] != case.files
                or cold_result["bytes"] != case.logical_bytes):
            raise ValueError("source pages remained resident; no timed call")
        result = original_import(daemon, case, stack, scope_seed)
        (folder / "cold-launch.json").write_text(json.dumps({
            "preflight_sha256": hashlib.sha256((folder / "cold-preflight.json").read_bytes()).hexdigest(),
            "launch_gap_ns": result["started_ns"] - cold_result["finished_ns"],
        }, sort_keys=True, indent=2) + "\n")
        return result

    runner.init.prepare = prepare
    runner.init.public_import = public_import
    runner.run(args.case, out, full_verify=args.verify)
    print(runner.report(out), end="")


if __name__ == "__main__":
    main()
