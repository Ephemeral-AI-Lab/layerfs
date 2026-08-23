#!/usr/bin/env python3
import argparse
import csv
import hashlib
import json
import os
import pathlib
import secrets
import shutil
import subprocess
import sys
import tempfile
import time

HERE = pathlib.Path(__file__).resolve().parent
REPO = HERE.parents[4]
TARGET = REPO / "target"
LOCK = TARGET / "phase4-g5-warm-projection" / "BENCHMARK_LOCK"
FREEZE = HERE / "method/SOURCE-FREEZE-v3.json"
INPUT_MANIFEST = HERE / "method/INPUT-MANIFEST-v3.json"
DRY_RUN = HERE / "DRY-RUN-v3.json"
PREPARATION_FORECAST = HERE / "PREPARATION-FORECAST-v3.json"
PRIMARY = HERE / "analyzers/primary.py"
INDEPENDENT = HERE / "analyzers/independent.py"
EXECUTION_PRIMARY = HERE / "analyzers/primary_attempt_3.py"
EXECUTION_INDEPENDENT = HERE / "analyzers/independent_attempt_3.py"
V2_DISPOSITION = HERE.parent / "v2/PREMEASUREMENT-REVISE-v2.json"
V3_DISPOSITION = HERE / "PREMEASUREMENT-REVISE-v3.json"
V3_READINESS = HERE / "PREMEASUREMENT-READINESS-AUDIT-v3.json"
G5_1_TERMINAL = HERE.parents[1] / "g5-trusted-reopen-edit/v27/G5-1-TERMINAL-AUDIT-v27.json"
METHOD_CONTRACT = HERE / "method/METHOD-CONTRACT-v3.json"
SCHEDULE = HERE / "method/SCHEDULE-v3.tsv"
FAULT_MATRIX = HERE / "method/FAULT-MATRIX-v3.tsv"
SOURCE_FAULT_PROOFS = HERE / "method/SOURCE-FAULT-PROOFS-v3.json"
FOCUSED_FAULT_EXECUTION = HERE / "evidence/FOCUSED-FAULT-EXECUTION-v3.json"
FINAL_SOURCE_BINDING = HERE / "EXACT-SOURCE-FINAL2-FOCUSED-FAULT-BINDING-v3.json"
ATTEMPT_3_BINDING = HERE / "ATTEMPT-3-METHOD-BINDING-v3.json"
RAW_FINAL2_FILES = tuple(sorted((HERE / "evidence/raw-final2").glob("*")))
LIMITS = {"screen": 19_999_999_999, "gate": 150_000_000_000}
POPULATIONS = {"screen": [2, 2], "gate": [64, 100]}
MODE_POPULATIONS = {"self-check": [1, 0], "screen-count": [1, 1], **POPULATIONS}
CAMPAIGNS = {"screen": ["self-check", "screen-count", "screen"], "gate": ["self-check", "screen-count", "gate"]}
PRODUCT_SCHEMA = "phase4-g5-projection-suite-v2"
FIXTURE_DESCRIPTOR = "G5-PROJECTION-FIXTURE-v2.tsv"
PRODUCT_RELEASE = REPO / "target/release/phase4_create_edit_benchmark"
FAULT_RUN_MODES = {
    "clone-failure": "fault-clone",
    "after-rename-lost-ack": "fault-rename-lost-ack",
}
RESULT_NAMES = {"screen": "phase4-g5-warm-projection-v3-screen-attempt-3", "gate": "phase4-g5-warm-projection-v3-gate-attempt-3"}
AUTHORITATIVE = (
    HERE / "runner.py", PRIMARY, INDEPENDENT, HERE / "PREREGISTRATION-v3.md",
    HERE / "LIMITATIONS-v3.md", HERE / "method/SCHEDULE-v3.tsv",
    HERE / "method/EXPECTED-OUTCOMES-v3.tsv", HERE / "FOCUSED-TEST-ATTEMPTS-v3.json",
    HERE / "method/CONSERVATION-SEMANTICS-v3.json", HERE / "method/FAULT-MATRIX-v3.tsv",
    HERE / "PROMOTION-READINESS-v3.md", HERE / "PREMEASUREMENT-REVISE-v3.json",
    HERE / "READINESS-PLAN-v3.md", PREPARATION_FORECAST, METHOD_CONTRACT, SOURCE_FAULT_PROOFS, FOCUSED_FAULT_EXECUTION, FINAL_SOURCE_BINDING, V2_DISPOSITION,
    V3_READINESS, G5_1_TERMINAL, INPUT_MANIFEST,
    REPO / "crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs",
    REPO / "crates/layerfs-engine/src/bin/phase4_g3_materialization.rs",
) + RAW_FINAL2_FILES


def compact(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":"))


