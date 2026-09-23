#!/usr/bin/env python3
"""One v0.1.6 release Init on the exact Core 100k fixture, with a cold GO barrier."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import re
import select
import shutil
import sqlite3
import subprocess
import time
import os

MANIFEST_SHA256 = "23246a276522418812df2392619c13391bc864835d09a6695b6bd6fc0307d7a5"
CACHE_PROFILE = "independent-source-payload-zero;metadata-residency-unmeasured"


def load_cold_driver(path):
    spec = importlib.util.spec_from_file_location("issue237_shared_cold", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def sha256(path):
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def write_json(path, value):
    path.write_text(json.dumps(value, sort_keys=True, indent=2) + "\n")


def wait_ready(process):
    lines = []
    deadline = time.monotonic() + 30
    while time.monotonic() < deadline:
        readable, _, _ = select.select([process.stderr], [], [], deadline - time.monotonic())
        if not readable:
            break
        line = process.stderr.readline()
        if not line:
            break
        lines.append(line)
        if line.strip() == "issue237-reference-ready-v1":
            return lines
    raise RuntimeError("reference process did not reach READY: " + "".join(lines)[-2000:])


def sqlite_after(path):
    with sqlite3.connect(f"file:{path}?mode=ro", uri=True) as db:
        return {
            "page_size": db.execute("PRAGMA page_size").fetchone()[0],
            "page_count": db.execute("PRAGMA page_count").fetchone()[0],
            "pack_rows": db.execute("SELECT count(*) FROM object_packs").fetchone()[0],
            "pack_blob_bytes": db.execute("SELECT coalesce(sum(length(data)),0) FROM object_packs").fetchone()[0],
            "object_rows": db.execute("SELECT count(*) FROM objects").fetchone()[0],
            "file_bytes": path.stat().st_size,
        }


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--master", type=Path, required=True)
    parser.add_argument("--cold-driver", type=Path, required=True)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    if args.binary.resolve().parent.name != "release":
        raise ValueError("v0.1.6 reference binary must be a Cargo release build")
    args.out.mkdir(parents=True)  # Fresh output only; no receipt is overwritten.
    manifest = args.master / "manifest.tsv"
    if sha256(manifest) != MANIFEST_SHA256:
        raise ValueError("shared fixture manifest identity")
    cold = load_cold_driver(args.cold_driver)
    copy = cold.independent_byte_copy(args.master / "payload", args.out / "source" / "payload")
    write_json(args.out / "source-copy.json", copy)
    seed = hashlib.sha256(b"issue237-v016-seed-v1\0" + bytes.fromhex(MANIFEST_SHA256)).hexdigest()
    nonce = seed[:16]
    env = os.environ.copy()
    env["LAYERFS_INITIALIZATION_DIAGNOSTIC_NONCE"] = nonce
    env["LAYERFS_BENCH_INITIALIZATION_SEED_HEX"] = seed
    store_parent = args.out / "store"
    command = [str(args.binary.resolve()), "namespace-init-diagnostic-core-fixture",
               str(store_parent), copy["source"], "namespace-100000", "1", MANIFEST_SHA256, CACHE_PROFILE]
    identity = {"tag_commit": "44cf748486863ab7c21ca47e731bd88e2b9a7b4a",
                "binary_sha256": sha256(args.binary), "harness_sha256": sha256(Path(__file__)),
                "cold_driver_sha256": sha256(args.cold_driver), "manifest_sha256": MANIFEST_SHA256,
                "seed_hex": seed, "diagnostic_nonce": nonce, "command": command,
                "verification": "SKIPPED", "cache_profile": CACHE_PROFILE}
    write_json(args.out / "identity.json", identity)
    started_ns = time.monotonic_ns()
    process = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                               stderr=subprocess.PIPE, text=True, bufsize=1, env=env)
    before_stderr = []
    cold_status = "NOT_RUN"
    recheck_wall_ns = None
    try:
        before_stderr = wait_ready(process)
        result = cold.cold_source(Path(copy["source"]), manifest)
        write_json(args.out / "cold-preflight.json", result)
        if (result["status"] != "VERIFIED_COLD" or result["files"] != 100_000
                or result["directories"] != 1_001 or result["bytes"] != 500_000_000):
            raise RuntimeError("source cold preflight failed")
        recheck = cold.recheck_source(Path(copy["source"]), manifest)
        recheck_wall_ns = time.time_ns()
        write_json(args.out / "cold-recheck.json", recheck)
        if (recheck["status"] != "VERIFIED_COLD" or recheck["files"] != 100_000
                or recheck["bytes"] != 500_000_000):
            raise RuntimeError("source cold recheck failed")
        if time.monotonic_ns() - recheck["finished_ns"] > cold.cold.MAX_LAUNCH_GAP_NS:
            raise RuntimeError("source cold recheck aged before GO")
        process.stdin.write("GO\n")
        process.stdin.flush()
        cold_status = "VERIFIED_COLD"
    except Exception as error:
        cold_status = "INELIGIBLE"
        write_json(args.out / "blocked.json", {"status": cold_status, "error": repr(error)})
    finally:
        process.stdin.close()
        process.stdin = None
    stdout, stderr = process.communicate(timeout=60)
    complete_wall_ns = time.monotonic_ns() - started_ns
    stderr = "".join(before_stderr) + stderr
    (args.out / "stdout.txt").write_text(stdout)
    (args.out / "stderr.txt").write_text(stderr)
    match = re.search(r"issue237-reference-t0-v1 unix_ns=(\d+)", stderr)
    gap_ns = int(match.group(1)) - recheck_wall_ns if match and recheck_wall_ns else None
    if gap_ns is None or not 0 <= gap_ns <= cold.cold.MAX_LAUNCH_GAP_NS:
        cold_status = "INELIGIBLE"
    store = store_parent / "store.sqlite"
    ids = re.search(r"issue237-reference-identities-v1 layer_id=([0-9a-f]{66}) root_id=([0-9a-f]{64})", stderr)
    readback = args.out / "readback"
    readback_error = None
    if process.returncode == 0 and ids:
        try:
            readback.mkdir()
            shutil.copyfile(store, readback / "store.sqlite")
            (readback / "layer-id").write_text(ids.group(1))
            (readback / "root-id").write_text(ids.group(2))
        except Exception as error:
            readback_error = repr(error)
    status = ("INELIGIBLE" if cold_status != "VERIFIED_COLD" else
              "DIAGNOSTIC" if process.returncode == 0 and ids and readback_error is None else "INCOMPLETE")
    receipt = {"status": status,
               "exit_code": process.returncode, "cold_status": cold_status,
               "recheck_to_t0_ns": gap_ns, "complete_wall_ns": complete_wall_ns,
               "complete_wall_scope": "process-spawn-through-exit-including-cold-preflight",
               "public_call_count": 1 if match else 0,
               "genesis_layer_id": ids.group(1) if ids else None,
               "root_id": ids.group(2) if ids else None,
               "readback_root": str(readback.resolve()) if process.returncode == 0 and readback_error is None and ids else None,
               "readback_error": readback_error,
               "sqlite": sqlite_after(store) if process.returncode == 0 and store.exists() else None,
               "result": json.loads(stdout) if process.returncode == 0 else None}
    write_json(args.out / "receipt.json", receipt)
    print(json.dumps({"status": receipt["status"], "output": str(args.out),
                      "init_ns": receipt["result"]["layerstack_init_ns"] if receipt["result"] else None,
                      "recheck_to_t0_ns": gap_ns}))


if __name__ == "__main__":
    main()
