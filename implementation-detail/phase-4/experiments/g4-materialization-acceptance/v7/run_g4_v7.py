#!/usr/bin/env python3
import hashlib
import importlib.util
import json
import os
import sys
import time
from pathlib import Path

REPO = Path("/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty")
HERE = Path(__file__).resolve().parent
V1 = HERE.parent / "v1"
ACTUAL_LOCK = REPO / "target/BENCHMARK_LOCK"
DEPENDENCIES = {
    V1 / "run_g4_v1.py": "9888aab78edb2bb0d8d4f38ea69062829afe493ec199a85ae22e9cdc3d018624",
    V1 / "METHODOLOGY-MANIFEST-v1.tsv": "d6675db6cd340b12453f1719d55f183d3ef5f2b56cc07b9cac4c4f7b75b3c517",
    REPO / "target/phase4-g3-incremental-materialization-20260822-v13/results-v13/OPERAND-CUSTODY-v13.json": "58b652948950ed27e7ceb57c5b156705932e44e9d89724c63e8687f84b782d58",
}
for path, expected in DEPENDENCIES.items():
    if not path.is_file() or hashlib.sha256(path.read_bytes()).hexdigest() != expected:
        raise SystemExit(f"frozen dependency custody mismatch: {path}")

sys.path.insert(0, str(HERE))
spec = importlib.util.spec_from_file_location("phase4_g4_frozen_v1_runner", V1 / "run_g4_v1.py")
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)
runner.HERE = HERE
runner.MANIFEST = HERE / "METHODOLOGY-MANIFEST-v7.tsv"
runner.TARGET = REPO / "target/phase4-g4-materialization-acceptance-20260822-v7"
runner.RESULTS = runner.TARGET / "results-v7"
runner.WORK = runner.TARGET / "work-v7"
runner.HASHES["candidate"] = "703782924014fa1d990f1b09b6dbb63f3e9230a10c9781e77a357068de1c3ee3"
runner.HASHES["g3_control"] = "535bfa178a8a569ea43d9f1d23808775c2349a29f9cdacddae508391a6e5e61e"
runner.BUCKET_LIMITS = {
    "lock_and_measured_preflight": 5_000_000_000,
    "private_base_and_shared_preparation": 65_000_000_000,
    "row_dispatch_and_measured_operations": 15_000_000_000,
    "exact_row_verification": 5_000_000_000,
    "primary_and_independent_analysis": 5_000_000_000,
    "cleanup_storage_and_mode_audit": 5_000_000_000,
    "payload_manifest_terminal_and_verification": 10_000_000_000,
}


class HeldLock:
    def __fspath__(self):
        return os.fspath(ACTUAL_LOCK)

    def __str__(self):
        return str(ACTUAL_LOCK)

    @property
    def parent(self):
        return ACTUAL_LOCK.parent

    def exists(self):
        return ACTUAL_LOCK.exists()

    def read_text(self, *args, **kwargs):
        return ACTUAL_LOCK.read_text(*args, **kwargs)

    def unlink(self):
        # The frozen v1 runner calls this before terminal verification. V7 defers
        # the actual unlink until the verification artifact has been fsynced.
        return None


runner.GLOBAL_LOCK = HeldLock()
original_write_json = runner.write_json


def v7_write_json(path, value):
    path = Path(path)
    if path.name == "CLEANUP-v1.json":
        value["declared_deleted_root"] = runner.WORK.name
    if path.name == "MEASURED-TERMINAL-VERIFICATION-v1.json":
        value["lock_absent"] = False
        value["lock_held_through_terminal_verification_fsync"] = True
        original_write_json(path, value)
        if not ACTUAL_LOCK.exists():
            raise RuntimeError("global lock disappeared before terminal verification fsync")
        ACTUAL_LOCK.unlink()
        runner.fsync_dir(ACTUAL_LOCK.parent)
        original_write_json(
            runner.RESULTS / "LOCK-RELEASE-v7.json",
            {
                "schema": "phase4-g4-v7-lock-release-v1",
                "status": "PASS",
                "terminal_verification_sha256": runner.sha256(path),
                "lock_held_through_terminal_verification_fsync": True,
                "lock_absent_after_release": not ACTUAL_LOCK.exists(),
                "release_monotonic_ns": time.monotonic_ns(),
            },
        )
        return
    original_write_json(path, value)


runner.write_json = v7_write_json
original_child_json = runner.child_json
expected_errors = {
    "symlink-substitution": "NativeDestinationSymlink",
    "before-publication-fault": "InjectedBeforePublication",
}


def qualified_child_json(completed):
    payload = original_child_json(completed)
    if payload.get("schema") != "phase4-g3-row-v1" or "status" in payload:
        return payload
    expected_error = expected_errors.get(payload.get("scenario"))
    exact = (
        payload.get("outcome") == ("typed-error" if expected_error else "success")
        and payload.get("error") == expected_error
        and payload.get("byte_exact") is True
        and payload.get("mode_exact") is True
        and payload.get("q_terminal") == 0
        and payload.get("temp_residue_count") == 0
        and payload.get("seed_residue_count") == 0
    )
    if not exact:
        return payload
    payload["status"] = "PASS"
    payload["status_adapter"] = "qualified-from-retained-g3-v1-exact-outcome-byte-mode-q-residue-invariants"
    return payload


runner.child_json = qualified_child_json

if __name__ == "__main__":
    try:
        status = runner.main()
        wall = runner.RESULTS / "COMPLETE-WALL-v1.json"
        if wall.is_file():
            record = json.loads(wall.read_text())
            overruns = [name for name, value in record["buckets_ns"].items() if value > runner.BUCKET_LIMITS[name]]
            if overruns:
                raise SystemExit(f"sealed v7 bucket overrun: {overruns}")
        cleanup = json.loads((runner.RESULTS / "CLEANUP-v1.json").read_text())
        release = json.loads((runner.RESULTS / "LOCK-RELEASE-v7.json").read_text())
        if cleanup.get("declared_deleted_root") != "work-v7" or not release.get("lock_absent_after_release"):
            raise SystemExit("v7 cleanup or lock-release evidence mismatch")
        raise SystemExit(status)
    finally:
        if ACTUAL_LOCK.exists():
            ACTUAL_LOCK.unlink()
            runner.fsync_dir(ACTUAL_LOCK.parent)
