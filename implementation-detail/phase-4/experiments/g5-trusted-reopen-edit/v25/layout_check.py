#!/usr/bin/env python3
import importlib.util
import json
import pathlib
import shutil
import time

HERE = pathlib.Path(__file__).resolve().parent
REPO = HERE.parents[4]
V24 = HERE.parent / "v24"
RESULT = REPO / "target/phase4-g5-trusted-reopen-edit-20260823-v25-page-precondition-final-check"
BINARY = HERE / "g5-benchmark/target/release/layerfs-g5-trusted-child-v25"

spec = importlib.util.spec_from_file_location("g5_v24_runner", V24 / "runner.py")
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)


def main():
    if RESULT.exists():
        raise RuntimeError("layout check already exists")
    started = time.monotonic_ns()
    runner.VERIFIED_INPUT_CUSTODY = runner.input_manifest_index()
    runner.VERIFIED_INPUT_MANIFEST_SHA256 = runner.sha256(runner.INPUT_MANIFEST)
    runner.verify_sealed_input_manifest()
    dry = json.loads((V24 / "DRY-RUN-v24.json").read_text(encoding="utf-8"))
    master = runner.INPUT_ROOT / "bases/first-edit-after-reopen-104857600"
    custody = runner.manifest_master_custody(master)
    RESULT.mkdir(mode=0o700)
    (RESULT / "children-v24").mkdir()
    (RESULT / "time-v24").mkdir()
    root = RESULT.parent / f"{RESULT.name}-work"
    runner.clone_master_attested(master, root)
    release_hash = runner.sha256(BINARY)
    child = runner.PersistentChild(
        BINARY, "verified", 104857600, "first-edit-after-reopen", 1,
        RESULT, custody, dry["full_wrapper_forecast_ns"], release_hash,
        label="page-preconditioned-overlay",
    )
    try:
        row = child.request({
            "id": "page-preconditioned-overlay-request-1",
            "root": str(root),
            "iteration": 0,
            "warmup": "false",
            "validation": "complete-roundtrip",
        })
        terminal = child.close()
    except BaseException:
        child.abort()
        raise
    product = row["product"]
    authority = product["phase_counters"][0]
    shutil.rmtree(root)
    runner.fsync_dir(root.parent)
    result = {
        "schema": "phase4-g5-1-page-precondition-check-v25",
        "status": "PASS",
        "classification": "PremeasurementProductCandidateDiagnostic",
        "elapsed_ns": time.monotonic_ns() - started,
        "limit_ns": 20000000000,
        "source_sha256": runner.sha256(REPO / "crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs"),
        "release_sha256": release_hash,
        "authority_wall_ns": product["same_open_authority_establishment_wall_ns"],
        "authenticated_bytes": authority["canonical_bytes_authenticated"],
        "authenticated_objects": authority["objects_authenticated"],
        "sql_query_calls": authority["sql_query_calls"],
        "borrowed_row_blob_reads": authority["borrowed_row_blob_reads"],
        "borrowed_row_blob_bytes": authority["borrowed_row_blob_bytes"],
        "root_id": product["root_id"],
        "transition_id": product["transition_id"],
        "q_current": product["q_current"],
        "external_time": terminal["external_time"],
        "work_root_terminal_absent": not root.exists(),
    }
    runner.write_json(RESULT / "LAYOUT-CHECK-v25.json", result)
    print(json.dumps(result, sort_keys=True, separators=(",", ":")))


if __name__ == "__main__":
    main()
