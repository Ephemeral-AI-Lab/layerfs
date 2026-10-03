#!/usr/bin/env python3
"""One-sample release-only SDK Init runner with append-only evidence."""
import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
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
from families import phase7_storage as phase7  # noqa: E402
from families import init_namespace as init  # noqa: E402
from families import history_retention as history  # noqa: E402
from families import workspace_write as write  # noqa: E402
from families import workspace_commit as commit  # noqa: E402
from families import workspace_commit_native as native  # noqa: E402
from families import workspace_namespace as namespace  # noqa: E402
from families import workspace_mutations as mutations  # noqa: E402
from families import workspace_shell_package as package  # noqa: E402

CONTRACT_COMMIT = "6dfd0c7cbcbe9036f69b834e1704f2126f95c5a2"
BUILD_PROFILE = "release"
BINARY_DIR = "release/examples"
VERIFY_TIMEOUT_S = 9.5
SAMPLE_POLICY = "stride64-size-log2-endpoints-v1"
BUILD = ["cargo", "+1.85.1", "build", "--release", "--manifest-path", "core/Cargo.toml", "--locked",
         "-p", "layerfs-sdk", "-p", "layerfs-server",
         "--example", "benchmark_init", "--example", "verify_namespace"]
BINARIES = ("benchmark_init", "verify_namespace")


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
            "contract_commit": CONTRACT_COMMIT, "build_profile": BUILD_PROFILE}


def build(out, target, identity):
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
               "operation_surface": "layerfs-sdk", "operation_entrypoint": "ProjectApi::init",
               "orchestration_executor": "Python host runner", "mutation_executor": "host Service",
               "timing_boundary_id": "sdk-init-call-v1", "clock_id": "Rust std::time::Instant",
               "start_event": "immediately before ProjectApi::init",
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


def verify_run_manifest(run):
    recorded = json.loads((run / "manifest.json").read_text())
    actual = {str(path.relative_to(run)) for path in run.rglob("*") if path.is_file() and path.name != "manifest.json"}
    if actual != set(recorded["files"]):
        raise ValueError("retained evidence inventory mismatch")
    for name, expected in recorded["files"].items():
        path = run / name
        if not path.is_file() or path.stat().st_size != expected["bytes"] or digest(path) != expected["sha256"]:
            raise ValueError(f"retained evidence mismatch: {name}")


def verify_run(run):
    verify_run_manifest(run)
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


