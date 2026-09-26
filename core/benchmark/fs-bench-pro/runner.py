#!/usr/bin/env python3
"""One-sample release-only SDK Init runner with append-only evidence."""
import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import re
import resource
import shutil
import subprocess
import sys
import time
import uuid

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
CORE = ROOT / "core"
RESULTS = ROOT / "benchmark-results/fs-bench-pro"
sys.path.insert(0, str(HERE))
from families import init_namespace as init  # noqa: E402
from shared import edit_contract as edit  # noqa: E402
from shared import edit_insert_v3  # noqa: E402
from shared import edit_insert_v4  # noqa: E402
from shared import edit_route  # noqa: E402

CONTRACT_COMMIT = "6dfd0c7cbcbe9036f69b834e1704f2126f95c5a2"
BUILD_PROFILE = "release"
BINARY_DIR = "release/examples"
VERIFY_TIMEOUT_S = 9.5
SAMPLE_POLICY = "stride64-size-log2-endpoints-v1"
BUILD = ["cargo", "+1.85.1", "build", "--release", "--manifest-path", "core/Cargo.toml", "--locked",
         "-p", "layerfs-sdk", "-p", "layerfs-server",
         "--example", "benchmark_init", "--example", "verify_namespace"]
BINARIES = ("benchmark_init", "verify_namespace")


# A registered performance driver may name the SDK for product operations, the
# server package only to compose the authority, and telemetry only to observe.
DRIVER_CRATES = {"layerfs_sdk", "layerfs_server", "layerfs_telemetry"}
COMPOSITION_ITEMS = {"Server", "ServerConfig", "HistoryMode"}
OBSERVATION_ITEMS = {"runtime", "output", "operation", "timer"}


def require_sdk_driver(name, source=None):
    """Reject a performance driver that bypasses the public SDK package."""
    source = Path(source) if source else (
        CORE / "crates/layerfs-api/sdk/examples" / f"{name}.rs")
    code = source.read_text()
    foreign = sorted(set(re.findall(r"\blayerfs_[a-z0-9_]+\b", code)) - DRIVER_CRATES)
    composition = set(re.findall(r"layerfs_server::([A-Za-z_][A-Za-z0-9_]*)", code))
    observation = set(re.findall(r"layerfs_telemetry::([A-Za-z_][A-Za-z0-9_]*)", code))
    if ("use layerfs_sdk" not in code or foreign or composition - COMPOSITION_ITEMS
            or observation - OBSERVATION_ITEMS
            or re.search(
                r"\b(?:std::fs::(?:write|create|remove|rename|copy|set_permissions|hard_link)"
                r"|std::process::Command|Command::new|File::create|OpenOptions::new)\b", code)):
        raise ValueError(f"{name} must use public layerfs-sdk for every product operation; "
                         f"foreign={foreign} composition={sorted(composition)} "
                         f"observation={sorted(observation)}")


def digest(path):
    hash_ = hashlib.sha256()
    with Path(path).open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            hash_.update(block)
    return hash_.hexdigest()


def write_json(path, value):
    path.write_text(json.dumps(value, sort_keys=True, indent=2) + "\n")


def owned(path, *, existing=False):
    path = Path(path).expanduser().resolve(strict=False)
    root = RESULTS.resolve(strict=False)
    if not path.is_relative_to(root) or path == root or path.exists() != existing:
        raise ValueError("output must be a fresh path under this worktree's benchmark-results/fs-bench-pro")
    return path


def target_path():
    supplied = os.environ.get("CARGO_TARGET_DIR")
    if supplied and not Path(supplied).expanduser().resolve().is_relative_to(ROOT):
        raise ValueError("foreign CARGO_TARGET_DIR")
    metadata = subprocess.check_output(
        ["cargo", "metadata", "--manifest-path", "core/Cargo.toml", "--locked", "--no-deps", "--format-version", "1"],
        cwd=ROOT, text=True)
    target = Path(json.loads(metadata)["target_directory"]).resolve()
    if not target.is_relative_to(ROOT):
        raise ValueError("Cargo resolved a foreign target")
    return target


def seal(paths):
    hash_ = hashlib.sha256()
    for path in sorted(paths):
        hash_.update(str(path.relative_to(ROOT)).encode() + b"\0")
        hash_.update(path.read_bytes())
    return hash_.hexdigest()


def edit_workload_seal():
    workload = HERE / "workload"
    paths = list((workload / "src").rglob("*.rs"))
    paths += [workload / "Cargo.toml", workload / "Cargo.lock"]
    return seal(paths)


def identities():
    product = list((CORE / "crates").glob("*/src/**/*.rs"))
    product += list((CORE / "crates").glob("*/examples/*.rs"))
    product += list((CORE / "crates").glob("*/Cargo.toml"))
    product += list((CORE / "crates/layerfs-api").glob("*/src/**/*.rs"))
    product += list((CORE / "crates/layerfs-api").glob("*/examples/*.rs"))
    product += list((CORE / "crates/layerfs-api").glob("*/Cargo.toml"))
    product += [CORE / "Cargo.toml", CORE / "Cargo.lock", ROOT / ".cargo/config.toml"]
    harness = list(HERE.glob("**/*.py"))
    source = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    tree = subprocess.check_output(["git", "rev-parse", "HEAD^{tree}"], cwd=ROOT, text=True).strip()
    dirty = subprocess.check_output(["git", "status", "--porcelain"], cwd=ROOT, text=True)
    return {"source_commit": source, "source_tree": tree, "source_dirty": bool(dirty),
            "dirty_paths": dirty.splitlines(), "product_seal": seal(product),
            "harness_seal": seal(harness), "cargo_lock_sha256": digest(CORE / "Cargo.lock"),
            "contract_commit": CONTRACT_COMMIT, "build_profile": BUILD_PROFILE}


