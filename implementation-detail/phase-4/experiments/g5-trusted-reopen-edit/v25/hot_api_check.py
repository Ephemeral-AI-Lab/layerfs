#!/usr/bin/env python3
import importlib.util
import json
import os
import pathlib
import shutil
import subprocess
import time

HERE = pathlib.Path(__file__).resolve().parent
REPO = HERE.parents[4]
V24 = HERE.parent / "v24"
RESULT = REPO / "target/phase4-g5-trusted-reopen-edit-20260823-v25-hot-api-check-attempt-2"
BINARY = REPO / "target/release/phase4_create_edit_benchmark"

spec = importlib.util.spec_from_file_location("g5_v24_runner", V24 / "runner.py")
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)


def run(mode, root, custody, release_hash):
    sidecar = RESULT / f"{mode}.time"
    command = [
        str(BINARY), "--g5-preverified-row", mode, str(root), "104857600",
        "first-edit-after-reopen", "0", "false", "complete-roundtrip",
        release_hash, custody["database_sha256"], custody["authority_sha256"],
        custody["expectations_sha256"],
    ]
    environment = os.environ.copy()
    environment.update(runner.PRODUCT_PROCESS_ENVIRONMENT)
    completed = subprocess.run(
        ["/usr/bin/time", "-l", "-o", str(sidecar), *command],
        cwd=REPO,
        text=True,
        capture_output=True,
        env=environment,
    )
    if completed.returncode != 0:
        raise RuntimeError(completed.stderr.strip())
    product = json.loads(completed.stdout.strip())
    authority = product["phase_counters"][0]
    return {
        "mode": mode,
        "authority_wall_ns": product["same_open_authority_establishment_wall_ns"],
        "decision_ns": sum((
            product["store_preflight_wall_ns"],
            product["sqlite_open_and_profile_wall_ns"],
            product["visible_head_lookup_and_open_wrapper_wall_ns"],
            product["same_open_authority_establishment_wall_ns"],
            product["durable_capture_total_wall_ns"],
        )),
        "authenticated_bytes": authority["canonical_bytes_authenticated"],
        "authenticated_objects": authority["objects_authenticated"],
        "sql_query_calls": authority["sql_query_calls"],
        "borrowed_row_blob_reads": authority["borrowed_row_blob_reads"],
        "borrowed_row_blob_bytes": authority["borrowed_row_blob_bytes"],
        "root_id": product["root_id"],
        "transition_id": product["transition_id"],
        "transactions": product["transactions"],
        "commits": product["commits"],
        "q_current": product["q_current"],
        "external_time": runner.parse_time(sidecar),
        "command": command,
    }


def main():
    if RESULT.exists():
        raise RuntimeError("hot API check already exists")
    started = time.monotonic_ns()
    runner.VERIFIED_INPUT_CUSTODY = runner.input_manifest_index()
    runner.VERIFIED_INPUT_MANIFEST_SHA256 = runner.sha256(runner.INPUT_MANIFEST)
    runner.verify_sealed_input_manifest()
    master = runner.INPUT_ROOT / "bases/first-edit-after-reopen-104857600"
    custody = runner.manifest_master_custody(master)
    release_hash = runner.sha256(BINARY)
    RESULT.mkdir(mode=0o700)
    roots = {mode: RESULT.parent / f"{RESULT.name}-{mode}" for mode in ("verified", "trusted-local-dev")}
    for root in roots.values():
        runner.clone_master_attested(master, root)
    rows = {mode: run(mode, root, custody, release_hash) for mode, root in roots.items()}
    for root in roots.values():
        shutil.rmtree(root)
    runner.fsync_dir(RESULT.parent)
    verified = rows["verified"]["decision_ns"]
    trusted = rows["trusted-local-dev"]["decision_ns"]
    result = {
        "schema": "phase4-g5-1-workspace-hot-api-check-v25",
        "status": "PASS",
        "classification": "PremeasurementProductCandidateDiagnostic",
        "elapsed_ns": time.monotonic_ns() - started,
        "limit_ns": 20000000000,
        "source_sha256": runner.sha256(REPO / "crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs"),
        "release_sha256": release_hash,
        "rows": rows,
        "trusted_improvement_basis_points": ((verified - trusted) * 10000) // verified,
        "work_roots_terminal_absent": all(not root.exists() for root in roots.values()),
    }
    runner.write_json(RESULT / "HOT-API-CHECK-v25.json", result)
    print(json.dumps(result, sort_keys=True, separators=(",", ":")))


if __name__ == "__main__":
    main()
