#!/usr/bin/env python3
"""One-sample SDK Init runner with append-only two-case evidence."""
import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import resource
import shutil
import signal
import sqlite3
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
sys.path.insert(0, str(CORE / "benchmark/fs-bench-pro-storage-content/shared"))
import residency  # noqa: E402

CONTRACT_COMMIT = "05fb205d391d551a17bde86a00c969310b6e7406"
BUILD = ["cargo", "+1.85.1", "build", "--manifest-path", "core/Cargo.toml", "--locked",
         "-p", "layerfs-sdk", "-p", "layerfs-service",
         "--example", "benchmark_init_memory", "--example", "verify_namespace"]
BUILD_RELEASE = BUILD[:3] + ["--release"] + BUILD[3:]
BINARIES = ("benchmark_init_memory", "verify_namespace")


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
    harness = list(HERE.glob("**/*.py")) + [Path(residency.__file__)]
    source = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    tree = subprocess.check_output(["git", "rev-parse", "HEAD^{tree}"], cwd=ROOT, text=True).strip()
    dirty = subprocess.check_output(["git", "status", "--porcelain"], cwd=ROOT, text=True)
    return {"source_commit": source, "source_tree": tree, "source_dirty": bool(dirty),
            "dirty_paths": dirty.splitlines(), "product_seal": seal(product),
            "harness_seal": seal(harness), "cargo_lock_sha256": digest(CORE / "Cargo.lock"),
            "contract_commit": CONTRACT_COMMIT}


def build(out, target, identity, *, release=False):
    cache = RESULTS / ("sdk-build-release.json" if release else "sdk-build.json")
    command = BUILD_RELEASE if release else BUILD
    profile = "release" if release else "debug"
    prior = json.loads(cache.read_text()) if cache.exists() else None
    if prior and prior.get("profile") == profile and prior.get("product_seal") == identity["product_seal"] and all(
        Path(prior["binaries"][name]["path"]).is_file() and
        digest(prior["binaries"][name]["path"]) == prior["binaries"][name]["sha256"]
        for name in BINARIES
    ):
        return {"status": "PASS", "mode": "exact-binary-reuse", "wall_ns": 0,
                "command": None, "profile": profile, "binaries": prior["binaries"]}
    started = time.monotonic_ns()
    with (out / "build.log").open("wb") as log:
        process = subprocess.run(command, cwd=ROOT,
                                 env={**os.environ, "CARGO_TARGET_DIR": str(target)},
                                 stdout=log, stderr=subprocess.STDOUT)
    wall = time.monotonic_ns() - started
    record = {"status": "BUILD_SLOW" if wall > 30_000_000_000 else "PASS" if process.returncode == 0 else "FAIL",
              "mode": "changed-product" if prior else "first-use", "wall_ns": wall,
              "budget_ns": 30_000_000_000, "exit_code": process.returncode,
              "compiled_units": (out / "build.log").read_text(errors="replace").count("Compiling "),
              "command": command, "profile": profile, "target": str(target)}
    if process.returncode == 0:
        binaries = {}
        for name in BINARIES:
            source = target / profile / "examples" / name
            binary_sha = digest(source)
            archive = RESULTS / "binary-archive" / binary_sha / name
            archive.parent.mkdir(parents=True, exist_ok=True)
            if not archive.exists():
                shutil.copy2(source, archive)
                archive.chmod(0o555)
            binaries[name] = {"path": str(archive), "sha256": binary_sha}
        record["binaries"] = binaries
        if record["status"] == "PASS":
            write_json(cache, {"product_seal": identity["product_seal"], "profile": profile,
                               "binaries": binaries})
    return record


def competing_work():
    process = subprocess.run(["ps", "-axo", "pid=,comm="], capture_output=True, text=True)
    return [line.strip() for line in process.stdout.splitlines()
            if any(word in line.lower() for word in ("cargo", "docker", "layerfs")) and
            not line.strip().startswith(str(os.getpid()) + " ")][:32]