def build(out, target, identity):
    for name in BINARIES:
        if name.startswith("benchmark_"):
            require_sdk_driver(name)
    cache = RESULTS / "sdk-build-release.json"
    prior = json.loads(cache.read_text()) if cache.exists() else None
    if prior and prior.get("build_profile") == BUILD_PROFILE and prior.get("product_seal") == identity["product_seal"] and all(
        Path(prior["binaries"][name]["path"]).is_file() and
        digest(prior["binaries"][name]["path"]) == prior["binaries"][name]["sha256"]
        for name in BINARIES
    ):
        return {"status": "PASS", "mode": "exact-binary-reuse", "wall_ns": 0,
                "command": None, "build_profile": BUILD_PROFILE, "binaries": prior["binaries"]}
    started = time.monotonic_ns()
    with (out / "build.log").open("wb") as log:
        process = subprocess.run(BUILD, cwd=ROOT,
                                 env={**os.environ, "CARGO_TARGET_DIR": str(target)},
                                 stdout=log, stderr=subprocess.STDOUT)
    wall = time.monotonic_ns() - started
    record = {"status": "BUILD_SLOW" if wall > 30_000_000_000 else "PASS" if process.returncode == 0 else "FAIL",
              "mode": "changed-product" if prior else "first-use", "wall_ns": wall,
              "budget_ns": 30_000_000_000, "exit_code": process.returncode,
              "compiled_units": (out / "build.log").read_text(errors="replace").count("Compiling "),
              "command": BUILD, "target": str(target), "build_profile": BUILD_PROFILE}
    if process.returncode == 0:
        binaries = {}
        for name in BINARIES:
            source = target / BINARY_DIR / name
            binary_sha = digest(source)
            archive = RESULTS / "binary-archive" / binary_sha / name
            archive.parent.mkdir(parents=True, exist_ok=True)
            if not archive.exists():
                shutil.copy2(source, archive)
                archive.chmod(0o555)
            binaries[name] = {"path": str(archive), "sha256": binary_sha}
        record["binaries"] = binaries
        if record["status"] == "PASS":
            write_json(cache, {"product_seal": identity["product_seal"],
                               "build_profile": BUILD_PROFILE, "binaries": binaries})
    return record


def competing_work():
    process = subprocess.run(["ps", "-axo", "pid=,comm="], capture_output=True, text=True)
    return [line.strip() for line in process.stdout.splitlines()
            if any(word in line.lower() for word in ("cargo", "docker", "layerfs")) and
            not line.strip().startswith(str(os.getpid()) + " ")][:32]


def lite_verification_pass(child, case, sample, fixture):
    return (isinstance(child, dict) and child.get("status") == "PASS"
            and child.get("paths") == case.files + case.directories + 1
            and child.get("discovered_files") == case.files
            and child.get("directories") == case.directories + 1
            and child.get("manifest_bytes") == case.logical_bytes
            and isinstance(child.get("sampled_files"), int) and 0 < child["sampled_files"] <= 195
            and isinstance(child.get("sampled_bytes"), int) and 0 < child["sampled_bytes"] <= case.logical_bytes
            and child.get("sample_policy") == SAMPLE_POLICY and child.get("workers") == 4
            and child.get("root") == sample["root"]
            and child.get("manifest_sha256") == fixture["manifest_sha256"])


def verify_child(binary, folder, store, history, sample, fixture, cursor, case):
    command = [binary, str(store), str(history), sample["root"], sample["stack_body"],
               fixture["manifest"], fixture["manifest_sha256"]]
    started = time.monotonic_ns()
    try:
        result = subprocess.run(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=VERIFY_TIMEOUT_S,
                                env={**os.environ, "LAYERFS_HISTORY_CURSOR_KEY": cursor})
        wall = time.monotonic_ns() - started
        stdout, stderr = result.stdout, result.stderr
        try:
            child = json.loads(stdout) if result.returncode == 0 else None
        except (ValueError, UnicodeDecodeError):
            child = None
        status = "PASS" if lite_verification_pass(child, case, sample, fixture) else "FAIL"
        exit_code = result.returncode
    except subprocess.TimeoutExpired as error:
        wall = time.monotonic_ns() - started
        stdout, stderr = error.stdout or b"", error.stderr or b""
        child, status, exit_code = None, "TIMEOUT", None
    (folder / "verifier.stdout").write_bytes(stdout)
    (folder / "verifier.stderr").write_bytes(stderr)
    receipt = {"status": status, "scope": "complete paths/kinds; sampled file metadata/content",
               "wall_ns": wall, "budget_ns": int(VERIFY_TIMEOUT_S * 1_000_000_000),
               "exit_code": exit_code, "command": command, "child": child,
               "stderr": stderr[:4096].decode(errors="replace")}
    write_json(folder / "verification.json", receipt)
    return receipt


