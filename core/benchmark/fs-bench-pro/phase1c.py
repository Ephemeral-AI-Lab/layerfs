#!/usr/bin/env python3
"""One-shot public SDK count diagnostics for the frozen #232 Phase 1C screen."""

import fcntl
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time

from runner import CORE, RESULTS, ROOT, identities
from shared import edit_cache, edit_insert_v4, edit_route
from shared import edit_complexity_v1 as contract

EVIDENCE = CORE / "docs/issues/232/evidence/phase1c-complexity"
BIN_DIR = CORE / "target/phase1c-host"
IMAGE_RECORD = CORE / "target/exec-fuse-complexity-v1/image.json"
KEY_PATH = RESULTS / "phase1c-cursor-key.txt"
BUILD = ["cargo", "+1.85.1", "build", "--manifest-path", "core/Cargo.toml",
         "--locked", "--offline", "--release", "-p", "layerfs-sdk", "-p", "layerfs-server",
         "--example", "benchmark_edit", "--example", "benchmark_init",
         "--example", "verify_edit"]
SIZES = sorted({row["fixture_bytes"] for row in contract.rows()})


def digest(path):
    return edit_route.sha256(path)


def write(path, value):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n")


def cursor_key():
    if not KEY_PATH.exists():
        KEY_PATH.parent.mkdir(parents=True, exist_ok=True)
        KEY_PATH.write_text(os.urandom(32).hex() + "\n")
        KEY_PATH.chmod(0o600)
    assert KEY_PATH.stat().st_mode & 0o077 == 0
    key = KEY_PATH.read_text().strip()
    assert len(key) == 64
    return key


def cutoff_master(size, binary, key):
    base = RESULTS / "prepared/phase1c"
    fixture = base / f"fixture-{size}" / "payload.bin"
    fixture.parent.mkdir(parents=True, exist_ok=True)
    if not fixture.exists():
        fixture.write_bytes(contract.base.stream_bytes(
            contract.base.FIXTURE_GENERATOR_SEED, 0, size))
        fixture.chmod(0o640)
    assert fixture.stat().st_size == size
    assert digest(fixture) == next(r["fixture_sha256"] for r in contract.rows()
                                   if r["fixture_bytes"] == size)
    folder = base / f"master-{size}-{digest(binary)[:16]}"
    if folder.exists():
        record = json.loads((folder / "master.json").read_text())
        assert all(digest(folder / name) == record[name.split('.')[0] + "_sha256"]
                   for name in ("store.sqlite", "history.sqlite"))
        return record
    folder.mkdir()
    store, history = folder / "store.sqlite", folder / "history.sqlite"
    command = [str(binary), str(fixture.parent), str(store), str(history),
               f"phase1c-{size}"]
    result = subprocess.run(command, capture_output=True, text=True, timeout=60,
                            env={**os.environ, "LAYERFS_HISTORY_CURSOR_KEY": key})
    (folder / "init.stdout").write_text(result.stdout)
    (folder / "init.stderr").write_text(result.stderr)
    if result.returncode != 0:
        raise RuntimeError(f"cutoff master Init failed; retained at {folder}")
    init = json.loads(result.stdout.strip().splitlines()[-1])
    assert init["status"] == "COMPLETE"
    record = {"fixture_bytes": size, "fixture_sha256": digest(fixture),
              "fixture_canonical_root": "-", "fixture_extent_count": "-",
              "project_id": init["project_id"], "genesis_layer": init["genesis_layer"],
              "genesis_root": init["root"], "genesis_root_serial": init["root_serial"],
              "store": str(store), "history": str(history),
              "store_sha256": digest(store), "history_sha256": digest(history),
              "init_binary_sha256": digest(binary), "init_receipt": init,
              "clone_method": "independent-writable-byte-copy"}
    write(folder / "master.json", record)
    return record


