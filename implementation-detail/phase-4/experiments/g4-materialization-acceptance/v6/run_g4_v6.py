#!/usr/bin/env python3
import hashlib
import importlib.util
import sys
from pathlib import Path

REPO = Path("/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty")
HERE = Path(__file__).resolve().parent
V1 = HERE.parent / "v1"
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
runner.MANIFEST = HERE / "METHODOLOGY-MANIFEST-v6.tsv"
runner.TARGET = REPO / "target/phase4-g4-materialization-acceptance-20260822-v6"
runner.RESULTS = runner.TARGET / "results-v6"
runner.WORK = runner.TARGET / "work-v6"
runner.HASHES["candidate"] = "3713f83aca0147f8e4f350f6464240a78f7ae3a14b0b5ef4edcf66007738d14d"
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
    status = runner.main()
    wall = runner.RESULTS / "COMPLETE-WALL-v1.json"
    if wall.is_file():
        import json
        record = json.loads(wall.read_text())
        overruns = [name for name, value in record["buckets_ns"].items() if value > runner.BUCKET_LIMITS[name]]
        if overruns:
            raise SystemExit(f"sealed v6 bucket overrun: {overruns}")
    raise SystemExit(status)
