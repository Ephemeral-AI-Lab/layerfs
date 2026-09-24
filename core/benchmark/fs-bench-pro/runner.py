#!/usr/bin/env python3
"""One-sample SDK Init runner with append-only two-case evidence."""
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
from shared import edit_route  # noqa: E402

CONTRACT_COMMIT = "05fb205d391d551a17bde86a00c969310b6e7406"
BUILD = ["cargo", "+1.85.1", "build", "--manifest-path", "core/Cargo.toml", "--locked",
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
    source = source or CORE / "crates/layerfs-api/sdk/examples" / f"{name}.rs"
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
            "contract_commit": CONTRACT_COMMIT}


def build(out, target, identity):
    for name in BINARIES:
        if name.startswith("benchmark_"):
            require_sdk_driver(name)
    cache = RESULTS / "sdk-build.json"
    prior = json.loads(cache.read_text()) if cache.exists() else None
    if prior and prior.get("product_seal") == identity["product_seal"] and all(
        Path(prior["binaries"][name]["path"]).is_file() and
        digest(prior["binaries"][name]["path"]) == prior["binaries"][name]["sha256"]
        for name in BINARIES
    ):
        return {"status": "PASS", "mode": "exact-binary-reuse", "wall_ns": 0,
                "command": None, "binaries": prior["binaries"]}
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
              "command": BUILD, "target": str(target)}
    if process.returncode == 0:
        binaries = {}
        for name in BINARIES:
            source = target / "debug/examples" / name
            binary_sha = digest(source)
            archive = RESULTS / "binary-archive" / binary_sha / name
            archive.parent.mkdir(parents=True, exist_ok=True)
            if not archive.exists():
                shutil.copy2(source, archive)
                archive.chmod(0o555)
            binaries[name] = {"path": str(archive), "sha256": binary_sha}
        record["binaries"] = binaries
        if record["status"] == "PASS":
            write_json(cache, {"product_seal": identity["product_seal"], "binaries": binaries})
    return record


def competing_work():
    process = subprocess.run(["ps", "-axo", "pid=,comm="], capture_output=True, text=True)
    return [line.strip() for line in process.stdout.splitlines()
            if any(word in line.lower() for word in ("cargo", "docker", "layerfs")) and
            not line.strip().startswith(str(os.getpid()) + " ")][:32]


def verify_child(binary, folder, store, history, sample, fixture, cursor):
    command = [binary, str(store), str(history), sample["root"], sample["stack_body"],
               fixture["manifest"], fixture["manifest_sha256"]]
    started = time.monotonic_ns()
    try:
        result = subprocess.run(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=5,
                                env={**os.environ, "LAYERFS_HISTORY_CURSOR_KEY": cursor})
        wall = time.monotonic_ns() - started
        stdout, stderr = result.stdout, result.stderr
        try:
            child = json.loads(stdout) if result.returncode == 0 else None
        except (ValueError, UnicodeDecodeError):
            child = None
        status = "PASS" if child and child.get("status") == "PASS" else "FAIL"
        exit_code = result.returncode
    except subprocess.TimeoutExpired as error:
        wall = time.monotonic_ns() - started
        stdout, stderr = error.stdout or b"", error.stderr or b""
        child, status, exit_code = None, "TIMEOUT", None
    (folder / "verifier.stdout").write_bytes(stdout)
    (folder / "verifier.stderr").write_bytes(stderr)
    receipt = {"status": status, "wall_ns": wall, "budget_ns": 5_000_000_000,
               "exit_code": exit_code, "command": command, "child": child,
               "stderr": stderr[:4096].decode(errors="replace")}
    write_json(folder / "verification.json", receipt)
    return receipt


def case_run(out, case, binaries, identity):
    folder = out / "sdk-host" / "init_namespace" / case.id
    folder.mkdir(parents=True)
    receipt = {"schema": "core-fs-bench-pro-sdk-init-v2", "case": case.id,
               "family_id": "init_namespace", "scenario_id": case.id, "scenario_version": 2,
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
                                        folder, store, history, perf, fixture, cursor)
            receipt["verification"] = verification
            receipt["functional_status"] = "PASS" if (
                receipt["performance_command_status"] == "PASS" and
                verification["status"] == "PASS" and verification["wall_ns"] <= 5_000_000_000
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
                    f"{row.get('functional_status')}\t{row['status']}\t{row['cache_contract']}")
    return ("case\tsamples\toperation_ns\tcommand_ns\tverification_ns\tfunctional\t"
            "performance\tcache\n" + "\n".join(rows) + "\n")


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
    write_json(out / "run.json", {"schema": "core-fs-bench-pro-sdk-run-v2", "selection": selection,
        "cases": list(init.SELECTED), "identity": identity, "blocked": blocked,
        "family_cycle_wall_ns": time.monotonic_ns() - cycle_started,
        "family_cycle_budget_ns": 30_000_000_000})
    (out / "report.txt").write_text(report(out))
    manifest_run(out)
    return out


