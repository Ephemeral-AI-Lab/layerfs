#!/usr/bin/env python3
"""One-shot 56-case #232 public SDK Exec/FUSE ioctl qualification."""

import fcntl
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time

from runner import CORE, RESULTS, ROOT, identities
from shared import edit_all_ioctl_v3 as contract, edit_cache, edit_insert_v4, edit_route
from phase1c import BUILD, cursor_key, digest, write

EVIDENCE = CORE / "docs/issues/232/evidence/phase2-all-ioctl"
BIN_DIR = CORE / "target/phase2-all-ioctl-host"
IMAGE_RECORD = CORE / "target/exec-fuse-all-ioctl-v3/image.json"
ROWS = contract.registry()["cases"]
TOP = contract.registry()


def prepare():
    identity = identities()
    assert not identity["source_dirty"], "commit all-ioctl source before preparation"
    build_folder = RESULTS / "phase2-all-ioctl-build"
    build_folder.mkdir(parents=True, exist_ok=True)
    with (build_folder / "build.log").open("wb") as log:
        built = subprocess.run(BUILD, cwd=ROOT, stdout=log, stderr=subprocess.STDOUT,
                               env={**os.environ, "CARGO_TARGET_DIR": str(BIN_DIR)})
    if built.returncode:
        raise RuntimeError(f"locked release host build failed; retained {build_folder}")
    binaries = {}
    for name in ("benchmark_edit", "benchmark_init", "verify_edit"):
        source = BIN_DIR / "release/examples" / name
        sha = digest(source)
        archive = RESULTS / "binary-archive" / sha / name
        archive.parent.mkdir(parents=True, exist_ok=True)
        if not archive.exists():
            shutil.copy2(source, archive)
            archive.chmod(0o555)
        assert digest(archive) == sha
        binaries[name] = {"path": str(archive), "sha256": sha}
    built = subprocess.run(["python3", "core/benchmark/fs-bench-pro/image/build.py",
                            "--all-ioctl"], cwd=ROOT, capture_output=True, text=True)
    (build_folder / "image.stdout").write_text(built.stdout)
    (build_folder / "image.stderr").write_text(built.stderr)
    if built.returncode:
        raise RuntimeError(f"locked release image build failed; retained {build_folder}")
    image = json.loads(IMAGE_RECORD.read_text())
    assert image["all_ioctl"] and image["profile"] == "release"
    assert image["registry_sha256"] == contract.REGISTRY_SHA256
    key = cursor_key()
    source_row = edit_insert_v4.registry()[0]
    masters = {}
    for size in sorted({row["fixture_bytes"] for row in ROWS}):
        master = edit_route.master(RESULTS / "prepared", size,
                                   {name: item["path"] for name, item in binaries.items()},
                                   identity, key, row=source_row)
        masters[str(size)] = {name: master[name] for name in
                              ("fixture_bytes", "fixture_sha256", "fixture_canonical_root",
                               "fixture_extent_count", "project_id", "genesis_layer",
                               "genesis_root", "genesis_root_serial", "store", "history",
                               "store_sha256", "history_sha256")}
    record = {"schema": "issue232-all-ioctl-pre-run-v3", "status": "PROSPECTIVE",
              "source_parent": identity["source_commit"],
              "execution_source_ref": "codex/issue232-all-ioctl-v3",
              "product_seal": identity["product_seal"],
              "harness_seal": identity["harness_seal"],
              "cargo_lock_sha256": identity["cargo_lock_sha256"],
              "workload_lock_sha256": digest(CORE / "benchmark/fs-bench-pro/workload/Cargo.lock"),
              "registry_sha256": contract.REGISTRY_SHA256,
              "v2_registry_sha256": digest(CORE / "benchmark/fs-bench-pro/registry/workspace-exec-edit-v2.json"),
              "binaries": binaries, "image": image, "masters": masters,
              "outputs": {row["scenario_id"]: str(RESULTS / "phase2-all-ioctl" / row["scenario_id"])
                          for row in ROWS},
              "cache_contract": TOP["cache_contract"],
              "construction_workers": 1,
              "complete_command_budget_ns": 15_000_000_000,
              "verifier_budget_ns": 10_000_000_000,
              "one_attempt_per_case": True,
              "numeric_targets": {row["scenario_id"]: row["raw_engineering_aim_ms"]
                                  for row in ROWS},
              "numeric_target_limit": "raw engineering aims only for four inserts; no cold latency PASS under uncheckable Linux backing cache"}
    assert all(not Path(path).exists() for path in record["outputs"].values())
    write(EVIDENCE / "PRE_RUN.json", record)
    print(EVIDENCE / "PRE_RUN.json")


