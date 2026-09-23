#!/usr/bin/env python3
"""One separate reopened readback per retained #237 slab arm."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[6]
RESULTS = ROOT / "benchmark-results/fs-bench-pro"


def sha256(path):
    with path.open("rb") as file:
        return hashlib.file_digest(file, "sha256").hexdigest()


def run(arm):
    case = RESULTS / f"issue237-slab-{arm}-01/daemon-host/init_namespace/namespace-10000"
    perf = json.loads((case / "perf.jsonl").read_text())
    receipt = json.loads((case / "receipt.json").read_text())
    build = json.loads((RESULTS / f"issue237-slab-{arm}-01/build.json").read_text())
    binary = build["binaries"]["verify_namespace"]["path"]
    manifest = receipt["fixture"]["manifest"]
    command = [binary, str(case / "store.sqlite"), str(case / "history.sqlite"),
               perf["root"], perf["stack_body"], manifest,
               receipt["fixture"]["manifest_sha256"]]
    output = Path(__file__).parent / f"readback-{arm}.json"
    if output.exists():
        raise FileExistsError(output)
    started = time.monotonic_ns()
    try:
        child = subprocess.run(command, capture_output=True, timeout=5,
                               env={**os.environ, "LAYERFS_HISTORY_CURSOR_KEY": os.urandom(32).hex()})
        wall = time.monotonic_ns() - started
        stdout, stderr = child.stdout, child.stderr
        try:
            passed = json.loads(stdout).get("status") == "PASS"
        except (ValueError, UnicodeDecodeError):
            passed = False
        status = "PASS" if child.returncode == 0 and passed else "FAIL"
        returncode = child.returncode
    except subprocess.TimeoutExpired as error:
        wall = time.monotonic_ns() - started
        stdout, stderr = error.stdout or b"", error.stderr or b""
        status, returncode = "TIMEOUT", None
    record = {"arm": arm, "status": status, "wall_ns": wall,
              "returncode": returncode, "command": command,
              "binary_sha256": build["binaries"]["verify_namespace"]["sha256"],
              "store_sha256": sha256(case / "store.sqlite"),
              "history_sha256": sha256(case / "history.sqlite"),
              "stdout": stdout.decode(errors="replace"),
              "stderr": stderr.decode(errors="replace")}
    with output.open("x") as file:
        json.dump(record, file, indent=2, sort_keys=True)
        file.write("\n")
    print(f"{arm}: {status}, wall_ns={wall}")


if __name__ == "__main__":
    for arm in sys.argv[1:]:
        run(arm)