def sha256(path):
    digest = hashlib.sha256()
    with pathlib.Path(path).open("rb") as handle:
        for block in iter(lambda: handle.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def fsync_dir(path):
    descriptor = os.open(path, os.O_RDONLY)
    try:
        os.fsync(descriptor)
    finally:
        os.close(descriptor)


def write_json(path, value):
    path = pathlib.Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(path.name + ".tmp")
    with temporary.open("w", encoding="utf-8") as handle:
        handle.write(compact(value) + "\n")
        handle.flush()
        os.fsync(handle.fileno())
    os.replace(temporary, path)
    fsync_dir(path.parent)


def write_text(path, value):
    path = pathlib.Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(path.name + ".tmp")
    with temporary.open("w", encoding="utf-8") as handle:
        handle.write(value)
        handle.flush()
        os.fsync(handle.fileno())
    os.replace(temporary, path)
    fsync_dir(path.parent)


def inventory(root):
    root = pathlib.Path(root)
    rows = []
    allowed_large = {"parent.source", "latest.source", "materialized.bin", "store.sqlite"}
    for path in sorted(root.rglob("*")):
        relative = path.relative_to(root).as_posix()
        if path.is_symlink():
            raise RuntimeError(f"symlink forbidden in fixture: {relative}")
        stat = path.stat()
        if path.is_dir():
            rows.append({"path": relative + "/", "kind": "directory", "mode": stat.st_mode & 0o7777})
        elif path.is_file():
            name = path.name
            if not (
                name in allowed_large
                or name in {FIXTURE_DESCRIPTOR, "store.sqlite.authority"}
                or name.endswith((".edge-token", ".edit"))
            ):
                raise RuntimeError(f"noncompact fixture entry forbidden: {relative}")
            rows.append({"path": relative, "kind": "file", "mode": stat.st_mode & 0o7777, "bytes": stat.st_size, "allocated_bytes": stat.st_blocks * 512, "sha256": sha256(path)})
        else:
            raise RuntimeError(f"unsupported fixture entry: {relative}")
    if sum(row.get("path", "").endswith("parent.source") for row in rows) != 1 or sum(row.get("path", "").endswith("latest.source") for row in rows) != 1:
        raise RuntimeError("compact fixture requires exactly one parent and one latest source")
    files = [row for row in rows if row.get("kind") == "file"]
    limits = load_json(METHOD_CONTRACT)["compact_fixture_limits"]
    descriptors = [root / row["path"] for row in files if row["path"].endswith(FIXTURE_DESCRIPTOR)]
    embedded_too_large = False
    embedded = []
    if len(descriptors) == 1:
        try:
            csv.field_size_limit(limits["max_token_or_edit_bytes"] + 1)
            with descriptors[0].open(newline="", encoding="utf-8") as handle:
                for row in csv.reader(handle, delimiter="\t"):
                    if not row:
                        continue
                    key, values = row[0], row[1:]
                    selected = values if key.endswith("_token") else values[-1:] if key.startswith("chain_") else values if key == "patch_bytes" else []
                    for value in selected:
                        size = len(value) // 2 if key == "patch_bytes" else len(value.encode())
                        embedded.append({"key": key, "bytes": size, "sha256": hashlib.sha256(value.encode()).hexdigest()})
                    if any(item["bytes"] > limits["max_token_or_edit_bytes"] for item in embedded) or sum(item["bytes"] for item in embedded) > limits["max_token_or_edit_bytes"]:
                        embedded_too_large = True
                        break
        except csv.Error:
            embedded_too_large = True
    apparent_bytes = sum(row["bytes"] for row in files)
    allocated_bytes = sum(row["allocated_bytes"] for row in files)
    if len(descriptors) != 1 or len(files) > limits["max_files"] or apparent_bytes > limits["max_aggregate_bytes"] or allocated_bytes > limits["max_aggregate_bytes"] or any(row["bytes"] > limits["max_file_bytes"] for row in files) or descriptors[0].stat().st_size > limits["max_token_or_edit_bytes"] or embedded_too_large or any(row["bytes"] > limits["max_token_or_edit_bytes"] for row in files if row["path"].endswith((".edge-token", ".edit"))):
        raise RuntimeError("compact fixture count/size/aggregate bound exceeded")
    return {"schema": "phase4-g5-2-compact-fixture-inventory-v3", "classification": "ParentLatestTokenEditNoPerRevisionSources", "entries": rows, "apparent_bytes": apparent_bytes, "allocated_bytes": allocated_bytes, "embedded_token_edit_entries": embedded, "embedded_token_edit_aggregate_bytes": sum(item["bytes"] for item in embedded), "sha256": hashlib.sha256(compact({"entries": rows, "embedded_token_edit_entries": embedded}).encode()).hexdigest()}


def make_tree_writable(root):
    root = pathlib.Path(root)
    if not root.exists():
        return
    if any(path.is_symlink() for path in root.rglob("*")):
        raise RuntimeError("private clone contains a symlink")
    for path in sorted((item for item in root.rglob("*") if item.is_dir()), key=lambda item: len(item.parts)):
        path.chmod(0o755)
    root.chmod(0o755)
    for path in root.rglob("*"):
        if path.is_file():
            path.chmod(0o600 if path.name.endswith(".authority") else 0o644)


def private_permission_receipt(root):
    root = pathlib.Path(root)
    entries = [{"path": ".", "kind": "directory", "mode": root.stat().st_mode & 0o7777}]
    for path in sorted(root.rglob("*")):
        relative = path.relative_to(root).as_posix()
        if path.is_symlink():
            raise RuntimeError(f"private clone symlink forbidden: {relative}")
        if path.is_dir():
            entries.append({"path": relative + "/", "kind": "directory", "mode": path.stat().st_mode & 0o7777})
        elif path.is_file():
            entries.append({"path": relative, "kind": "authority" if path.name.endswith(".authority") else "ordinary", "mode": path.stat().st_mode & 0o7777})
        else:
            raise RuntimeError(f"unsupported private clone entry: {relative}")
    authority = [row for row in entries if row["kind"] == "authority"]
    if len(authority) != 1 or any(row["mode"] != 0o755 for row in entries if row["kind"] == "directory") or any(row["mode"] != 0o644 for row in entries if row["kind"] == "ordinary") or authority[0]["mode"] != 0o600:
        raise RuntimeError("private clone permission map mismatch")
    return {
        "schema": "phase4-g5-2-private-clone-permissions-v3", "status": "PASS",
        "classification": "Directories0755Ordinary0644AuthoritySidecar0600NoSymlinks",
        "entries": entries, "authority_files": 1, "symlinks": 0,
        "map_sha256": hashlib.sha256(compact(entries).encode()).hexdigest(),
    }


def rebind_cloned_fixture(attempt, process_root):
    attempt, process_root = pathlib.Path(attempt), pathlib.Path(process_root)
    fixture = attempt / FIXTURE_DESCRIPTOR
    before = fixture.read_bytes()
    lines = before.splitlines(keepends=True)
    matches = [index for index, line in enumerate(lines) if line.split(b"\t", 1)[0] == b"directory"]
    if matches != [0] or b"\t" not in lines[0]:
        raise RuntimeError("fixture directory field is not uniquely bound")
    old = lines[0].split(b"\t", 1)[1].rstrip(b"\r\n")
    target = attempt / "g3-qualified-one-byte"
    ending = b"\r\n" if lines[0].endswith(b"\r\n") else b"\n" if lines[0].endswith(b"\n") else b""
    replacement = b"directory\t" + str(target).encode() + ending
    if old == str(target).encode() or not target.is_dir():
        raise RuntimeError("fixture directory rebind target is invalid")
    after_lines = list(lines)
    after_lines[0] = replacement
    after = b"".join(after_lines)
    if len(after_lines) != len(lines) or after_lines[1:] != lines[1:] or sum(left != right for left, right in zip(lines, after_lines)) != 1:
        raise RuntimeError("fixture rebind changed more than the directory field")
    write_text(fixture, after.decode())
    if fixture.read_bytes() != after:
        raise RuntimeError("fixture directory rebind did not persist exactly")
    receipt = {
        "schema": "phase4-g5-2-clone-directory-rebind-v3", "status": "PASS",
        "scope": "SealedCloneDirectoryPathRebindOnly", "field": "directory",
        "changed_fields": 1, "all_other_tsv_fields_byte_identical": True,
        "old_value": old.decode(), "new_value": str(target),
        "before_bytes": len(before), "after_bytes": len(after),
        "before_sha256": hashlib.sha256(before).hexdigest(),
        "after_sha256": hashlib.sha256(after).hexdigest(),
    }
    rebind_path = process_root / "REBIND.json"
    write_json(rebind_path, receipt)
    command = load_json(process_root / "COMMAND.json")
    command["fixture_rebind_receipt_sha256"] = sha256(rebind_path)
    write_json(process_root / "COMMAND.json", command)
    return receipt, sha256(rebind_path)


def seal_input_tree(root):
    root = pathlib.Path(root)
    for path in sorted(item for item in root.rglob("*") if item.is_file()):
        with path.open("rb") as handle:
            os.fsync(handle.fileno())
        path.chmod(0o444)
    directories = [root, *(item for item in root.rglob("*") if item.is_dir())]
    for path in sorted(directories, key=lambda item: len(item.parts), reverse=True):
        fsync_dir(path)
        path.chmod(0o555)
    fsync_dir(root.parent)


def verify_sealed_fixture(root, expected_sha256=None):
    root = pathlib.Path(root)
    observed = inventory(root)
    if root.stat().st_mode & 0o7777 != 0o555 or any(row["mode"] != (0o444 if row["kind"] == "file" else 0o555) for row in observed["entries"]) or (expected_sha256 is not None and observed["sha256"] != expected_sha256):
        raise RuntimeError("sealed fixture mode/hash mismatch")
    return observed


def verify_sealed_manifest_inputs(manifest):
    modes = load_json(METHOD_CONTRACT)["fixture_mode_size_bytes"]
    if manifest.get("sealed") is not True or manifest.get("seal_reopened_verified") is not True or set(manifest.get("inputs", {})) != set(modes):
        raise RuntimeError("sealed input manifest missing")
    roots = []
    for mode in modes:
        record = manifest["inputs"][mode]
        root = pathlib.Path(record["root"])
        roots.append(root.parent)
        verify_sealed_fixture(root, record["inventory"]["sha256"])
    if len(set(roots)) != 1 or roots[0].stat().st_mode & 0o7777 != 0o555:
        raise RuntimeError("sealed input root mismatch")
    return True


def load_json(path):
    return json.loads(pathlib.Path(path).read_text(encoding="utf-8"))


def fault_matrix_rows():
    with FAULT_MATRIX.open(newline="", encoding="utf-8") as handle:
        rows = list(csv.DictReader(handle, delimiter="\t"))
    expected = {"case", "stage", "required_observation", "decision"}
    if len(rows) != 12 or set(rows[0] if rows else ()) != expected or len({row["case"] for row in rows}) != 12 or any(row["decision"] != "hard" for row in rows):
        raise RuntimeError("fault matrix must contain 12 unique exact hard cases")
    return rows


def require_promotion_authorized(action):
    disposition = load_json(V3_DISPOSITION)
    readiness = load_json(V3_READINESS) if V3_READINESS.is_file() else {}
    g5_1 = load_json(G5_1_TERMINAL) if G5_1_TERMINAL.is_file() else {}
    authority = readiness.get("g5_1_authority", {})
    authorization = readiness.get("authorization", {})
    required_actions = ("forecast", "dry_run") if action == "forecast" else (action,)
    if (
        disposition.get("status") != "SUPERSEDED_BY_READINESS_PASS"
        or disposition.get("authorization") != "AUTHORIZED"
        or readiness.get("status") != "PASS"
        or g5_1.get("status") != "PASS"
        or authority.get("path") != str(G5_1_TERMINAL.relative_to(REPO))
        or authority.get("sha256") != sha256(G5_1_TERMINAL)
        or authority.get("status") != "PASS"
        or any(authorization.get(name) is not True for name in required_actions)
    ):
        raise RuntimeError("G5-2 v3 remains PREMEASUREMENT_REVISE; preparation/freeze/campaign closed")


def require_product_contract(product, mode, population):
    contract = load_json(METHOD_CONTRACT)
    if product.get("status") != "PASS" or product.get("mode") != mode or product.get("size_bytes") != 250_000 or product.get("route_class") != contract["route_class"] or [product.get("exact_every_root_population"), product.get("latest_following_population")] != population:
        raise RuntimeError(f"{mode} product size/route/population contract mismatch")


def require_final_release(executable):
    binding = load_json(FINAL_SOURCE_BINDING)
    if pathlib.Path(executable).resolve() != PRODUCT_RELEASE.resolve() or binding.get("executable", {}).get("sha256") != sha256(executable):
        raise RuntimeError("executable is not the exact bound final release")


def verify_attempt_3_binding():
    binding = load_json(ATTEMPT_3_BINDING)
    expected = {
        "runner_attempt_3_sha256": sha256(pathlib.Path(__file__)),
        "frozen_runner_sha256": sha256(HERE / "runner.py"),
        "runner_attempt_2_sha256": sha256(HERE / "runner_attempt_2.py"),
        "attempt_2_binding_sha256": sha256(HERE / "ATTEMPT-2-METHOD-BINDING-v3.json"),
        "attempt_2_disposition_sha256": sha256(HERE / "SCREEN-ATTEMPT-2-v3.json"),
        "source_freeze_sha256": sha256(FREEZE),
        "input_manifest_sha256": sha256(INPUT_MANIFEST),
        "dry_run_sha256": sha256(DRY_RUN),
        "product_release_sha256": sha256(PRODUCT_RELEASE),
        "primary_analyzer_sha256": sha256(PRIMARY),
        "independent_analyzer_sha256": sha256(INDEPENDENT),
        "primary_attempt_3_sha256": sha256(EXECUTION_PRIMARY),
        "independent_attempt_3_sha256": sha256(EXECUTION_INDEPENDENT),
    }
    if binding.get("status") != "PROSPECTIVE_BEFORE_ATTEMPT_3" or binding.get("scope") != "CloneAuthoritySidecar0600OnlyPlusPriorRebind" or binding.get("hashes") != expected or binding.get("screen_limit_ns") != LIMITS["screen"] or binding.get("gate_limit_ns") != LIMITS["gate"] or binding.get("mode_populations") != MODE_POPULATIONS or binding.get("threshold_or_population_change") is not False:
        raise RuntimeError("attempt-3 method binding mismatch")
    return binding


def prepare_inputs(executable, input_root):
    require_promotion_authorized("input_preparation")
    executable, input_root = pathlib.Path(executable).resolve(), pathlib.Path(input_root).resolve()
    require_final_release(executable)
    if INPUT_MANIFEST.exists() or input_root.exists():
        raise RuntimeError("input preparation is one-shot; manifest/root already exists")
    contract = load_json(METHOD_CONTRACT)
    limit_ns = contract["preparation_complete_wall_limit_ns"]
    expected_sizes = contract["fixture_mode_size_bytes"]
    preparation_forecast = load_json(PREPARATION_FORECAST)
    if preparation_forecast.get("status") != "PASS" or preparation_forecast.get("fixture_mode_size_bytes") != expected_sizes or preparation_forecast.get("preferred_wall_ns") != contract["preparation_preferred_wall_ns"] or preparation_forecast.get("preparation_complete_wall_limit_ns") != limit_ns or preparation_forecast.get("forecast_ns", limit_ns + 1) > limit_ns or preparation_forecast.get("input_root_apparent_and_allocated_limit_bytes") != contract["compact_fixture_limits"]["max_input_root_bytes"]:
        raise RuntimeError("preparation forecast/method contract mismatch")
    started = time.monotonic_ns()
    records = {}
    try:
        input_root.mkdir(parents=True)
        for mode in ("self-check", "screen-count", "screen", "gate"):
            fixture = input_root / mode
            remaining_ns = limit_ns - (time.monotonic_ns() - started)
            if remaining_ns <= 0:
                raise RuntimeError("input preparation complete-wall limit exceeded")
            completed = subprocess.run([str(executable), "--g5-projection-prepare", str(fixture), mode], text=True, capture_output=True, check=True, timeout=remaining_ns / 1_000_000_000)
            product = json.loads(completed.stdout)
            if product.get("status") != "PASS" or product.get("mode") != mode or product.get("size_bytes") != expected_sizes[mode] or product.get("preparation_timing") != "outside-campaign":
                raise RuntimeError(f"invalid {mode} preparation receipt")
            records[mode] = {"root": str(fixture), "product": product}
        seal_input_tree(input_root)
        for mode in expected_sizes:
            records[mode]["inventory"] = verify_sealed_fixture(records[mode]["root"])
        files = [path.stat() for path in input_root.rglob("*") if path.is_file()]
        apparent_bytes = sum(value.st_size for value in files)
        allocated_bytes = sum(value.st_blocks * 512 for value in files)
        elapsed_ns = time.monotonic_ns() - started
        if elapsed_ns > limit_ns or max(apparent_bytes, allocated_bytes) > contract["compact_fixture_limits"]["max_input_root_bytes"]:
            raise RuntimeError("input preparation wall or final root size limit exceeded")
        manifest = {"schema": "phase4-g5-2-input-manifest-v3", "status": "PASS", "executable_sha256": sha256(executable), "preparation_timing": "outside-campaign", "preparation_complete_wall_ns": elapsed_ns, "preparation_preferred_wall_ns": contract["preparation_preferred_wall_ns"], "within_preferred_wall": elapsed_ns <= contract["preparation_preferred_wall_ns"], "preparation_complete_wall_limit_ns": limit_ns, "fixture_mode_size_bytes": expected_sizes, "input_root_apparent_bytes": apparent_bytes, "input_root_allocated_bytes": allocated_bytes, "max_input_root_bytes": contract["compact_fixture_limits"]["max_input_root_bytes"], "sealed": True, "seal_file_mode": 0o444, "seal_directory_mode": 0o555, "seal_reopened_verified": True, "inputs": records}
        write_json(INPUT_MANIFEST, manifest)
        return manifest
    except Exception:
        make_tree_writable(input_root)
        shutil.rmtree(input_root, ignore_errors=True)
        fsync_dir(input_root.parent)
        raise


def freeze(executable):
    require_promotion_authorized("freeze")
    executable = pathlib.Path(executable).resolve()
    require_final_release(executable)
    if FREEZE.exists():
        raise RuntimeError("freeze already exists")
    manifest = load_json(INPUT_MANIFEST)
    if manifest.get("status") != "PASS" or manifest.get("executable_sha256") != sha256(executable):
        raise RuntimeError("input/executable custody mismatch")
    verify_sealed_manifest_inputs(manifest)
    missing = [str(path) for path in AUTHORITATIVE if not path.is_file()]
    if missing:
        raise RuntimeError(f"missing authoritative files: {missing}")
    receipt = {
        "schema": "phase4-g5-2-source-freeze-v3", "status": "FROZEN_BEFORE_DRY_RUN",
        "executable": str(executable), "executable_sha256": sha256(executable),
        "input_manifest_sha256": sha256(INPUT_MANIFEST),
        "method_contract_sha256": sha256(METHOD_CONTRACT),
        "schedule_sha256": sha256(SCHEDULE),
        "fault_matrix_sha256": sha256(FAULT_MATRIX),
        "authoritative_files": [{"path": str(path.relative_to(REPO)), "bytes": path.stat().st_size, "sha256": sha256(path)} for path in AUTHORITATIVE],
        "screen_limit_ns": LIMITS["screen"], "gate_limit_ns": LIMITS["gate"],
        "populations": POPULATIONS, "mode_populations": MODE_POPULATIONS,
        "campaigns": CAMPAIGNS,
    }
    write_json(FREEZE, receipt)
    return receipt


def verify_frozen_method_fields(frozen):
    if frozen.get("method_contract_sha256") != sha256(METHOD_CONTRACT) or frozen.get("schedule_sha256") != sha256(SCHEDULE) or frozen.get("fault_matrix_sha256") != sha256(FAULT_MATRIX):
        raise RuntimeError("method/schedule/fault matrix freeze mismatch")


def verify_freeze():
    frozen = load_json(FREEZE)
    if frozen.get("status") != "FROZEN_BEFORE_DRY_RUN" or frozen.get("input_manifest_sha256") != sha256(INPUT_MANIFEST):
        raise RuntimeError("freeze binding mismatch")
    verify_frozen_method_fields(frozen)
    if frozen.get("screen_limit_ns") != LIMITS["screen"] or frozen.get("gate_limit_ns") != LIMITS["gate"] or frozen.get("populations") != POPULATIONS or frozen.get("mode_populations") != MODE_POPULATIONS or frozen.get("campaigns") != CAMPAIGNS:
        raise RuntimeError("frozen method mismatch")
    executable = pathlib.Path(frozen["executable"])
    if sha256(executable) != frozen.get("executable_sha256"):
        raise RuntimeError("executable changed")
    expected = {row["path"]: row["sha256"] for row in frozen.get("authoritative_files", [])}
    actual = {str(path.relative_to(REPO)): sha256(path) for path in AUTHORITATIVE}
    if expected != actual:
        raise RuntimeError("authoritative source changed")
    manifest = load_json(INPUT_MANIFEST)
    verify_sealed_manifest_inputs(manifest)
    return frozen, manifest


def forecast():
    require_promotion_authorized("forecast")
    verify_freeze()
    exact_ns = 4 * 66 * 8_000_000
    latest_ns = 4 * 101 * 10_000_000
    fallback_ns = 4 * 400_000_000
    fault_process_ns = 4 * len(FAULT_RUN_MODES) * 1_000_000_000
    wrapper_ns = 10_000_000_000
    total = exact_ns + latest_ns + fallback_ns + fault_process_ns + wrapper_ns
    receipt = {
        "schema": "phase4-g5-2-zero-row-forecast-v3", "status": "PASS" if total <= LIMITS["gate"] else "REVISE",
        "product_processes": 0, "product_rows": 0, "lock_acquisitions": 0,
        "components_ns": {"exact_population_4x": exact_ns, "latest_population_4x": latest_ns, "fallback_4x": fallback_ns, "two_exact_fault_processes_4x_one_second": fault_process_ns, "clone_checkpoint_analyzers_custody_cleanup": wrapper_ns},
        "forecast_ns": total, "gate_limit_ns": LIMITS["gate"], "reserve_ns": LIMITS["gate"] - total,
        "classification": "ProspectiveFeasibilityNotProductTiming",
    }
    write_json(DRY_RUN, receipt)
    if receipt["status"] != "PASS":
        raise RuntimeError("zero-row forecast exceeds gate")
    return receipt


def acquire_lock(intent):
    LOCK.parent.mkdir(parents=True, exist_ok=True)
    descriptor = os.open(LOCK, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o444)
    token = secrets.token_hex(32)
    bound = dict(intent, ownership_token=token)
    try:
        os.write(descriptor, (compact(bound) + "\n").encode())
        os.fsync(descriptor)
        fsync_dir(LOCK.parent)
        stat = os.fstat(descriptor)
        return {"descriptor": descriptor, "device": stat.st_dev, "inode": stat.st_ino, "token": token, "intent_sha256": hashlib.sha256((compact(bound) + "\n").encode()).hexdigest()}
    except Exception:
        os.close(descriptor)
        LOCK.unlink(missing_ok=True)
        fsync_dir(LOCK.parent)
        raise


def release_lock(ownership):
    current = LOCK.stat(follow_symlinks=False)
    descriptor = os.fstat(ownership["descriptor"])
    if (current.st_dev, current.st_ino) != (ownership["device"], ownership["inode"]) or (descriptor.st_dev, descriptor.st_ino) != (ownership["device"], ownership["inode"]):
        raise RuntimeError("benchmark lock ownership identity changed")
    LOCK.unlink()
    fsync_dir(LOCK.parent)
    os.close(ownership["descriptor"])
    return {"device": ownership["device"], "inode": ownership["inode"], "ownership_token_sha256": hashlib.sha256(ownership["token"].encode()).hexdigest(), "intent_sha256": ownership["intent_sha256"], "lock_absent": not LOCK.exists()}


def parse_rss(path):
    for line in pathlib.Path(path).read_text().splitlines():
        if "maximum resident set size" in line:
            return int(line.split()[0])
    raise RuntimeError("missing /usr/bin/time RSS")


def text_value(value):
    if value is None:
        return ""
    return value.decode(errors="replace") if isinstance(value, bytes) else str(value)


def begin_process_evidence(process_root, command):
    process_root = pathlib.Path(process_root)
    process_root.mkdir(parents=True, exist_ok=False)
    write_json(process_root / "COMMAND.json", {"schema": "phase4-g5-2-process-command-v3", "argv": list(command)})


def persist_process_evidence(process_root, stdout, stderr, returncode, timed_out, rss_text, process_started_ns=None, process_ended_ns=None):
    process_root = pathlib.Path(process_root)
    if not (process_root / "COMMAND.json").is_file():
        raise RuntimeError("process command intent was not persisted")
    write_text(process_root / "STDOUT.txt", text_value(stdout))
    write_text(process_root / "STDERR.txt", text_value(stderr))
    elapsed_ns = process_ended_ns - process_started_ns if process_started_ns is not None and process_ended_ns is not None else None
    write_json(process_root / "RETURN.json", {"schema": "phase4-g5-2-process-return-v3", "returncode": returncode, "timed_out": timed_out, "process_started_ns": process_started_ns, "process_ended_ns": process_ended_ns, "process_elapsed_ns": elapsed_ns})
    write_text(process_root / "RSS.txt", rss_text)
    try:
        parsed = json.loads(text_value(stdout))
        parsed_evidence = {"schema": "phase4-g5-2-parsed-product-receipt-v3", "status": "Available", "receipt": parsed}
    except Exception as error:
        parsed = None
        parsed_evidence = {"schema": "phase4-g5-2-parsed-product-receipt-v3", "status": "Unavailable", "error": f"{type(error).__name__}: {error}"}
    write_json(process_root / "PARSED-RECEIPT.json", parsed_evidence)
    rss = None
    try:
        rss = parse_rss(process_root / "RSS.txt")
    except Exception:
        pass
    files = ("COMMAND.json", "CLONE.json", "STDOUT.txt", "STDERR.txt", "RETURN.json", "RSS.txt", "PARSED-RECEIPT.json")
    evidence = {
        "schema": "phase4-g5-2-lossless-process-evidence-v3",
        "status": "PASS" if returncode == 0 and not timed_out and parsed is not None and rss is not None else "REVISE",
        "returncode": returncode, "timed_out": timed_out, "maximum_resident_set_size": rss, "process_started_ns": process_started_ns, "process_ended_ns": process_ended_ns, "process_elapsed_ns": elapsed_ns,
        "artifacts": [{"name": name, "bytes": (process_root / name).stat().st_size, "sha256": sha256(process_root / name)} for name in files],
    }
    rebind_path = process_root / "REBIND.json"
    if rebind_path.is_file():
        evidence["rebind"] = {"path": "REBIND.json", "bytes": rebind_path.stat().st_size, "sha256": sha256(rebind_path)}
    write_json(process_root / "PROCESS-EVIDENCE.json", evidence)
    return evidence, parsed


def run_analyzer(script, raw, output, self_check_authority=False):
    command = [sys.executable, str(script), str(raw), str(output)]
    if self_check_authority:
        command.append("--self-check-authority")
    return subprocess.run(command, text=True, capture_output=True)


def campaign(phase):
    require_promotion_authorized(phase)
    result = TARGET / RESULT_NAMES[phase]
    if result.exists():
        raise RuntimeError("one-shot result root already exists")
    intent = {"schema": "phase4-g5-2-lock-intent-v3", "phase": phase, "result": str(result), "freeze_sha256": sha256(FREEZE), "pid": os.getpid()}
    started = time.monotonic_ns()
    lock_ownership = acquire_lock(intent)
    attempt = None
    products = []
    fault_runs = []
    process_evidence = []
    failure = None
    terminal = None
    try:
        if sha256(FREEZE) != intent["freeze_sha256"]:
            raise RuntimeError("freeze changed across lock acquisition")
        frozen, manifest = verify_freeze()
        dry = load_json(DRY_RUN)
        if dry.get("status") != "PASS" or dry.get("product_rows") != 0 or dry.get("gate_limit_ns") != LIMITS["gate"]:
            raise RuntimeError("authoritative zero-row forecast missing")
        result.mkdir(parents=True)
        for ordinal, mode in enumerate(CAMPAIGNS[phase], 1):
            attempt = result / f"fixture-{mode}"
            source = pathlib.Path(manifest["inputs"][mode]["root"])
            process_root = result / f"PROCESS-{ordinal:02d}-{mode}"
            time_path = process_root / "RSS.txt.pending"
            command = ["/usr/bin/time", "-l", "-o", str(time_path), frozen["executable"], "--g5-projection-run", str(attempt), mode]
            begin_process_evidence(process_root, command)
            try:
                verify_sealed_fixture(source, manifest["inputs"][mode]["inventory"]["sha256"])
                subprocess.run(["/bin/cp", "-cR", str(source), str(attempt)], check=True, timeout=10)
                source_inventory, attempt_inventory = manifest["inputs"][mode]["inventory"], inventory(attempt)
                clone = {"schema": "phase4-g5-2-clone-receipt-v3", "status": "PASS", "method": "APFSCloneCpC", "inventory_equal": source_inventory["sha256"] == attempt_inventory["sha256"], "source_inventory_sha256": source_inventory["sha256"], "attempt_inventory_sha256": attempt_inventory["sha256"], "source_sealed_reverified": True, "private_attempt_permissions": "WritableAfterExactCloneInventory"}
                if not clone["inventory_equal"]:
                    raise RuntimeError(f"{mode} clone inventory mismatch")
                write_json(process_root / "CLONE.json", clone)
                make_tree_writable(attempt)
                rebind, rebind_sha256 = rebind_cloned_fixture(attempt, process_root)
                permissions = private_permission_receipt(attempt)
                clone.update({"rebind_receipt": rebind, "rebind_receipt_sha256": rebind_sha256, "private_permission_receipt": permissions, "private_permission_map_sha256": permissions["map_sha256"]})
                write_json(process_root / "CLONE.json", clone)
            except Exception as clone_error:
                write_json(process_root / "CLONE.json", {"schema": "phase4-g5-2-clone-receipt-v3", "status": "REVISE", "method": "APFSCloneCpC", "inventory_equal": False, "error": f"{type(clone_error).__name__}: {clone_error}"})
                evidence, _ = persist_process_evidence(process_root, "", f"product not started: {type(clone_error).__name__}: {clone_error}\n", None, False, "")
                process_evidence.append({"mode": mode, "path": process_root.name, "process_started": False, "sha256": sha256(process_root / "PROCESS-EVIDENCE.json")})
                raise
            remaining = max(1, (LIMITS[phase] - (time.monotonic_ns() - started)) / 1_000_000_000)
            process_started_ns = time.monotonic_ns() - started
            try:
                completed = subprocess.run(command, text=True, capture_output=True, timeout=remaining)
                stdout, stderr, returncode, timed_out = completed.stdout, completed.stderr, completed.returncode, False
            except subprocess.TimeoutExpired as error:
                stdout, stderr, returncode, timed_out = error.stdout, error.stderr, None, True
            process_ended_ns = time.monotonic_ns() - started
            rss_text = time_path.read_text(errors="replace") if time_path.exists() else ""
            evidence, product = persist_process_evidence(process_root, stdout, stderr, returncode, timed_out, rss_text, process_started_ns, process_ended_ns)
            time_path.unlink(missing_ok=True)
            fsync_dir(process_root)
            process_evidence.append({"mode": mode, "path": process_root.name, "process_started": True, "sha256": sha256(process_root / "PROCESS-EVIDENCE.json")})
            if evidence["status"] != "PASS":
                raise RuntimeError(f"{mode} product process evidence is REVISE")
            require_product_contract(product, mode, MODE_POPULATIONS[mode])
            products.append({"mode": mode, "attempt_root": str(attempt), "product": product, "maximum_resident_set_size": evidence["maximum_resident_set_size"], "clone": clone, "process_evidence_path": process_root.name, "process_evidence_sha256": process_evidence[-1]["sha256"]})
            shutil.rmtree(attempt)
            attempt = None
            fsync_dir(result)
        fault_source = pathlib.Path(manifest["inputs"]["self-check"]["root"])
        for ordinal, (fault_case, mode) in enumerate(FAULT_RUN_MODES.items(), 1):
            attempt = result / f"fixture-fault-{fault_case}"
            process_root = result / f"FAULT-{ordinal:02d}-{fault_case}"
            time_path = process_root / "RSS.txt.pending"
            command = ["/usr/bin/time", "-l", "-o", str(time_path), frozen["executable"], "--g5-projection-run", str(attempt), mode]
            begin_process_evidence(process_root, command)
            try:
                verify_sealed_fixture(fault_source, manifest["inputs"]["self-check"]["inventory"]["sha256"])
                subprocess.run(["/bin/cp", "-cR", str(fault_source), str(attempt)], check=True, timeout=10)
                source_inventory, attempt_inventory = manifest["inputs"]["self-check"]["inventory"], inventory(attempt)
                clone = {"schema": "phase4-g5-2-clone-receipt-v3", "status": "PASS", "method": "APFSCloneCpC", "inventory_equal": source_inventory["sha256"] == attempt_inventory["sha256"], "source_inventory_sha256": source_inventory["sha256"], "attempt_inventory_sha256": attempt_inventory["sha256"], "source_sealed_reverified": True, "private_attempt_permissions": "WritableAfterExactCloneInventory"}
                if not clone["inventory_equal"]:
                    raise RuntimeError(f"{fault_case} clone inventory mismatch")
                write_json(process_root / "CLONE.json", clone)
                make_tree_writable(attempt)
                rebind, rebind_sha256 = rebind_cloned_fixture(attempt, process_root)
                permissions = private_permission_receipt(attempt)
                clone.update({"rebind_receipt": rebind, "rebind_receipt_sha256": rebind_sha256, "private_permission_receipt": permissions, "private_permission_map_sha256": permissions["map_sha256"]})
                write_json(process_root / "CLONE.json", clone)
            except Exception as clone_error:
                write_json(process_root / "CLONE.json", {"schema": "phase4-g5-2-clone-receipt-v3", "status": "REVISE", "method": "APFSCloneCpC", "inventory_equal": False, "error": f"{type(clone_error).__name__}: {clone_error}"})
                evidence, _ = persist_process_evidence(process_root, "", f"product not started: {type(clone_error).__name__}: {clone_error}\n", None, False, "")
                process_evidence.append({"mode": mode, "path": process_root.name, "process_started": False, "sha256": sha256(process_root / "PROCESS-EVIDENCE.json")})
                raise
            remaining = max(1, (LIMITS[phase] - (time.monotonic_ns() - started)) / 1_000_000_000)
            process_started_ns = time.monotonic_ns() - started
            try:
                completed = subprocess.run(command, text=True, capture_output=True, timeout=remaining)
                stdout, stderr, returncode, timed_out = completed.stdout, completed.stderr, completed.returncode, False
            except subprocess.TimeoutExpired as error:
                stdout, stderr, returncode, timed_out = error.stdout, error.stderr, None, True
            process_ended_ns = time.monotonic_ns() - started
            rss_text = time_path.read_text(errors="replace") if time_path.exists() else ""
            evidence, product = persist_process_evidence(process_root, stdout, stderr, returncode, timed_out, rss_text, process_started_ns, process_ended_ns)
            time_path.unlink(missing_ok=True)
            fsync_dir(process_root)
            process_evidence.append({"mode": mode, "path": process_root.name, "process_started": True, "sha256": sha256(process_root / "PROCESS-EVIDENCE.json")})
            if evidence["status"] != "PASS":
                raise RuntimeError(f"{fault_case} product process evidence is REVISE")
            require_product_contract(product, mode, MODE_POPULATIONS["self-check"])
            matrix_row = next(row for row in fault_matrix_rows() if row["case"] == fault_case)
            fault_runs.append({"fault_class": fault_case, "matrix_row": matrix_row, "matrix_row_sha256": hashlib.sha256(compact(matrix_row).encode()).hexdigest(), "mode": mode, "attempt_root": str(attempt), "product": product, "maximum_resident_set_size": evidence["maximum_resident_set_size"], "clone": clone, "process_evidence_path": process_root.name, "process_evidence_sha256": process_evidence[-1]["sha256"]})
            shutil.rmtree(attempt)
            attempt = None
            fsync_dir(result)
        source_fault_proofs = load_json(SOURCE_FAULT_PROOFS).get("proofs", [])
        envelope = {
            "schema": "phase4-g5-2-harness-row-v3", "status": "PASS", "phase": phase,
            "analysis_stage": "preliminary",
            "product_processes": len(products), "fault_processes": len(fault_runs), "products": products,
            "fault_runs": fault_runs, "source_fault_proofs": source_fault_proofs,
            "maximum_resident_set_size": max(item["maximum_resident_set_size"] for item in products + fault_runs),
            "evidence_assembly_elapsed_ns": time.monotonic_ns() - started,
            "cache_state": "WarmUnknownPreparedFixtureAPFSClone", "cold_reopen_claim": False,
        }
        preliminary_raw = result / "RAW-PRELIMINARY-v3.jsonl"
        write_text(preliminary_raw, compact(envelope) + "\n")
        preliminary_primary = result / "PRIMARY-PRELIMINARY-v3.json"
        preliminary_independent = result / "INDEPENDENT-PRELIMINARY-v3.json"
        first = run_analyzer(EXECUTION_PRIMARY, preliminary_raw, preliminary_primary)
        second = run_analyzer(EXECUTION_INDEPENDENT, preliminary_raw, preliminary_independent)
        if first.returncode or second.returncode:
            raise RuntimeError(f"preliminary analyzer failure: {first.stdout} {second.stdout}")
        primary, independent = load_json(preliminary_primary), load_json(preliminary_independent)
        if primary.get("normalized") != independent.get("normalized"):
            raise RuntimeError("preliminary analyzer disagreement")
        released = release_lock(lock_ownership)
        lock_ownership = None
        write_json(result / "LOCK-RELEASE-v3.json", {"schema": "phase4-g5-2-lock-release-v3", "status": "PASS", "phase": phase, **released})
        complete_wall = time.monotonic_ns() - started
        terminal = {"schema": "phase4-g5-2-terminal-v3", "status": "PASS" if complete_wall <= LIMITS[phase] else "REVISE", "phase": phase, "complete_wall_ns": complete_wall, "limit_ns": LIMITS[phase], "wall_scope": "CustodyClonePrimaryAndFaultProductsCleanupOneAnalyzerPairThroughLockRelease", "lock_released": True, "product_processes": len(products) + len(fault_runs), "product_rows": len(products) + len(fault_runs), "primary_decision_rows": 1, "preliminary_analyzer_agreement": True, "terminal_fixture_roots": 0}
        write_json(result / "TERMINAL-v3.json", terminal)
        envelope["analysis_stage"] = "final"
        envelope["terminal"] = terminal
        envelope["terminal_sha256"] = sha256(result / "TERMINAL-v3.json")
        envelope["lock_release"] = load_json(result / "LOCK-RELEASE-v3.json")
        envelope["lock_release_sha256"] = sha256(result / "LOCK-RELEASE-v3.json")
        raw = result / "RAW-v3.jsonl"
        write_text(raw, compact(envelope) + "\n")
        primary_path, independent_path = result / "PRIMARY-v3.json", result / "INDEPENDENT-v3.json"
        first, second = run_analyzer(EXECUTION_PRIMARY, raw, primary_path), run_analyzer(EXECUTION_INDEPENDENT, raw, independent_path)
        if first.returncode or second.returncode:
            raise RuntimeError(f"final analyzer failure: {first.stdout} {second.stdout}")
        primary, independent = load_json(primary_path), load_json(independent_path)
        if primary.get("normalized") != independent.get("normalized"):
            raise RuntimeError("final analyzer disagreement")
        write_json(result / "FINAL-DECISION-v3.json", {"schema": "phase4-g5-2-final-decision-v3", "status": "PASS", "analysis_scope": "PostWallCustodyRecomputation", "terminal_sha256": envelope["terminal_sha256"], "primary_sha256": sha256(primary_path), "independent_sha256": sha256(independent_path), "normalized_sha256": primary["normalized_sha256"]})
    except Exception as error:
        failure = error
    finally:
        if attempt is not None and attempt.exists():
            shutil.rmtree(attempt, ignore_errors=True)
            attempt = None
            if result.exists():
                fsync_dir(result)
        if lock_ownership is not None:
            released = release_lock(lock_ownership)
            lock_ownership = None
            write_json(result / "LOCK-RELEASE-v3.json", {"schema": "phase4-g5-2-lock-release-v3", "status": "PASS", "phase": phase, **released})
        if failure is not None:
            complete_wall = time.monotonic_ns() - started
            write_json(result / "FAILED-v3.json", {"schema": "phase4-g5-2-failure-v3", "status": "REVISE", "phase": phase, "error": f"{type(failure).__name__}: {failure}", "complete_wall_ns": complete_wall, "cleanup_complete": attempt is None, "lock_released": not LOCK.exists(), "successful_product_receipts_retained": len(products), "process_evidence": process_evidence})
    if failure is not None:
        raise failure
    return terminal


def synthetic_row():
    product = {
        "schema": PRODUCT_SCHEMA, "status": "PASS", "size_bytes": 250_000,
        "route_class": "CompositePredeclaredExactCloneSparsePatchAndFullFallback", "worker_count": 1,
        "submitted": 6, "coalesced": 1, "started": 5, "published": 5, "cancelled": 0, "failed": 0, "stale": 0,
        "max_in_flight": 1, "max_pending": 1, "exact_every_root_population": 2, "latest_following_population": 2,
        "full_fallbacks": 2, "range_fetches": 2, "fetched_bytes": 2, "clone_calls": 2, "clone_failures": 0,
        "clone_successes": 2, "seed_rotations": 5,
        "foreground_transactions": 1, "foreground_commits": 1,
        "contention_worker_start_ns": 1, "contention_worker_end_ns": 4,
        "contention_foreground_start_ns": 2, "contention_foreground_end_ns": 3,
        "contention_intervals_overlap": True, "reader_barrier_autocommit": 1,
        "reader_barrier_scope_live": 1, "reader_commit_autocommit": 1,
        "reader_commit_scope_live": 0, "foreground_commit_primary_code": 0,
        "foreground_commit_extended_code": 0,
        "end_to_end_edit_t0_ns": 1, "end_to_end_canonical_ack_t1_ns": 2,
        "end_to_end_enqueue_t2_ns": 3, "end_to_end_worker_start_t3_ns": 4,
        "end_to_end_native_ack_t4_ns": 5, "end_to_end_population": 1,
        "end_to_end_scope": "ObservedEditT0CanonicalAckT1EnqueueT2WorkerT3NativeAckT4",
        "end_to_end_canonical_transactions": 1, "end_to_end_canonical_commits": 1,
        "projected_root": "root", "last_requested_root": "root", "projected_equals_last_requested": True,
        "sqlite_write_calls": 0, "sqlite_transactions": 0, "sqlite_commits": 0, "sqlite_busy_errors": 0, "sqlite_locked_errors": 0, "reconciliation_calls": 0,
        "max_buffer_bytes": 1_048_576, "exact_build_ns": [4_000_000], "exact_p50_ns": 4_000_000, "exact_p95_ns": 4_000_000,
        "sparse_build_ns": [5_000_000], "sparse_p50_ns": 5_000_000, "sparse_p95_ns": 5_000_000,
        "full_fallback_build_ns": [300_000_000], "full_fallback_p50_ns": 300_000_000, "full_fallback_p95_ns": 300_000_000,
        "full_fallback_g3_bound_ns": 329_237_000, "full_fallback_within_g3_bound": True,
        "contention_full_fallback_build_ns": [500_000_000], "contention_full_fallback_p50_ns": 500_000_000,
        "contention_full_fallback_p95_ns": 500_000_000, "contention_full_fallback_latency_claim": "NotClaimedDifferentConcurrentExecutionShape",
        "reader_initialization_ns": 1_000_000, "reader_initialization_classification": "OneTimeReadOnlyProcessInitializationInsideCompleteWallOutsideServiceSamples",
        "reader_initialization_calls": 1, "reader_initialization_bytes_requested": 1,
        "reader_initialization_sql_queries": 1, "reader_initialization_authenticated_objects": 1,
        "reader_initialization_authenticated_bytes": 64, "reader_initialization_q_high_water": 64,
        "reader_initialization_read_only": True, "reader_initialization_query_only": True,
        "reader_initialization_inside_complete_wall": True, "reader_initialization_excluded_from_service_samples": True,
        "fault_selector": "None", "fault_receipt": {"status": "NotInjectedInPerformanceRun", "complete_apply_hooks": True},
        "build_evidence": [
            {"plan": "Ranges", "parent_length": 10, "target_length": 10, "range_count": 0, "wall_ns": 4_000_000, "contention": False},
            {"plan": "Ranges", "parent_length": 10, "target_length": 10, "range_count": 1, "wall_ns": 5_000_000, "contention": False},
            {"plan": "Ranges", "parent_length": 10, "target_length": 11, "range_count": 1, "wall_ns": 300_000_000, "contention": False},
            {"plan": "Ranges", "parent_length": 11, "target_length": 11, "range_count": 1, "wall_ns": 5_000_000, "contention": False},
            {"plan": "Ranges", "parent_length": 11, "target_length": 12, "range_count": 2, "wall_ns": 500_000_000, "contention": True},
        ],
        "shutdown": "drained", "checkpoint_outside_service_timer": True,
        "terminal_in_flight": 0, "terminal_pending": 0, "terminal_workers": 0, "terminal_active_descriptors": 0,
        "terminal_successor_descriptors": 0, "terminal_temp_residue": 0, "q_terminal": 0,
        "initial_descriptor_verification_bytes": 10,
        "initial_storage_logical_bytes": 10, "initial_storage_apparent_bytes": 10,
        "initial_storage_allocated_bytes": 4096,
        "terminal_descriptor_classification": "ProvenByWorkerJoinAndOwnedDescriptorDrop",
        "terminal_storage_logical_bytes": 12, "terminal_storage_apparent_bytes": 12,
        "terminal_storage_allocated_bytes": 4096,
    }
    records = []
    for mode, population in (("self-check", [1, 0]), ("screen-count", [1, 1]), ("screen", [2, 2])):
        value = json.loads(compact(product))
        exact, latest = population
        exact_values = [4_000_000]
        sparse_values = [5_000_000] * (exact + min(latest, 2) + 1)
        started = 1 + len(sparse_values) + 2
        exact_sparse = [
            {"plan": "Ranges", "parent_length": 10, "target_length": 10, "range_count": 1, "wall_ns": 5_000_000, "contention": False, "policy": "ExactEveryRoot", "ordinal": ordinal}
            for ordinal in range(1, exact)
        ]
        latest_sparse = [
            {"plan": "Ranges", "parent_length": 10, "target_length": 10, "range_count": 1, "wall_ns": 5_000_000, "contention": False, "policy": "LatestFollowingSameSize", "ordinal": ordinal}
            for ordinal in ([] if latest == 0 else ([0] if latest == 1 else [0, latest - 1]))
        ]
        value.update({
            "submitted": exact + latest + 5, "coalesced": latest - min(latest, 2) + 1,
            "started": started, "published": started, "seed_rotations": started,
            "exact_every_root_population": exact, "latest_following_population": latest,
            "exact_build_ns": exact_values, "exact_p50_ns": 4_000_000, "exact_p95_ns": 4_000_000,
            "sparse_build_ns": sparse_values, "sparse_p50_ns": 5_000_000, "sparse_p95_ns": 5_000_000,
            "range_fetches": len(sparse_values), "clone_calls": 1 + len(sparse_values),
            "clone_successes": 1 + len(sparse_values),
            "build_evidence": (
                [{"plan": "Ranges", "parent_length": 10, "target_length": 10, "range_count": 0, "wall_ns": 4_000_000, "contention": False, "policy": "ExactEveryRoot", "ordinal": 0}]
                + [{"plan": "Ranges", "parent_length": 10, "target_length": 10, "range_count": 1, "wall_ns": 5_000_000, "contention": False, "policy": "IsolatedSparseSentinel", "ordinal": 0}]
                + exact_sparse + latest_sparse
                + [{"plan": "Ranges", "parent_length": 10, "target_length": 10, "range_count": 1, "wall_ns": 5_000_000, "contention": False, "policy": "LatestFollowingCountStorm", "ordinal": 0}]
                + [
                    {"plan": "FullFallback", "parent_length": 10, "target_length": 11, "range_count": 0, "wall_ns": 300_000_000, "contention": False, "policy": "IsolatedOrdinaryFallback", "ordinal": 0},
                    {"plan": "FullFallback", "parent_length": 11, "target_length": 12, "range_count": 0, "wall_ns": 500_000_000, "contention": True, "policy": "LatestFollowingCountStorm", "ordinal": 2},
                ]
            ),
        })
        value["mode"] = mode
        records.append({"mode": mode, "attempt_root": f"/synthetic/{mode}", "product": value, "maximum_resident_set_size": 20_000_000, "clone": {"method": "APFSCloneCpC", "inventory_equal": True, "source_sealed_reverified": True, "private_attempt_permissions": "WritableAfterExactCloneInventory"}, "process_evidence_path": f"PROCESS-{len(records)+1:02d}-{mode}", "process_evidence_sha256": "0" * 64})
    fault_runs = []
    for index, case in enumerate((row["case"] for row in fault_matrix_rows()), 1):
        value = json.loads(compact(records[0]["product"]))
        value.update(mode=f"fault-{case}", fault_selector=case, fault_receipt={"status": "ObservedCompleteApply", "complete_apply_hooks": True})
        matrix_row = next(row for row in fault_matrix_rows() if row["case"] == case)
        fault_runs.append({"fault_class": case, "matrix_row": matrix_row, "matrix_row_sha256": hashlib.sha256(compact(matrix_row).encode()).hexdigest(), "mode": value["mode"], "attempt_root": f"/synthetic/fault-{case}", "product": value, "maximum_resident_set_size": 20_000_000, "clone": {"method": "APFSCloneCpC", "inventory_equal": True, "source_sealed_reverified": True, "private_attempt_permissions": "WritableAfterExactCloneInventory"}, "process_evidence_path": f"FAULT-{index:02d}-{case}", "process_evidence_sha256": "0" * 64})
    return {"schema": "phase4-g5-2-harness-row-v3", "status": "PASS", "phase": "screen", "analysis_stage": "preliminary", "product_processes": 3, "fault_processes": len(fault_runs), "products": records, "fault_runs": fault_runs, "source_fault_proofs": [], "maximum_resident_set_size": 20_000_000, "evidence_assembly_elapsed_ns": 1_000_000_000, "cache_state": "WarmUnknownPreparedFixtureAPFSClone", "cold_reopen_claim": False}


def bind_synthetic_evidence(case_root, row):
    for record in (*row["products"], *row.get("fault_runs", [])):
        process_root = case_root / record["process_evidence_path"]
        begin_process_evidence(process_root, ["/usr/bin/time", "-l", "-o", str(process_root / "RSS.txt.pending"), "synthetic-product", "--g5-projection-run", record["attempt_root"], record["mode"]])
        permission_entries = [
            {"path": ".", "kind": "directory", "mode": 0o755},
            {"path": FIXTURE_DESCRIPTOR, "kind": "ordinary", "mode": 0o644},
            {"path": "g3-qualified-one-byte/", "kind": "directory", "mode": 0o755},
            {"path": "g3-qualified-one-byte/store.sqlite.authority", "kind": "authority", "mode": 0o600},
        ]
        permission = {"schema": "phase4-g5-2-private-clone-permissions-v3", "status": "PASS", "classification": "Directories0755Ordinary0644AuthoritySidecar0600NoSymlinks", "entries": permission_entries, "authority_files": 1, "symlinks": 0, "map_sha256": hashlib.sha256(compact(permission_entries).encode()).hexdigest()}
        rebind = {"schema": "phase4-g5-2-clone-directory-rebind-v3", "status": "PASS", "scope": "SealedCloneDirectoryPathRebindOnly", "field": "directory", "changed_fields": 1, "all_other_tsv_fields_byte_identical": True, "old_value": "/synthetic/source/g3-qualified-one-byte", "new_value": str(pathlib.Path(record["attempt_root"]) / "g3-qualified-one-byte"), "before_bytes": 1, "after_bytes": 1, "before_sha256": "a" * 64, "after_sha256": "b" * 64}
        write_json(process_root / "REBIND.json", rebind)
        record["clone"].update({"private_permission_receipt": permission, "private_permission_map_sha256": permission["map_sha256"], "rebind_receipt": rebind, "rebind_receipt_sha256": sha256(process_root / "REBIND.json")})
        clone = {"schema": "phase4-g5-2-clone-receipt-v3", "status": "PASS", **record["clone"]}
        write_json(process_root / "CLONE.json", clone)
        evidence, _ = persist_process_evidence(
            process_root, compact(record["product"]) + "\n", "", 0, False,
            f" {record['maximum_resident_set_size']}  maximum resident set size\n", 1, 2,
        )
        record["process_evidence_sha256"] = sha256(process_root / "PROCESS-EVIDENCE.json")
    if row.get("analysis_stage") == "final":
        write_json(case_root / "TERMINAL-v3.json", row["terminal"])
        row["terminal_sha256"] = sha256(case_root / "TERMINAL-v3.json")
        release = {"schema": "phase4-g5-2-lock-release-v3", "status": "PASS", "phase": row["phase"], "device": 1, "inode": 2, "ownership_token_sha256": "a" * 64, "intent_sha256": "b" * 64, "lock_absent": True}
        write_json(case_root / "LOCK-RELEASE-v3.json", release)
        row["lock_release"], row["lock_release_sha256"] = release, sha256(case_root / "LOCK-RELEASE-v3.json")


def resign_synthetic_artifact(case_root, record, name):
    process_root = case_root / record["process_evidence_path"]
    receipt = load_json(process_root / "PROCESS-EVIDENCE.json")
    artifact = next(item for item in receipt["artifacts"] if item["name"] == name)
    artifact.update(bytes=(process_root / name).stat().st_size, sha256=sha256(process_root / name))
    write_json(process_root / "PROCESS-EVIDENCE.json", receipt)
    record["process_evidence_sha256"] = sha256(process_root / "PROCESS-EVIDENCE.json")


def self_check():
    with tempfile.TemporaryDirectory() as directory:
        root = pathlib.Path(directory)
        cases = [("valid", None), ("valid-final", None), ("product-schema", ("product", "schema", "phase4-g5-projection-suite-v1")), ("product-size", ("product", "size_bytes", 250_001)), ("product-route", ("product", "route_class", "ForgedRoute")), ("pending", ("product", "max_pending", 2)), ("commit", ("product", "sqlite_commits", 1)), ("contention-tuple", ("product", "reader_commit_autocommit", 0)), ("barrier-scope", ("product", "reader_barrier_scope_live", 0)), ("commit-scope", ("product", "reader_commit_scope_live", 1)), ("t0-t4", ("product", "end_to_end_worker_start_t3_ns", 2)), ("canonical-transaction", ("product", "end_to_end_canonical_transactions", 0)), ("descriptor", ("product", "terminal_descriptor_classification", "Forged")), ("storage", ("product", "terminal_storage_allocated_bytes", -1)), ("root", ("product", "projected_root", "wrong")), ("rss", (None, "maximum_resident_set_size", 40_000_000)), ("terminal", ("product", "terminal_workers", 1)), ("population", ("product", "exact_every_root_population", 3)), ("fault-population", None), ("authority-mode", None), ("claim", (None, "cold_reopen_claim", True)), ("plural-fault-receipts", None), ("fault-mode-skipped", None), ("fault-matrix-case-mismatch", None), ("forged-source-proof", None), ("stdout-mismatch", None), ("command-mismatch", None), ("policy-forgery", None), ("policy-route-constant", None), ("route-forgery", None), ("terminal-forgery", None), ("nested-artifact-forgery", None)]
        for label, mutation in cases:
            row = json.loads(compact(synthetic_row()))
            if mutation:
                scope, key, value = mutation
                (row if scope is None else row["products"][-1]["product"])[key] = value
            if label == "policy-forgery":
                product = row["products"][-1]["product"]
                product["exact_every_root_population"] = 3
                product.update({"submitted": 10, "started": 9, "published": 9, "seed_rotations": 9, "coalesced": 1})
                forged = {"plan": "Ranges", "parent_length": 10, "target_length": 10, "range_count": 1, "wall_ns": 5_000_000, "contention": False, "policy": "ExactEveryRoot", "ordinal": 2}
                product["build_evidence"].insert(-2, forged)
                product["sparse_build_ns"].append(5_000_000)
                product["range_fetches"] += 1
                product["clone_calls"] += 1
                product["clone_successes"] += 1
            if label == "plural-fault-receipts":
                row["products"][-1]["product"]["fault_receipts"] = []
            if label == "fault-population":
                row["fault_runs"][0]["product"]["latest_following_population"] = 1
            if label == "fault-mode-skipped":
                row["fault_runs"].pop()
            if label == "fault-matrix-case-mismatch":
                row["fault_runs"][0]["matrix_row"] = row["fault_runs"][1]["matrix_row"]
            if label == "forged-source-proof":
                removed = row["fault_runs"].pop()
                row["source_fault_proofs"] = [{"fault_class": removed["fault_class"], "classification": "ProvenByConstruction", "source_path": "forged", "source_sha256": "0" * 64, "execution_receipt_path": "forged", "execution_receipt_sha256": "0" * 64, "matrix_row": removed["matrix_row"], "matrix_row_sha256": removed["matrix_row_sha256"], "test_locator": "forged", "typed_outcome": "forged", "counter_claims": {"q_terminal": 0}}]
            if label == "route-forgery":
                product = row["products"][-1]["product"]
                sparse = next(item for item in product["build_evidence"] if item["range_count"] == 1 and item["parent_length"] == item["target_length"])
                sparse["range_count"] = 0
                product["exact_build_ns"].append(sparse["wall_ns"])
                product["sparse_build_ns"].remove(sparse["wall_ns"])
                product["exact_p50_ns"], product["exact_p95_ns"] = 4_000_000, 5_000_000
            if label == "policy-route-constant":
                product = row["products"][-1]["product"]
                target = next(item for item in product["build_evidence"] if item["policy"] == "IsolatedSparseSentinel")
                target["policy"] = "ExactEveryRoot"
                target["ordinal"] = 99
            if label == "terminal-forgery":
                terminal = {"schema": "phase4-g5-2-terminal-v3", "status": "PASS", "complete_wall_ns": LIMITS["screen"] + 1, "limit_ns": LIMITS["screen"], "lock_released": True, "terminal_fixture_roots": 0, "product_processes": 15, "product_rows": 15}
                row.update({"analysis_stage": "final", "terminal": terminal, "terminal_sha256": hashlib.sha256((compact(terminal) + "\n").encode()).hexdigest()})
            if label == "valid-final":
                terminal = {"schema": "phase4-g5-2-terminal-v3", "status": "PASS", "complete_wall_ns": 1_000_000_000, "limit_ns": LIMITS["screen"], "lock_released": True, "terminal_fixture_roots": 0, "product_processes": 15, "product_rows": 15}
                row.update({"analysis_stage": "final", "terminal": terminal, "terminal_sha256": hashlib.sha256((compact(terminal) + "\n").encode()).hexdigest()})
            case_root = root / label
            case_root.mkdir()
            bind_synthetic_evidence(case_root, row)
            if label == "authority-mode":
                record = row["products"][-1]
                clone = record["clone"]
                next(item for item in clone["private_permission_receipt"]["entries"] if item["kind"] == "authority")["mode"] = 0o644
                clone["private_permission_receipt"]["map_sha256"] = hashlib.sha256(compact(clone["private_permission_receipt"]["entries"]).encode()).hexdigest()
                clone["private_permission_map_sha256"] = clone["private_permission_receipt"]["map_sha256"]
                write_json(case_root / record["process_evidence_path"] / "CLONE.json", {"schema": "phase4-g5-2-clone-receipt-v3", "status": "PASS", **clone})
                resign_synthetic_artifact(case_root, record, "CLONE.json")
            if label == "stdout-mismatch":
                record = row["products"][0]
                write_text(case_root / record["process_evidence_path"] / "STDOUT.txt", compact({"forged": True}) + "\n")
                resign_synthetic_artifact(case_root, record, "STDOUT.txt")
            if label == "command-mismatch":
                record = row["products"][0]
                write_json(case_root / record["process_evidence_path"] / "COMMAND.json", {"schema": "phase4-g5-2-process-command-v3", "argv": ["synthetic-product", record["mode"]]})
                resign_synthetic_artifact(case_root, record, "COMMAND.json")
            if label == "nested-artifact-forgery":
                target = case_root / row["products"][0]["process_evidence_path"] / "STDOUT.txt"
                target.write_text(target.read_text() + "forged")
            raw = case_root / f"{label}.jsonl"
            raw.write_text(compact(row) + "\n")
            reports = []
            for script, name in ((EXECUTION_PRIMARY, "p"), (EXECUTION_INDEPENDENT, "i")):
                output = root / f"{label}-{name}.json"
                run_analyzer(script, raw, output, self_check_authority=True)
                reports.append(load_json(output)["normalized"])
            if label in ("valid", "valid-final") and (reports[0] != reports[1] or reports[0]["status"] != "PASS"):
                raise RuntimeError("valid analyzer agreement failed")
            if label not in ("valid", "valid-final") and any(report["status"] != "REVISE" for report in reports):
                raise RuntimeError(f"mutation accepted: {label}")
        process_root = root / "partial-process"
        begin_process_evidence(process_root, ["fake-product", "--mode", "self-check"])
        write_json(process_root / "CLONE.json", {"schema": "phase4-g5-2-clone-receipt-v3", "status": "PASS", "method": "APFSCloneCpC", "inventory_equal": True})
        evidence, parsed = persist_process_evidence(process_root, compact({"schema": "synthetic-product", "status": "PASS"}) + "\n", "later process failure\n", 7, False, " 20000000  maximum resident set size\n", 1, 2)
        retained = all((process_root / name).is_file() for name in ("COMMAND.json", "CLONE.json", "STDOUT.txt", "STDERR.txt", "RETURN.json", "RSS.txt", "PARSED-RECEIPT.json", "PROCESS-EVIDENCE.json"))
        if evidence["status"] != "REVISE" or parsed is None or not retained:
            raise RuntimeError("partial process evidence was not retained losslessly")
        compact_root = root / "fixture-contract"
        compact_root.mkdir()
        write_text(compact_root / "parent.source", "p")
        write_text(compact_root / "latest.source", "l")
        write_text(compact_root / "G5-PROJECTION-FIXTURE-v1.tsv", "forbidden\n")
        try:
            inventory(compact_root)
        except RuntimeError:
            pass
        else:
            raise RuntimeError("legacy v1 fixture descriptor was accepted")
        oversized = root / "fixture-oversized-embedded-token"
        oversized.mkdir()
        write_text(oversized / "parent.source", "p")
        write_text(oversized / "latest.source", "l")
        write_text(oversized / FIXTURE_DESCRIPTOR, "storm_a_token\t" + "x" * (8_388_608 + 1) + "\n")
        try:
            inventory(oversized)
        except RuntimeError:
            pass
        else:
            raise RuntimeError("oversized fixture-embedded token was accepted")
        rebind_root = root / "rebind-unit"
        (rebind_root / "g3-qualified-one-byte").mkdir(parents=True)
        write_text(rebind_root / "g3-qualified-one-byte/store.sqlite.authority", "x" * 32)
        write_text(rebind_root / FIXTURE_DESCRIPTOR, "directory\t/frozen/source/g3-qualified-one-byte\nparent_namespace\tabc\n")
        rebind_process = root / "rebind-process"
        begin_process_evidence(rebind_process, ["fake-product", "--g5-projection-run", str(rebind_root), "self-check"])
        make_tree_writable(rebind_root)
        receipt, receipt_sha256 = rebind_cloned_fixture(rebind_root, rebind_process)
        permissions = private_permission_receipt(rebind_root)
        command = load_json(rebind_process / "COMMAND.json")
        if receipt.get("changed_fields") != 1 or receipt.get("all_other_tsv_fields_byte_identical") is not True or command.get("fixture_rebind_receipt_sha256") != receipt_sha256 or not (rebind_process / "REBIND.json").is_file() or permissions.get("authority_files") != 1 or permissions.get("classification") != "Directories0755Ordinary0644AuthoritySidecar0600NoSymlinks":
            raise RuntimeError("attempt-3 clone rebind/permission self-check failed")
        contract = load_json(METHOD_CONTRACT)
        if contract.get("product_schema") != PRODUCT_SCHEMA or contract.get("fixture_descriptor") != FIXTURE_DESCRIPTOR:
            raise RuntimeError("runner/method product vocabulary mismatch")
        if contract.get("fixture_mode_size_bytes") != {"self-check": 250_000, "screen-count": 250_000, "screen": 250_000, "gate": 250_000} or contract.get("route_class") != "CompositePredeclaredExactCloneSparsePatchAndFullFallback" or contract.get("preparation_preferred_wall_ns") != 20_000_000_000 or contract.get("preparation_complete_wall_limit_ns") != 60_000_000_000 or contract.get("compact_fixture_limits") != {"max_files": 8, "max_file_bytes": 100_000_000, "max_input_root_bytes": 10_000_000, "max_aggregate_bytes": 10_000_000, "max_token_or_edit_bytes": 8_388_608} or contract.get("input_sealing") != {"files_mode": 0o444, "directories_mode": 0o555, "fsync_before_seal": True, "reopen_rehash_mode_check": True, "reverify_before_each_clone": True}:
            raise RuntimeError("runner/method preparation size-wall contract mismatch")
        if any("schedule_sha256" not in path.read_text() or "fault_matrix_sha256" not in path.read_text() for path in (pathlib.Path(__file__), PRIMARY, INDEPENDENT, EXECUTION_PRIMARY, EXECUTION_INDEPENDENT)):
            raise RuntimeError("freeze authority hash guard missing")
        try:
            verify_frozen_method_fields({"method_contract_sha256": sha256(METHOD_CONTRACT), "fault_matrix_sha256": sha256(FAULT_MATRIX)})
        except RuntimeError:
            pass
        else:
            raise RuntimeError("missing schedule hash was accepted")
        binding = load_json(FINAL_SOURCE_BINDING)
        execution, proofs = load_json(FOCUSED_FAULT_EXECUTION), load_json(SOURCE_FAULT_PROOFS)
        if binding.get("status") != "PASS" or binding.get("source", {}).get("sha256") != sha256(REPO / "crates/layerfs-engine/src/bin/phase4_g3_materialization.rs") or binding.get("executable", {}).get("sha256") != sha256(PRODUCT_RELEASE) or binding.get("focused_tests", {}).get("passed") != 26:
            raise RuntimeError("final2 exact source/executable/focused binding mismatch")
        if binding.get("focused_fault_execution", {}).get("sha256") != sha256(FOCUSED_FAULT_EXECUTION) or binding.get("source_fault_proofs", {}).get("sha256") != sha256(SOURCE_FAULT_PROOFS) or execution.get("status") != "PASS" or execution.get("source_sha256") != binding["source"]["sha256"]:
            raise RuntimeError("focused fault authority mismatch")
        expected_focused = {row["case"] for row in fault_matrix_rows()} - set(FAULT_RUN_MODES)
        if len(execution.get("cases", [])) != 10 or {case.get("fault_class") for case in execution["cases"]} != expected_focused or {proof.get("fault_class") for proof in proofs.get("proofs", [])} != expected_focused or any(proof.get("classification") != "ObservedFocusedCompleteApply" for proof in proofs.get("proofs", [])):
            raise RuntimeError("exact 12-case campaign/focused classification mismatch")
        for artifact in execution.get("raw_artifacts", []):
            path = HERE / "evidence/raw-final2" / artifact["name"]
            if not path.is_file() or path.stat().st_size != artifact["bytes"] or sha256(path) != artifact["sha256"]:
                raise RuntimeError("final2 raw artifact mismatch")
    result = {"schema": "phase4-g5-2-runner-self-check-v3-attempt-3", "status": "PASS", "checks": len(cases) + 11, "product_processes": 0, "product_rows": 0, "partial_process_evidence_retained": True, "directory_rebind_exact": True, "private_permission_map_exact": True}
    print(compact(result))
    return result


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("action", choices=("self-check", "prepare-inputs", "freeze", "forecast", "screen", "gate"))
    parser.add_argument("--executable")
    parser.add_argument("--input-root")
    args = parser.parse_args()
    verify_attempt_3_binding()
    if args.action == "self-check":
        self_check()
    elif args.action == "prepare-inputs":
        print(compact(prepare_inputs(args.executable, args.input_root)))
    elif args.action == "freeze":
        print(compact(freeze(args.executable)))
    elif args.action == "forecast":
        print(compact(forecast()))
    else:
        print(compact(campaign(args.action)))


if __name__ == "__main__":
    raise SystemExit(main())
