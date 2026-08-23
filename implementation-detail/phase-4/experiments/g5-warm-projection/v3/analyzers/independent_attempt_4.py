#!/usr/bin/env python3
import csv, hashlib, json, pathlib, sys

HERE = pathlib.Path(__file__).resolve().parents[1]
REPO = HERE.parents[4]
FAULT_MATRIX = HERE / "method/FAULT-MATRIX-v3.tsv"
SOURCE_FAULT_PROOFS = HERE / "method/SOURCE-FAULT-PROOFS-v3.json"
FOCUSED_FAULT_EXECUTION = HERE / "evidence/FOCUSED-FAULT-EXECUTION-v3.json"
PRODUCT_SOURCE = REPO / "crates/layerfs-engine/src/bin/phase4_g3_materialization.rs"
RAW_FINAL2 = HERE / "evidence/raw-final2"
INPUT_MANIFEST = HERE / "method/INPUT-MANIFEST-v3.json"
def canonical(v): return json.dumps(v, sort_keys=True, separators=(",", ":"))
def private_clone_is_exact(clone, attempt, evidence=None):
    permission = clone.get("private_permission_receipt")
    rebind = clone.get("rebind_receipt")
    if type(permission) is not dict or type(rebind) is not dict or type(permission.get("entries")) is not list: return False
    entries = permission["entries"]
    expected_modes = {"directory": 0o755, "ordinary": 0o644, "authority": 0o600}
    names = tuple(item.get("path") for item in entries if type(item) is dict)
    names_ok = len(names) == len(entries) == len(set(names)) and all(type(name) is str and not pathlib.PurePath(name).is_absolute() and ".." not in pathlib.PurePath(name).parts for name in names)
    modes_ok = all(item.get("kind") in expected_modes and item.get("mode") == expected_modes[item["kind"]] for item in entries if type(item) is dict) and sum(item.get("kind") == "authority" for item in entries) == 1
    permission_digest = hashlib.sha256(canonical(entries).encode()).hexdigest()
    rebind_digest = hashlib.sha256((canonical(rebind) + "\n").encode()).hexdigest()
    evidence_rebind = evidence.get("rebind", {}) if type(evidence) is dict else {}
    nested_ok = not evidence or evidence_rebind.get("path") == "REBIND.json" and type(evidence_rebind.get("bytes")) is int and evidence_rebind["bytes"] > 0 and evidence_rebind.get("sha256") == rebind_digest
    return names_ok and modes_ok and permission.get("status") == "PASS" and permission.get("classification") == "Directories0755Ordinary0644AuthoritySidecar0600NoSymlinks" and (permission.get("authority_files"), permission.get("symlinks")) == (1, 0) and permission.get("map_sha256") == clone.get("private_permission_map_sha256") == permission_digest and rebind.get("status") == "PASS" and rebind.get("scope") == "SealedCloneDirectoryPathRebindOnly" and rebind.get("field") == "directory" and rebind.get("changed_fields") == 1 and rebind.get("all_other_tsv_fields_byte_identical") is True and rebind.get("new_value") == str(pathlib.Path(attempt) / "g3-qualified-one-byte") and rebind.get("old_value") != rebind.get("new_value") and clone.get("rebind_receipt_sha256") == rebind_digest and nested_ok
