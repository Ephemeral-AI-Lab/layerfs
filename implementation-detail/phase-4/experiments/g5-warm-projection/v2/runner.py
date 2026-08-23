#!/usr/bin/env python3
import argparse
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
FREEZE = HERE / "method/SOURCE-FREEZE-v2.json"
INPUT_MANIFEST = HERE / "method/INPUT-MANIFEST-v2.json"
DRY_RUN = HERE / "DRY-RUN-v2.json"
PRIMARY = HERE / "analyzers/primary.py"
INDEPENDENT = HERE / "analyzers/independent.py"
V1_DISPOSITION = HERE.parent / "v1/PREMEASUREMENT-REVISE-v1.json"
V2_DISPOSITION = HERE / "PREMEASUREMENT-REVISE-v2.json"
LIMITS = {"screen": 19_999_999_999, "gate": 150_000_000_000}
POPULATIONS = {"screen": [2, 2], "gate": [64, 100]}
MODE_POPULATIONS = {"self-check": [1, 0], "screen-count": [1, 1], **POPULATIONS}
CAMPAIGNS = {"screen": ["self-check", "screen-count", "screen"], "gate": ["self-check", "screen-count", "gate"]}
RESULT_NAMES = {"screen": "phase4-g5-warm-projection-v2-screen", "gate": "phase4-g5-warm-projection-v2-gate"}
AUTHORITATIVE = (
    HERE / "runner.py", PRIMARY, INDEPENDENT, HERE / "PREREGISTRATION-v2.md",
    HERE / "LIMITATIONS-v2.md", HERE / "method/SCHEDULE-v2.tsv",
    HERE / "method/EXPECTED-OUTCOMES-v2.tsv", HERE / "FOCUSED-TEST-ATTEMPTS-v2.json",
    HERE / "method/CONSERVATION-SEMANTICS-v2.json", HERE / "method/FAULT-MATRIX-v2.tsv",
    HERE / "PROMOTION-READINESS-v2.md", HERE / "PREMEASUREMENT-REVISE-v2.json",
    V1_DISPOSITION, INPUT_MANIFEST,
    REPO / "crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs",
    REPO / "crates/layerfs-engine/src/bin/phase4_g3_materialization.rs",
)


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
    for path in sorted(root.rglob("*")):
        relative = path.relative_to(root).as_posix()
        if path.is_symlink():
            raise RuntimeError(f"symlink forbidden in fixture: {relative}")
        stat = path.stat()
        if path.is_dir():
            rows.append({"path": relative + "/", "kind": "directory", "mode": stat.st_mode & 0o7777})
        elif path.is_file():
            rows.append({"path": relative, "kind": "file", "mode": stat.st_mode & 0o7777, "bytes": stat.st_size, "sha256": sha256(path)})
        else:
            raise RuntimeError(f"unsupported fixture entry: {relative}")
    return {"entries": rows, "sha256": hashlib.sha256(compact(rows).encode()).hexdigest()}


def load_json(path):
    return json.loads(pathlib.Path(path).read_text(encoding="utf-8"))


def require_promotion_authorized():
    disposition = load_json(V2_DISPOSITION)
    if disposition.get("status") != "PASS" or disposition.get("authorization") != "AUTHORIZED":
        raise RuntimeError("G5-2 v2 remains PREMEASUREMENT_REVISE; preparation/freeze/campaign closed")


def prepare_inputs(executable, input_root):
    require_promotion_authorized()
    executable, input_root = pathlib.Path(executable).resolve(), pathlib.Path(input_root).resolve()
    if INPUT_MANIFEST.exists() or input_root.exists():
        raise RuntimeError("input preparation is one-shot; manifest/root already exists")
    input_root.mkdir(parents=True)
    records = {}
    try:
        for mode in ("self-check", "screen-count", "screen", "gate"):
            fixture = input_root / mode
            completed = subprocess.run([str(executable), "--g5-projection-prepare", str(fixture), mode], text=True, capture_output=True, check=True)
            product = json.loads(completed.stdout)
            if product.get("status") != "PASS" or product.get("mode") != mode or product.get("preparation_timing") != "outside-campaign":
                raise RuntimeError(f"invalid {mode} preparation receipt")
            records[mode] = {"root": str(fixture), "product": product, "inventory": inventory(fixture)}
        manifest = {"schema": "phase4-g5-2-input-manifest-v2", "status": "PASS", "executable_sha256": sha256(executable), "preparation_timing": "outside-campaign", "inputs": records}
        write_json(INPUT_MANIFEST, manifest)
        return manifest
    except Exception:
        shutil.rmtree(input_root, ignore_errors=True)
        raise