def case_run(out, case, binaries, identity):
    folder = out / "sdk-host" / "init_namespace" / case.id
    folder.mkdir(parents=True)
    receipt = {"schema": "core-fs-bench-pro-sdk-init-release-v5-lite", "case": case.id,
               "build_profile": BUILD_PROFILE,
               "verification_scope": "complete paths/kinds; sampled file metadata/content",
               "sample_policy": SAMPLE_POLICY,
               "benchmark_registration": ("REGISTERED_SDK_RELEASE_V5_LITE" if case.id in init.SELECTED
                                          else "UNREGISTERED_DIAGNOSTIC"),
               "family_id": "init_namespace", "scenario_id": case.id, "scenario_version": 5,
               "route": init.ROUTE, "fixture_profile": init.PROFILE, "seed": 1,
               "operation_contract_id": "sdk-init-project-host-v1",
               "operation_surface": "layerfs-sdk", "operation_entrypoint": "Client::init_project",
               "orchestration_executor": "Python host runner", "mutation_executor": "host Service",
               "timing_boundary_id": "sdk-init-call-v1", "clock_id": "Rust std::time::Instant",
               "start_event": "immediately before Client::init_project",
               "end_event": "immediately after typed result",
               "identity": identity, "sample_count": 0, "public_api_call_count": None,
               "attempted_operation_count": None, "completed_operation_count": 0,
               "forbidden_route_count": 0, "fallback_count": 0,
               "cache_contract": "source-cache-uncontrolled-v1", "admission_eligible": False,
               "performance_gate": "NOT_FROZEN", "performance_status": "INELIGIBLE",
               "telemetry_status": "UNAVAILABLE: direct SDK driver has no LFT1 capture",
               "placement": {"sdk": "host", "service": "host", "store": "host",
                             "daemon": None, "image_id": None},
               "run_id": uuid.uuid4().hex, "competing_work": competing_work()}
    try:
        fixture = init.prepare(case, RESULTS / "sdk-prepared")
        receipt["fixture"] = fixture
        private, cursor = os.urandom(32).hex(), os.urandom(32).hex()
        store, history = folder / "store.sqlite", folder / "history.sqlite"
        before = resource.getrusage(resource.RUSAGE_CHILDREN)
        perf, wall, stdout, stderr, exit_code, timed_out, command = init.public_import(
            binaries["benchmark_init"]["path"], case, Path(fixture["source"]), store, history,
            private, cursor)
        after = resource.getrusage(resource.RUSAGE_CHILDREN)
        (folder / "driver.stdout").write_bytes(stdout)
        (folder / "driver.stderr").write_bytes(stderr)
        (folder / "perf.jsonl").write_text(json.dumps(perf, sort_keys=True) + "\n")
        entered = isinstance(perf.get("operation_ns"), int)
        complete = perf.get("status") == "COMPLETE" and exit_code == 0 and not timed_out
        receipt.update(command=command, sample_count=int(entered),
                       public_api_call_count=1 if entered else None,
                       attempted_operation_count=1 if entered else None,
                       completed_operation_count=int(complete),
                       raw_operation_ns=perf.get("operation_ns"),
                       performance_command_wall_ns=wall,
                       performance_command_budget_ns=15_000_000_000,
                       performance_command_status="PASS" if wall <= 15_000_000_000 and not timed_out else "COMMAND_SLOW",
                       driver_exit_code=exit_code, driver_timeout=timed_out,
                       external_resources={"scope": "driver-process-lifecycle",
                           "user_cpu_s": after.ru_utime - before.ru_utime,
                           "system_cpu_s": after.ru_stime - before.ru_stime,
                           "rss_phase_peak": None, "rss_reason": "rusage peak is lifetime, not phase"})
        if complete:
            verification = verify_child(binaries["verify_namespace"]["path"],
                                        folder, store, history, perf, fixture, cursor, case)
            receipt["verification"] = verification
            receipt["functional_status"] = "PASS" if (
                receipt["performance_command_status"] == "PASS" and
                verification["status"] == "PASS" and verification["wall_ns"] <= int(VERIFY_TIMEOUT_S * 1_000_000_000)
            ) else "FAIL"
        else:
            receipt["verification"] = {"status": "NOT_RUN", "reason": "no confirmed SDK root"}
            write_json(folder / "verification.json", receipt["verification"])
            receipt["functional_status"] = "FAIL"
        receipt["cleanup_status"] = "PASS" if not timed_out else "UNKNOWN"
        receipt["verification_status"] = receipt["verification"]["status"]
        receipt["resource_status"] = "lifecycle CPU only; phase RSS unavailable"
        receipt["custody_status"] = "PASS"
        receipt["status"] = "INELIGIBLE" if receipt["functional_status"] == "PASS" else "FAIL"
        receipt["store_bytes"] = store.stat().st_size if store.exists() else None
        receipt["history_bytes"] = history.stat().st_size if history.exists() else None
    except Exception as error:
        receipt["status"] = receipt["functional_status"] = "FAIL"
        receipt["error"] = repr(error)
    finally:
        write_json(folder / "receipt.json", receipt)
    return receipt


def report(run):
    rows = []
    for case in init.CASES.values():
        file = run / "sdk-host" / "init_namespace" / case.id / "receipt.json"
        if not file.exists():
            raise ValueError(f"missing registered SDK case: {case.id}")
        row = json.loads(file.read_text())
        rows.append(f"{case.id}\t{row['sample_count']}\t{row.get('raw_operation_ns')}\t"
                    f"{row.get('performance_command_wall_ns')}\t"
                    f"{row.get('verification', {}).get('wall_ns')}\t"
                    f"{row.get('functional_status')}\t{row['status']}\t{row['cache_contract']}\t"
                    f"{row.get('verification_scope')}")
    return ("case\tsamples\toperation_ns\tcommand_ns\tverification_ns\tfunctional\t"
            "performance\tcache\tverification_scope\n" + "\n".join(rows) + "\n")


def manifest_run(run):
    items = {}
    for file in sorted(run.rglob("*")):
        if file.is_file() and file.name != "manifest.json":
            items[str(file.relative_to(run))] = {"bytes": file.stat().st_size, "sha256": digest(file)}
    write_json(run / "manifest.json", {"schema": "core-fs-bench-pro-manifest-v1", "files": items})


def verify_run(run):
    recorded = json.loads((run / "manifest.json").read_text())
    actual = {str(path.relative_to(run)) for path in run.rglob("*") if path.is_file() and path.name != "manifest.json"}
    if actual != set(recorded["files"]):
        raise ValueError("retained evidence inventory mismatch")
    for name, expected in recorded["files"].items():
        path = run / name
        if not path.is_file() or path.stat().st_size != expected["bytes"] or digest(path) != expected["sha256"]:
            raise ValueError(f"retained evidence mismatch: {name}")
    for case in init.CASES.values():
        folder = run / "sdk-host" / "init_namespace" / case.id
        receipt = json.loads((folder / "receipt.json").read_text())
        if receipt["sample_count"] not in (0, 1):
            raise ValueError("invalid one-sample cardinality")
        if receipt.get("schema") == "core-fs-bench-pro-sdk-init-release-v5-lite" and (
            receipt.get("build_profile") != BUILD_PROFILE
            or receipt.get("verification_scope") != "complete paths/kinds; sampled file metadata/content"
            or receipt.get("sample_policy") != SAMPLE_POLICY
        ):
            raise ValueError("invalid lite verifier identity")
        if receipt["sample_count"] == 1:
            lines = (folder / "perf.jsonl").read_text().splitlines()
            if len(lines) != 1 or json.loads(lines[0])["operation_ns"] != receipt["raw_operation_ns"]:
                raise ValueError("raw SDK timing mismatch")
            if json.loads((folder / "verification.json").read_text()) != receipt["verification"]:
                raise ValueError("SDK verification receipt mismatch")
    return "PASS"


