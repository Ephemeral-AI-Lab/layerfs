#!/usr/bin/env python3
import hashlib
import importlib.util
import json
import time
from pathlib import Path

REPO = Path("/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty")
HERE = Path(__file__).resolve().parent
V8_RUNNER = HERE.parent / "v8/run_g4_v8.py"
EXPECTED_V8_RUNNER = "22e924e37ddba807917818acefeffe1c7feeec290b1ab64847c2d9e3dfa14de4"
if hashlib.sha256(V8_RUNNER.read_bytes()).hexdigest() != EXPECTED_V8_RUNNER:
    raise SystemExit("frozen v8 runner custody mismatch")
spec = importlib.util.spec_from_file_location("phase4_g4_frozen_v8_runner", V8_RUNNER)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
runner = module.runner
runner.HERE = HERE
runner.MANIFEST = HERE / "METHODOLOGY-MANIFEST-v9.tsv"
runner.TARGET = REPO / "target/phase4-g4-materialization-acceptance-20260822-v9"
runner.RESULTS = runner.TARGET / "results-v9"
runner.WORK = runner.TARGET / "work-v9"


def v9_write_json(path, value):
    path = Path(path)
    if path.name == "CLEANUP-v1.json":
        value["declared_deleted_root"] = runner.WORK.name
    if path.name == "MEASURED-TERMINAL-VERIFICATION-v1.json":
        value["lock_absent"] = False
        value["lock_held_through_terminal_verification_fsync"] = True
        module.module.original_write_json(path, value)
        if not module.module.ACTUAL_LOCK.exists():
            raise RuntimeError("global lock disappeared before terminal verification fsync")
        module.module.ACTUAL_LOCK.unlink()
        runner.fsync_dir(module.module.ACTUAL_LOCK.parent)
        module.module.original_write_json(
            runner.RESULTS / "LOCK-RELEASE-v9.json",
            {
                "schema": "phase4-g4-v9-lock-release-v1",
                "status": "PASS",
                "terminal_verification_sha256": runner.sha256(path),
                "lock_held_through_terminal_verification_fsync": True,
                "lock_absent_after_release": not module.module.ACTUAL_LOCK.exists(),
                "release_monotonic_ns": time.monotonic_ns(),
            },
        )
        return
    module.module.original_write_json(path, value)


runner.write_json = v9_write_json

if __name__ == "__main__":
    try:
        status = runner.main()
        wall = json.loads((runner.RESULTS / "COMPLETE-WALL-v1.json").read_text())
        overruns = [name for name, value in wall["buckets_ns"].items() if value > runner.BUCKET_LIMITS[name]]
        cleanup = json.loads((runner.RESULTS / "CLEANUP-v1.json").read_text())
        release = json.loads((runner.RESULTS / "LOCK-RELEASE-v9.json").read_text())
        if overruns or cleanup.get("declared_deleted_root") != "work-v9" or not release.get("lock_absent_after_release"):
            raise SystemExit(f"v9 wrapper closure mismatch: {overruns}")
        raise SystemExit(status)
    finally:
        if module.module.ACTUAL_LOCK.exists():
            module.module.ACTUAL_LOCK.unlink()
            runner.fsync_dir(module.module.ACTUAL_LOCK.parent)
