#!/usr/bin/env python3
import hashlib
import importlib.util
import json
import os
import subprocess
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
runner.MANIFEST = HERE / "METHODOLOGY-MANIFEST-v10.tsv"
runner.TARGET = REPO / "target/phase4-g4-materialization-acceptance-20260822-v10"
runner.RESULTS = runner.TARGET / "results-v10"
runner.WORK = runner.TARGET / "work-v10"
runner.HASHES["candidate"] = "770dcfa8db17f1f9e1b90336a26923eb0530073590a9da5578e06339d85813e8"
runner.BUCKET_LIMITS = {
    "lock_and_measured_preflight": 5_000_000_000,
    "private_base_and_shared_preparation": 85_000_000_000,
    "row_dispatch_and_measured_operations": 40_000_000_000,
    "exact_row_verification": 5_000_000_000,
    "primary_and_independent_analysis": 5_000_000_000,
    "cleanup_storage_and_mode_audit": 5_000_000_000,
    "payload_manifest_terminal_and_verification": 10_000_000_000,
}

ESTIMATED_SEQUENCES = {8, 16, 17, 18, 19, 20, 22, 24, 25, 26, 27, 29, 30}
original_run_capture = runner.run_capture


def protected_ns(sequence, payload):
    if 16 <= sequence <= 27:
        return payload["operation_total_ns"]
    if sequence == 8:
        return payload["range_measurements"][0]["wall_ns"]
    if sequence == 30:
        return payload["fresh_reopen_head_wall_ns"]
    return payload["durable_capture_total_wall_ns"]


def set_protected_ns(sequence, payload, value):
    if 16 <= sequence <= 27:
        payload["operation_total_ns"] = value
    elif sequence == 8:
        payload["range_measurements"] = [dict(payload["range_measurements"][0], wall_ns=value)]
    elif sequence == 30:
        payload["fresh_reopen_head_wall_ns"] = value
    else:
        payload["durable_capture_total_wall_ns"] = value


def aggregate_external(values):
    summed = lambda key: sum(value[key] for value in values)
    switches = lambda key: summed(key) if all(isinstance(value[key], int) for value in values) else "Unavailable"
    return {
        "external_real_seconds": summed("external_real_seconds"),
        "external_user_seconds": summed("external_user_seconds"),
        "external_system_seconds": summed("external_system_seconds"),
        "maximum_resident_set_bytes": max(value["maximum_resident_set_bytes"] for value in values),
        "voluntary_context_switches": switches("voluntary_context_switches"),
        "involuntary_context_switches": switches("involuntary_context_switches"),
    }


def second_command(command, label, env):
    command = [str(item) for item in command]
    second = command.copy()
    second_env = dict(env) if env is not None else os.environ.copy()
    sequence = int(label[:2])
    if "--g3-row" in command:
        second[2] = f"{command[2]}-estimator-2"
        return second, second_env
    if "--fast-row" not in command:
        raise RuntimeError(f"unsupported v10 estimator command: {label}")
    iteration = int(command[5]) + 100_000
    second[5] = str(iteration)
    candidate = runner.RESULTS / "operands-v1/phase4_create_edit_benchmark-g4"
    prep_label = f"prepare-{sequence:02d}-{label}-estimator-2"
    original_run_capture(
        [candidate, "--fast-prepare", command[2], command[3], command[4], iteration],
        prep_label,
        runner.RESULTS / "preparation-v1",
        timed=False,
    )
    internal = {"read-range-1m": "read-range-1m", "write": "full", "edit-same": "same-middle", "reopen": "reopen"}
    database = Path(command[2]) / f"db-K64-F64-{command[3]}-{internal[command[4]]}-{iteration}.sqlite"
    second_env.update(
        {
            "WP4M_BASE_DATABASE_SHA256": runner.sha256(database),
            "WP4M_BASE_AUTHORITY_SHA256": runner.sha256(Path(f"{database}.authority")),
            "WP4M_BASE_EXPECTATIONS_SHA256": runner.sha256(Path(f"{database}.expectations")),
        }
    )
    return second, second_env


