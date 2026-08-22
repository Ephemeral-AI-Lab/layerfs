#!/usr/bin/env python3
import hashlib
import importlib.util
import json
import sys
import time
from pathlib import Path

REPO = Path("/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty")
HERE = Path(__file__).resolve().parent
V7_RUNNER = HERE.parent / "v7/run_g4_v7.py"
EXPECTED_V7_RUNNER = "d3b6f7361cdaa549f5d6ad332fd93c768663c7375cb9c2cb7d6156faf922bbc7"
if hashlib.sha256(V7_RUNNER.read_bytes()).hexdigest() != EXPECTED_V7_RUNNER:
    raise SystemExit("frozen v7 runner custody mismatch")
spec = importlib.util.spec_from_file_location("phase4_g4_frozen_v7_runner", V7_RUNNER)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
runner = module.runner
runner.HERE = HERE
runner.MANIFEST = HERE / "METHODOLOGY-MANIFEST-v8.tsv"
runner.TARGET = REPO / "target/phase4-g4-materialization-acceptance-20260822-v8"
runner.RESULTS = runner.TARGET / "results-v8"
runner.WORK = runner.TARGET / "work-v8"
runner.HASHES["candidate"] = "c60a19cb3cecb83bb801ba9c36835297e6fc503d736171213ec78e69bd5d6d76"
runner.BUCKET_LIMITS = {
    "lock_and_measured_preflight": 5_000_000_000,
    "private_base_and_shared_preparation": 65_000_000_000,
    "row_dispatch_and_measured_operations": 15_000_000_000,
    "exact_row_verification": 5_000_000_000,
    "primary_and_independent_analysis": 5_000_000_000,
    "cleanup_storage_and_mode_audit": 5_000_000_000,
    "payload_manifest_terminal_and_verification": 10_000_000_000,
}
runner.GLOBAL_LOCK = module.HeldLock()


def v8_write_json(path, value):
    path = Path(path)
    if path.name == "CLEANUP-v1.json":
        value["declared_deleted_root"] = runner.WORK.name
    if path.name == "MEASURED-TERMINAL-VERIFICATION-v1.json":
        value["lock_absent"] = False
        value["lock_held_through_terminal_verification_fsync"] = True
        module.original_write_json(path, value)
        if not module.ACTUAL_LOCK.exists():
            raise RuntimeError("global lock disappeared before terminal verification fsync")
        module.ACTUAL_LOCK.unlink()
        runner.fsync_dir(module.ACTUAL_LOCK.parent)
        module.original_write_json(
            runner.RESULTS / "LOCK-RELEASE-v8.json",
            {
                "schema": "phase4-g4-v8-lock-release-v1",
                "status": "PASS",
                "terminal_verification_sha256": runner.sha256(path),
                "lock_held_through_terminal_verification_fsync": True,
                "lock_absent_after_release": not module.ACTUAL_LOCK.exists(),
                "release_monotonic_ns": time.monotonic_ns(),
            },
        )
        return
    module.original_write_json(path, value)


runner.write_json = v8_write_json

if __name__ == "__main__":
    try:
        status = runner.main()
        wall = json.loads((runner.RESULTS / "COMPLETE-WALL-v1.json").read_text())
        overruns = [name for name, value in wall["buckets_ns"].items() if value > runner.BUCKET_LIMITS[name]]
        cleanup = json.loads((runner.RESULTS / "CLEANUP-v1.json").read_text())
        release = json.loads((runner.RESULTS / "LOCK-RELEASE-v8.json").read_text())
        if overruns or cleanup.get("declared_deleted_root") != "work-v8" or not release.get("lock_absent_after_release"):
            raise SystemExit(f"v8 wrapper closure mismatch: {overruns}")
        raise SystemExit(status)
    finally:
        if module.ACTUAL_LOCK.exists():
            module.ACTUAL_LOCK.unlink()
            runner.fsync_dir(module.ACTUAL_LOCK.parent)