def freeze(executable):
    require_promotion_authorized()
    executable = pathlib.Path(executable).resolve()
    if FREEZE.exists():
        raise RuntimeError("freeze already exists")
    manifest = load_json(INPUT_MANIFEST)
    if manifest.get("status") != "PASS" or manifest.get("executable_sha256") != sha256(executable):
        raise RuntimeError("input/executable custody mismatch")
    missing = [str(path) for path in AUTHORITATIVE if not path.is_file()]
    if missing:
        raise RuntimeError(f"missing authoritative files: {missing}")
    receipt = {
        "schema": "phase4-g5-2-source-freeze-v2", "status": "PASS",
        "executable": str(executable), "executable_sha256": sha256(executable),
        "input_manifest_sha256": sha256(INPUT_MANIFEST),
        "authoritative_files": [{"path": str(path.relative_to(REPO)), "bytes": path.stat().st_size, "sha256": sha256(path)} for path in AUTHORITATIVE],
        "screen_limit_ns": LIMITS["screen"], "gate_limit_ns": LIMITS["gate"],
        "populations": POPULATIONS, "mode_populations": MODE_POPULATIONS,
        "campaigns": CAMPAIGNS,
    }
    write_json(FREEZE, receipt)
    return receipt


def verify_freeze():
    frozen = load_json(FREEZE)
    if frozen.get("status") != "PASS" or frozen.get("input_manifest_sha256") != sha256(INPUT_MANIFEST):
        raise RuntimeError("freeze binding mismatch")
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
    for mode in ("self-check", "screen-count", "screen", "gate"):
        if inventory(manifest["inputs"][mode]["root"])["sha256"] != manifest["inputs"][mode]["inventory"]["sha256"]:
            raise RuntimeError(f"sealed {mode} input changed")
    return frozen, manifest