EXEC_EDIT_BUILD = ["cargo", "+1.85.1", "build", "--manifest-path", "core/Cargo.toml", "--locked",
                  "--offline", "--release", "-p", "layerfs-sdk", "-p", "layerfs-server",
                  "--example", "benchmark_edit", "--example", "verify_edit"]
EXEC_EDIT_BINARIES = {"benchmark_edit": "benchmark_edit", "verify_edit": "verify_edit"}


def exec_edit_cursor_key():
    return os.urandom(32).hex()


def build_exec_edit(out, target, identity):
    """Builds the two release examples this route needs, sealed by hash."""
    for name in EXEC_EDIT_BINARIES:
        require_sdk_driver(name)
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
    return record


def run_exec_edit(selection, out):
    """One #232 performance sample; never a rerun of the same arm."""
    out = owned(out)
    identity = identities()
    if identity["source_dirty"]:
        raise ValueError("commit the Exec/FUSE route before collecting benchmark samples")
    target = target_path()
    out.mkdir(parents=True)
    build_receipt = build_exec_edit(out, target, identity)
    write_json(out / "build.json", build_receipt)
    image_path = CORE / "target/exec-fuse-edit/image.json"
    if not image_path.is_file():
        raise ValueError("build the sealed edit image before sampling")
    image = json.loads(image_path.read_text())
    if image.get("registry_sha256") != edit.REGISTRY_SHA256:
        raise ValueError("sealed image was built from a different registry")
    rows = {row["scenario_id"]: row for row in edit.registry()}
    if selection not in rows:
        raise ValueError("unregistered Exec/FUSE scenario")
    row = rows[selection]
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
    results = RESULTS
    results.mkdir(parents=True, exist_ok=True)
    with (results / ".run.lock").open("a+b") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        receipt = edit_route.sample(results, row, binaries, image["image_id"], identity,
                                    exec_edit_cursor_key(), telemetry_run,
                                    os.urandom(16).hex())
    write_json(out / "run.json", {"schema": "core-fs-bench-pro-exec-fuse-run-v1",
                                  "selection": selection, "identity": identity,
                                  "image": image, "receipt": receipt})
    (out / "report.txt").write_text(
        f"{selection}\t{receipt.get('status')}\tsamples={receipt.get('sample_count')}\t"
        f"edit_commit_ns={receipt.get('edit_commit_ns')}\t"
        f"target_ms={row['g2_target_ms']}\t"
        f"command_wall_ns={receipt.get('complete_command_wall_ns')}\t"
        f"lft1={receipt.get('lft1_lines')}\t"
        f"verification={(receipt.get('verification') or {}).get('status')}\n")
    manifest_run(out)
    return out


def main():
    parser = argparse.ArgumentParser()
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("list")
    run_parser = commands.add_parser("run")
    selector = run_parser.add_mutually_exclusive_group(required=True)
    selector.add_argument("--case")
    selector.add_argument("--family", choices=["init_namespace", "workspace_exec_edit_length_preserving",
                                               "workspace_exec_edit_length_changing",
                                               "workspace_exec_edit_canonical_chunk_count"])
    run_parser.add_argument("--out", required=True)
    for name in ("verify", "report"):
        commands.add_parser(name).add_argument("--run", required=True)
    args = parser.parse_args()
    edit_rows = {row["scenario_id"]: row for row in edit.registry()}
    edit_families = {row["family_id"] for row in edit.registry()}
    if args.command == "list":
        for case in init.CASES.values():
            print(f"{case.id}\t{case.files}\t{case.logical_bytes}\t"
                  f"{'SDK selected' if case.id in init.SELECTED else 'NOT_RUN ' + init.NOT_RUN_REASON}")
        for row in edit.registry():
            state = (f"REGISTERED target={row['g2_target_ms']:.2f}ms"
                     if row["registration_status"] == "REGISTERED"
                     else f"NOT_RUN {row['not_run_reason'].split(':')[0]}")
            print(f"{row['scenario_id']}\t{row['fixture_bytes']}\t{row['final_bytes']}\t"
                  f"{row['family_id']} {state}")
    elif args.command == "run":
        selection = args.case or args.family
        if selection in edit_rows or selection in edit_families:
            selection = next((row["scenario_id"] for row in edit.registry()
                              if row["scenario_id"] == selection
                              or row["family_id"] == selection
                              and row["registration_status"] == "REGISTERED"), None)
            if selection is None:
                parser.error("no registered Exec/FUSE case in that selection")
            print(run_exec_edit(selection, args.out))
        elif selection in (*init.SELECTED, "init_namespace"):
            print(run(selection, args.out))
        else:
            parser.error("unknown or deferred SDK case")
    elif args.command == "verify":
        print(verify_run(owned(args.run, existing=True)))
    else:
        print(report(owned(args.run, existing=True)), end="")


if __name__ == "__main__":
    main()
