#!/usr/bin/env python3
"""One-sample daemon-host Init loop with append-only evidence."""
import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import resource
import select
import shutil
import subprocess
import sys
import threading
import time
import uuid

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
CORE = ROOT / "core"
RESULTS = ROOT / "benchmark-results/fs-bench-pro"
sys.path.insert(0, str(HERE))
from families import init_namespace as init  # noqa: E402
from shared import telemetry  # noqa: E402

BUILD = ["cargo", "+1.85.1", "build", "--manifest-path", "core/Cargo.toml", "--locked",
         "-p", "layerfs-service", "-p", "layerfs-daemon", "-p", "layerfs-bridge",
         "--bin", "layerfs-service", "--bin", "layerfs-daemon", "--example", "public_key",
         "--example", "create_store", "--example", "verify_namespace"]
BINARIES = ("layerfs-service", "layerfs-daemon", "public_key", "create_store", "verify_namespace")
CANARY_LABELS = ["ConstructFile", "ConstructFile", "HistoryCommand", "HistoryCommand",
                 "HistoryQuery", "Inspect", "HistoryCommand", "HistoryCommand",
                 "HistoryCommand", "HistoryCommand", "HistoryQuery", "Inspect",
                 "HistoryCommand", "HistoryCommand", "HistoryCommand", "HistoryCommand",
                 "HistoryCommand"]


def sha(data):
    return hashlib.sha256(data).hexdigest()


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
    if not path.is_relative_to(root) or path == root or (path.exists() != existing):
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
    product += [CORE / "Cargo.toml", CORE / "Cargo.lock", ROOT / ".cargo/config.toml"]
    harness = list(HERE.glob("**/*.py"))
    source = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    dirty = subprocess.check_output(["git", "status", "--porcelain"], cwd=ROOT, text=True)
    return {"source_commit": source, "source_dirty": bool(dirty), "dirty_paths": dirty.splitlines(),
            "product_seal": seal(product), "harness_seal": seal(harness),
            "cargo_lock_sha256": digest(CORE / "Cargo.lock")}


def build(out, target, identity):
    cache = RESULTS / "build.json"
    prior = json.loads(cache.read_text()) if cache.exists() else None
    if prior and prior.get("product_seal") == identity["product_seal"] and all(
        Path(path).is_file() and digest(path) == prior["binaries"][name]["sha256"]
        for name, path in ((name, prior["binaries"][name]["path"]) for name in BINARIES)
    ):
        return {"mode": "exact-binary-reuse", "wall_ns": 0, "status": "PASS", "command": None,
                "binaries": prior["binaries"]}
    env = {**os.environ, "CARGO_TARGET_DIR": str(target)}
    started = time.monotonic_ns()
    with (out / "build.log").open("wb") as log:
        result = subprocess.run(BUILD, cwd=ROOT, env=env, stdout=log, stderr=subprocess.STDOUT)
    wall = time.monotonic_ns() - started
    record = {"mode": "changed-product" if prior else "first-use", "wall_ns": wall,
              "status": "BUILD_SLOW" if wall > 30_000_000_000 else "PASS" if result.returncode == 0 else "FAIL",
              "exit_code": result.returncode, "command": BUILD, "target": str(target)}
    if result.returncode == 0:
        binaries = {}
        for name in BINARIES:
            source = target / "debug" / (name if name in ("layerfs-service", "layerfs-daemon") else f"examples/{name}")
            identity_hash = digest(source)
            archive = RESULTS / "binary-archive" / identity_hash / name
            archive.parent.mkdir(parents=True, exist_ok=True)
            if not archive.exists():
                shutil.copy2(source, archive)
                archive.chmod(0o555)
            binaries[name] = {"path": str(archive), "sha256": identity_hash}
        record["binaries"] = binaries
        write_json(cache, {"product_seal": identity["product_seal"], "binaries": binaries})
    return record


def public_key(binary, private):
    return subprocess.check_output([binary,], env={**os.environ, "LAYERFS_PRIVATE_KEY": private}, text=True).strip()


