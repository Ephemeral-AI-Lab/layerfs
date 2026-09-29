#!/usr/bin/env python3
"""Labelled causal probe of APFS allocation granularity; never a gate sample."""

import fcntl
import hashlib
import json
import os
from pathlib import Path
import struct
import subprocess
import sys
import time


ROOT = Path(__file__).resolve().parents[5]
sys.path.insert(0, str(ROOT / "core/benchmark/fs-bench-pro"))
from families import history_retention as history  # noqa: E402

MIB = 1 << 20
def digest(path):
    value = hashlib.sha256()
    with Path(path).open("rb") as source:
        for block in iter(lambda: source.read(MIB), b""):
            value.update(block)
    return value.hexdigest()


def stat(path):
    value = path.stat()
    return {"apparent": value.st_size, "allocated": value.st_blocks * 512}


def main():
    if len(sys.argv) != 4 or sys.argv[1] != "--binary-sha":
        raise SystemExit("usage: r030.py --binary-sha SHA256 FRESH_OUTPUT")
    expected_sha, output = sys.argv[2], Path(sys.argv[3]).resolve()
    if len(expected_sha) != 64:
        raise ValueError("binary SHA width")
    if subprocess.check_output(["git", "status", "--porcelain"], cwd=ROOT):
        raise ValueError("diagnostic source is dirty")
    metadata = json.loads(subprocess.check_output([
        "cargo", "metadata", "--manifest-path",
        "core/benchmark/fs-bench-pro-storage-content/Cargo.toml", "--locked",
        "--no-deps", "--format-version", "1"], cwd=ROOT))
    target = Path(metadata["target_directory"]).resolve()
    if not target.is_relative_to(ROOT):
        raise ValueError("foreign Cargo target")
    binary = target / "release/fs-bench-storage-content"
    if digest(binary) != expected_sha:
        raise ValueError("binary SHA mismatch")
    case = history.CASES["history-retention-stride-1-total-storage-v3"]
    pins = case.pin_root / f"{case.backend_id}.tsv"
    ledger = json.loads((case.pin_root / "manifest.json").read_text())
    pin_sha = ledger["cases"][case.backend_id]["roots_sha256"]
    if digest(pins) != pin_sha:
        raise ValueError("root ledger mismatch")
    output.mkdir(parents=True, exist_ok=False)
    native = output / "native"
    store = native / "sample.sqlite"
    command = [str(binary), "--case", case.backend_id, "--corpus",
               str(history.corpus.DEFAULT_ROOT), "--out", str(native)]
    env = {**os.environ, **history.ENV,
           "LAYERFS_HISTORY_RETAINED_CATALOG": "3",
           "LAYERFS_HISTORY_ROOT_PINS": str(pins),
           "LAYERFS_HISTORY_ROOT_PINS_SHA256": pin_sha}
    declaration = {
        "kind": "count-driven physical preallocation diagnostic; never a gate sample",
        "source_commit": subprocess.check_output(["git", "rev-parse", "HEAD"],
                                                 cwd=ROOT, text=True).strip(),
        "binary": str(binary), "binary_sha256": expected_sha, "command": command,
        "root_pin_sha256": pin_sha, "timeout_s": 170,
        "policy": "when st_blocks is below ceil((st_size+2MiB)/1MiB), F_PREALLOCATE from physical EOF by at most 4MiB; poll 25ms",
        "cache_contract": "uncontrolled source cache; no time or storage admission",
        "construction_workers": 1,
    }
    (output / "declaration.json").write_text(json.dumps(declaration, indent=2, sort_keys=True) + "\n")
    observations = []
    lock_path = ROOT / "benchmark-results/fs-bench-pro/.run.lock"
    with lock_path.open("a+b") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        started = time.monotonic_ns()
        with (output / "diagnostic.stdout").open("xb") as stdout, (output / "diagnostic.stderr").open("xb") as stderr:
            child = subprocess.Popen(command, cwd=ROOT, env=env, stdout=stdout, stderr=stderr)
            timed_out = False
            probe_error = None
            try:
                while child.poll() is None:
                    if time.monotonic_ns() - started >= 170_000_000_000:
                        timed_out = True
                        child.kill()
                        break
                    if store.exists() and store.stat().st_size:
                        before = stat(store)
                        wanted = ((before["apparent"] + 2 * MIB + MIB - 1) // MIB) * MIB
                        amount = min(4 * MIB, max(0, wanted - before["allocated"]))
                        if amount:
                            descriptor = os.open(store, os.O_RDWR | os.O_NOFOLLOW)
                            try:
                                reply = fcntl.fcntl(descriptor, 42, struct.pack("IIqqq", 0, 3, 0, amount, 0))
                            finally:
                                os.close(descriptor)
                            observations.append({"at_ns": time.monotonic_ns() - started,
                                                 "before": before, "requested": amount,
                                                 "allocated_by_fcntl": struct.unpack("IIqqq", reply)[4],
                                                 "after": stat(store)})
                    time.sleep(0.025)
            except Exception as error:
                probe_error = repr(error)
                child.kill()
            exit_code = child.wait()
    status = {"exit_code": exit_code, "timed_out": timed_out, "probe_error": probe_error,
              "wall_ns": time.monotonic_ns() - started, "preallocation_calls": len(observations),
              "final_store": stat(store) if store.exists() else None,
              "final_history": stat(native / "history.sqlite") if (native / "history.sqlite").exists() else None,
              "admission_status": "NOT_A_SAMPLE"}
    (output / "preallocation.json").write_text(json.dumps(observations, indent=2, sort_keys=True) + "\n")
    (output / "status.json").write_text(json.dumps(status, indent=2, sort_keys=True) + "\n")
    print(json.dumps(status, sort_keys=True))


if __name__ == "__main__":
    main()