def prepare():
    contract.check()
    identity = identities()
    assert not identity["source_dirty"], "commit diagnostic source before preparation"
    build_folder = RESULTS / "phase1c-build"
    build_folder.mkdir(parents=True, exist_ok=True)
    with (build_folder / "build.log").open("wb") as log:
        result = subprocess.run(BUILD, cwd=ROOT, stdout=log, stderr=subprocess.STDOUT,
                                env={**os.environ, "CARGO_TARGET_DIR": str(BIN_DIR)})
    if result.returncode:
        raise RuntimeError(f"host release build failed; retained {build_folder / 'build.log'}")
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
    image_build = subprocess.run(
        ["python3", "core/benchmark/fs-bench-pro/image/build.py",
         "--scenario-version", "4", "--complexity-diagnostic"],
        cwd=ROOT, capture_output=True, text=True)
    (build_folder / "image.stdout").write_text(image_build.stdout)
    (build_folder / "image.stderr").write_text(image_build.stderr)
    if image_build.returncode:
        raise RuntimeError(f"image build failed; retained {build_folder}")
    image = json.loads(IMAGE_RECORD.read_text())
    assert image["complexity_diagnostic"] and image["profile"] == "release"
    assert image["payloads"][contract.PAYLOAD_NAME]["sha256"] == contract.PAYLOAD_SHA256
    key = cursor_key()
    masters = {}
    source_row = edit_insert_v4.registry()[0]
    for size in SIZES:
        if size in contract.base.FIXTURE_SHA256:
            master = edit_route.master(RESULTS / "prepared", size,
                                       {name: item["path"] for name, item in binaries.items()},
                                       identity, key, row=source_row)
        else:
            master = cutoff_master(size, Path(binaries["benchmark_init"]["path"]), key)
        masters[str(size)] = {name: master[name] for name in
                              ("fixture_bytes", "fixture_sha256", "fixture_canonical_root",
                               "fixture_extent_count", "project_id", "genesis_layer",
                               "genesis_root", "genesis_root_serial", "store", "history",
                               "store_sha256", "history_sha256")}
    record = {"schema": "issue232-phase1c-pre-run-v1", "status": "PROSPECTIVE",
              "source_parent": identity["source_commit"],
              "execution_source_ref": "codex/issue232-phase1c-diagnostic-v1",
              "product_seal": identity["product_seal"],
              "harness_seal": identity["harness_seal"],
              "cargo_lock_sha256": identity["cargo_lock_sha256"],
              "registry_sha256": contract.REGISTRY_SHA256,
              "payload_sha256": contract.PAYLOAD_SHA256,
              "cursor_key_sha256": hashlib.sha256(bytes.fromhex(key)).hexdigest(),
              "binaries": binaries, "image": image, "masters": masters,
              "outputs": {row["label"]: str(RESULTS / f"phase1c-{row['label']}-01")
                          for row in contract.rows()},
              "cache_contract": "macOS Store mincore; Linux FUSE backing INELIGIBLE",
              "cutoff_bytes": 131072, "construction_workers": 1,
              "complete_command_budget_ns": 15_000_000_000,
              "verifier_budget_ns": 10_000_000_000,
              "one_attempt_per_case": True}
    write(EVIDENCE / "PRE_RUN.json", record)
    print(EVIDENCE / "PRE_RUN.json")


def case_fields(row, master, run):
    return {
        "family_id": "phase1c_" + row["group"],
        "scenario_id": row["scenario_id"], "route": "sdk-exec-fuse-range-splice-commit-v1",
        "operation_contract_id": row["operation_contract_id"],
        "fixture_bytes": row["fixture_bytes"], "edit_start": row["edit_start"],
        "delete_len": row["delete_len"], "replacement_len": 4096,
        "replacement_sha256": row["replacement_sha256"],
        "final_bytes": row["final_bytes"], "final_sha256": row["final_sha256"],
        "g2_target_ms": 0, "command": row["command"],
        "project_id": master["project_id"], "genesis_layer": master["genesis_layer"],
        "genesis_root": master["genesis_root"],
        "genesis_root_serial": master["genesis_root_serial"],
        "stack_body": master["project_id"][2:34],
        "branch_body": hashlib.sha256(row["scenario_id"].encode()).hexdigest()[:32],
        "fixture_canonical_root": master["fixture_canonical_root"],
        "fixture_extent_count": master["fixture_extent_count"],
        "fixture_mode": 416, "expected_mode": 416,
        "fixture_sha256": row["fixture_sha256"],
        "expected_chunked": row["expected_chunked"],
        "full_file_digest": 1, "window_count": 0,
        "canonical_root_expected": "-", "canonical_count_expected": "-",
        "telemetry_run": run, "edit_count": row["edit_count"],
    }