def _drain(pipe, file):
    with file.open("ab") as output:
        shutil.copyfileobj(pipe, output)


def _start_service(binary, env, stderr):
    process = subprocess.Popen([binary], env=env, stdin=subprocess.PIPE, stdout=subprocess.DEVNULL,
                               stderr=subprocess.PIPE)
    if not select.select([process.stderr], [], [], 5)[0]:
        raise TimeoutError("Service did not announce readiness")
    first = process.stderr.readline()
    stderr.write_bytes(first)
    if not first.startswith(b"layerfs-service ready "):
        raise RuntimeError(f"Service startup: {first[:200]!r}")
    port = int(first.rstrip().rsplit(b":", 1)[1])
    thread = threading.Thread(target=_drain, args=(process.stderr, stderr), daemon=True)
    thread.start()
    return process, thread, port


def _start_daemon(binary, env, stderr):
    process = subprocess.Popen([binary], env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                               stderr=subprocess.PIPE)
    thread = threading.Thread(target=_drain, args=(process.stderr, stderr), daemon=True)
    thread.start()
    return process, thread


def _stop(process, thread):
    if process is None:
        return "NOT_STARTED"
    try:
        if process.stdin and not process.stdin.closed:
            process.stdin.close()
        process.wait(timeout=3)
    except (subprocess.TimeoutExpired, OSError):
        process.terminate()
        try:
            process.wait(timeout=2)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait(timeout=2)
        return "FAIL"
    finally:
        if thread:
            thread.join(timeout=2)
    return "PASS" if process.returncode == 0 else "FAIL"


def _competing():
    process = subprocess.run(["ps", "-axo", "pid=,comm="], capture_output=True, text=True)
    return [line.strip() for line in process.stdout.splitlines()
            if any(word in line.lower() for word in ("cargo", "docker", "layerfs")) and
            not line.strip().startswith(str(os.getpid()) + " ")][:32]


def _verify_child(binary, case_dir, store, history, sample, fixture, cursor):
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
    (case_dir / "verifier.stdout").write_bytes(stdout)
    (case_dir / "verifier.stderr").write_bytes(stderr)
    receipt = {"status": status, "wall_ns": wall, "budget_ns": 5_000_000_000,
               "exit_code": exit_code, "command": command, "child": child,
               "stderr": stderr[:4096].decode(errors="replace")}
    write_json(case_dir / "verification.json", receipt)
    return receipt


