#!/usr/bin/env python3
import importlib.util
import json
import pathlib
import shutil
import time

HERE = pathlib.Path(__file__).resolve().parent
REPO = HERE.parents[4]
RESULT = REPO / "target/phase4-g5-trusted-reopen-edit-20260823-v24-precondition-ab"
BUFFER_BYTES = 64 * 1024

spec = importlib.util.spec_from_file_location("g5_v24_runner", HERE / "runner.py")
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)


def precondition(root):
    databases = sorted(root.glob("*.sqlite"))
    if len(databases) != 1:
        raise RuntimeError("expected one candidate database")
    database = databases[0]
    expected = database.stat().st_size
    buffer = bytearray(BUFFER_BYTES)
    view = memoryview(buffer)
    observed = 0
    started = time.monotonic_ns()
    with database.open("rb", buffering=0) as source:
        while count := source.readinto(view):
            observed += count
    wall = time.monotonic_ns() - started
    if observed != expected:
        raise RuntimeError("precondition byte count mismatch")
    return {"database": database.name, "bytes": observed, "buffer_bytes": BUFFER_BYTES, "wall_ns": wall}


def run(label, root, custody, dry, release_hash, receipt=None):
    child = runner.PersistentChild(
        runner.G5_CHILD_BINARY, "verified", 104857600, "first-edit-after-reopen", 1,
        RESULT, custody, dry["full_wrapper_forecast_ns"], release_hash, label=label,
    )
    try:
        row = child.request({
            "id": f"{label}-request-1", "root": str(root), "iteration": 0,
            "warmup": "false", "validation": "complete-roundtrip",
        })
        terminal = child.close()
    except BaseException:
        child.abort()
        raise
    product = row["product"]
    authority = product["phase_counters"][0]
    return {
        "precondition": receipt,
        "authority_wall_ns": product["same_open_authority_establishment_wall_ns"],
        "authenticated_bytes": authority["canonical_bytes_authenticated"],
        "authenticated_objects": authority["objects_authenticated"],
        "sql_query_calls": authority["sql_query_calls"],
        "borrowed_row_blob_reads": authority["borrowed_row_blob_reads"],
        "borrowed_row_blob_bytes": authority["borrowed_row_blob_bytes"],
        "root_id": product["root_id"], "transition_id": product["transition_id"],
        "q_current": product["q_current"], "external_time": terminal["external_time"],
    }


def main():
    if RESULT.exists():
        raise RuntimeError("precondition A/B already exists")
    started = time.monotonic_ns()
    freeze = runner.verify_freeze(require_dry=True)
    dry = runner.verify_dry_run(freeze)
    master = runner.INPUT_ROOT / "bases/first-edit-after-reopen-104857600"
    custody = runner.manifest_master_custody(master)
    RESULT.mkdir(mode=0o700)
    (RESULT / "children-v24").mkdir()
    (RESULT / "time-v24").mkdir()
    cold_root = RESULT.parent / f"{RESULT.name}-cold"
    warm_root = RESULT.parent / f"{RESULT.name}-preconditioned"
    runner.clone_master_attested(master, cold_root)
    runner.clone_master_attested(master, warm_root)
    release_hash = runner.sha256(runner.G5_CHILD_BINARY)
    cold = run("cold", cold_root, custody, dry, release_hash)
    receipt = precondition(warm_root)
    warm = run("preconditioned", warm_root, custody, dry, release_hash, receipt)
    shutil.rmtree(cold_root)
    shutil.rmtree(warm_root)
    runner.fsync_dir(RESULT.parent)
    result = {
        "schema": "phase4-g5-1-fixed-buffer-precondition-ab-v24",
        "status": "PASS",
        "classification": "AppendOnlyDiagnosticNotThroughputAuthority",
        "elapsed_ns": time.monotonic_ns() - started,
        "limit_ns": 20000000000,
        "release_sha256": release_hash,
        "source_sha256": runner.sha256(REPO / "crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs"),
        "cold": cold,
        "preconditioned": warm,
        "product_counters_identical": all(cold[key] == warm[key] for key in (
            "authenticated_bytes", "authenticated_objects", "sql_query_calls",
            "borrowed_row_blob_reads", "borrowed_row_blob_bytes", "root_id", "transition_id",
        )),
        "work_roots_terminal_absent": not cold_root.exists() and not warm_root.exists(),
    }
    runner.write_json(RESULT / "PRECONDITION-AB-v24.json", result)
    print(json.dumps(result, sort_keys=True, separators=(",", ":")))


if __name__ == "__main__":
    main()