def sample(label):
    contract.check()
    pre = json.loads((EVIDENCE / "PRE_RUN.json").read_text())
    identity = identities()
    tag = subprocess.check_output(["git", "rev-parse", pre["execution_source_ref"]],
                                  cwd=ROOT, text=True).strip()
    assert not identity["source_dirty"] and identity["source_commit"] == tag
    assert identity["product_seal"] == pre["product_seal"]
    assert identity["harness_seal"] == pre["harness_seal"]
    assert all(digest(item["path"]) == item["sha256"] for item in pre["binaries"].values())
    image_id = subprocess.check_output(
        ["docker", "image", "inspect", pre["image"]["image_id"], "--format", "{{.Id}}"],
        text=True).strip()
    assert image_id == pre["image"]["image_id"]
    row = next(row for row in contract.rows() if row["label"] == label)
    folder = Path(pre["outputs"][label])
    with (RESULTS / ".run.lock").open("a+b") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        folder.mkdir(parents=True, exist_ok=False)
        try:
            _sample(folder, row, pre, identity)
        except Exception as error:
            write(folder / "attempt-error.json", {"status": "FAIL", "error": repr(error),
                                                  "scenario_id": row["scenario_id"],
                                                  "source_commit": identity["source_commit"]})
            raise


def _sample(folder, row, pre, identity):
    master = pre["masters"][str(row["fixture_bytes"])]
    key = cursor_key()
    assert hashlib.sha256(bytes.fromhex(key)).hexdigest() == pre["cursor_key_sha256"]
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
               "phase1c-" + row["label"]]
    environment = {**os.environ, "LAYERFS_HISTORY_CURSOR_KEY": key,
                   "LAYERFS_CONSTRUCTION_WORKERS": "1",
                   "LAYERFS_COMPLEXITY_DIAGNOSTIC": "1"}
    started = time.monotonic_ns()
    try:
        result = subprocess.run(command, capture_output=True, timeout=15, env=environment)
        stdout, stderr, exit_code, timeout = result.stdout, result.stderr, result.returncode, False
    except subprocess.TimeoutExpired as error:
        stdout, stderr, exit_code, timeout = error.stdout or b"", error.stderr or b"", None, True
    wall = time.monotonic_ns() - started
    (folder / "driver.raw.stdout").write_bytes(stdout)
    (folder / "driver.raw.stderr").write_bytes(stderr)
    receipts = [line[8:] for line in stdout.splitlines() if line.startswith(b"RECEIPT\t")]
    driver = json.loads(receipts[-1]) if len(receipts) == 1 else None
    if driver:
        write(folder / "driver-receipt.json", driver)
        if driver.get("status") == "COMPLETE" and not timeout and exit_code == 0:
            fields.update({"branch_id": driver["branch_id"],
                           "expected_head_commit": driver["head_commit"],
                           "expected_mtime_seconds": driver["observed_mtime_seconds"],
                           "expected_mtime_nanoseconds": driver["observed_mtime_nanoseconds"]})
            case_path.write_text("".join(f"{k}={v}\n" for k, v in sorted(fields.items())))
    verification = {"status": "NOT_RUN", "reason": "driver did not complete"}
    if driver and driver.get("status") == "COMPLETE" and exit_code == 0 and not timeout:
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
    counts = driver.get("projection_counts", "") if driver else ""
    expected = (f"range_state={row['expected_state_callbacks']},"
                f"range_edit={row['expected_edit_callbacks']}")
    route_ok = (driver is not None and expected in counts
                and driver.get("range_accepted_payload_bytes") == 4096
                and driver.get("range_shifted_suffix_bytes") == 0)
    status = ("INELIGIBLE" if exit_code == 0 and not timeout and wall <= 15_000_000_000
              and driver and driver.get("status") == "COMPLETE" and route_ok
              and driver.get("unmount_ok") and driver.get("sandbox_delete_ok")
              and verification["status"] == "PASS" and verification["wall_ns"] < 10_000_000_000
              and cache["macos-store"]["status"] == "PASS" else "FAIL")
    receipt = {"schema": "issue232-phase1c-count-receipt-v1", "status": status,
               "scenario_id": row["scenario_id"], "sample_count": 1,
               "source_commit": identity["source_commit"],
               "product_seal": identity["product_seal"],
               "harness_seal": identity["harness_seal"],
               "registry_sha256": contract.REGISTRY_SHA256,
               "image_id": pre["image"]["image_id"], "cache": cache,
               "clone_method": "independent-writable-byte-copy",
               "command": command, "driver_exit_code": exit_code, "driver_timeout": timeout,
               "complete_command_wall_ns": wall,
               "complete_command_status": "PASS" if wall <= 15_000_000_000 and exit_code == 0 else "FAIL",
               "driver": driver, "route_ok": route_ok, "verification": verification}
    write(folder / "receipt.json", receipt)
    print(row["label"], status, wall, verification["status"])


if __name__ == "__main__":
    if sys.argv[1:] == ["prepare"]:
        prepare()
    elif len(sys.argv) == 3 and sys.argv[1] == "run":
        sample(sys.argv[2])
    else:
        raise SystemExit("usage: phase1c.py prepare | run <registered-label>")