def fill_not_run(out, selection, blocked=None):
    for case in init.CASES.values():
        folder = out / "sdk-host" / "init_namespace" / case.id
        if folder.exists():
            continue
        folder.mkdir(parents=True)
        selected = selection == "init_namespace" and case.id in init.SELECTED or selection == case.id
        reason = blocked if selected and blocked else "not selected" if case.id in init.SELECTED else init.NOT_RUN_REASON
        write_json(folder / "receipt.json", {"case": case.id, "status": "NOT_RUN", "sample_count": 0,
                                           "build_profile": BUILD_PROFILE,
                                           "verification_scope": "complete paths/kinds; sampled file metadata/content",
                                           "functional_status": "NOT_RUN", "cache_contract": "source-cache-uncontrolled-v1",
                                           "reason": reason})


def run(selection, out):
    out = owned(out)
    identity = identities()
    if identity["source_dirty"]:
        raise ValueError("commit the SDK route before collecting benchmark samples")
    target = target_path()
    out.mkdir(parents=True)
    build_receipt = build(out, target, identity)
    write_json(out / "build.json", build_receipt)
    cycle_started = time.monotonic_ns()
    blocked = build_receipt["status"] if build_receipt["status"] != "PASS" else None
    if blocked is None:
        RESULTS.mkdir(parents=True, exist_ok=True)
        with (RESULTS / ".run.lock").open("a+b") as lock:
            try:
                fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            except BlockingIOError:
                blocked = "same-worktree run active"
            if blocked is None:
                cases = [init.CASES[selection]] if selection in init.CASES else [init.CASES[name] for name in init.SELECTED]
                for case in cases:
                    case_run(out, case, build_receipt["binaries"], identity)
    fill_not_run(out, selection, blocked)
    write_json(out / "run.json", {"schema": "core-fs-bench-pro-sdk-run-release-v5-lite", "selection": selection,
        "build_profile": BUILD_PROFILE,
        "verification_scope": "complete paths/kinds; sampled file metadata/content",
        "benchmark_registration": ("UNREGISTERED_DIAGNOSTIC" if selection in init.CASES
                                   and selection not in init.SELECTED else "REGISTERED_SDK_RELEASE_V5_LITE"),
        "cases": list(init.SELECTED), "identity": identity, "blocked": blocked,
        "family_cycle_wall_ns": time.monotonic_ns() - cycle_started,
        "family_cycle_budget_ns": 30_000_000_000})
    (out / "report.txt").write_text(report(out))
    manifest_run(out)
    return out


EXEC_EDIT_BUILD = ["cargo", "+1.85.1", "build", "--manifest-path", "core/Cargo.toml", "--locked",
                  "--offline", "--release", "-p", "layerfs-sdk", "-p", "layerfs-server",
                  "--example", "benchmark_edit", "--example", "verify_edit",
                  "--example", "benchmark_init"]
EXEC_EDIT_BINARIES = {"benchmark_edit": "benchmark_edit", "verify_edit": "verify_edit",
                     "benchmark_init": "benchmark_init"}


def edit_selection(scenario_id):
    """Resolve a case against its own sealed registry, never the other version."""
    contracts = ((edit_insert_v4,) if scenario_id.endswith("-exec-v4") else
                 (edit_insert_v3,) if scenario_id.endswith("-exec-v3") else (edit,))
    for contract in contracts:
        rows = {row["scenario_id"]: row for row in contract.registry()}
        if scenario_id in rows:
            path = HERE / contract.REGISTRY_PATH
            if digest(path) != contract.REGISTRY_SHA256:
                raise ValueError("committed edit registry hash differs from frozen identity")
            if contract in (edit_insert_v3, edit_insert_v4):
                contract.validate_registry()
            return contract, rows[scenario_id]
    raise ValueError("unregistered Exec/FUSE scenario")


def require_edit_image(image, contract, row, identity=None):
    """Reject an image from another registry, build profile or payload recipe."""
    expected_schema = (f"core-fs-bench-pro-exec-fuse-insert-image-v{contract.SCENARIO_VERSION}" if
                       contract in (edit_insert_v3, edit_insert_v4) else
                       "core-fs-bench-pro-exec-fuse-edit-image-v1")
    payload = row["payload_source"].rsplit("/", 1)[-1] if row["payload_source"] else None
    payload_ok = (payload is None or (image.get("payloads") or {}).get(payload) ==
                  {"bytes": row["replacement_len"], "sha256": row["replacement_sha256"]})
    identity = identity or identities()
    if (image.get("schema") != expected_schema or
            image.get("registry_sha256") != contract.REGISTRY_SHA256 or
            (contract in (edit_insert_v3, edit_insert_v4) and
             (image.get("scenario_version") != contract.SCENARIO_VERSION or
              image.get("carrier_abi") != contract.ABI)) or
            image.get("profile") != "release" or
            image.get("target") != "aarch64-unknown-linux-musl" or
            image.get("lockfile_sha256") != digest(CORE / "Cargo.lock") or
            image.get("cargo_config_sha256") != digest(ROOT / ".cargo/config.toml") or
            image.get("product_seal") != identity["product_seal"] or
            image.get("workload_source_seal") != edit_workload_seal() or
            not payload_ok or
            not re.fullmatch(r"sha256:[0-9a-f]{64}", str(image.get("image_id", ""))) or
            any(not re.fullmatch(r"[0-9a-f]{64}", str(image.get(name, ""))) for name in
                ("daemon_sha256", "edit_tool_sha256", "dockerfile_sha256"))):
        raise ValueError("sealed image does not match selected edit registry and inputs")


def recheck_exec_edit(out):
    """Re-derives the telemetry verdict from one receipt's retained raw LFT1.

    This is evidence re-reading, not another sample: the raw `telemetry.lft1`
    file is unchanged and the re-derived verdict is written beside it, bound to
    the same receipt. It exists because a parser defect must never force a rerun
    of an arm whose raw evidence is already complete.
    """
    run = json.loads((out / "run.json").read_text())
    receipt = run.get("receipt") or {}
    folder = Path(receipt.get("folder", ""))
    _, row = edit_selection(receipt["scenario_id"])
    raw_path = folder / ("driver.raw.stderr" if row["scenario_version"] == 4 else
                         "telemetry.lft1")
    raw = raw_path.read_bytes()
    if not raw:
        raise ValueError("no retained raw LFT1 to re-derive from")
    telemetry = (edit_route.edit_telemetry_v4.check(
        raw, receipt.get("driver") or {},
        (receipt.get("driver") or {}).get("telemetry_run", ""))
        if row["scenario_version"] == 4 else edit_route.edit_telemetry_check(raw.splitlines()))
    receipt = {**receipt, "telemetry": telemetry,
               "verification": retained_edit_verification(out, receipt, row)}
    outcome = edit_route.terminal_status(receipt, row)
    write_json(out / "telemetry-check.json", {
        "schema": "core-fs-bench-pro-exec-fuse-edit-telemetry-recheck-v1",
        "scenario_id": receipt["scenario_id"],
        "raw_lft1": str(raw_path),
        "raw_lft1_sha256": edit_route.sha256(raw_path),
        "resample": False,
        "telemetry": telemetry,
    })
    write_json(out / "status-rechecked.json", {
        "schema": "core-fs-bench-pro-exec-fuse-edit-status-recheck-v1",
        "scenario_id": receipt["scenario_id"],
        "resample": False,
        "raw_edit_commit_ns": receipt.get("edit_commit_ns"),
        "g2_target_ms": row["g2_target_ms"],
        "outcome": outcome,
    })
    return json.dumps({"telemetry": telemetry["status"], "outcome": outcome["status"]})


