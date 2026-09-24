"""#232 Workspace Exec/FUSE route: preparation, one sample, verification.

The runner owns selection, receipts and reporting; this module owns the route:
master preparation through the public SDK, the independent writable byte copy
each case uses, the declared cache qualification, the release SDK driver run,
raw LFT1 retention and the separate bounded verifier.
"""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time

BENCH = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(BENCH))
from shared import edit_cache, edit_contract as contract  # noqa: E402

CURSOR_KEY_ENV = "LAYERFS_HISTORY_CURSOR_KEY"
DRIVER = "benchmark_edit"
VERIFIER = "verify_edit"
BINDING_KEY = b"layerfs-bench-pro"
COMPLETE_COMMAND_TIMEOUT_S = 15
VERIFIER_TIMEOUT_S = 15


def sha256(path):
    digest = hashlib.sha256()
    with Path(path).open("rb") as source:
        for block in iter(lambda: source.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def fixture_bytes(size):
    """The pristine fixture bytes, streamed from the frozen recipe."""
    out = bytearray()
    offset = 0
    while offset < size:
        take = min(8 << 20, size - offset)
        out.extend(contract.chunk(contract.FIXTURE_GENERATOR_SEED, offset, take))
        offset += take
    return bytes(out)


def prepare_fixture(root, size):
    """Writes one pristine fixture directory and returns its receipt."""
    directory = Path(root) / f"fixture-{size}"
    payload = directory / contract.FIXTURE_PATH
    if not payload.is_file():
        directory.mkdir(parents=True, exist_ok=True)
        payload.write_bytes(fixture_bytes(size))
        payload.chmod(contract.FIXTURE_FILE_MODE)
    digest = sha256(payload)
    if digest != contract.FIXTURE_SHA256[size]:
        raise ValueError(f"fixture recipe mismatch for {size}")
    return {"size": size, "path": str(directory), "payload": str(payload),
            "sha256": digest, "bytes": payload.stat().st_size}


def compatibility_key(identity, fixture):
    """The master compatibility key: unknown compatibility fails closed."""
    return {
        "fixture_bytes": fixture["bytes"],
        "fixture_sha256": fixture["sha256"],
        "fixture_generator_seed": contract.FIXTURE_GENERATOR_SEED,
        "registry_sha256": contract.REGISTRY_SHA256,
        "cargo_lock_sha256": identity["cargo_lock_sha256"],
        "product_seal": identity["product_seal"],
        "harness_seal": identity["harness_seal"],
        "route": contract.ROUTE,
    }


def master(root, size, binaries, identity, cursor_key, budget_ns=60_000_000_000):
    """Prepares (or reuses) the one closed master Store/history pair for a size."""
    root = Path(root)
    fixture = prepare_fixture(root, size)
    directory = root / f"master-{size}"
    record_path = directory / "master.json"
    key = compatibility_key(identity, fixture)
    if record_path.is_file():
        record = json.loads(record_path.read_text())
        if record.get("compatibility_key") == key:
            return {**record, "reuse": "exact-compatibility-key"}
        raise ValueError(f"prepared master {size} is incompatible with this source")
    directory.mkdir(parents=True, exist_ok=True)
    store = directory / "store.sqlite"
    history = directory / "history.sqlite"
    for path in (store, history):
        if path.exists():
            raise ValueError(f"prepared master {size} is incomplete; remove it explicitly")
    started = time.monotonic_ns()
    environment = {**os.environ, CURSOR_KEY_ENV: cursor_key}
    result = subprocess.run(
        [str(binaries["benchmark_init"]), fixture["path"], str(store), str(history),
         f"exec-fuse-edit-{size}"],
        capture_output=True, text=True, env=environment, timeout=budget_ns / 1e9)
    wall_ns = time.monotonic_ns() - started
    if result.returncode != 0:
        raise RuntimeError(f"master preparation failed: {result.stdout}\n{result.stderr}")
    created = json.loads(result.stdout.strip().splitlines()[-1])
    if created.get("status") != "COMPLETE":
        raise RuntimeError(f"master Init did not complete: {created}")
    record = {
        "schema": "core-fs-bench-pro-exec-fuse-edit-master-v1",
        "fixture_bytes": size,
        "fixture_sha256": fixture["sha256"],
        "fixture_canonical_root": contract.FIXTURE_CANONICAL_ROOT[size],
        "fixture_extent_count": contract.FIXTURE_INITIAL_COUNT[size],
        "project_id": created["project_id"],
        "genesis_layer": created["genesis_layer"],
        "genesis_root": created["root"],
        "genesis_root_serial": created["root_serial"],
        "first_use_wall_ns": wall_ns,
        "store": str(store),
        "history": str(history),
        "store_bytes": store.stat().st_size,
        "history_bytes": history.stat().st_size,
        "compatibility_key": key,
        "route": contract.ROUTE,
        "init_receipt": created,
    }
    record_path.write_text(json.dumps(record, indent=2, sort_keys=True) + "\n")
    return {**record, "reuse": "first-use"}


def case_record(row, master_record, branch_body):
    """The complete frozen input of one case: plan, oracle and identities."""
    start = row["edit_start"]
    replacement_len = row["replacement_len"]
    replacement = contract.payload_bytes(row["payload_seed"], replacement_len,
                                        row["replacement_kind"])
    final = row["final_bytes"]
    full_digest = final <= contract.FULL_DIGEST_MAX_BYTES
    begin, length = contract.boundary_window(start, row["delete_len"], replacement_len, final)
    window = contract.result_window(row["fixture_bytes"], start, row["delete_len"], replacement,
                                   begin, length)
    return {
        "family_id": row["family_id"],
        "scenario_id": row["scenario_id"],
        "route": row["route"],
        "fixture_bytes": row["fixture_bytes"],
        "edit_start": start,
        "delete_len": row["delete_len"],
        "replacement_len": replacement_len,
        "replacement_kind": row["replacement_kind"],
        "replacement_sha256": row["replacement_sha256"],
        "final_bytes": final,
        "final_sha256": row["final_sha256"],
        "g2_target_ms": row["g2_target_ms"],
        "command": row["command"],
        "editor_algorithm": row["editor_algorithm"],
        "project_id": master_record["project_id"],
        "genesis_layer": master_record["genesis_layer"],
        "genesis_root": master_record["genesis_root"],
        "genesis_root_serial": master_record["genesis_root_serial"],
        "stack_body": master_record["project_id"][2:34],
        "fixture_canonical_root": master_record["fixture_canonical_root"],
        "fixture_extent_count": master_record["fixture_extent_count"],
        "branch_body": branch_body,
        "full_file_digest": 1 if full_digest else 0,
        "window_offset": begin,
        "window_bytes": length,
        "window_sha256": hashlib.sha256(window).hexdigest(),
        "canonical_root_expected": row.get("canonical_root_expected", "-"),
        "canonical_count_expected": (str(row["canonical_count_expected"])
                                     if "canonical_count_expected" in row else "-"),
    }


def case_file(path, record):
    """Retains the case record as evidence and returns the driver's one argument."""
    body = "".join(f"{key}={value}\n" for key, value in sorted(record.items())
                   if value is not None)
    Path(path).write_text(body)
    return path


def case_spec(record):
    """The case as one argument: the driver opens no host path of its own."""
    return ";".join(f"{key}={value}" for key, value in sorted(record.items())
                    if value is not None)


def sample(run_root, row, binaries, image, identity, cursor_key, telemetry_run, branch_body,
           verification="inline"):
    """Runs one performance sample and its separate verification."""
    folder = Path(run_root) / "sdk-exec-fuse" / row["family_id"] / row["scenario_id"]
    folder.mkdir(parents=True, exist_ok=True)
    master_record = master(Path(run_root).parent / "prepared", row["fixture_bytes"], binaries,
                           identity, cursor_key)
    record = case_record(row, master_record, branch_body)
    record["telemetry_run"] = telemetry_run
    store = folder / "store.sqlite"
    history = folder / "history.sqlite"
    # One independent writable byte copy per case; a clone is setup reuse and is
    # never treated as a cold claim.
    shutil.copyfile(master_record["store"], store)
    shutil.copyfile(master_record["history"], history)
    case_path = case_file(folder / "case.txt", record)
    receipt = {
        "schema": "core-fs-bench-pro-exec-fuse-edit-receipt-v1",
        "family_id": row["family_id"],
        "scenario_id": row["scenario_id"],
        "scenario_version": contract.SCENARIO_VERSION,
        "route": contract.ROUTE,
        "operation_contract_id": contract.OPERATION_CONTRACT_ID,
        "operation_surface": contract.OPERATION_SURFACE,
        "operation_entrypoint": contract.OPERATION_ENTRYPOINT,
        "acknowledgement_boundary": contract.ACKNOWLEDGEMENT_BOUNDARY,
        "repetition": contract.REPETITION,
        "sample_count": 0,
        "case_file": str(case_path),
        "folder": str(folder),
        "store": str(store),
        "history": str(history),
        "identity": identity,
        "image_id": image,
        "master": {key: master_record[key] for key in
                   ("fixture_bytes", "fixture_sha256", "first_use_wall_ns", "reuse",
                    "compatibility_key", "store_bytes", "history_bytes")},
        "clone_method": contract.CLONE_METHOD,
        "cache_contract": contract.CACHE_CONTRACT,
        "cache": {},
        "g2_target_ms": row["g2_target_ms"],
        "performance_gate": row["performance_gate"],
        "admission_eligible": False,
    }
    try:
        receipt["cache"]["macos-store"] = edit_cache.qualify_store([store, history])
        receipt["cache"]["linux-fuse-backing"] = edit_cache.qualify_fuse_backing()
        command = [str(binaries[DRIVER]), case_spec(record), str(store), str(history), image,
                   f"exec-{row['scenario_id'][:40]}"]
        environment = {**os.environ, CURSOR_KEY_ENV: cursor_key}
        started = time.monotonic_ns()
        try:
            result = subprocess.run(command, capture_output=True, timeout=COMPLETE_COMMAND_TIMEOUT_S,
                                    env=environment)
            stdout, stderr, code, timed_out = result.stdout, result.stderr, result.returncode, False
        except subprocess.TimeoutExpired as error:
            stdout, stderr, code, timed_out = error.stdout or b"", error.stderr or b"", None, True
        wall = time.monotonic_ns() - started
        lft1 = [line for line in stdout.splitlines() if line.startswith(b"LFT1 ")]
        receipts = [line[len(b"RECEIPT\t"):] for line in stdout.splitlines()
                    if line.startswith(b"RECEIPT\t")]
        other = b"\n".join(line for line in stdout.splitlines()
                           if not line.startswith((b"LFT1 ", b"RECEIPT\t")))
        (folder / "telemetry.lft1").write_bytes(b"\n".join(lft1) + (b"\n" if lft1 else b""))
        (folder / "driver.stdout").write_bytes(other)
        if receipts:
            (folder / "driver-receipt.json").write_bytes(receipts[-1] + b"\n")
        (folder / "driver.stderr").write_bytes(stderr)
        receipt.update(
            command=command,
            sample_count=1 if code == 0 else 0,
            driver_exit_code=code,
            driver_timeout=timed_out,
            complete_command_wall_ns=wall,
            complete_command_budget_ns=contract.COMPLETE_COMMAND_BUDGET_NS,
            complete_command_status=("PASS" if code == 0 and not timed_out
                                     and wall <= contract.COMPLETE_COMMAND_BUDGET_NS
                                     else "COMMAND_SLOW" if code == 0 and not timed_out
                                     else "FAIL"),
            lft1_lines=len(lft1),
        )
        if (folder / "driver-receipt.json").is_file():
            driver = json.loads((folder / "driver-receipt.json").read_text())
            receipt["driver"] = driver
            receipt["edit_commit_ns"] = driver.get("edit_commit_ns")
            receipt["preparation_ns"] = driver.get("preparation_ns")
            receipt["cleanup_ns"] = driver.get("cleanup_ns")
            receipt["edit_ns"] = driver.get("edit_ns")
            receipt["commit_ns"] = driver.get("commit_ns")
            (folder / "perf.jsonl").write_text(json.dumps(driver, sort_keys=True) + "\n")
        receipt["telemetry"] = edit_telemetry_check(lft1)
        if verification == "inline":
            receipt["verification"] = verify(folder, binaries, case_path, store, history, row,
                                            receipt, cursor_key)
        else:
            receipt["verification"] = {
                "status": "SKIPPED",
                "reason": ("performance-only exploratory sample at this source identity; the "
                           "independent verifier runs separately against this retained receipt"),
                "scenario_id": row["scenario_id"],
            }
        receipt.update(terminal_status(receipt, row))
    except Exception as error:  # noqa: BLE001 - retained as evidence, never hidden
        receipt["status"] = "FAIL"
        receipt["error"] = repr(error)
    finally:
        (folder / "receipt.json").write_text(json.dumps(receipt, indent=2, sort_keys=True) + "\n")
    return receipt


def edit_telemetry_check(lines):
    """Label/cardinality check of the one caller root and its two children."""
    import re
    roots = []
    for line in lines:
        text = line.decode(errors="replace")
        match = re.search(r'"kind":"operation","key":(\d+),"timing":(.*)\}$', text)
        if not match:
            continue
        roots.append((int(match.group(1)), match.group(2)))
    root = [body for key, body in roots if key == 232_000]
    children = {name: body for key, body in roots for name in ()}
    report = {
        "caller_root_present": len(root) == 1,
        "root_count": len(root),
        "edit_child": '"edit"' in (root[0] if root else ""),
        "commit_child": '"commit"' in (root[0] if root else ""),
        "non_operation_lines": sum(1 for line in lines if b'"kind":"operation"' not in line),
        "status": "PENDING",
    }
    del children
    report["status"] = ("PASS" if report["caller_root_present"] and report["edit_child"]
                        and report["commit_child"] else "UNAVAILABLE")
    return report


def verify(folder, binaries, case_path, store, history, row, receipt, cursor_key, output=None):
    """Runs the independent bounded verifier once, under its hard cap."""
    output = Path(output or folder)
    driver = receipt.get("driver") or {}
    record = dict(line.split("=", 1) for line in Path(case_path).read_text().splitlines() if line)
    record["expected_head_commit"] = driver.get("head_commit", "")
    bound = folder / "verifier-case.txt"
    bound.write_text("".join(f"{key}={value}\n" for key, value in sorted(record.items())))
    command = [str(binaries[VERIFIER]), str(bound), str(store), str(history)]
    environment = {**os.environ, CURSOR_KEY_ENV: cursor_key}
    started = time.monotonic_ns()
    try:
        result = subprocess.run(command, capture_output=True, timeout=VERIFIER_TIMEOUT_S,
                                env=environment)
        wall = time.monotonic_ns() - started
        stdout, stderr, code, timed_out = result.stdout, result.stderr, result.returncode, False
    except subprocess.TimeoutExpired as error:
        wall = time.monotonic_ns() - started
        stdout, stderr, code, timed_out = error.stdout or b"", error.stderr or b"", None, True
    output.mkdir(parents=True, exist_ok=True)
    (output / "verifier.stdout").write_bytes(stdout)
    (output / "verifier.stderr").write_bytes(stderr)
    try:
        child = json.loads(stdout.decode().strip().splitlines()[-1]) if stdout else None
    except (ValueError, IndexError):
        child = None
    status = "PASS" if code == 0 and child and child.get("status") == "PASS" else (
        "TIMEOUT" if timed_out else "FAIL")
    receipt_json = {
        "status": status,
        "wall_ns": wall,
        "budget_ns": contract.VERIFIER_HARD_BUDGET_NS,
        "timeout": timed_out,
        "exit_code": code,
        "command": command,
        "child": child,
        "stderr": stderr[:4096].decode(errors="replace"),
        "scenario_id": row["scenario_id"],
    }
    (output / "verification.json").write_text(json.dumps(receipt_json, indent=2, sort_keys=True) + "\n")
    return receipt_json


def terminal_status(receipt, row):
    """Applies the frozen eligibility order; a fast row is never a PASS alone."""
    driver = receipt.get("driver") or {}
    cache = receipt.get("cache", {})
    reasons = []
    if receipt.get("complete_command_status") != "PASS":
        reasons.append("complete command did not finish inside its budget")
    if driver.get("status") != "COMPLETE":
        reasons.append("the measured attempt did not complete")
    if (receipt.get("telemetry") or {}).get("status") != "PASS":
        reasons.append("caller LFT1 root/children missing")
    verification = (receipt.get("verification") or {}).get("status")
    if verification not in ("PASS", "SKIPPED"):
        reasons.append("independent verification did not pass")
    if verification == "SKIPPED":
        reasons.append("independent verification not run on this sample")
    if driver.get("sandbox_delete_ok") is not True:
        reasons.append("sandbox cleanup was not confirmed")
    if (cache.get("macos-store") or {}).get("status") != "PASS":
        reasons.append("macOS Store domain is not cold-qualified")
    fuse_warm = (cache.get("linux-fuse-backing") or {}).get("status") != "PASS"
    if verification == "SKIPPED":
        reasons.remove("independent verification not run on this sample")
    if reasons:
        return {"status": "FAIL", "status_reasons": reasons}
    if fuse_warm:
        return {"status": "INELIGIBLE",
                "status_reasons": [contract.ELIGIBILITY_INELIGIBLE_REASON],
                "raw_edit_commit_ns": driver.get("edit_commit_ns")}
    observed = driver.get("edit_commit_ns")
    target_ns = row["g2_target_ms"] * 1_000_000
    return {"status": "GOAL_MET" if observed <= target_ns else "TARGET_MISS",
            "raw_edit_commit_ns": observed, "g2_target_ns": target_ns}