def rank(v, p):
    v = sorted(v)
    return v[max(1, (len(v) * p + 99) // 100) - 1]

def raw_bundle_valid(receipt, frozen_hashes):
    entries = receipt.get("raw_artifacts")
    expected = {path.name for path in RAW_FINAL2.iterdir() if path.is_file()}
    if type(entries) is not list or {entry.get("name") for entry in entries if type(entry) is dict} != expected: return False
    for entry in entries:
        path = RAW_FINAL2 / entry["name"]
        if pathlib.PurePath(entry["name"]).name != entry["name"] or not path.is_file() or path.stat().st_size != entry.get("bytes") or hashlib.sha256(path.read_bytes()).hexdigest() != entry.get("sha256") or entry.get("sha256") not in frozen_hashes: return False
    return True

def input_manifest_valid(manifest, contract):
    limits, modes = contract["compact_fixture_limits"], contract["fixture_mode_size_bytes"]
    elapsed = manifest.get("preparation_complete_wall_ns", -1)
    valid = manifest.get("status") == "PASS" and manifest.get("preparation_preferred_wall_ns") == contract["preparation_preferred_wall_ns"] and manifest.get("within_preferred_wall") is (elapsed <= contract["preparation_preferred_wall_ns"]) and manifest.get("preparation_complete_wall_limit_ns") == contract["preparation_complete_wall_limit_ns"] and 0 <= elapsed <= contract["preparation_complete_wall_limit_ns"] and manifest.get("fixture_mode_size_bytes") == modes and manifest.get("max_input_root_bytes") == limits["max_input_root_bytes"] and max(manifest.get("input_root_apparent_bytes", limits["max_input_root_bytes"] + 1), manifest.get("input_root_allocated_bytes", limits["max_input_root_bytes"] + 1)) <= limits["max_input_root_bytes"] and set(manifest.get("inputs", {})) == set(modes) and manifest.get("sealed") is True and manifest.get("seal_reopened_verified") is True and [manifest.get("seal_file_mode"), manifest.get("seal_directory_mode")] == [0o444, 0o555]
    if not valid: return False
    for mode, record in manifest["inputs"].items():
        inventory = record.get("inventory", {}); files = [row for row in inventory.get("entries", ()) if row.get("kind") == "file"]
        if record.get("product", {}).get("size_bytes") != modes[mode] or len(files) > limits["max_files"] or any(row.get("bytes", limits["max_file_bytes"] + 1) > limits["max_file_bytes"] or row.get("mode") != 0o444 for row in files) or any(row.get("mode") != 0o555 for row in inventory.get("entries", ()) if row.get("kind") == "directory") or inventory.get("apparent_bytes", limits["max_aggregate_bytes"] + 1) > limits["max_aggregate_bytes"] or inventory.get("allocated_bytes", limits["max_aggregate_bytes"] + 1) > limits["max_aggregate_bytes"]: return False
    return True

def authority(allow_unfrozen=False):
    contract_file, schedule_file = HERE / "method/METHOD-CONTRACT-v3.json", HERE / "method/SCHEDULE-v3.tsv"
    contract = json.loads(contract_file.read_text())
    schedule = tuple(csv.DictReader(schedule_file.open(newline=""), delimiter="\t"))
    if not allow_unfrozen:
        frozen = json.loads((HERE / "method/SOURCE-FREEZE-v3.json").read_text())
        checks = (frozen.get("status") == "FROZEN_BEFORE_DRY_RUN", frozen.get("method_contract_sha256") == hashlib.sha256(contract_file.read_bytes()).hexdigest(), frozen.get("schedule_sha256") == hashlib.sha256(schedule_file.read_bytes()).hexdigest(), frozen.get("fault_matrix_sha256") == hashlib.sha256(FAULT_MATRIX.read_bytes()).hexdigest())
        if not all(checks): raise RuntimeError("frozen authority mismatch")
        manifest = json.loads(INPUT_MANIFEST.read_text())
        if frozen.get("input_manifest_sha256") != hashlib.sha256(INPUT_MANIFEST.read_bytes()).hexdigest() or not input_manifest_valid(manifest, contract): raise RuntimeError("frozen preparation/input authority mismatch")
        contract["_frozen_source_hashes"] = {entry.get("sha256") for entry in frozen.get("authoritative_files", ()) if type(entry) is dict}
        contract["_executable"] = frozen.get("executable")
        contract["_self_check"] = False
        contract["_input_manifest"] = manifest
    else:
        contract["_frozen_source_hashes"] = set()
        contract["_executable"] = "synthetic-product"
        contract["_self_check"] = True
    with FAULT_MATRIX.open(newline="") as handle:
        matrix = tuple(csv.DictReader(handle, delimiter="\t"))
    if len(matrix) != 12 or len({row["case"] for row in matrix}) != 12 or any(row.get("decision") != "hard" for row in matrix): raise RuntimeError("exact fault matrix authority mismatch")
    contract["_fault_rows"] = {row["case"]: row for row in matrix}
    for row in schedule:
        method = contract["campaigns"][row["phase"]]
        observed = (int(row["invocations"]), int(row["complete_wall_limit_ns"]), row["primary_fixture_mode"], row["sentinel_modes"].split(","), int(row["primary_exact_population"]), int(row["primary_latest_population"]))
        expected = (len(method["modes"]) + 2, method["complete_wall_limit_ns"], method["modes"][-1], method["modes"][:-1], method["primary"]["exact_every_root"], method["primary"]["latest_following"])
        if observed != expected: raise RuntimeError("schedule authority mismatch")
    return contract


def audit_process(record, base, method, bad):
    relative = record.get("process_evidence_path")
    parts = pathlib.PurePath(relative).parts if isinstance(relative, str) else ()
    if not parts or pathlib.PurePath(relative).is_absolute() or ".." in parts:
        bad.append(f"{record.get('mode')}:process-path")
        return
    directory = base.joinpath(*parts)
    try:
        evidence_bytes = (directory / "PROCESS-EVIDENCE.json").read_bytes()
        evidence = json.loads(evidence_bytes)
    except Exception:
        bad.append(f"{record.get('mode')}:process-read")
        return
    check = lambda okay, name: bad.append(f"{record.get('mode')}:{name}") if not okay else None
    check(hashlib.sha256(evidence_bytes).hexdigest() == record.get("process_evidence_sha256") and evidence.get("status") == "PASS", "process-binding")
    nested = evidence.get("artifacts")
    files = {}
    if isinstance(nested, list):
        for entry in nested:
            name = entry.get("name") if isinstance(entry, dict) else None
            target = directory / name if isinstance(name, str) else directory / "INVALID"
            valid = isinstance(name, str) and pathlib.PurePath(name).name == name and name not in files and target.is_file() and target.stat().st_size == entry.get("bytes") and hashlib.sha256(target.read_bytes()).hexdigest() == entry.get("sha256")
            check(valid, "nested-artifact")
            if valid: files[name] = target
    expected = {"COMMAND.json", "CLONE.json", "STDOUT.txt", "STDERR.txt", "RETURN.json", "RSS.txt", "PARSED-RECEIPT.json"}
    check(set(files) == expected, "nested-set")
    if set(files) == expected:
        parsed, returned, clone = (json.loads(files[name].read_text()) for name in ("PARSED-RECEIPT.json", "RETURN.json", "CLONE.json"))
        argv = json.loads(files["COMMAND.json"].read_text()).get("argv")
        stdout_lines = tuple(line for line in files["STDOUT.txt"].read_text().splitlines() if line.strip())
        try: stdout_receipt = json.loads(stdout_lines[0]) if len(stdout_lines) == 1 else None
        except Exception: stdout_receipt = None
        expected_attempt = f"/synthetic/fault-{record['fault_class']}" if method["_self_check"] and "fault_class" in record else f"/synthetic/{record.get('mode')}" if method["_self_check"] else str(base / (f"fixture-fault-{record['fault_class']}" if "fault_class" in record else f"fixture-{record.get('mode')}"))
        expected_argv = ["/usr/bin/time", "-l", "-o", str(directory / "RSS.txt.pending"), method["_executable"], method.get("run_flag", "--g5-projection-run"), expected_attempt, record.get("mode")]
        rss = tuple(int(line.split()[0]) for line in files["RSS.txt"].read_text().splitlines() if "maximum resident set size" in line and line.split())
        times = tuple(returned.get(name) for name in ("process_started_ns", "process_ended_ns", "process_elapsed_ns"))
        check(record.get("attempt_root") == expected_attempt and parsed.get("status") == "Available" and parsed.get("receipt") == record.get("product") and stdout_receipt == parsed.get("receipt") and argv == expected_argv and rss == (record.get("maximum_resident_set_size"),) and returned.get("returncode") == 0 and returned.get("timed_out") is False and evidence.get("maximum_resident_set_size") == record.get("maximum_resident_set_size") and all(type(value) is int for value in times) and times[1] - times[0] == times[2] and all(clone.get(name) == record.get("clone", {}).get(name) for name in ("method", "inventory_equal", "source_sealed_reverified", "private_attempt_permissions")) and private_clone_is_exact(clone, expected_attempt, evidence), "nested-semantics")


def fault_closure(row, method, base, bad):
    runs = row.get("fault_runs")
    if type(runs) is not list or row.get("fault_processes") != len(runs): bad.append("fault-processes/count")
    observed = set()
    direct_modes = {"clone-failure": "fault-clone", "after-rename-lost-ack": "fault-rename-lost-ack"}
    direct_selectors = {"clone-failure": "CloneFailure", "after-rename-lost-ack": "RenameLostAck"}
    if type(runs) is list:
        for record in runs:
            case = record.get("fault_class") if type(record) is dict else None
            product = record.get("product", {}) if type(record) is dict else {}
            snapshot = tuple(product.get(name) for name in ("cancelled", "failed", "stale", "clone_failures", "reconciliation_calls", "sqlite_write_calls", "sqlite_transactions", "sqlite_commits", "sqlite_busy_errors", "sqlite_locked_errors", "q_terminal"))
            matrix_row = method["_fault_rows"].get(case)
            expected_mode = f"fault-{case}" if method["_self_check"] else direct_modes.get(case)
            expected_selector = case if method["_self_check"] else direct_selectors.get(case)
            init = type(product.get("reader_initialization_ns")) is int and product["reader_initialization_ns"] > 0 and (product.get("reader_initialization_calls"), product.get("reader_initialization_bytes_requested")) == (1, 1) and all(type(product.get(name)) is int and product[name] > 0 for name in ("reader_initialization_sql_queries", "reader_initialization_authenticated_objects", "reader_initialization_authenticated_bytes", "reader_initialization_q_high_water")) and all(product.get(name) is True for name in ("reader_initialization_read_only", "reader_initialization_query_only", "reader_initialization_inside_complete_wall", "reader_initialization_excluded_from_service_samples"))
            terminal_names = ("terminal_in_flight", "terminal_pending", "terminal_workers", "terminal_active_descriptors", "terminal_successor_descriptors", "terminal_temp_residue", "q_terminal")
            terminal = product.get("max_buffer_bytes", method["buffer_limit_bytes"] + 1) <= method["buffer_limit_bytes"] and all(product.get(name) == 0 for name in terminal_names) and record.get("maximum_resident_set_size", method["rss_limit_bytes"] + 1) <= method["rss_limit_bytes"]
            contention = tuple(product.get(name) for name in ("foreground_transactions", "foreground_commits", "reader_commit_autocommit", "reader_commit_scope_live", "foreground_commit_primary_code", "foreground_commit_extended_code")) == (1, 1, 1, 0, 0, 0) and (product.get("reader_barrier_autocommit"), product.get("reader_barrier_scope_live")) == (1, 1)
            stamps = tuple(product.get(name) for name in ("end_to_end_edit_t0_ns", "end_to_end_canonical_ack_t1_ns", "end_to_end_enqueue_t2_ns", "end_to_end_worker_start_t3_ns", "end_to_end_native_ack_t4_ns"))
            timing = all(type(value) is int and value >= 0 for value in stamps) and tuple(sorted(stamps)) == stamps and (product.get("end_to_end_canonical_transactions"), product.get("end_to_end_canonical_commits")) == (1, 1)
            valid = case in method["_fault_rows"] and case not in observed and record.get("mode") == expected_mode and record.get("matrix_row") == matrix_row and record.get("matrix_row_sha256") == hashlib.sha256(canonical(matrix_row).encode()).hexdigest() and product.get("schema") == method["product_schema"] and product.get("status") == "PASS" and product.get("size_bytes") == 250_000 and product.get("route_class") == method["route_class"] and [product.get("exact_every_root_population"), product.get("latest_following_population")] == method["mode_populations"]["self-check"] and product.get("fault_selector") == expected_selector and product.get("fault_receipt") == {"status": "ObservedCompleteApply", "complete_apply_hooks": True} and all(type(value) is int and value >= 0 for value in snapshot) and init and terminal and contention and timing and "fault_receipts" not in product
            if not valid: bad.append(f"fault-run:{case}")
            audit_process(record, base, method, bad)
            if case in method["_fault_rows"]: observed.add(case)
    if not method["_self_check"] and observed != set(direct_modes): bad.append("direct-fault-modes")
    proven = set()
    proofs = row.get("source_fault_proofs")
    if not method["_self_check"] and SOURCE_FAULT_PROOFS.is_file() and proofs != json.loads(SOURCE_FAULT_PROOFS.read_text()).get("proofs"): bad.append("source-fault-proof-authority")
    if type(proofs) is list:
        for proof in proofs:
            case = proof.get("fault_class") if type(proof) is dict else None
            claims = proof.get("counter_claims") if type(proof) is dict else None
            matrix_row = method["_fault_rows"].get(case)
            source = REPO / proof.get("source_path", "INVALID") if type(proof) is dict else REPO / "INVALID"
            execution = REPO / proof.get("execution_receipt_path", "INVALID") if type(proof) is dict else REPO / "INVALID"
            try:
                execution_receipt = json.loads(execution.read_text())
                execution_cases = execution_receipt.get("cases", [execution_receipt])
                observed_case = next(item for item in execution_cases if item.get("fault_class") == case)
            except Exception:
                execution_receipt, observed_case = {}, {}
            receipt_hashes = observed_case.get("command_sha256") == hashlib.sha256(canonical(observed_case.get("command")).encode()).hexdigest() and observed_case.get("stdout_sha256") == hashlib.sha256(observed_case.get("stdout", "").encode()).hexdigest() and observed_case.get("stderr_sha256") == hashlib.sha256(observed_case.get("stderr", "").encode()).hexdigest() and observed_case.get("return_sha256") == hashlib.sha256(canonical({"returncode": observed_case.get("returncode")}).encode()).hexdigest()
            receipt_matches = execution_receipt.get("status") == "PASS" and execution_receipt.get("source_sha256") == proof.get("source_sha256") and raw_bundle_valid(execution_receipt, method["_frozen_source_hashes"]) and observed_case.get("test_locator") == proof.get("test_locator") and observed_case.get("typed_outcome") == proof.get("typed_outcome") and observed_case.get("counter_claims") == claims and isinstance(observed_case.get("command"), list) and bool(observed_case["command"]) and isinstance(observed_case.get("stdout"), str) and bool(observed_case["stdout"]) and isinstance(observed_case.get("stderr"), str) and observed_case.get("returncode") == 0 and receipt_hashes
            valid = case not in observed | proven and source == PRODUCT_SOURCE and execution == FOCUSED_FAULT_EXECUTION and proof.get("classification") == "ObservedFocusedCompleteApply" and matrix_row is not None and proof.get("matrix_row") == matrix_row and proof.get("required_observation") == matrix_row["required_observation"] and proof.get("matrix_row_sha256") == hashlib.sha256(canonical(matrix_row).encode()).hexdigest() and source.is_file() and execution.is_file() and hashlib.sha256(source.read_bytes()).hexdigest() == proof.get("source_sha256") and hashlib.sha256(execution.read_bytes()).hexdigest() == proof.get("execution_receipt_sha256") and proof.get("source_sha256") in method["_frozen_source_hashes"] and proof.get("execution_receipt_sha256") in method["_frozen_source_hashes"] and receipt_matches and isinstance(proof.get("test_locator"), str) and bool(proof["test_locator"]) and isinstance(proof.get("typed_outcome"), str) and bool(proof["typed_outcome"]) and type(claims) is dict and bool(claims) and all(type(value) is int and value >= 0 for value in claims.values())
            if not valid: bad.append("source-fault-proof")
            elif valid: proven.add(case)
    if observed | proven != set(method["_fault_rows"]): bad.append("fault-matrix-incomplete")


def audit(record, method, evidence_root):
    mode, p, bad = record.get("mode"), record.get("product", {}), []
    def check(ok, name):
        if not ok: bad.append(f"{mode}:{name}")
    check(p.get("schema") == method["product_schema"] and p.get("status") == "PASS" and p.get("mode") == mode and p.get("size_bytes") == 250_000 and p.get("route_class") == method["route_class"], "schema/status/mode/size/route")
    check([p.get("exact_every_root_population"), p.get("latest_following_population")] == method["mode_populations"].get(mode), "populations")
    exact, latest = method["mode_populations"].get(mode, [-1, -1])
    expected = (exact + latest + 5, exact + min(latest, 2) + 4, latest - min(latest, 2) + 1)
    slots = method["slot_limits"]
    check(p.get("worker_count") == slots["workers"] and isinstance(p.get("max_in_flight"), int) and 0 <= p["max_in_flight"] <= slots["in_flight"] and isinstance(p.get("max_pending"), int) and 0 <= p["max_pending"] <= slots["pending"], "worker/slots")
    s = p.get("started", -1)
    check((p.get("submitted"), s, p.get("published"), p.get("coalesced")) == (expected[0], expected[1], expected[1], expected[2]) and not any(p.get(k) for k in ("cancelled", "failed", "stale")), "exact-vs-latest-conservation")
    check(p.get("submitted") == p.get("coalesced", 0) + s, "request-conservation")
    check(s == p.get("published", -1) + p.get("cancelled", -1) + p.get("failed", -1) + p.get("stale", -1), "build-conservation")
    check(p.get("seed_rotations") == p.get("published"), "seed-rotation")
    check(p.get("projected_equals_last_requested") is True and p.get("projected_root") == p.get("last_requested_root"), "terminal-root")
    check(all(p.get(k) == 0 for k in ("sqlite_write_calls", "sqlite_transactions", "sqlite_commits", "sqlite_busy_errors", "sqlite_locked_errors")), "sqlite-read-only")
    check(tuple(p.get(k) for k in ("foreground_transactions", "foreground_commits", "reader_commit_autocommit", "reader_commit_scope_live", "foreground_commit_primary_code", "foreground_commit_extended_code")) == (1, 1, 1, 0, 0, 0) and p.get("contention_worker_and_foreground_transaction_intervals_overlap") is True and p.get("contention_overlap_scope") == "ObservedBroadWorkerAndForegroundTransactionIntervals" and p.get("foreground_commit_within_end_to_end_t3_t4_claim") == "NotClaimedDifferentRequest", "foreground-contention-tuple")
    check(p.get("contention_worker_start_ns", 1) < p.get("contention_worker_end_ns", 0) and p.get("contention_foreground_start_ns", 1) < p.get("contention_foreground_end_ns", 0) and max(p.get("contention_worker_start_ns", 0), p.get("contention_foreground_start_ns", 0)) < min(p.get("contention_worker_end_ns", 0), p.get("contention_foreground_end_ns", 0)), "contention-equation")
    check((p.get("reader_barrier_autocommit"), p.get("reader_barrier_scope_live")) == (1, 1), "contention-barrier-state")
    check(1 <= p.get("full_fallbacks", -1) <= s and 1 <= p.get("range_fetches", -1) <= 256 * s and 0 < p.get("fetched_bytes", -1) <= 8_388_608 * s, "bounded-routes")
    check(1 <= p.get("clone_successes", -1) <= p.get("clone_calls", -1) <= s, "clone-counters")
    zero = ("terminal_in_flight", "terminal_pending", "terminal_workers", "terminal_active_descriptors", "terminal_successor_descriptors", "terminal_temp_residue", "q_terminal")
    check(p.get("max_buffer_bytes", method["buffer_limit_bytes"] + 1) <= method["buffer_limit_bytes"] and all(p.get(k) == 0 for k in zero), "terminal/buffer")
    check(p.get("shutdown") == "drained" and p.get("checkpoint_outside_service_timer") is True, "shutdown/checkpoint")
    storage_names = ("initial_descriptor_verification_bytes", "initial_storage_logical_bytes", "initial_storage_apparent_bytes", "initial_storage_allocated_bytes", "terminal_storage_logical_bytes", "terminal_storage_apparent_bytes", "terminal_storage_allocated_bytes")
    check(all(type(p.get(name)) is int and p[name] >= 0 for name in storage_names) and p.get("initial_descriptor_verification_bytes", 0) > 0 and p.get("initial_storage_logical_bytes") == p.get("initial_descriptor_verification_bytes") and p.get("terminal_storage_logical_bytes", 0) > 0 and p.get("terminal_descriptor_classification") == "ProvenByWorkerJoinAndOwnedDescriptorDrop", "descriptor-storage")
    stamps = tuple(p.get(name) for name in ("end_to_end_edit_t0_ns", "end_to_end_canonical_ack_t1_ns", "end_to_end_enqueue_t2_ns", "end_to_end_worker_start_t3_ns", "end_to_end_native_ack_t4_ns"))
    check(all(type(value) is int and value >= 0 for value in stamps) and tuple(sorted(stamps)) == stamps and p.get("end_to_end_population") == 1 and p.get("end_to_end_scope") == "ObservedEditT0CanonicalAckT1EnqueueT2WorkerT3NativeAckT4" and (p.get("end_to_end_canonical_transactions"), p.get("end_to_end_canonical_commits")) == (1, 1), "observed-t0-t4")
    latency = method["latency_limits_ns"]
    for name, middle, tail in (("exact", latency["exact_p50"], latency["exact_p95"]), ("sparse", latency["sparse_p50"], latency["sparse_p95"])):
        values = p.get(name + "_build_ns")
        valid = isinstance(values, list) and bool(values) and all(isinstance(x, int) and not isinstance(x, bool) and x >= 0 for x in values)
        check(valid, name + "-timers")
        if valid: check([rank(values, 50), rank(values, 95)] == [p.get(name + "_p50_ns"), p.get(name + "_p95_ns")] and p[name + "_p50_ns"] <= middle and p[name + "_p95_ns"] <= tail, name + "-latency")
    evidence = p.get("build_evidence")
    buckets = [[], [], [], []]
    policy_ordinals = {name: [] for name in ("ExactEveryRoot", "IsolatedSparseSentinel", "IsolatedOrdinaryFallback", "LatestFollowingSameSize", "LatestFollowingCountStorm")}
    policy_times = {name: [] for name in policy_ordinals}
    evidence_ok = isinstance(evidence, list) and len(evidence) == s
    if evidence_ok:
        for item in evidence:
            evidence_ok = isinstance(item, dict) and isinstance(item.get("contention"), bool) and item.get("plan") in {"Ranges", "FullFallback"} and item.get("policy") in policy_ordinals and all(isinstance(item.get(field), int) and not isinstance(item[field], bool) and item[field] >= 0 for field in ("parent_length", "target_length", "range_count", "wall_ns", "ordinal"))
            if not evidence_ok: break
            policy_ordinals[item["policy"]].append(item["ordinal"])
            policy_times[item["policy"]].append(item["wall_ns"])
            fallback = item["plan"] == "FullFallback" or item["parent_length"] != item["target_length"]
            index = (3 if item["contention"] else 2) if fallback else (0 if item["range_count"] == 0 else 1)
            if not fallback and item["contention"]: evidence_ok = False; break
            buckets[index].append(item["wall_ns"])
    check(evidence_ok, "build-evidence")
    if evidence_ok:
        check(buckets == [p.get("exact_build_ns"), p.get("sparse_build_ns"), p.get("full_fallback_build_ns"), p.get("contention_full_fallback_build_ns")], "route-classification")
        expected_same = [] if latest == 0 else ([0] if latest == 1 else [0, latest - 1])
        expected_policy = {"ExactEveryRoot": list(range(exact)), "IsolatedSparseSentinel": [0], "IsolatedOrdinaryFallback": [0], "LatestFollowingSameSize": expected_same, "LatestFollowingCountStorm": [0, 2]}
        check({name: sorted(ordinals) for name, ordinals in policy_ordinals.items()} == expected_policy, "semantic-policy-stream-ordinals")
        check(len(buckets[0]) == 1 and len(buckets[1]) == exact + min(latest, 2) + 1, "execution-route-classification")
        fallbacks = buckets[2]
        contention_fallbacks = buckets[3]
        check(bool(fallbacks) and len(fallbacks) + len(contention_fallbacks) == p.get("full_fallbacks") and [rank(fallbacks, 50), rank(fallbacks, 95)] == [p.get("full_fallback_p50_ns"), p.get("full_fallback_p95_ns")], "fallback-timers")
        fallback_limit = method["fallback_isolated_limit_ns"]
        check(p.get("full_fallback_g3_bound_ns") == fallback_limit and p.get("full_fallback_within_g3_bound") is True and max(fallbacks) <= fallback_limit, "fallback-g3-bound")
        check(len(fallbacks) == 1 and len(contention_fallbacks) == 1 and [rank(contention_fallbacks, 50), rank(contention_fallbacks, 95)] == [p.get("contention_full_fallback_p50_ns"), p.get("contention_full_fallback_p95_ns")] and p.get("contention_full_fallback_latency_claim") == "NotClaimedDifferentConcurrentExecutionShape", "contention-fallback")
    check(isinstance(p.get("reader_initialization_ns"), int) and not isinstance(p["reader_initialization_ns"], bool) and p["reader_initialization_ns"] > 0 and p.get("reader_initialization_classification") == "OneTimeReadOnlyProcessInitializationInsideCompleteWallOutsideServiceSamples", "reader-initialization")
    check((p.get("reader_initialization_calls"), p.get("reader_initialization_bytes_requested")) == (1, 1) and all(isinstance(p.get(field), int) and not isinstance(p[field], bool) and p[field] > 0 for field in ("reader_initialization_sql_queries", "reader_initialization_authenticated_objects", "reader_initialization_authenticated_bytes", "reader_initialization_q_high_water")), "reader-initialization-work")
    check(all(p.get(field) is True for field in ("reader_initialization_read_only", "reader_initialization_query_only", "reader_initialization_inside_complete_wall", "reader_initialization_excluded_from_service_samples")) and len(p.get("build_evidence", ())) == s, "reader-initialization-boundary")
    check(p.get("fault_selector") == "None" and p.get("fault_receipt") == {"status": "NotInjectedInPerformanceRun", "complete_apply_hooks": True} and "fault_receipts" not in p, "performance-fault-receipt-singular")
    check(record.get("maximum_resident_set_size", method["rss_limit_bytes"] + 1) <= method["rss_limit_bytes"], "rss")
    check(record.get("clone", {}).get("method") == "APFSCloneCpC" and record.get("clone", {}).get("inventory_equal") is True and record.get("clone", {}).get("source_sealed_reverified") is True and record.get("clone", {}).get("private_attempt_permissions") == "WritableAfterExactCloneInventory" and private_clone_is_exact(record.get("clone", {}), record.get("attempt_root")), "clone/sealed-source/private-modes/rebind")
    digest = record.get("process_evidence_sha256")
    check(isinstance(digest, str) and len(digest) == 64 and set(digest) <= set("0123456789abcdef"), "process-evidence-binding")
    audit_process(record, evidence_root, method, bad)
    return bad, {"policy_latency_ns": policy_times, "route_latency_ns": {"exact": buckets[0], "sparse": buckets[1], "full_fallback": buckets[2], "contention_fallback": buckets[3]}}

def recompute(row, method, evidence_root):
    bad, phase, records = [], row.get("phase"), row.get("products")
    derived = []
    plan = method.get("campaigns", {}).get(phase, {}).get("modes")
    if row.get("schema") != "phase4-g5-2-harness-row-v3" or row.get("status") != "PASS": bad.append("row-schema/status")
    if not isinstance(records, list) or [x.get("mode") for x in records] != plan or row.get("product_processes") != len(plan or ()): bad.append("product-processes/modes")
    else:
        for record in records:
            record_bad, record_derived = audit(record, method, evidence_root)
            bad.extend(record_bad); derived.append(record_derived)
    all_processes = records + row.get("fault_runs", []) if isinstance(records, list) and isinstance(row.get("fault_runs"), list) else []
    if all_processes and row.get("maximum_resident_set_size") != max(x.get("maximum_resident_set_size", -1) for x in all_processes): bad.append("aggregate-rss-max")
    fault_closure(row, method, evidence_root, bad)
    stage, terminal = row.get("analysis_stage"), row.get("terminal")
    if stage == "preliminary":
        if terminal is not None: bad.append("preliminary-terminal-present")
        complete_wall = None
    elif stage == "final":
        method_limit = method["campaigns"][phase]["complete_wall_limit_ns"] if phase in method["campaigns"] else -1
        expected_processes = row.get("product_processes", -1) + row.get("fault_processes", -1)
        okay = isinstance(terminal, dict) and terminal.get("schema") == "phase4-g5-2-terminal-v3" and terminal.get("status") == "PASS" and terminal.get("limit_ns") == method_limit and terminal.get("complete_wall_ns", -1) <= method_limit and terminal.get("lock_released") is True and terminal.get("terminal_fixture_roots") == 0 and terminal.get("product_processes") == expected_processes and terminal.get("product_rows") == expected_processes
        if not okay: bad.append("terminal-complete-wall")
        if isinstance(terminal, dict) and row.get("terminal_sha256") != hashlib.sha256((canonical(terminal) + "\n").encode()).hexdigest(): bad.append("terminal-binding")
        try:
            terminal_file, release_file = evidence_root / "TERMINAL-v3.json", evidence_root / "LOCK-RELEASE-v3.json"
            disk_terminal, disk_release = json.loads(terminal_file.read_text()), json.loads(release_file.read_text())
            release = row.get("lock_release")
            identity = isinstance(release, dict) and release == disk_release and release.get("schema") == "phase4-g5-2-lock-release-v3" and release.get("status") == "PASS" and release.get("lock_absent") is True and all(type(release.get(key)) is int and release[key] >= 0 for key in ("device", "inode")) and all(isinstance(release.get(key), str) and len(release[key]) == 64 for key in ("ownership_token_sha256", "intent_sha256"))
            if disk_terminal != terminal or hashlib.sha256(terminal_file.read_bytes()).hexdigest() != row.get("terminal_sha256") or hashlib.sha256(release_file.read_bytes()).hexdigest() != row.get("lock_release_sha256") or not identity: bad.append("terminal-lock-file-custody")
        except Exception:
            bad.append("terminal-lock-file-custody")
        complete_wall = terminal.get("complete_wall_ns") if isinstance(terminal, dict) else None
    else:
        bad.append("analysis-stage")
        complete_wall = None
    if row.get("cache_state") != "WarmUnknownPreparedFixtureAPFSClone" or row.get("cold_reopen_claim") is not False: bad.append("cache-claim")
    p = records[-1].get("product", {}) if records else {}
    normalized = {"status": "PASS" if not bad else "REVISE", "analysis_stage": stage, "phase": phase, "hard_failures": sorted(bad), "product_processes": row.get("product_processes"), "sentinel_modes": plan[:-1] if plan else None, "primary_mode": plan[-1] if plan else None, "exact_population": p.get("exact_every_root_population"), "latest_population": p.get("latest_following_population"), "projected_root": p.get("projected_root"), "rss_bytes": row.get("maximum_resident_set_size"), "complete_wall_ns": complete_wall, "exact_p50_ns": p.get("exact_p50_ns"), "exact_p95_ns": p.get("exact_p95_ns"), "sparse_p50_ns": p.get("sparse_p50_ns"), "sparse_p95_ns": p.get("sparse_p95_ns"), "fallback_p50_ns": p.get("full_fallback_p50_ns"), "fallback_p95_ns": p.get("full_fallback_p95_ns"), "primary_policy_latency_ns": derived[-1]["policy_latency_ns"] if derived else None, "primary_route_latency_ns": derived[-1]["route_latency_ns"] if derived else None}
    return {"schema": "phase4-g5-2-independent-v3", "normalized": normalized, "normalized_sha256": hashlib.sha256(canonical(normalized).encode()).hexdigest()}

def main():
    method = authority(len(sys.argv) == 4 and sys.argv[3] == "--self-check-authority")
    rows = [json.loads(x) for x in pathlib.Path(sys.argv[1]).read_text().splitlines() if x.strip()]
    report = recompute(rows[0], method, pathlib.Path(sys.argv[1]).parent) if len(rows) == 1 else {"schema": "phase4-g5-2-independent-v3", "normalized": {"status": "REVISE", "hard_failures": ["row-count"]}}
    pathlib.Path(sys.argv[2]).write_text(canonical(report) + "\n"); print(canonical(report))
    return report["normalized"]["status"] != "PASS"
if __name__ == "__main__": raise SystemExit(main())