def edit_verification_record(run_root, receipt):
    """Read the original inline or later separate verifier without changing it."""
    verification = receipt.get("verification") or {}
    retained = Path(run_root) / "verification.json"
    if retained.is_file():
        verification = json.loads(retained.read_text())
    return verification


def retained_edit_verification(run_root, receipt, row):
    """Derive the current gate from the retained verifier's raw result."""
    verification = edit_verification_record(run_root, receipt)
    if verification.get("status") in ("PASS", "FAIL", "TIMEOUT"):
        status, identity_match, within_budget = edit_route.verifier_result(
            verification.get("exit_code"), verification.get("timeout"),
            verification.get("wall_ns"), verification.get("child"), row,
            receipt.get("driver") or {}, receipt.get("store"), receipt.get("history"))
        if verification.get("scenario_id") != row["scenario_id"]:
            status = "FAIL"
        verification = {**verification, "status": status,
                        "identity_match": identity_match, "within_budget": within_budget}
    return verification


def verify_exec_edit(out):
    """Verifies one retained #232 receipt without repeating its measurement."""
    run = json.loads((out / "run.json").read_text())
    receipt = run.get("receipt") or {}
    folder = Path(receipt.get("folder", ""))
    case_path = Path(receipt.get("case_file", ""))
    store = Path(receipt.get("store", ""))
    history = Path(receipt.get("history", ""))
    for path in (folder, case_path, store, history):
        if not path.exists():
            raise ValueError(f"retained evidence is missing: {path}")
    if (receipt.get("verification") or {}).get("status") != "SKIPPED":
        raise ValueError("this receipt already carries a verification result; verify it once")
    _, row = edit_selection(receipt["scenario_id"])
    build = json.loads((out / "build.json").read_text())
    binaries = {name: entry["path"] for name, entry in build["binaries"].items()}
    cursor_key = (out / "cursor-key.txt").read_text().strip()
    verification = edit_route.verify(folder, binaries, case_path, store, history, row, receipt,
                                     cursor_key, output=out)
    write_json(out / "verification-bound.json", {
        "schema": "core-fs-bench-pro-exec-fuse-edit-verification-binding-v1",
        "performance_run": str(out),
        "scenario_id": receipt["scenario_id"],
        "performance_command_wall_ns": receipt.get("complete_command_wall_ns"),
        "edit_commit_ns": receipt.get("edit_commit_ns"),
        "identity": build.get("identity", run.get("identity")),
        "image_id": (run.get("image") or {}).get("image_id"),
        "verification_status": verification["status"],
        "verification_wall_ns": verification["wall_ns"],
        "fresh_reopen": True,
        "full_file_bytes_verified": (verification.get("child") or {}).get(
            "full_file_bytes_verified"),
        "coverage": (verification.get("child") or {}).get("coverage"),
    })
    manifest_run(out)
    return json.dumps({"status": verification["status"], "wall_ns": verification["wall_ns"]})


def exec_edit_cursor_key():
    return os.urandom(32).hex()


def build_exec_edit(out, target, identity):
    """Builds the release examples this route needs, sealed by hash.

    An unchanged product seal reuses the immutable archived binaries by digest,
    exactly as the SDK Init path does: build reuse is required setup economy and
    never reaches inside a timed operation.
    """
    # Only a performance driver must use the public SDK for every product
    # operation; the independent verifier reads public C1/C2/C5 readers instead.
    for name in EXEC_EDIT_BINARIES:
        if name.startswith("benchmark_"):
            require_sdk_driver(name)
    cache = RESULTS / "exec-edit-build.json"
    prior = json.loads(cache.read_text()) if cache.exists() else None
    if prior and prior.get("product_seal") == identity["product_seal"] and all(
        Path(prior["binaries"][name]["path"]).is_file() and
        digest(prior["binaries"][name]["path"]) == prior["binaries"][name]["sha256"]
        for name in EXEC_EDIT_BINARIES
    ):
        return {"status": "PASS", "mode": "exact-binary-reuse", "profile": "release",
                "wall_ns": 0, "command": None, "binaries": prior["binaries"],
                "target": str(target)}
    started = time.monotonic_ns()
    with (out / "build.log").open("wb") as log:
        process = subprocess.run(EXEC_EDIT_BUILD, cwd=ROOT,
                                 env={**os.environ, "CARGO_TARGET_DIR": str(target)},
                                 stdout=log, stderr=subprocess.STDOUT)
    wall = time.monotonic_ns() - started
    record = {"status": "PASS" if process.returncode == 0 else "FAIL", "mode": "release",
              "profile": "release", "wall_ns": wall, "exit_code": process.returncode,
              "compiled_units": (out / "build.log").read_text(errors="replace").count("Compiling "),
              "command": EXEC_EDIT_BUILD, "target": str(target), "binaries": {}}
    if process.returncode == 0:
        for name in EXEC_EDIT_BINARIES:
            source = target / "release/examples" / name
            binary_sha = digest(source)
            archive = RESULTS / "binary-archive" / binary_sha / name
            archive.parent.mkdir(parents=True, exist_ok=True)
            if not archive.exists():
                shutil.copy2(source, archive)
                archive.chmod(0o555)
            record["binaries"][name] = {"path": str(archive), "sha256": binary_sha}
        write_json(cache, {"product_seal": identity["product_seal"],
                           "binaries": record["binaries"]})
    return record


