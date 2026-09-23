#!/usr/bin/env python3
"""One separate read-only full verifier call for a retained perf-only Init row."""

import hashlib
import json
import os
from pathlib import Path
import secrets
import subprocess
import sys
import time


def digest(path):
    sha = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            sha.update(block)
    return sha.hexdigest()


def main():
    run, output = (Path(arg).resolve() for arg in sys.argv[1:])
    if output.exists():
        raise ValueError("readback output exists")
    cell = run / "daemon-host/init_namespace/namespace-10000"
    perf = json.loads((cell / "perf.jsonl").read_text())
    receipt = json.loads((cell / "receipt.json").read_text())
    build = json.loads((run / "build.json").read_text())
    if perf["status"] != "COMPLETE" or receipt["verification"]["status"] != "SKIPPED":
        raise ValueError("not a complete perf-only source row")
    binary = Path(build["binaries"]["verify_namespace"]["path"])
    binary_hash = digest(binary)
    if binary_hash != build["binaries"]["verify_namespace"]["sha256"]:
        raise ValueError("archived verifier identity changed")
    manifest = Path(receipt["fixture"]["manifest"])
    if digest(manifest) != receipt["fixture"]["manifest_sha256"]:
        raise ValueError("sealed manifest identity changed")
    command = [str(binary), str(cell / "store.sqlite"), str(cell / "history.sqlite"),
               perf["root"], perf["stack_body"], str(manifest),
               receipt["fixture"]["manifest_sha256"]]
    started = time.monotonic_ns()
    try:
        done = subprocess.run(command, capture_output=True, timeout=5, env={
            **os.environ, "LAYERFS_HISTORY_CURSOR_KEY": secrets.token_hex(32)})
        elapsed = time.monotonic_ns() - started
        stdout, stderr, exit_code = done.stdout, done.stderr, done.returncode
        child = json.loads(stdout) if exit_code == 0 else None
        status = "PASS" if child and child.get("status") == "PASS" else "FAIL"
    except subprocess.TimeoutExpired as error:
        elapsed = time.monotonic_ns() - started
        stdout, stderr, exit_code, child, status = (error.stdout or b""), (error.stderr or b""), None, None, "TIMEOUT"
    output.mkdir(parents=True)
    (output / "readback.stdout").write_bytes(stdout)
    (output / "readback.stderr").write_bytes(stderr)
    result = {"status": status, "wall_ns": elapsed, "budget_ns": 5_000_000_000,
              "exit_code": exit_code, "command": command, "binary_sha256": binary_hash,
              "source_commit": receipt["identity"]["source_commit"],
              "product_seal": receipt["identity"]["product_seal"], "child": child,
              "cursor_key_scope": "fresh nonzero read-only capability; no history pagination",
              "stdout_sha256": hashlib.sha256(stdout).hexdigest(),
              "stderr_sha256": hashlib.sha256(stderr).hexdigest()}
    (output / "readback.json").write_text(json.dumps(result, sort_keys=True, indent=2) + "\n")
    print(status, elapsed)


if __name__ == "__main__":
    if len(sys.argv) != 3:
        raise SystemExit("usage: readback_once.py RUN_DIRECTORY FRESH_OUTPUT_DIRECTORY")
    main()