def case_fields(row, master, run):
    fields = {
        "family_id": row["family_id"], "scenario_id": row["scenario_id"],
        "route": row["route"], "operation_contract_id": TOP["operation_contract_id"],
        "fixture_bytes": row["fixture_bytes"], "edit_start": row["edit_start"],
        "delete_len": row["delete_len"], "replacement_len": row["replacement_len"],
        "replacement_sha256": row["replacement_sha256"],
        "final_bytes": row["final_bytes"], "final_sha256": row["final_sha256"],
        "g2_target_ms": 0, "command": row["command"],
        "expected_accepted_logical_bytes": row["expected_accepted_logical_bytes"],
        "expected_accepted_literal_bytes": row["expected_accepted_literal_bytes"],
        "expected_ioctl_calls": row["expected_callback_total"] - 2,
        "project_id": master["project_id"], "genesis_layer": master["genesis_layer"],
        "genesis_root": master["genesis_root"],
        "genesis_root_serial": master["genesis_root_serial"],
        "stack_body": master["project_id"][2:34],
        "branch_body": hashlib.sha256(row["scenario_id"].encode()).hexdigest()[:32],
        "fixture_canonical_root": master["fixture_canonical_root"],
        "fixture_extent_count": master["fixture_extent_count"],
        "fixture_mode": 416, "expected_mode": 416,
        "full_file_digest": 1, "window_count": len(row["oracle"]["windows"]),
        "canonical_root_expected": row.get("canonical_root_expected", "-"),
        "canonical_count_expected": row.get("canonical_count_expected", "-"),
        "telemetry_run": run,
    }
    for index, window in enumerate(row["oracle"]["windows"]):
        for key in ("offset", "bytes", "sha256"):
            fields[f"window{index}_{key}"] = window[key]
    return fields


def caller_lft(stderr):
    records = [json.loads(line[5:]) for line in stderr.decode(errors="replace").splitlines()
               if line.startswith("LFT1 ")]
    roots = [item for item in records if item.get("namespace") == 7
             and item.get("role") == 1 and item.get("kind") == "operation"
             and item.get("key") == 232000]
    summaries = [item for item in records if item.get("kind") == "run-summary"
                 and item.get("namespace") == 7 and item.get("role") == 1]
    ok = (len(roots) == 1 and roots[0]["success"]
          and roots[0]["timing"]["name"] == "sdk.edit_commit.fuse"
          and [part["name"] for part in roots[0]["timing"]["children"]] == ["edit", "commit"]
          and len(summaries) == 1 and summaries[0]["dropped"] == 0
          and not summaries[0]["overflow"])
    return {"status": "PASS" if ok else "INCOMPLETE", "root": roots[0] if roots else None,
            "summaries": summaries}


def sample(scenario_id):
    pre = json.loads((EVIDENCE / "PRE_RUN.json").read_text())
    identity = identities()
    tag = subprocess.check_output(["git", "rev-parse", pre["execution_source_ref"]],
                                  cwd=ROOT, text=True).strip()
    assert not identity["source_dirty"] and identity["source_commit"] == tag
    assert identity["product_seal"] == pre["product_seal"]
    assert identity["harness_seal"] == pre["harness_seal"]
    assert digest(CORE / "benchmark/fs-bench-pro/registry/workspace-exec-edit-v3.json") == pre["registry_sha256"]
    assert all(digest(item["path"]) == item["sha256"] for item in pre["binaries"].values())
    image_id = subprocess.check_output(
        ["docker", "image", "inspect", pre["image"]["image_id"], "--format", "{{.Id}}"],
        text=True).strip()
    assert image_id == pre["image"]["image_id"]
    row = next(row for row in ROWS if row["scenario_id"] == scenario_id)
    folder = Path(pre["outputs"][scenario_id])
    with (RESULTS / ".run.lock").open("a+b") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        folder.mkdir(parents=True, exist_ok=False)
        try:
            _sample(folder, row, pre, identity)
        except Exception as error:
            write(folder / "attempt-error.json", {"status": "FAIL", "error": repr(error),
                                                  "scenario_id": scenario_id,
                                                  "source_commit": identity["source_commit"]})
            raise