def verify_child(binary, folder, store, history, sample, fixture, cursor, timeout=5):
    command = [binary, str(store), str(history), sample["root"], sample["stack_body"],
               fixture["manifest"], fixture["manifest_sha256"]]
    started = time.monotonic_ns()
    try:
        result = subprocess.run(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=timeout,
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
    receipt = {"status": status, "wall_ns": wall, "budget_ns": timeout * 1_000_000_000,
               "exit_code": exit_code, "command": command, "child": child,
               "stderr": stderr[:4096].decode(errors="replace")}
    write_json(folder / "verification.json", receipt)
    return receipt


def source_copy(case, fixture, out):
    """Full byte copy and oracle check before invalidating its payload pages."""
    source = Path(fixture["source"])
    home = out / "source-copy" / case.id
    started = time.monotonic_ns()
    shutil.copytree(source.parent, home, copy_function=shutil.copy2)
    init._check_reuse(case, home)
    copied = home / "payload"
    files = bytes_ = 0
    with (home / "manifest.tsv").open() as manifest:
        for row in manifest:
            relative, kind, _, _, size, sha = row.rstrip("\n").split("\t")
            if kind != "f":
                continue
            original, path = source / relative, copied / relative
            if (original.stat().st_dev, original.stat().st_ino) == (path.stat().st_dev, path.stat().st_ino):
                raise ValueError("source copy shares an inode with the prepared master")
            if path.stat().st_nlink != 1 or path.stat().st_size != int(size) or digest(path) != sha:
                raise ValueError(f"source copy content mismatch: {relative}")
            files += 1
            bytes_ += int(size)
    if (files, bytes_) != (case.files, case.logical_bytes):
        raise ValueError("source copy cardinality mismatch")
    record = {"status": "PASS", "method": "shutil.copytree+copy2-independent-byte-copy",
              "source": str(source), "copy": str(copied), "files": files, "bytes": bytes_,
              "manifest_sha256": fixture["manifest_sha256"], "wall_ns": time.monotonic_ns() - started}
    write_json(out / "source-copy.json", record)
    return copied


def payload_pages(source, manifest, *, invalidate):
    files = bytes_ = pages = resident = invalidated = 0
    with Path(manifest).open() as rows:
        for row in rows:
            relative, kind, _, _, size, _ = row.rstrip("\n").split("\t")
            if kind != "f":
                continue
            path = source / relative
            reading = residency.de_warm(path) if invalidate else residency.residency(path)
            expected = (int(size) + residency.page_size() - 1) // residency.page_size()
            if path.stat().st_size != int(size) or reading.total_pages != expected:
                raise ValueError(f"source payload page count mismatch: {relative}")
            files += 1
            bytes_ += int(size)
            pages += reading.total_pages
            resident += reading.resident_after if invalidate else reading.resident_pages
            invalidated += int(reading.invalidated) if invalidate else 0
    return {"files": files, "bytes": bytes_, "page_size_bytes": residency.page_size(),
            "expected_pages": pages, "resident_pages": resident, "invalidated_files": invalidated}


def diagnostic_child(command, folder, private, cursor, timeout=610):
    """wait4 scopes CPU and peak RSS to this driver, including its threads."""
    started = time.monotonic_ns()
    with (folder / "driver.stdout").open("wb") as output, (folder / "driver.stderr").open("wb") as errors:
        child = subprocess.Popen(command, stdout=output, stderr=errors, start_new_session=True,
                                 env={**os.environ, "LAYERFS_PRIVATE_KEY": private,
                                      "LAYERFS_HISTORY_CURSOR_KEY": cursor})
        timed_out = False
        while True:
            pid, status, usage = os.wait4(child.pid, os.WNOHANG)
            if pid:
                break
            if time.monotonic_ns() - started >= timeout * 1_000_000_000:
                timed_out = True
                try:
                    os.killpg(child.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                _, status, usage = os.wait4(child.pid, 0)
                break
            time.sleep(0.05)
        child.returncode = os.waitstatus_to_exitcode(status)
    wall = time.monotonic_ns() - started
    stdout = (folder / "driver.stdout").read_bytes()
    try:
        perf = json.loads(stdout)
    except (ValueError, UnicodeDecodeError):
        perf = {"status": "INCOMPLETE", "operation_ns": None,
                "error": "missing or malformed SDK result"}
    rss_units = "bytes" if sys.platform == "darwin" else "KiB"
    return perf, {"wall_ns": wall, "watchdog_ns": timeout * 1_000_000_000,
                  "exit_code": child.returncode, "timed_out": timed_out,
                  "external_resources": {"scope": "one-driver-process-lifecycle-including-threads",
                                         "source": "wait4 per-child rusage",
                                         "user_cpu_s": usage.ru_utime, "system_cpu_s": usage.ru_stime,
                                         "peak_rss_bytes": usage.ru_maxrss * (1 if sys.platform == "darwin" else 1024),
                                         "raw_peak_rss": usage.ru_maxrss, "raw_peak_rss_unit": rss_units}}


def sqlite_geometry(path, *, packs=False):
    if not path.exists():
        return {"status": "NOT_CREATED"}
    stat_ = path.stat()
    record = {"apparent_bytes": stat_.st_size, "allocated_bytes": stat_.st_blocks * 512}
    with sqlite3.connect(f"file:{path}?mode=ro", uri=True) as connection:
        record["page_size_bytes"] = connection.execute("PRAGMA page_size").fetchone()[0]
        record["page_count"] = connection.execute("PRAGMA page_count").fetchone()[0]
        if packs:
            record["pack_rows"], record["pack_blob_bytes"] = connection.execute(
                "SELECT count(*), coalesce(sum(length(data)), 0) FROM object_packs").fetchone()
    record["status"] = "PASS" if record["page_size_bytes"] == 4096 else "FAIL"
    return record


def diagnostic_100k(out, binaries, identity, *, release=False):
    case = init.CASES["namespace-100000"]
    folder = out / "sdk-host" / ("diagnostic-100k-release" if release else "diagnostic-100k") / case.id
    folder.mkdir(parents=True)
    receipt = {"schema": ("core-fs-bench-pro-sdk-init-100k-release-diagnostic-v1" if release
                          else "core-fs-bench-pro-sdk-init-100k-diagnostic-v1"),
               "benchmark_registration": "UNREGISTERED_DIAGNOSTIC", "case": case.id,
               "build_profile": "release" if release else "debug",
               "route": init.ROUTE, "fixture_profile": init.PROFILE, "seed": 1,
               "identity": identity, "binaries": binaries, "sample_count": 0,
               "public_api_call_count": 0, "admission_eligible": False,
               "performance_status": "INELIGIBLE", "status": "INCOMPLETE",
               "cache_contract": "zero-resident-source-payload; inode-and-directory-metadata-unqualified",
               "competing_work": competing_work(), "driver_watchdog_s": 610,
               "verifier_watchdog_s": 600}
    store, history = folder / "store.sqlite", folder / "history.sqlite"
    try:
        failures = residency.self_check()
        if failures:
            raise RuntimeError(f"residency backend self-check failed: {failures}")
        fixture = init.prepare(case, RESULTS / "sdk-prepared")
        receipt["fixture"] = fixture
        source = source_copy(case, fixture, out)
        preflight = payload_pages(source, fixture["manifest"], invalidate=True)
        write_json(folder / "cold-preflight.json", preflight)
        if (preflight["files"], preflight["bytes"], preflight["resident_pages"]) != (100_000, 500_000_000, 0):
            raise RuntimeError("source payload preflight failed")
        preflight_end = time.monotonic_ns()
        launch = payload_pages(source, fixture["manifest"], invalidate=False)
        launch["gap_since_preflight_ns"] = time.monotonic_ns() - preflight_end
        write_json(folder / "cold-launch.json", launch)
        if any(launch[key] != preflight[key] for key in ("files", "bytes", "page_size_bytes", "expected_pages")) or launch["resident_pages"] != 0:
            raise RuntimeError("source payload launch check failed")
        private, cursor = os.urandom(32).hex(), os.urandom(32).hex()
        command = [binaries["benchmark_init_memory"]["path"], str(source), str(store), str(history), case.id]
        receipt["command"] = command
        receipt["gap_preflight_to_driver_launch_ns"] = time.monotonic_ns() - preflight_end
        perf, process = diagnostic_child(command, folder, private, cursor)
        receipt.update(process)
        receipt["perf"] = perf
        write_json(folder / "source-residency-after.json", payload_pages(source, fixture["manifest"], invalidate=False))
        (folder / "perf.jsonl").write_text(json.dumps(perf, sort_keys=True) + "\n")
        entered = isinstance(perf.get("operation_ns"), int)
        receipt["sample_count"] = receipt["public_api_call_count"] = int(entered)
        receipt["raw_operation_ns"] = perf.get("operation_ns")
        complete = perf.get("status") == "COMPLETE" and process["exit_code"] == 0 and not process["timed_out"]
        if complete:
            verification = verify_child(binaries["verify_namespace"]["path"], folder,
                                        store, history, perf, fixture, cursor, timeout=600)
            expected = {"paths": 101_001, "directories": 1_001, "files": 100_000,
                        "bytes": 500_000_000, "manifest_sha256": fixture["manifest_sha256"],
                        "root": perf["root"]}
            if verification["status"] == "PASS" and any(
                verification["child"].get(key) != value for key, value in expected.items()
            ):
                verification["status"] = "FAIL"
                verification["error"] = "full oracle cardinality/root mismatch"
                write_json(folder / "verification.json", verification)
            receipt["verification"] = verification
        else:
            receipt["verification"] = {"status": "NOT_RUN", "reason": "no confirmed SDK root"}
            write_json(folder / "verification.json", receipt["verification"])
        receipt["cleanup_status"] = "PASS" if not process["timed_out"] else "KILLED_AFTER_WATCHDOG"
        receipt["status"] = "COMPLETE" if complete and receipt["verification"]["status"] == "PASS" else "FAIL"
    except Exception as error:
        receipt["status"] = "FAIL"
        receipt["error"] = repr(error)
    finally:
        for name, path in (("store", store), ("history", history)):
            try:
                write_json(folder / f"{name}-geometry.json", sqlite_geometry(path, packs=name == "store"))
            except Exception as error:
                write_json(folder / f"{name}-geometry.json", {"status": "FAIL", "error": repr(error)})
        receipt["scratch"] = [str(path.relative_to(folder)) for path in folder.rglob("*")
                              if path.is_file() and path.suffix not in (".json", ".jsonl", ".stdout", ".stderr", ".sqlite")]
        write_json(folder / "receipt.json", receipt)
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
            binaries["benchmark_init_memory"]["path"], case, Path(fixture["source"]), store, history,
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
    for name in ("diagnostic-100k", "diagnostic-100k-release"):
        diagnostic = run / "sdk-host" / name / "namespace-100000/receipt.json"
        if diagnostic.exists():
            row = json.loads(diagnostic.read_text())
            rows.append(f"{name}/{row['case']}\t{row['sample_count']}\t{row.get('raw_operation_ns')}\t"
                        f"{row.get('wall_ns')}\t{row.get('verification', {}).get('wall_ns')}\t"
                        f"{row['status']}\t{row['performance_status']}\t{row['cache_contract']}")
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
    for name, schema in (("diagnostic-100k", "core-fs-bench-pro-sdk-init-100k-diagnostic-v1"),
                         ("diagnostic-100k-release", "core-fs-bench-pro-sdk-init-100k-release-diagnostic-v1")):
        diagnostic = run / "sdk-host" / name / "namespace-100000"
        if diagnostic.exists():
            receipt = json.loads((diagnostic / "receipt.json").read_text())
            if receipt["schema"] != schema or receipt["sample_count"] not in (0, 1):
                raise ValueError("invalid diagnostic identity or cardinality")
            if receipt["sample_count"] == 1:
                lines = (diagnostic / "perf.jsonl").read_text().splitlines()
                if len(lines) != 1 or json.loads(lines[0])["operation_ns"] != receipt["raw_operation_ns"]:
                    raise ValueError("diagnostic raw timing mismatch")
                if json.loads((diagnostic / "verification.json").read_text()) != receipt["verification"]:
                    raise ValueError("diagnostic verification receipt mismatch")
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
    if identity["source_dirty"] and selection not in ("diagnostic-100k", "diagnostic-100k-release"):
        raise ValueError("commit the SDK route before collecting benchmark samples")
    target = target_path()
    out.mkdir(parents=True)
    build_receipt = build(out, target, identity, release=selection == "diagnostic-100k-release")
    write_json(out / "build.json", build_receipt)
    cycle_started = time.monotonic_ns()
    blocked = build_receipt["status"] if build_receipt["status"] == "FAIL" else None
    if blocked is None:
        RESULTS.mkdir(parents=True, exist_ok=True)
        with (RESULTS / ".run.lock").open("a+b") as lock:
            try:
                fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            except BlockingIOError:
                blocked = "same-worktree run active"
            if blocked is None:
                if selection in ("diagnostic-100k", "diagnostic-100k-release"):
                    diagnostic_100k(out, build_receipt["binaries"], identity,
                                    release=selection == "diagnostic-100k-release")
                else:
                    cases = [init.CASES[selection]] if selection in init.CASES else [init.CASES[name] for name in init.SELECTED]
                    for case in cases:
                        case_run(out, case, build_receipt["binaries"], identity)
    fill_not_run(out, selection, blocked)
    write_json(out / "run.json", {"schema": "core-fs-bench-pro-sdk-run-v2", "selection": selection,
        "benchmark_registration": "UNREGISTERED_DIAGNOSTIC" if selection.startswith("diagnostic-") else "REGISTERED_V2",
        "cases": list(init.SELECTED), "identity": identity, "blocked": blocked,
        "family_cycle_wall_ns": time.monotonic_ns() - cycle_started,
        "family_cycle_budget_ns": None if selection.startswith("diagnostic-") else 30_000_000_000})
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
    selector.add_argument("--family", choices=["init_namespace"])
    selector.add_argument("--diagnostic-100k", action="store_true")
    selector.add_argument("--diagnostic-100k-release", action="store_true")
    run_parser.add_argument("--out", required=True)
    for name in ("verify", "report"):
        commands.add_parser(name).add_argument("--run", required=True)
    args = parser.parse_args()
    if args.command == "list":
        for case in init.CASES.values():
            print(f"{case.id}\t{case.files}\t{case.logical_bytes}\t"
                  f"{'SDK selected' if case.id in init.SELECTED else 'NOT_RUN ' + init.NOT_RUN_REASON}")
    elif args.command == "run":
        selection = ("diagnostic-100k-release" if args.diagnostic_100k_release else
                     "diagnostic-100k" if args.diagnostic_100k else args.case or args.family)
        if selection not in (*init.SELECTED, "init_namespace", "diagnostic-100k", "diagnostic-100k-release"):
            parser.error("unknown or deferred SDK case")
        print(run(selection, args.out))
    elif args.command == "verify":
        print(verify_run(owned(args.run, existing=True)))
    else:
        print(report(owned(args.run, existing=True)), end="")


if __name__ == "__main__":
    main()