def run_exec_edit(selection, out, verification="inline"):
    """One registered Exec/FUSE performance sample; never a rerun of an arm."""
    contract, row = edit_selection(selection)
    out = owned(out)
    identity = identities()
    if identity["source_dirty"]:
        raise ValueError("commit the Exec/FUSE route before collecting benchmark samples")
    target = target_path()
    out.mkdir(parents=True)
    build_receipt = build_exec_edit(out, target, identity)
    write_json(out / "build.json", build_receipt)
    image_path = CORE / (f"target/exec-fuse-insert-v{contract.SCENARIO_VERSION}/image.json"
                         if contract in (edit_insert_v3, edit_insert_v4) else
                         "target/exec-fuse-edit/image.json")
    if not image_path.is_file():
        raise ValueError("build the sealed edit image before sampling")
    image = json.loads(image_path.read_text())
    require_edit_image(image, contract, row, identity)
    actual_image = subprocess.check_output(
        ["docker", "image", "inspect", image["image_id"], "--format", "{{.Id}}"],
        text=True).strip()
    if actual_image != image["image_id"]:
        raise ValueError("sealed edit image ID is not present locally")
    if row["registration_status"] != "REGISTERED":
        receipt = {"status": "NOT_RUN", "sample_count": 0, "reason": row["not_run_reason"]}
        write_json(out / "run.json", {"schema": "core-fs-bench-pro-exec-fuse-run-v1",
                                      "selection": selection, "identity": identity,
                                      "receipt": receipt})
        (out / "report.txt").write_text(f"{selection}\tNOT_RUN\t{row['not_run_reason']}\n")
        manifest_run(out)
        return out
    binaries = {name: entry["path"] for name, entry in build_receipt["binaries"].items()}
    telemetry_run = int.from_bytes(os.urandom(16), "big") or 1
    cursor_key = exec_edit_cursor_key()
    # The benchmark-local history cursor capability, retained so the separate
    # verification can reopen the case's own byte copy read-only.
    (out / "cursor-key.txt").write_text(cursor_key + "\n")
    (out / "cursor-key.txt").chmod(0o600)
    results = RESULTS
    results.mkdir(parents=True, exist_ok=True)
    with (results / ".run.lock").open("a+b") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        receipt = edit_route.sample(results, row, binaries, image["image_id"], identity,
                                    cursor_key, telemetry_run,
                                    os.urandom(16).hex(), verification,
                                    attempt_root=out)
    write_json(out / "run.json", {"schema": "core-fs-bench-pro-exec-fuse-run-v1",
                                  "selection": selection, "identity": identity,
                                  "selection_mode": "registered-case", "image": image,
                                  "receipt": receipt})
    (out / "report.txt").write_text(
        f"{selection}\t{receipt.get('status')}\tsamples={receipt.get('sample_count')}\t"
        f"edit_commit_ns={receipt.get('edit_commit_ns')}\t"
        f"target_ms={row['g2_target_ms']}\t"
        f"command_wall_ns={receipt.get('complete_command_wall_ns')}\t"
        f"lft1={receipt.get('lft1_lines')}\t"
        f"verification={(receipt.get('verification') or {}).get('status')}\n")
    manifest_run(out)
    return out


EDIT_REPORT_COLUMNS = (
    "scenario_id", "family_id", "fixture_bytes", "final_bytes", "editor_algorithm",
    "attempted", "completed", "driver_exit", "timeout", "verification", "terminal",
    "edit_commit_ns", "g2_target_ms", "goal", "raw_vs_target", "master_reuse", "clone_copy_ns",
    "cache_ns", "cache_store", "cache_fuse", "driver_preparation_ns", "cleanup_ns",
    "complete_command_wall_ns", "complete_command_status", "verifier_wall_ns",
    "verification_coverage", "full_file_bytes_verified",
    "lft1_root_ns", "lft1_edit_ns", "lft1_commit_ns", "lft1_cpu_user_ns", "lft1_cpu_system_ns",
    "lft1_sampled_max_rss", "lft1_resource_status", "lft1_scope", "projection_counts",
    "lft1_status", "lft1_raw_sha256", "lft1_boundary_covered",
    "daemon_workspace_exec_ns", "daemon_cpu_user_ns", "daemon_cpu_system_ns",
    "daemon_sampled_max_rss", "daemon_resource_status", "daemon_boundary_covered",
    "recorded_telemetry_status", "lft1_partial_coverage",
    "upstream_calls", "unmount_ok", "sandbox_delete_ok", "evidence",
    "recorded_terminal", "derived_verification_gate", "current_admission_status",
)