def main():
    parser = argparse.ArgumentParser()
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("list")
    run_parser = commands.add_parser("run")
    selector = run_parser.add_mutually_exclusive_group(required=True)
    selector.add_argument("--case")
    selector.add_argument("--family", choices=["init_namespace", "history-retention", "workspace_write", "workspace-commit", "workspace-commit-native", "workspace-commit-native-tail", "workspace-commit-native-counts", "workspace-commit-native-reconciliation", "workspace_namespace", "workspace-namespace-native", "workspace-namespace-sdk", "workspace_mutations", "workspace-mutations-native", "workspace-mutations-native-tail", "workspace-mutations-sdk", "workspace_shell_package", "workspace-shell-package-tail"])
    run_parser.add_argument("--out", required=True)
    run_parser.add_argument("--arm", choices=["baseline", "candidate"])
    run_parser.add_argument("--baseline-root")
    proof_parser = commands.add_parser("prove")
    proof_parser.add_argument("--run", required=True)
    proof_parser.add_argument("--out", required=True)
    for name in ("verify", "report"):
        commands.add_parser(name).add_argument("--run", required=True)
    args = parser.parse_args()
    if args.command == "list":
        for case in phase7.CASES.values():
            print(f"{case.id}\tstrict candidate < baseline; storage ceiling {case.storage_ceiling}")
        for case in init.CASES.values():
            print(f"{case.id}\t{case.files}\t{case.logical_bytes}\t"
                  f"{'SDK selected' if case.id in init.SELECTED else 'NOT_RUN ' + init.NOT_RUN_REASON}")
        for case in history.CASES.values():
            print(f"{case.id}\t{case.states} states\tallocated < {case.ceiling_bytes} B\t"
                  f"{'selected' if case.id in history.SELECTED else 'explicit run-only'}")
        for case in write.CASES.values():
            print(f"{case.id}\t{case.writes} {case.pattern} writes\t"
                  f"command <= {case.command_budget_ns / 1e9:g} s; verifier <= 9 s\t"
                  "selected")
        for case in commit.CASES.values():
            print(f"{case.id}\tSDK same-Workspace retained pin\tcommand <= {case.command_budget_ns / 1e9:g} s; separate proof <= 9 s\t"
                  f"{'historical method retired' if case.retired else 'fast lane selected' if case.id in commit.SELECTED else 'explicit-only ' + case.role}")
        for case in native.CASES.values():
            print(f"{case.id}\tnative functional component; command <= 60 s; no SDK time\t"
                  f"{'SKIPPED / OWNER-DEFERRED (#276)' if case.owner_deferred else 'selected'}")
        for case in namespace.NATIVE.values():
            print(f"{case.id}\tnative namespace control; command <= {case.budget_ns / 1e9:g} s; no SDK time")
        for case in namespace.SDK.values():
            print(f"{case.id}\tpublic SDK/FUSE; command <= {case.budget_ns / 1e9:g} s; separate full proof < 9 s")
        for case in mutations.NATIVE.values():
            print(f"{case.id}\tnative mixed mutation/custody; command <= 15 s; no SDK time")
        for case in mutations.SDK.values():
            print(f"{case.id}\tpublic SDK/FUSE; command <= 15 s; separate full proof < 9 s\t"
                  f"{'historical cleanup FAIL r069; superseded v2 explicit recovery' if case.retired else 'current selected'}")
        for case in package.cases().values():
            print(f"{case.id}\tpublic SDK/POSIX package; command <= {case.budget_ns / 1e9:g} s; separate full proof < 9 s")
        for case in package.NATIVE.values():
            print(f"{case.id}\tnative same-thread progress/refusal custody; command <=15s; SDK time N/A")
        print(f"{package.SDK_CAUSE}\tcount/custody SDK diagnostic; original1025 workload/25s bound; not a speed/gate arm")
        print(f"{package.DEFERRED}\t{package.DEFER_REASON}")
        for name, reason in commit.REMAINING.items():
            print(f"{name}\tNOT_RUN: {reason}")
    elif args.command == "run":
        selection = args.case or args.family
        if selection in phase7.CASES:
            if args.arm is None or args.arm == "baseline" and args.baseline_root is None:
                parser.error("phase7 requires --arm and a baseline root for baseline")
            print(phase7.run(selection, args.out, args.arm, args.baseline_root, sys.modules[__name__]))
            return
        if selection in (*package.cases(), *package.NATIVE, package.SDK_CAUSE, package.DEFERRED, "workspace_shell_package", "workspace-shell-package-tail"):
            print(package.run(selection, args.out, sys.modules[__name__]))
            return
        if selection in (*mutations.NATIVE, *mutations.SDK, "workspace_mutations", "workspace-mutations-native", "workspace-mutations-native-tail", "workspace-mutations-sdk"):
            print(mutations.run(selection, args.out, sys.modules[__name__]))
            return
        if selection in (*namespace.NATIVE, *namespace.SDK, "workspace_namespace", "workspace-namespace-native", "workspace-namespace-sdk"):
            print(namespace.run(selection, args.out, sys.modules[__name__]))
            return
        if selection in (*history.CASES, "history-retention"):
            print(history.run(selection, args.out, sys.modules[__name__]))
            return
        if selection in (*write.CASES, "workspace_write"):
            print(write.run(selection, args.out, sys.modules[__name__]))
            return
        if selection in (*native.CASES, "workspace-commit-native", "workspace-commit-native-tail", "workspace-commit-native-counts", "workspace-commit-native-reconciliation"):
            print(native.run(selection, args.out, sys.modules[__name__]))
            return
        if selection in (*commit.CASES, "workspace-commit"):
            print(commit.run(selection, args.out, sys.modules[__name__]))
            return
        if selection not in (*init.CASES, "init_namespace"):
            parser.error("unknown or deferred SDK case")
        print(run(selection, args.out))
    elif args.command == "prove":
        path = owned(args.run, existing=True)
        if json.loads((path / "run.json").read_text()).get("schema") == package.SCHEMA:
            print(package.prove(path, args.out, sys.modules[__name__]))
            return
        if json.loads((path / "run.json").read_text()).get("schema") == mutations.SCHEMA:
            print(mutations.prove(path, args.out, sys.modules[__name__]))
            return
        if json.loads((path / "run.json").read_text()).get("schema") == namespace.SCHEMA:
            print(namespace.prove(path, args.out, sys.modules[__name__]))
            return
        if json.loads((path / "run.json").read_text()).get("schema") != "core-workspace-commit-fast-run-v2":
            parser.error("separate prove currently covers Family 4 fast-lane receipts")
        print(commit.prove(path, args.out, sys.modules[__name__]))
    elif args.command == "verify":
        path = owned(args.run, existing=True)
        if json.loads((path / "run.json").read_text()).get("schema") in ("core-history-retention-run-v1", "core-history-retention-run-v2", "core-history-retention-run-v3", "core-history-retention-run-v4"):
            print(history.verify(path, sys.modules[__name__]))
        elif json.loads((path / "run.json").read_text()).get("schema") == "core-workspace-write-run-v1":
            print(write.verify(path, sys.modules[__name__]))
        elif json.loads((path / "run.json").read_text()).get("schema") in ("core-workspace-commit-fast-run-v2", "core-workspace-commit-proof-v2"):
            verify_run_manifest(path)
            print("PASS: retained evidence custody only")
        elif json.loads((path / "run.json").read_text()).get("schema") in (namespace.SCHEMA, namespace.PROOF_SCHEMA, namespace.NATIVE_SCHEMA, mutations.SCHEMA, mutations.PROOF_SCHEMA, mutations.NATIVE_SCHEMA, package.SCHEMA, package.PROOF_SCHEMA, package.NATIVE_SCHEMA):
            verify_run_manifest(path)
            print("PASS: retained namespace evidence custody only")
        elif json.loads((path / "run.json").read_text()).get("schema") == native.SCHEMA:
            verify_run_manifest(path)
            print("PASS: retained native evidence custody only")
        else:
            print(verify_run(path))
    else:
        path = owned(args.run, existing=True)
        if json.loads((path / "run.json").read_text()).get("schema") in ("core-history-retention-run-v1", "core-history-retention-run-v2", "core-history-retention-run-v3", "core-history-retention-run-v4"):
            print(history.report(path), end="")
        elif json.loads((path / "run.json").read_text()).get("schema") == "core-workspace-write-run-v1":
            print(write.report(path), end="")
        elif json.loads((path / "run.json").read_text()).get("schema") in ("core-workspace-commit-fast-run-v2", "core-workspace-commit-proof-v2"):
            print(commit.report(path), end="")
        elif json.loads((path / "run.json").read_text()).get("schema") in (namespace.SCHEMA, namespace.PROOF_SCHEMA, namespace.NATIVE_SCHEMA, mutations.SCHEMA, mutations.PROOF_SCHEMA, mutations.NATIVE_SCHEMA, package.SCHEMA, package.PROOF_SCHEMA, package.NATIVE_SCHEMA):
            print(namespace.report(path), end="")
        elif json.loads((path / "run.json").read_text()).get("schema") == native.SCHEMA:
            print(native.report(path), end="")
        else:
            print(report(path), end="")


if __name__ == "__main__":
    main()
