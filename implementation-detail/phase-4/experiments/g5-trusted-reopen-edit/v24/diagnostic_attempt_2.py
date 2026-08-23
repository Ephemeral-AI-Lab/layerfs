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
RESULT = REPO / "target/phase4-g5-trusted-reopen-edit-20260823-v24-diagnostic-attempt-1"
WORK = RESULT.parent / f"{RESULT.name}-work-2"
WORKSPACE_BINARY = REPO / "target/release/phase4_create_edit_benchmark"

spec = importlib.util.spec_from_file_location("g5_v24_runner", HERE / "runner.py")
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)


def observation(label, product, external):
    authority = product["phase_counters"][0]
    return {
        "label": label,
        "authority_wall_ns": product["same_open_authority_establishment_wall_ns"],
        "authenticated_bytes": authority["canonical_bytes_authenticated"],
        "authenticated_objects": authority["objects_authenticated"],
        "sql_query_calls": authority["sql_query_calls"],
        "borrowed_row_blob_reads": authority["borrowed_row_blob_reads"],
        "borrowed_row_blob_bytes": authority["borrowed_row_blob_bytes"],
        "q_current": product["q_current"],
        "root_id": product["root_id"],
        "transition_id": product["transition_id"],
        "external_time": external,
    }


def main():
    output = RESULT / "DIAGNOSTIC-BUILD-CONTEXT-v24.json"
    if output.exists() or WORK.exists():
        raise RuntimeError("build-context diagnostic already exists")
    started = time.monotonic_ns()
    freeze = runner.verify_freeze(require_dry=True)
    dry = runner.verify_dry_run(freeze)
    master = runner.INPUT_ROOT / "bases/first-edit-after-reopen-104857600"
    custody = runner.manifest_master_custody(master)
    WORK.mkdir(mode=0o700)
    workspace_root = WORK / "workspace-release"
    overlay_root = WORK / "private-overlay"
    runner.clone_master_attested(master, workspace_root)
    runner.clone_master_attested(master, overlay_root)

    workspace_hash = runner.sha256(WORKSPACE_BINARY)
    workspace_time = RESULT / "time-v24/workspace-release.txt"
    workspace_command = [
        str(WORKSPACE_BINARY), "--fast-row", str(workspace_root), "104857600",
        "first-edit-after-reopen", "0", "false", "complete-roundtrip",
    ]
    workspace_environment_values = {
        **runner.PRODUCT_PROCESS_ENVIRONMENT,
        "LAYERFS_FAST_LANE": "1",
        "WP4M_EXECUTABLE_SHA256": workspace_hash,
        "WP4M_BASE_COPY_METHOD": "fast-lane-isolated-prepared-row",
        "WP4M_BASE_DATABASE_SHA256": custody["database_sha256"],
        "WP4M_BASE_AUTHORITY_SHA256": custody["authority_sha256"],
        "WP4M_BASE_EXPECTATIONS_SHA256": custody["expectations_sha256"],
    }
    workspace_environment = os.environ.copy()
    workspace_environment.update(workspace_environment_values)
    completed = subprocess.run(
        ["/usr/bin/time", "-l", "-o", str(workspace_time), *workspace_command],
        cwd=REPO,
        text=True,
        capture_output=True,
        env=workspace_environment,
    )
    if completed.returncode != 0:
        raise RuntimeError(completed.stderr.strip())
    workspace_product = json.loads(completed.stdout.strip())
    workspace_external = runner.parse_time(workspace_time)

    overlay = runner.PersistentChild(
        runner.G5_CHILD_BINARY,
        "verified",
        104857600,
        "first-edit-after-reopen",
        1,
        RESULT,
        custody,
        dry["full_wrapper_forecast_ns"],
        freeze["g5_executable_sha256"],
        label="build-context-private-overlay",
    )
    try:
        overlay_row = overlay.request({
            "id": "build-context-private-overlay-request-1",
            "root": str(overlay_root),
            "iteration": 0,
            "warmup": "false",
            "validation": "complete-roundtrip",
        })
        overlay_terminal = overlay.close()
    except BaseException:
        overlay.abort()
        raise

    shutil.rmtree(WORK)
    runner.fsync_dir(WORK.parent)
    result = {
        "schema": "phase4-g5-1-build-context-diagnostic-v24",
        "status": "PASS",
        "classification": "AppendOnlyDiagnosticNotThroughputAuthority",
        "elapsed_ns": time.monotonic_ns() - started,
        "limit_ns": 20000000000,
        "source_sha256": runner.sha256(REPO / "crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs"),
        "workspace_release_sha256": workspace_hash,
        "private_overlay_sha256": freeze["g5_executable_sha256"],
        "environment": runner.PRODUCT_PROCESS_ENVIRONMENT,
        "workspace_release": observation("workspace-release", workspace_product, workspace_external),
        "private_overlay": observation(
            "private-overlay", overlay_row["product"], overlay_terminal["external_time"]
        ),
        "workspace_command": workspace_command,
        "workspace_environment": workspace_environment_values,
        "work_root_terminal_absent": not WORK.exists(),
    }
    runner.write_json(output, result)
    print(json.dumps(result, sort_keys=True, separators=(",", ":")))


if __name__ == "__main__":
    main()
