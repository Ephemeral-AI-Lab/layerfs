#!/usr/bin/env python3
import importlib.util
import json
import pathlib
import shutil
import time

HERE = pathlib.Path(__file__).resolve().parent
REPO = HERE.parents[4]
RESULT = REPO / "target/phase4-g5-trusted-reopen-edit-20260823-v24-diagnostic-attempt-1"

spec = importlib.util.spec_from_file_location("g5_v24_runner", HERE / "runner.py")
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)


def clone(master, root):
    runner.clone_master_attested(master, root)
    return root


def observation(label, row):
    product = row["product"]
    return {
        "label": label,
        "authority_wall_ns": product["same_open_authority_establishment_wall_ns"],
        "authenticated_bytes": product["phase_counters"][0]["canonical_bytes_authenticated"],
        "authenticated_objects": product["phase_counters"][0]["objects_authenticated"],
        "sql_query_calls": product["phase_counters"][0]["sql_query_calls"],
        "borrowed_row_blob_reads": product["phase_counters"][0]["borrowed_row_blob_reads"],
        "borrowed_row_blob_bytes": product["phase_counters"][0]["borrowed_row_blob_bytes"],
        "q_current": product["q_current"],
        "transactions": product["transactions"],
        "commits": product["commits"],
        "root_id": product["root_id"],
        "transition_id": product["transition_id"],
    }


def run_child(label, expected_rows, roots, custody, forecast, release_hash):
    child = runner.PersistentChild(
        runner.G5_CHILD_BINARY,
        "verified",
        104857600,
        "first-edit-after-reopen",
        expected_rows,
        RESULT,
        custody,
        forecast,
        release_hash,
        label=label,
    )
    rows = []
    try:
        for index, root in enumerate(roots, 1):
            value = child.request({
                "id": f"{label}-request-{index}",
                "root": str(root),
                "iteration": 0,
                "warmup": "false",
                "validation": "complete-roundtrip",
            })
            rows.append(observation(f"{label}-request-{index}", value))
        terminal = child.close()
    except BaseException:
        child.abort()
        raise
    return rows, terminal


def main():
    if RESULT.exists():
        raise RuntimeError(f"diagnostic result already exists: {RESULT}")
    started = time.monotonic_ns()
    freeze = runner.verify_freeze(require_dry=True)
    dry = runner.verify_dry_run(freeze)
    master = runner.INPUT_ROOT / "bases/first-edit-after-reopen-104857600"
    custody = runner.manifest_master_custody(master)
    RESULT.mkdir(mode=0o700)
    (RESULT / "children-v24").mkdir()
    (RESULT / "time-v24").mkdir()
    work = RESULT.parent / f"{RESULT.name}-work"
    work.mkdir(mode=0o700)
    single_root = clone(master, work / "single-request")
    persistent_roots = [clone(master, work / f"persistent-request-{index}") for index in range(1, 6)]
    single_rows, single_terminal = run_child(
        "fresh-single-request",
        1,
        [single_root],
        custody,
        dry["full_wrapper_forecast_ns"],
        freeze["g5_executable_sha256"],
    )
    persistent_rows, persistent_terminal = run_child(
        "persistent-five-requests",
        5,
        persistent_roots,
        custody,
        dry["full_wrapper_forecast_ns"],
        freeze["g5_executable_sha256"],
    )
    shutil.rmtree(work)
    runner.fsync_dir(work.parent)
    result = {
        "schema": "phase4-g5-1-execution-context-diagnostic-v24",
        "status": "PASS",
        "classification": "AppendOnlyDiagnosticNotThroughputAuthority",
        "elapsed_ns": time.monotonic_ns() - started,
        "limit_ns": 20000000000,
        "source_freeze_sha256": runner.sha256(runner.SOURCE_FREEZE),
        "release_sha256": freeze["g5_executable_sha256"],
        "input_manifest_sha256": freeze["input_manifest_sha256"],
        "environment": runner.PRODUCT_PROCESS_ENVIRONMENT,
        "single_request": {"rows": single_rows, "terminal": single_terminal},
        "persistent_five_requests": {"rows": persistent_rows, "terminal": persistent_terminal},
        "post_request_release": "outside product row after report/request/Store owners drop and Q0",
        "work_root_terminal_absent": not work.exists(),
    }
    runner.write_json(RESULT / "DIAGNOSTIC-v24.json", result)
    print(json.dumps(result, sort_keys=True, separators=(",", ":")))


if __name__ == "__main__":
    main()