def edit_report_row(row, folder):
    """One registered row of the campaign report, from retained evidence only."""
    values = {name: None for name in EDIT_REPORT_COLUMNS}
    values.update({"scenario_id": row["scenario_id"], "family_id": row["family_id"],
                   "fixture_bytes": row["fixture_bytes"], "final_bytes": row["final_bytes"],
                   "editor_algorithm": row["editor_algorithm"],
                   "g2_target_ms": row["g2_target_ms"], "attempted": 0, "completed": 0,
                   "lft1_scope": "caller-process-window (host caller + Service)",
                   "evidence": str(folder)})
    run_file = Path(folder) / "run.json"
    if not run_file.is_file():
        values.update({"terminal": "NOT_RUN", "goal": "NOT_RUN",
                       "verification": "NOT_RUN", "recorded_terminal": "NOT_RUN",
                       "derived_verification_gate": "NOT_RUN",
                       "current_admission_status": "NOT_RUN"})
        return values
    run = json.loads(run_file.read_text())
    receipt = run.get("receipt") or {}
    image = run.get("image") or {}
    if row["scenario_version"] in (3, 4) and (
            run.get("selection") != row["scenario_id"] or
            receipt.get("scenario_id") != row["scenario_id"] or
            image.get("registry_sha256") !=
            (edit_insert_v4 if row["scenario_version"] == 4 else
             edit_insert_v3).REGISTRY_SHA256 or
            (row["scenario_version"] == 4 and
             (image.get("schema") != "core-fs-bench-pro-exec-fuse-insert-image-v4" or
              image.get("scenario_version") != 4 or
              image.get("carrier_abi") != edit_insert_v4.ABI))):
        values.update({"terminal": "INVALID_EVIDENCE", "goal": "INELIGIBLE",
                       "verification": "INELIGIBLE", "recorded_terminal": receipt.get("status"),
                       "derived_verification_gate": "INELIGIBLE",
                       "current_admission_status": "INVALID_EVIDENCE",
                       "invalid_reason": "splice selection/image identity mismatch"})
        return values
    driver = receipt.get("driver") or {}
    recorded_telemetry = receipt.get("telemetry") or {}
    telemetry = recorded_telemetry
    if row["scenario_version"] == 4:
        raw_path = Path(receipt.get("folder", "")) / "driver.raw.stderr"
        if not raw_path.is_file() or edit_route.sha256(raw_path) != recorded_telemetry.get("raw_sha256"):
            values.update({"terminal": "INVALID_EVIDENCE", "goal": "INELIGIBLE",
                           "recorded_terminal": receipt.get("status"),
                           "invalid_reason": "v4 raw telemetry is missing or differs from its receipt"})
            return values
        telemetry = edit_route.edit_telemetry_v4.check(
            raw_path.read_bytes(), driver, driver.get("telemetry_run", ""))
    v4_coverage = (telemetry.get("coverage") or {}) if row["scenario_version"] == 4 else {}
    host_window = v4_coverage.get("edit_commit") or {}
    daemon_window = v4_coverage.get("workspace_exec") or {}
    master = receipt.get("master") or {}
    # The identity-matched verification is a separate command with its own
    # retained receipt; the performance receipt keeps the SKIPPED marker it was
    # collected with, so the report reads both without repeating either.
    raw_verification = edit_verification_record(folder, receipt)
    verification = retained_edit_verification(folder, receipt, row)
    effective = {**receipt, "verification": verification, "telemetry": telemetry}
    recorded_terminal = receipt.get("status")
    current_admission_status = (recorded_terminal if recorded_terminal == "NOT_RUN" else
                                edit_route.terminal_status(effective, row)["status"])
    if receipt.get("status") == "FAIL":
        current_admission_status = "FAIL"
    # Historical v2 receipts have no gate profile. Keep their displayed terminal
    # unchanged while exposing the corrected audit decision in its own column.
    current_profile = (row["scenario_version"] == 3 or
                       receipt.get("gate_profile") == "post-verifier-cleanup-v1")
    terminal = current_admission_status if current_profile else recorded_terminal
    observed = receipt.get("edit_commit_ns")
    target = row["g2_target_ms"] * 1_000_000
    values.update({
        "attempted": 1 if receipt.get("sample_count") or driver else 0,
        "completed": 1 if driver.get("status") == "COMPLETE" else 0,
        "driver_exit": receipt.get("driver_exit_code"),
        "timeout": bool(receipt.get("driver_timeout")),
        "verification": (verification if current_profile else raw_verification).get("status"),
        "derived_verification_gate": verification.get("status"),
        "verifier_wall_ns": verification.get("wall_ns"),
        "verification_coverage": (verification.get("child") or {}).get("coverage"),
        "full_file_bytes_verified": (verification.get("child") or {}).get(
            "full_file_bytes_verified"),
        "raw_vs_target": (None if not isinstance(observed, int)
                          else "AT_OR_BELOW" if observed <= target else "ABOVE"),
        "terminal": terminal,
        "recorded_terminal": recorded_terminal,
        "current_admission_status": current_admission_status,
        "edit_commit_ns": receipt.get("edit_commit_ns"),
        "goal": terminal,
        "master_reuse": master.get("reuse"),
        "clone_copy_ns": receipt.get("clone_copy_wall_ns"),
        "cache_ns": receipt.get("cache_wall_ns"),
        "cache_store": (receipt.get("cache", {}).get("macos-store") or {}).get("status"),
        "cache_fuse": (receipt.get("cache", {}).get("linux-fuse-backing") or {}).get("status"),
        "cleanup_ns": receipt.get("cleanup_ns"),
        "driver_preparation_ns": receipt.get("preparation_ns"),
        "complete_command_wall_ns": receipt.get("complete_command_wall_ns"),
        "complete_command_status": receipt.get("complete_command_status"),
        "lft1_root_ns": (telemetry.get("edit_commit_elapsed_ns") if row["scenario_version"] == 4
                         else telemetry.get("root_elapsed_ns")),
        "lft1_edit_ns": ((telemetry.get("children") or {}).get("edit") or {}).get("elapsed_ns")
        if row["scenario_version"] == 4 else telemetry.get("child_elapsed_ns", {}).get("edit"),
        "lft1_commit_ns": ((telemetry.get("children") or {}).get("commit") or {}).get("elapsed_ns")
        if row["scenario_version"] == 4 else telemetry.get("child_elapsed_ns", {}).get("commit"),
        "lft1_cpu_user_ns": ((host_window.get("cpu_shared_ns") or [None, None])[0]
                             if row["scenario_version"] == 4 else
                             (telemetry.get("root_cpu_shared_ns") or [None, None])[0]),
        "lft1_cpu_system_ns": ((host_window.get("cpu_shared_ns") or [None, None])[1]
                               if row["scenario_version"] == 4 else
                               (telemetry.get("root_cpu_shared_ns") or [None, None])[1]),
        "lft1_sampled_max_rss": (host_window.get("sampled_max_rss")
                                 if row["scenario_version"] == 4 else
                                 telemetry.get("root_sampled_max_rss")),
        "lft1_resource_status": (host_window.get("status") if row["scenario_version"] == 4 else
                                  telemetry.get("root_resource_status")),
        "lft1_status": telemetry.get("status"),
        "recorded_telemetry_status": recorded_telemetry.get("status"),
        "lft1_partial_coverage": "; ".join(telemetry.get("partial") or []),
        "lft1_raw_sha256": telemetry.get("raw_sha256"),
        "lft1_boundary_covered": host_window.get("boundary_covered"),
        "daemon_workspace_exec_ns": telemetry.get("workspace_exec_elapsed_ns"),
        "daemon_cpu_user_ns": (daemon_window.get("cpu_shared_ns") or [None, None])[0],
        "daemon_cpu_system_ns": (daemon_window.get("cpu_shared_ns") or [None, None])[1],
        "daemon_sampled_max_rss": daemon_window.get("sampled_max_rss"),
        "daemon_resource_status": daemon_window.get("status"),
        "daemon_boundary_covered": daemon_window.get("boundary_covered"),
        "projection_counts": driver.get("projection_counts"),
        "upstream_calls": driver.get("upstream_calls"),
        "unmount_ok": driver.get("unmount_ok"),
        "sandbox_delete_ok": driver.get("sandbox_delete_ok"),
    })
    return values