def _case(out, case, binaries, identity):
    case_dir = out / "daemon-host" / "init_namespace" / case.id
    case_dir.mkdir(parents=True)
    started = time.monotonic_ns()
    receipt = {"schema": "core-fs-bench-pro-init-v1", "case": case.id, "route": "daemon-host",
               "identity": identity, "cache_contract": "source-cache-uncontrolled-v1",
               "admission_eligible": False, "performance_gate": "NOT_FROZEN", "sample_count": 0,
               "placement": {"service": "host", "daemon": "host", "image_id": None}}
    service = daemon = service_thread = daemon_thread = None
    service_log, daemon_log = case_dir / "service.stderr", case_dir / "daemon.stderr"
    fixture = perf = verification = parsed = None
    cleanup = {}
    try:
        fixture = init.prepare(case, RESULTS / "prepared")
        receipt["fixture"] = fixture
        store, history = case_dir / "store.sqlite", case_dir / "history.sqlite"
        store_started = time.monotonic_ns()
        subprocess.run([binaries["create_store"]["path"], str(store)], check=True,
                       stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        receipt["store_create_ns"] = time.monotonic_ns() - store_started
        server_private, client_private = os.urandom(32).hex(), os.urandom(32).hex()
        server_public = public_key(binaries["public_key"]["path"], server_private)
        client_public = public_key(binaries["public_key"]["path"], client_private)
        run_id = uuid.uuid4().int
        run_hex = f"{run_id:032x}"
        namespace = (uuid.uuid4().int & ((1 << 63) - 1)) or 1
        cursor = os.urandom(32).hex()
        receipt.update({"run_id": run_hex, "service_namespace": namespace,
                        "daemon_namespace": namespace + 1, "competing_work": _competing()})
        expires = int(time.time()) + 3600
        common = {"LAYERFS_TELEMETRY": "forward", "LAYERFS_RUN_ID": str(run_id)}
        service_env = {**os.environ, **common, "LAYERFS_NAMESPACE": str(namespace),
                       "LAYERFS_PRIVATE_KEY": server_private,
                       "LAYERFS_PEERS": f"1,{client_public},{expires},127",
                       "LAYERFS_STORE": str(store), "LAYERFS_LISTEN": "127.0.0.1:0",
                       "LAYERFS_IMPORT_ROOT": fixture["source"],
                       "LAYERFS_HISTORY_CATALOG": str(history), "LAYERFS_HISTORY_BINDING": "layerfs-bench-pro",
                       "LAYERFS_HISTORY_CREATE": "1", "LAYERFS_HISTORY_INCARNATION": "1",
                       "LAYERFS_HISTORY_CURSOR_KEY": cursor}
        cpu_before = resource.getrusage(resource.RUSAGE_CHILDREN)
        command_started = time.monotonic_ns()
        service, service_thread, port = _start_service(binaries["layerfs-service"]["path"], service_env, service_log)
        receipt["service_pid"] = service.pid
        daemon_env = {**os.environ, **common, "LAYERFS_NAMESPACE": str(namespace + 1),
                      "LAYERFS_PRIVATE_KEY": client_private, "LAYERFS_SERVER_KEY": server_public,
                      "LAYERFS_ENDPOINT": f"127.0.0.1:{port}", "LAYERFS_SELECTOR": "1"}
        daemon, daemon_thread = _start_daemon(binaries["layerfs-daemon"]["path"], daemon_env, daemon_log)
        receipt["daemon_pid"] = daemon.pid
        perf = init.public_import(daemon, case, os.urandom(16), os.urandom(32))
        (case_dir / "perf.jsonl").write_text(json.dumps(perf, sort_keys=True) + "\n")
        receipt["sample_count"] = 1
        cleanup_started = time.monotonic_ns()
        cleanup["daemon"] = _stop(daemon, daemon_thread)
        daemon = None
        cleanup["service"] = _stop(service, service_thread)
        service = None
        cpu_after = resource.getrusage(resource.RUSAGE_CHILDREN)
        receipt["external_resources"] = {"scope": "service-and-daemon-lifecycle",
                                         "user_cpu_s": cpu_after.ru_utime - cpu_before.ru_utime,
                                         "system_cpu_s": cpu_after.ru_stime - cpu_before.ru_stime,
                                         "rss_phase_peak": None, "rss_reason": "rusage peak is lifetime, not phase"}
        receipt["performance_command_wall_ns"] = time.monotonic_ns() - command_started
        if perf["status"] == "COMPLETE":
            verification = _verify_child(binaries["verify_namespace"]["path"], case_dir, store, history, perf, fixture, cursor)
        else:
            verification = {"status": "NOT_RUN", "reason": "public Init did not return a confirmed root"}
            write_json(case_dir / "verification.json", verification)
        parsed = telemetry.ingest([
            {"name": "service", "path": str(service_log), "pid": receipt.get("service_pid"), "role": 1, "namespace": namespace},
            {"name": "daemon", "path": str(daemon_log), "pid": receipt.get("daemon_pid"), "role": 2, "namespace": namespace + 1},
        ], case_dir / "telemetry.lft1", run_hex, allow_final_failure=perf["status"] != "COMPLETE")
        receipt["telemetry"] = parsed
        if parsed["status"] == "PASS" and cleanup["daemon"] == cleanup["service"] == "PASS":
            service_log.unlink()
            daemon_log.unlink()
            cleanup["stderr_recycle"] = "PASS"
        else:
            cleanup["stderr_recycle"] = "RETAINED_INCOMPLETE"
        receipt["cleanup_wall_ns"] = time.monotonic_ns() - cleanup_started
        receipt["verification"] = verification
        receipt["status"] = "INELIGIBLE" if (perf["status"] == "COMPLETE" and verification["status"] == "PASS" and parsed["status"] == "PASS" and
                                                  all(value == "PASS" for value in cleanup.values())) else "FAIL" if any(value == "FAIL" for value in cleanup.values()) else "INCOMPLETE"
    except Exception as error:
        receipt["status"] = "FAIL" if perf else "NOT_RUN"
        receipt["error"] = repr(error)
    finally:
        if daemon is not None:
            cleanup["daemon"] = _stop(daemon, daemon_thread)
        if service is not None:
            cleanup["service"] = _stop(service, service_thread)
        receipt["cleanup"] = cleanup
        receipt["wall_ns"] = time.monotonic_ns() - started
        receipt["store_bytes"] = (case_dir / "store.sqlite").stat().st_size if (case_dir / "store.sqlite").exists() else None
        receipt["history_bytes"] = (case_dir / "history.sqlite").stat().st_size if (case_dir / "history.sqlite").exists() else None
        if perf:
            receipt["raw_operation_ns"] = perf["operation_ns"]
        if receipt.get("performance_command_wall_ns", 0) > 15_000_000_000:
            receipt["performance_command_status"] = "COMMAND_SLOW"
        else:
            receipt["performance_command_status"] = "PASS" if perf else "NOT_RUN"
        write_json(case_dir / "receipt.json", receipt)
    return receipt


def _canary(out, binaries, identity):
    """The #179 daemon/Service/C1/C2/C5 public route, including its expected refusal."""
    folder = out / "canary"
    folder.mkdir()
    service = daemon = service_thread = daemon_thread = None
    service_log, daemon_log = folder / "service.stderr", folder / "daemon.stderr"
    receipt = {"schema": "core-fs-bench-pro-canary-v1", "identity": identity,
               "route": "history_route.run_cases", "status": "INCOMPLETE"}
    try:
        store, history = folder / "store.sqlite", folder / "history.sqlite"
        subprocess.run([binaries["create_store"]["path"], str(store)], check=True,
                       stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        server_private, client_private = os.urandom(32).hex(), os.urandom(32).hex()
        server_public = public_key(binaries["public_key"]["path"], server_private)
        client_public = public_key(binaries["public_key"]["path"], client_private)
        run_id = uuid.uuid4().int
        run_hex = f"{run_id:032x}"
        namespace = (uuid.uuid4().int & ((1 << 63) - 1)) or 1
        cursor = os.urandom(32).hex()
        receipt.update(run_id=run_hex, service_namespace=namespace, daemon_namespace=namespace + 1)
        expires = int(time.time()) + 3600
        common = {"LAYERFS_TELEMETRY": "forward", "LAYERFS_RUN_ID": str(run_id)}
        service_env = {**os.environ, **common, "LAYERFS_NAMESPACE": str(namespace),
                       "LAYERFS_PRIVATE_KEY": server_private,
                       "LAYERFS_PEERS": f"1,{client_public},{expires},127",
                       "LAYERFS_STORE": str(store), "LAYERFS_LISTEN": "127.0.0.1:0",
                       "LAYERFS_HISTORY_CATALOG": str(history), "LAYERFS_HISTORY_BINDING": "layerfs-bench-pro",
                       "LAYERFS_HISTORY_CREATE": "1", "LAYERFS_HISTORY_INCARNATION": "1",
                       "LAYERFS_HISTORY_CURSOR_KEY": cursor}
        service, service_thread, port = _start_service(binaries["layerfs-service"]["path"], service_env, service_log)
        receipt["service_pid"] = service.pid
        daemon_env = {**os.environ, **common, "LAYERFS_NAMESPACE": str(namespace + 1),
                      "LAYERFS_PRIVATE_KEY": client_private, "LAYERFS_SERVER_KEY": server_public,
                      "LAYERFS_ENDPOINT": f"127.0.0.1:{port}", "LAYERFS_SELECTOR": "1"}
        daemon, daemon_thread = _start_daemon(binaries["layerfs-daemon"]["path"], daemon_env, daemon_log)
        receipt["daemon_pid"] = daemon.pid
        witnessed = []
        result = init._route().run_cases(daemon, witnessed)
        receipt["public_result"] = result
        receipt["witnesses"] = witnessed
        daemon_result = _stop(daemon, daemon_thread)
        receipt["daemon_terminal"] = {"status": "EXPECTED_REFUSAL" if daemon_result == "FAIL" and daemon.returncode == 1 else daemon_result,
                                      "exit_code": daemon.returncode,
                                      "expected_refusal": True}
        daemon = None
        receipt["service_cleanup"] = _stop(service, service_thread)
        service = None
        parsed = telemetry.ingest([
            {"name": "service", "path": str(service_log), "pid": receipt["service_pid"], "role": 1, "namespace": namespace},
            {"name": "daemon", "path": str(daemon_log), "pid": receipt["daemon_pid"], "role": 2, "namespace": namespace + 1},
        ], folder / "telemetry.lft1", run_hex, labels=CANARY_LABELS, allow_final_failure=True)
        receipt["telemetry"] = parsed
        if parsed["status"] == "PASS":
            service_log.unlink()
            daemon_log.unlink()
            receipt["stderr_recycle"] = "PASS"
        else:
            receipt["stderr_recycle"] = "RETAINED_INCOMPLETE"
        receipt["status"] = "PASS" if parsed["status"] == "PASS" and receipt["service_cleanup"] == "PASS" and len(witnessed) == 10 else "INCOMPLETE"
    except Exception as error:
        receipt["error"] = repr(error)
    finally:
        if daemon is not None:
            receipt["daemon_cleanup"] = _stop(daemon, daemon_thread)
        if service is not None:
            receipt["service_cleanup"] = _stop(service, service_thread)
        write_json(folder / "receipt.json", receipt)
    return receipt


def report(run):
    rows = []
    for case in init.CASES.values():
        file = run / "daemon-host" / "init_namespace" / case.id / "receipt.json"
        if not file.exists():
            raise ValueError(f"missing registered case receipt: {case.id}")
        row = json.loads(file.read_text())
        rows.append(f"{case.id}\t{case.files}\t{case.logical_bytes}\t{row['status']}\t"
                    f"{row.get('sample_count', 0)}\t{row.get('raw_operation_ns')}\t"
                    f"{row.get('performance_command_wall_ns')}\t{row.get('cache_contract', 'not-run')}\t"
                    f"{row.get('admission_eligible', False)}\t{row.get('verification', {}).get('status')}\t"
                    f"{row.get('telemetry', {}).get('status')}\t{row.get('cleanup')}")
    canary = run / "canary" / "receipt.json"
    prefix = f"canary: {json.loads(canary.read_text())['status']}\n" if canary.exists() else ""
    return prefix + "case\tfiles\tlogical_bytes\tstatus\tsamples\toperation_ns\tcommand_ns\tcache\teligible\tverification\ttelemetry\tcleanup\n" + "\n".join(rows) + "\n"


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
    canary_file = run / "canary" / "receipt.json"
    if canary_file.exists():
        canary = json.loads(canary_file.read_text())
        parsed = canary.get("telemetry")
        if parsed:
            again = telemetry.inspect((canary_file.parent / "telemetry.lft1").read_bytes(),
                                      parsed["producers"], canary["run_id"], labels=CANARY_LABELS,
                                      allow_final_failure=True)
            if again["status"] != parsed["status"]:
                raise ValueError("canary telemetry interpretation drift")
    for case in init.CASES.values():
        folder = run / "daemon-host" / "init_namespace" / case.id
        receipt = json.loads((folder / "receipt.json").read_text())
        if case.id == "namespace-100000":
            if receipt["status"] != "NOT_RUN" or receipt.get("sample_count") != 0:
                raise ValueError("deferred tier changed")
            continue
        if receipt.get("sample_count") not in (0, 1):
            raise ValueError("invalid one-sample cardinality")
        if receipt["sample_count"] == 1:
            lines = (folder / "perf.jsonl").read_text().splitlines()
            if len(lines) != 1 or json.loads(lines[0])["operation_ns"] != receipt["raw_operation_ns"]:
                raise ValueError("raw performance mismatch")
            parsed = receipt["telemetry"]
            again = telemetry.inspect((folder / "telemetry.lft1").read_bytes(), parsed["producers"], receipt["run_id"],
                                      allow_final_failure=json.loads(lines[0]).get("status") != "COMPLETE")
            if again["status"] != parsed["status"] or again["operation_counts"] != parsed["operation_counts"]:
                raise ValueError("telemetry interpretation drift")
            child = json.loads((folder / "verification.json").read_text())
            if child != receipt["verification"]:
                raise ValueError("verifier receipt drift")
    return "PASS"


def _fill_not_run(out, selection, blocked=None):
    for case in init.CASES.values():
        folder = out / "daemon-host" / "init_namespace" / case.id
        if folder.exists():
            continue
        folder.mkdir(parents=True)
        selected = selection == "init_namespace" and case.id in init.SELECTED or selection == case.id
        write_json(folder / "receipt.json", {"case": case.id, "status": "NOT_RUN", "sample_count": 0,
                                             "reason": init.NOT_RUN_REASON if case.id == "namespace-100000"
                                             else blocked if selected and blocked else "not selected"})


def run(selection, out):
    target = target_path()
    identity = identities()
    out = owned(out)
    out.mkdir(parents=True)
    started = time.monotonic_ns()
    build_receipt = build(out, target, identity)
    write_json(out / "build.json", build_receipt)
    cycle_started = time.monotonic_ns()
    blocked = None
    if build_receipt["status"] != "PASS":
        blocked = build_receipt["status"]
    else:
        RESULTS.mkdir(parents=True, exist_ok=True)
        with (RESULTS / ".run.lock").open("a+b") as lock:
            try:
                fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            except BlockingIOError:
                blocked = "same-worktree run active"
            if blocked is None:
                cases = [init.CASES[selection]] if selection in init.CASES else [init.CASES[name] for name in init.SELECTED] if selection == "init_namespace" else []
                if selection == "history-canary":
                    _canary(out, build_receipt["binaries"], identity)
                for case in cases:
                    if case.id != "namespace-100000":
                        _case(out, case, build_receipt["binaries"], identity)
    if blocked:
        write_json(out / "blocked.json", {"status": "NOT_RUN", "reason": blocked, "identity": identity})
    _fill_not_run(out, selection, blocked)
    write_json(out / "run.json", {"schema": "core-fs-bench-pro-run-v1", "selection": selection,
                                  "identity": identity, "family_cycle_wall_ns": time.monotonic_ns() - cycle_started,
                                  "complete_run_wall_ns": time.monotonic_ns() - started,
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
    selector.add_argument("--family", choices=["init_namespace"])
    run_parser.add_argument("--out", required=True)
    for name in ("verify", "report"):
        sub = commands.add_parser(name)
        sub.add_argument("--run", required=True)
    args = parser.parse_args()
    if args.command == "list":
        print("history-canary\tdiagnostic #179 public route")
        for case in init.CASES.values():
            print(f"{case.id}\t{case.files}\t{case.logical_bytes}\t"
                  f"{'NOT_RUN ' + init.NOT_RUN_REASON if case.id == 'namespace-100000' else 'discovery'}")
    elif args.command == "run":
        selection = args.case or args.family
        if selection not in (*init.SELECTED, "init_namespace", "history-canary"):
            parser.error("unknown or deferred case")
        print(run(selection, args.out))
    elif args.command == "verify":
        print(verify_run(owned(args.run, existing=True)))
    else:
        print(report(owned(args.run, existing=True)), end="")


if __name__ == "__main__":
    main()