def _sample(folder, row, pre, identity):
    master = pre["masters"][str(row["fixture_bytes"])]
    key = cursor_key()
    for name in ("store", "history"):
        assert digest(master[name]) == master[name + "_sha256"]
        shutil.copyfile(master[name], folder / f"{name}.sqlite")
        assert digest(folder / f"{name}.sqlite") == master[name + "_sha256"]
    run = int.from_bytes(os.urandom(16), "big") or 1
    fields = case_fields(row, master, run)
    case_path = folder / "case.txt"
    case_path.write_text("".join(f"{k}={v}\n" for k, v in sorted(fields.items())))
    store, history = folder / "store.sqlite", folder / "history.sqlite"
    cache = {"macos-store": edit_cache.qualify_store([store, history]),
             "linux-fuse-backing": edit_cache.qualify_fuse_backing()}
    command = [pre["binaries"]["benchmark_edit"]["path"],
               ";".join(f"{k}={v}" for k, v in sorted(fields.items())),
               str(store), str(history), pre["image"]["image_id"],
               "ioctl-" + hashlib.sha256(row["scenario_id"].encode()).hexdigest()[:20]]
    environment = {**os.environ, "LAYERFS_HISTORY_CURSOR_KEY": key,
                   "LAYERFS_CONSTRUCTION_WORKERS": "1"}
    started = time.monotonic_ns()
    try:
        result = subprocess.run(command, capture_output=True, timeout=15, env=environment)
        stdout, stderr, code, timeout = result.stdout, result.stderr, result.returncode, False
    except subprocess.TimeoutExpired as error:
        stdout, stderr, code, timeout = error.stdout or b"", error.stderr or b"", None, True
    wall = time.monotonic_ns() - started
    (folder / "driver.raw.stdout").write_bytes(stdout)
    (folder / "driver.raw.stderr").write_bytes(stderr)
    receipts = [line[8:] for line in stdout.splitlines() if line.startswith(b"RECEIPT\t")]
    driver = json.loads(receipts[-1]) if len(receipts) == 1 else None
    if driver:
        write(folder / "driver-receipt.json", driver)
        if driver.get("status") == "COMPLETE" and not timeout and code == 0:
            fields.update({"branch_id": driver["branch_id"],
                           "expected_head_commit": driver["head_commit"],
                           "expected_mtime_seconds": driver["observed_mtime_seconds"],
                           "expected_mtime_nanoseconds": driver["observed_mtime_nanoseconds"]})
            case_path.write_text("".join(f"{k}={v}\n" for k, v in sorted(fields.items())))
    verification = {"status": "NOT_RUN", "reason": "driver did not complete"}
    if driver and driver.get("status") == "COMPLETE" and code == 0 and not timeout:
        verify_command = [pre["binaries"]["verify_edit"]["path"], str(case_path),
                          str(store), str(history)]
        began = time.monotonic_ns()
        try:
            checked = subprocess.run(verify_command, capture_output=True, timeout=10,
                                     env=environment)
            vout, verr, vcode, vtimeout = (checked.stdout, checked.stderr,
                                          checked.returncode, False)
        except subprocess.TimeoutExpired as error:
            vout, verr, vcode, vtimeout = (error.stdout or b"", error.stderr or b"", None, True)
        (folder / "verifier.stdout").write_bytes(vout)
        (folder / "verifier.stderr").write_bytes(verr)
        verification = {"status": "PASS" if vcode == 0 and not vtimeout else "FAIL",
                        "wall_ns": time.monotonic_ns() - began, "exit_code": vcode,
                        "timeout": vtimeout, "command": verify_command,
                        "child": json.loads(vout) if vcode == 0 else None}
    write(folder / "verification.json", verification)
    callbacks = driver.get("projection_counts", "") if driver else ""
    expected = f"range_state=2,range_edit={row['expected_callback_total'] - 2}"
    route_ok = (driver is not None and expected in callbacks
                and driver.get("range_accepted_payload_bytes") == row["expected_accepted_literal_bytes"]
                and driver.get("range_shifted_suffix_bytes") == 0)
    telemetry = caller_lft(stderr)
    functional = (code == 0 and not timeout and wall <= 15_000_000_000
                  and driver and driver.get("status") == "COMPLETE" and route_ok
                  and driver.get("unmount_ok") and driver.get("sandbox_delete_ok")
                  and telemetry["status"] == "PASS"
                  and verification["status"] == "PASS" and verification["wall_ns"] < 10_000_000_000)
    status = ("INELIGIBLE" if functional and cache["macos-store"]["status"] == "PASS"
              and cache["linux-fuse-backing"]["status"] == "INELIGIBLE" else
              "INCOMPLETE" if functional or telemetry["status"] == "INCOMPLETE" and code == 0 else "FAIL")
    write(folder / "receipt.json", {
        "schema": "issue232-all-ioctl-receipt-v3", "status": status,
        "scenario_id": row["scenario_id"], "sample_count": 1,
        "source_commit": identity["source_commit"],
        "product_seal": identity["product_seal"],
        "harness_seal": identity["harness_seal"],
        "registry_sha256": contract.REGISTRY_SHA256,
        "image_id": pre["image"]["image_id"], "cache": cache,
        "clone_method": "independent-writable-byte-copy",
        "command": command, "driver_exit_code": code, "driver_timeout": timeout,
        "complete_command_wall_ns": wall,
        "complete_command_status": "PASS" if wall <= 15_000_000_000 and code == 0 else "FAIL",
        "driver": driver, "route_ok": route_ok, "telemetry": telemetry,
        "verification": verification,
        "raw_engineering_aim_ms": row["raw_engineering_aim_ms"],
        "admission_eligible": False,
    })
    print(row["scenario_id"], status, wall, verification["status"])


if __name__ == "__main__":
    if sys.argv[1:] == ["prepare"]:
        prepare()
    elif len(sys.argv) == 3 and sys.argv[1] == "run":
        sample(sys.argv[2])
    else:
        raise SystemExit("usage: phase2_all_ioctl.py prepare | run <registered-scenario-id>")