def report_edit(campaign, out=None, scenario_version=2):
    """Complete report for exactly one registered scenario version."""
    campaign = Path(campaign)
    contract = {3: edit_insert_v3, 4: edit_insert_v4}.get(scenario_version, edit)
    if scenario_version in (3, 4):
        contract.validate_registry()
    elif digest(HERE / edit.REGISTRY_PATH) != edit.REGISTRY_SHA256:
        raise ValueError("v2 registry identity changed")
    rows = [edit_report_row(row, campaign / row["scenario_id"]) for row in contract.registry()]
    if len(rows) != (4 if scenario_version in (3, 4) else 56):
        raise ValueError("every registered row must appear in the report")
    header = "\t".join(EDIT_REPORT_COLUMNS)
    body = "\n".join("\t".join("" if row[name] is None else str(row[name])
                                for name in EDIT_REPORT_COLUMNS) for row in rows)
    table = header + "\n" + body + "\n"
    counts = {}
    for name in ("attempted", "completed"):
        counts[name] = sum(int(row[name] or 0) for row in rows)
    for name in ("verification", "terminal", "goal", "derived_verification_gate",
                 "current_admission_status"):
        tally = {}
        for row in rows:
            key = str(row[name])
            tally[key] = tally.get(key, 0) + 1
        counts[name] = tally
    counts["rows"] = len(rows)
    counts["timeouts"] = sum(1 for row in rows if row["timeout"])
    counts["complete_command_slow"] = sum(
        1 for row in rows if row["complete_command_status"] == "COMMAND_SLOW")
    counts["target_misses"] = sum(1 for row in rows if row["goal"] == "TARGET_MISS")
    counts["raw_target_misses"] = sum(1 for row in rows if row["raw_vs_target"] == "ABOVE")
    counts["raw_at_or_below_target"] = sum(1 for row in rows
                                           if row["raw_vs_target"] == "AT_OR_BELOW")
    document = {"schema": "core-fs-bench-pro-exec-fuse-edit-campaign-report-v1",
                "scenario_version": scenario_version, "registry_sha256": contract.REGISTRY_SHA256,
                "campaign": str(campaign), "counts": counts, "rows": rows}
    if out:
        Path(out).mkdir(parents=True, exist_ok=True)
        (Path(out) / "report-edit.tsv").write_text(table)
        write_json(Path(out) / "report-edit.json", document)
    return table, document


def main():
    parser = argparse.ArgumentParser()
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("list")
    run_parser = commands.add_parser("run")
    selector = run_parser.add_mutually_exclusive_group(required=True)
    selector.add_argument("--case")
    selector.add_argument("--family", choices=["init_namespace", "edit_length_preserving",
                                               "edit_length_changing",
                                               "edit_canonical_chunk_count"])
    run_parser.add_argument("--out", required=True)
    run_parser.add_argument("--verification", choices=["inline", "skipped"], default="inline")
    for name in ("verify", "report"):
        commands.add_parser(name).add_argument("--run", required=True)
    commands.add_parser("verify-edit").add_argument("--run", required=True)
    commands.add_parser("recheck-edit").add_argument("--run", required=True)
    report_edit_parser = commands.add_parser("report-edit")
    report_edit_parser.add_argument("--runs", required=True)
    report_edit_parser.add_argument("--out")
    report_edit_parser.add_argument("--scenario-version", type=int, choices=[2, 3, 4], default=2)
    args = parser.parse_args()
    edit_registry = (edit.registry() + edit_insert_v3.registry() + edit_insert_v4.registry()
                     if args.command == "list" else
                     edit_insert_v4.registry() if args.command == "run" and args.case and
                     args.case.endswith("-exec-v4") else
                     edit_insert_v3.registry() if args.command == "run" and args.case and
                     args.case.endswith("-exec-v3") else
                     edit.registry() if args.command == "run" else [])
    edit_rows = {row["scenario_id"]: row for row in edit_registry}
    edit_families = {row["family_id"] for row in edit_registry}
    if args.command == "list":
        for case in init.CASES.values():
            print(f"{case.id}\t{case.files}\t{case.logical_bytes}\t"
                  f"{'SDK selected' if case.id in init.SELECTED else 'NOT_RUN ' + init.NOT_RUN_REASON}")
        for row in edit_registry:
            state = (f"REGISTERED target={row['g2_target_ms']:.2f}ms"
                     if row["registration_status"] == "REGISTERED"
                     else f"NOT_RUN {row['not_run_reason'].split(':')[0]}")
            print(f"{row['scenario_id']}\t{row['fixture_bytes']}\t{row['final_bytes']}\t"
                  f"{row['family_id']} {state}")
    elif args.command == "run":
        selection = args.case or args.family
        if selection in edit_rows or selection in edit_families:
            selection = next((row["scenario_id"] for row in edit_registry
                              if row["scenario_id"] == selection
                              or row["family_id"] == selection
                              and row["registration_status"] == "REGISTERED"), None)
            if selection is None:
                parser.error("no registered Exec/FUSE case in that selection")
            print(run_exec_edit(selection, args.out, args.verification))
        elif selection in (*init.CASES, "init_namespace"):
            print(run(selection, args.out))
        else:
            parser.error("unknown or deferred SDK case")
    elif args.command == "verify":
        print(verify_run(owned(args.run, existing=True)))
    elif args.command == "verify-edit":
        print(verify_exec_edit(owned(args.run, existing=True)))
    elif args.command == "recheck-edit":
        print(recheck_exec_edit(owned(args.run, existing=True)))
    elif args.command == "report-edit":
        table, document = report_edit(owned(args.runs, existing=True), args.out,
                                      args.scenario_version)
        print(table, end="")
        print(json.dumps(document["counts"], sort_keys=True))
    else:
        print(report(owned(args.run, existing=True)), end="")


if __name__ == "__main__":
    main()