def v10_run_capture(command, label, directory, env=None, timed=True, allow_nonzero=False):
    sequence = int(label[:2]) if len(label) >= 2 and label[:2].isdigit() else None
    estimated = timed and sequence in ESTIMATED_SEQUENCES and ("--g3-row" in command or "--fast-row" in command)
    if not estimated:
        return original_run_capture(command, label, directory, env, timed, allow_nonzero)
    first_label = f"{label}-sample-1"
    second_label = f"{label}-sample-2"
    first, first_external = original_run_capture(command, first_label, directory, env, timed, allow_nonzero)
    second, second_env = second_command(command, label, env)
    second_result, second_external = original_run_capture(
        second, second_label, directory, second_env, timed, allow_nonzero
    )
    payloads = [runner.child_json(first), runner.child_json(second_result)]
    samples = [protected_ns(sequence, payload) for payload in payloads]
    aggregate = dict(payloads[0])
    set_protected_ns(sequence, aggregate, (sum(samples) + 1) // 2)
    sample_paths = [f"arm-raw-v1/{first_label}.stdout", f"arm-raw-v1/{second_label}.stdout"]
    aggregate["adjacent_estimator_v10"] = {
        "schema": "phase4-g4-adjacent-estimator-v10",
        "replications_per_role": 2,
        "estimator": "equal-weight-arithmetic-mean",
        "relative_limit_basis_points": 10_500,
        "samples_ns": samples,
        "sum_ns": sum(samples),
        "mean_ns_ceil": (sum(samples) + 1) // 2,
        "sample_payload_paths": sample_paths,
        "sample_payload_sha256": [runner.sha256(runner.RESULTS / path) for path in sample_paths],
        "sample_commands": [[str(item) for item in command], second],
        "sample_external": [first_external, second_external],
    }
    encoded = json.dumps(aggregate, separators=(",", ":"), sort_keys=True) + "\n"
    runner.write_text(Path(directory) / f"{label}.stdout", encoded)
    runner.write_text(
        Path(directory) / f"{label}.stderr",
        json.dumps({"schema": "phase4-g4-v10-aggregate-stderr", "samples": [first_label, second_label]}, sort_keys=True) + "\n",
    )
    completed = subprocess.CompletedProcess([str(item) for item in command], 0, stdout=encoded, stderr="")
    return completed, aggregate_external([first_external, second_external])


runner.run_capture = v10_run_capture


def v10_write_json(path, value):
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
            runner.RESULTS / "LOCK-RELEASE-v10.json",
            {
                "schema": "phase4-g4-v10-lock-release-v1",
                "status": value["status"],
                "terminal_verification_sha256": runner.sha256(path),
                "lock_held_through_terminal_verification_fsync": True,
                "lock_absent_after_release": not module.module.ACTUAL_LOCK.exists(),
                "release_monotonic_ns": time.monotonic_ns(),
            },
        )
        return
    module.module.original_write_json(path, value)


runner.write_json = v10_write_json

if __name__ == "__main__":
    try:
        status = runner.main()
        wall = json.loads((runner.RESULTS / "COMPLETE-WALL-v1.json").read_text())
        overruns = [name for name, value in wall["buckets_ns"].items() if value > runner.BUCKET_LIMITS[name]]
        cleanup = json.loads((runner.RESULTS / "CLEANUP-v1.json").read_text())
        release = json.loads((runner.RESULTS / "LOCK-RELEASE-v10.json").read_text())
        if overruns or cleanup.get("declared_deleted_root") != "work-v10" or not release.get("lock_absent_after_release"):
            raise SystemExit(f"v10 wrapper closure mismatch: {overruns}")
        raise SystemExit(status)
    finally:
        if module.module.ACTUAL_LOCK.exists():
            module.module.ACTUAL_LOCK.unlink()
            runner.fsync_dir(module.module.ACTUAL_LOCK.parent)