def forecast():
    require_promotion_authorized()
    verify_freeze()
    exact_ns = 4 * 66 * 8_000_000
    latest_ns = 4 * 101 * 10_000_000
    fallback_ns = 4 * 400_000_000
    wrapper_ns = 10_000_000_000
    total = exact_ns + latest_ns + fallback_ns + wrapper_ns
    receipt = {
        "schema": "phase4-g5-2-zero-row-forecast-v2", "status": "PASS" if total <= LIMITS["gate"] else "REVISE",
        "product_processes": 0, "product_rows": 0, "lock_acquisitions": 0,
        "components_ns": {"exact_population_4x": exact_ns, "latest_population_4x": latest_ns, "fallback_4x": fallback_ns, "clone_checkpoint_analyzers_custody_cleanup": wrapper_ns},
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
    write_json(process_root / "COMMAND.json", {"schema": "phase4-g5-2-process-command-v2", "argv": list(command)})


def persist_process_evidence(process_root, stdout, stderr, returncode, timed_out, rss_text, process_started_ns=None, process_ended_ns=None):
    process_root = pathlib.Path(process_root)
    if not (process_root / "COMMAND.json").is_file():
        raise RuntimeError("process command intent was not persisted")
    write_text(process_root / "STDOUT.txt", text_value(stdout))
    write_text(process_root / "STDERR.txt", text_value(stderr))
    elapsed_ns = process_ended_ns - process_started_ns if process_started_ns is not None and process_ended_ns is not None else None
    write_json(process_root / "RETURN.json", {"schema": "phase4-g5-2-process-return-v2", "returncode": returncode, "timed_out": timed_out, "process_started_ns": process_started_ns, "process_ended_ns": process_ended_ns, "process_elapsed_ns": elapsed_ns})
    write_text(process_root / "RSS.txt", rss_text)
    try:
        parsed = json.loads(text_value(stdout))
        parsed_evidence = {"schema": "phase4-g5-2-parsed-product-receipt-v2", "status": "Available", "receipt": parsed}
    except Exception as error:
        parsed = None
        parsed_evidence = {"schema": "phase4-g5-2-parsed-product-receipt-v2", "status": "Unavailable", "error": f"{type(error).__name__}: {error}"}
    write_json(process_root / "PARSED-RECEIPT.json", parsed_evidence)
    rss = None
    try:
        rss = parse_rss(process_root / "RSS.txt")
    except Exception:
        pass
    files = ("COMMAND.json", "CLONE.json", "STDOUT.txt", "STDERR.txt", "RETURN.json", "RSS.txt", "PARSED-RECEIPT.json")
    evidence = {
        "schema": "phase4-g5-2-lossless-process-evidence-v2",
        "status": "PASS" if returncode == 0 and not timed_out and parsed is not None and rss is not None else "REVISE",
        "returncode": returncode, "timed_out": timed_out, "maximum_resident_set_size": rss, "process_started_ns": process_started_ns, "process_ended_ns": process_ended_ns, "process_elapsed_ns": elapsed_ns,
        "artifacts": [{"name": name, "bytes": (process_root / name).stat().st_size, "sha256": sha256(process_root / name)} for name in files],
    }
    write_json(process_root / "PROCESS-EVIDENCE.json", evidence)
    return evidence, parsed


def run_analyzer(script, raw, output):
    return subprocess.run([sys.executable, str(script), str(raw), str(output)], text=True, capture_output=True)


def campaign(phase):
    require_promotion_authorized()
    result = TARGET / RESULT_NAMES[phase]
    if result.exists():
        raise RuntimeError("one-shot result root already exists")
    intent = {"schema": "phase4-g5-2-lock-intent-v2", "phase": phase, "result": str(result), "freeze_sha256": sha256(FREEZE), "pid": os.getpid()}
    started = time.monotonic_ns()
    lock_ownership = acquire_lock(intent)
    attempt = None
    products = []
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
                subprocess.run(["/bin/cp", "-cR", str(source), str(attempt)], check=True, timeout=10)
                source_inventory, attempt_inventory = manifest["inputs"][mode]["inventory"], inventory(attempt)
                clone = {"schema": "phase4-g5-2-clone-receipt-v2", "status": "PASS", "method": "APFSCloneCpC", "inventory_equal": source_inventory["sha256"] == attempt_inventory["sha256"], "source_inventory_sha256": source_inventory["sha256"], "attempt_inventory_sha256": attempt_inventory["sha256"]}
                if not clone["inventory_equal"]:
                    raise RuntimeError(f"{mode} clone inventory mismatch")
                write_json(process_root / "CLONE.json", clone)
            except Exception as clone_error:
                write_json(process_root / "CLONE.json", {"schema": "phase4-g5-2-clone-receipt-v2", "status": "REVISE", "method": "APFSCloneCpC", "inventory_equal": False, "error": f"{type(clone_error).__name__}: {clone_error}"})
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
            products.append({"mode": mode, "product": product, "maximum_resident_set_size": evidence["maximum_resident_set_size"], "clone": clone, "process_evidence_sha256": process_evidence[-1]["sha256"]})
            shutil.rmtree(attempt)
            attempt = None
            fsync_dir(result)
        envelope = {
            "schema": "phase4-g5-2-harness-row-v2", "status": "PASS", "phase": phase,
            "analysis_stage": "preliminary",
            "product_processes": len(products), "products": products,
            "maximum_resident_set_size": max(item["maximum_resident_set_size"] for item in products),
            "evidence_assembly_elapsed_ns": time.monotonic_ns() - started,
            "cache_state": "WarmUnknownPreparedFixtureAPFSClone", "cold_reopen_claim": False,
        }
        preliminary_raw = result / "RAW-PRELIMINARY-v2.jsonl"
        write_text(preliminary_raw, compact(envelope) + "\n")
        preliminary_primary = result / "PRIMARY-PRELIMINARY-v2.json"
        preliminary_independent = result / "INDEPENDENT-PRELIMINARY-v2.json"
        first = run_analyzer(PRIMARY, preliminary_raw, preliminary_primary)
        second = run_analyzer(INDEPENDENT, preliminary_raw, preliminary_independent)
        if first.returncode or second.returncode:
            raise RuntimeError(f"preliminary analyzer failure: {first.stdout} {second.stdout}")
        primary, independent = load_json(preliminary_primary), load_json(preliminary_independent)
        if primary.get("normalized") != independent.get("normalized"):
            raise RuntimeError("preliminary analyzer disagreement")
        released = release_lock(lock_ownership)
        lock_ownership = None
        write_json(result / "LOCK-RELEASE-v2.json", {"schema": "phase4-g5-2-lock-release-v2", "status": "PASS", "phase": phase, **released})
        complete_wall = time.monotonic_ns() - started
        terminal = {"schema": "phase4-g5-2-terminal-v2", "status": "PASS" if complete_wall <= LIMITS[phase] else "REVISE", "phase": phase, "complete_wall_ns": complete_wall, "limit_ns": LIMITS[phase], "wall_scope": "CustodyCloneThreeProductsCleanupOneAnalyzerPairThroughLockRelease", "lock_released": True, "product_processes": 3, "product_rows": 3, "primary_decision_rows": 1, "preliminary_analyzer_agreement": True, "terminal_fixture_roots": 0}
        write_json(result / "TERMINAL-v2.json", terminal)
        envelope["analysis_stage"] = "final"
        envelope["terminal"] = terminal
        envelope["terminal_sha256"] = sha256(result / "TERMINAL-v2.json")
        raw = result / "RAW-v2.jsonl"
        write_text(raw, compact(envelope) + "\n")
        primary_path, independent_path = result / "PRIMARY-v2.json", result / "INDEPENDENT-v2.json"
        first, second = run_analyzer(PRIMARY, raw, primary_path), run_analyzer(INDEPENDENT, raw, independent_path)
        if first.returncode or second.returncode:
            raise RuntimeError(f"final analyzer failure: {first.stdout} {second.stdout}")
        primary, independent = load_json(primary_path), load_json(independent_path)
        if primary.get("normalized") != independent.get("normalized"):
            raise RuntimeError("final analyzer disagreement")
        write_json(result / "FINAL-DECISION-v2.json", {"schema": "phase4-g5-2-final-decision-v2", "status": "PASS", "analysis_scope": "PostWallCustodyRecomputation", "terminal_sha256": envelope["terminal_sha256"], "primary_sha256": sha256(primary_path), "independent_sha256": sha256(independent_path), "normalized_sha256": primary["normalized_sha256"]})
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
            write_json(result / "LOCK-RELEASE-v2.json", {"schema": "phase4-g5-2-lock-release-v2", "status": "PASS", "phase": phase, **released})
        if failure is not None:
            complete_wall = time.monotonic_ns() - started
            write_json(result / "FAILED-v2.json", {"schema": "phase4-g5-2-failure-v2", "status": "REVISE", "phase": phase, "error": f"{type(failure).__name__}: {failure}", "complete_wall_ns": complete_wall, "cleanup_complete": attempt is None, "lock_released": not LOCK.exists(), "successful_product_receipts_retained": len(products), "process_evidence": process_evidence})
    if failure is not None:
        raise failure
    return terminal


def synthetic_row():
    product = {
        "schema": "phase4-g5-projection-suite-v1", "status": "PASS", "worker_count": 1,
        "submitted": 6, "coalesced": 1, "started": 5, "published": 5, "cancelled": 0, "failed": 0, "stale": 0,
        "max_in_flight": 1, "max_pending": 1, "exact_every_root_population": 2, "latest_following_population": 2,
        "full_fallbacks": 2, "range_fetches": 2, "fetched_bytes": 2, "clone_calls": 2,
        "clone_successes": 2, "seed_rotations": 5,
        "foreground_transactions": 1, "foreground_commits": 1,
        "contention_worker_start_ns": 1, "contention_worker_end_ns": 4,
        "contention_foreground_start_ns": 2, "contention_foreground_end_ns": 3,
        "contention_intervals_overlap": True, "reader_barrier_autocommit": 1,
        "reader_barrier_scope_live": 0, "reader_commit_autocommit": 1,
        "reader_commit_scope_live": 0, "foreground_commit_primary_code": 0,
        "foreground_commit_extended_code": 0,
        "projected_root": "root", "last_requested_root": "root", "projected_equals_last_requested": True,
        "sqlite_write_calls": 0, "sqlite_transactions": 0, "sqlite_commits": 0, "sqlite_busy_errors": 0, "sqlite_locked_errors": 0,
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
        records.append({"mode": mode, "product": value, "maximum_resident_set_size": 20_000_000, "clone": {"method": "APFSCloneCpC", "inventory_equal": True}, "process_evidence_sha256": "0" * 64})
    return {"schema": "phase4-g5-2-harness-row-v2", "status": "PASS", "phase": "screen", "analysis_stage": "preliminary", "product_processes": 3, "products": records, "maximum_resident_set_size": 20_000_000, "evidence_assembly_elapsed_ns": 1_000_000_000, "cache_state": "WarmUnknownPreparedFixtureAPFSClone", "cold_reopen_claim": False}


def self_check():
    with tempfile.TemporaryDirectory() as directory:
        root = pathlib.Path(directory)
        cases = [("valid", None), ("valid-final", None), ("pending", ("product", "max_pending", 2)), ("commit", ("product", "sqlite_commits", 1)), ("root", ("product", "projected_root", "wrong")), ("rss", (None, "maximum_resident_set_size", 40_000_000)), ("terminal", ("product", "terminal_workers", 1)), ("population", ("product", "exact_every_root_population", 3)), ("claim", (None, "cold_reopen_claim", True)), ("policy-forgery", None), ("route-forgery", None), ("terminal-forgery", None)]
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
            if label == "route-forgery":
                product = row["products"][-1]["product"]
                sparse = next(item for item in product["build_evidence"] if item["range_count"] == 1 and item["parent_length"] == item["target_length"])
                sparse["range_count"] = 0
                product["exact_build_ns"].append(sparse["wall_ns"])
                product["sparse_build_ns"].remove(sparse["wall_ns"])
                product["exact_p50_ns"], product["exact_p95_ns"] = 4_000_000, 5_000_000
            if label == "terminal-forgery":
                terminal = {"schema": "phase4-g5-2-terminal-v2", "status": "PASS", "complete_wall_ns": LIMITS["screen"] + 1, "limit_ns": LIMITS["screen"], "lock_released": True, "terminal_fixture_roots": 0}
                row.update({"analysis_stage": "final", "terminal": terminal, "terminal_sha256": hashlib.sha256((compact(terminal) + "\n").encode()).hexdigest()})
            if label == "valid-final":
                terminal = {"schema": "phase4-g5-2-terminal-v2", "status": "PASS", "complete_wall_ns": 1_000_000_000, "limit_ns": LIMITS["screen"], "lock_released": True, "terminal_fixture_roots": 0}
                row.update({"analysis_stage": "final", "terminal": terminal, "terminal_sha256": hashlib.sha256((compact(terminal) + "\n").encode()).hexdigest()})
            raw = root / f"{label}.jsonl"
            raw.write_text(compact(row) + "\n")
            reports = []
            for script, name in ((PRIMARY, "p"), (INDEPENDENT, "i")):
                output = root / f"{label}-{name}.json"
                run_analyzer(script, raw, output)
                reports.append(load_json(output)["normalized"])
            if label in ("valid", "valid-final") and (reports[0] != reports[1] or reports[0]["status"] != "PASS"):
                raise RuntimeError("valid analyzer agreement failed")
            if label not in ("valid", "valid-final") and any(report["status"] != "REVISE" for report in reports):
                raise RuntimeError(f"mutation accepted: {label}")
        process_root = root / "partial-process"
        begin_process_evidence(process_root, ["fake-product", "--mode", "self-check"])
        write_json(process_root / "CLONE.json", {"schema": "phase4-g5-2-clone-receipt-v2", "status": "PASS", "method": "APFSCloneCpC", "inventory_equal": True})
        evidence, parsed = persist_process_evidence(process_root, compact({"schema": "synthetic-product", "status": "PASS"}) + "\n", "later process failure\n", 7, False, " 20000000  maximum resident set size\n", 1, 2)
        retained = all((process_root / name).is_file() for name in ("COMMAND.json", "CLONE.json", "STDOUT.txt", "STDERR.txt", "RETURN.json", "RSS.txt", "PARSED-RECEIPT.json", "PROCESS-EVIDENCE.json"))
        if evidence["status"] != "REVISE" or parsed is None or not retained:
            raise RuntimeError("partial process evidence was not retained losslessly")
    result = {"schema": "phase4-g5-2-runner-self-check-v2", "status": "PASS", "checks": len(cases) + 1, "product_processes": 0, "product_rows": 0, "partial_process_evidence_retained": True}
    print(compact(result))
    return result


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("action", choices=("self-check", "prepare-inputs", "freeze", "forecast", "screen", "gate"))
    parser.add_argument("--executable")
    parser.add_argument("--input-root")
    args = parser.parse_args()
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
