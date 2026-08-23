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
TERMINAL = ("terminal_in_flight", "terminal_pending", "terminal_workers", "terminal_active_descriptors", "terminal_successor_descriptors", "terminal_temp_residue", "q_terminal")

def compact(v): return json.dumps(v, sort_keys=True, separators=(",", ":"))
def attempt_clone_receipts_valid(clone, expected_attempt, process_evidence=None):
    permissions, rebind = clone.get("private_permission_receipt"), clone.get("rebind_receipt")
    if not isinstance(permissions, dict) or not isinstance(rebind, dict): return False
    entries = permissions.get("entries")
    if not isinstance(entries, list) or not entries: return False
    paths = [row.get("path") for row in entries if isinstance(row, dict)]
    modes = all(row.get("mode") == ({"directory": 0o755, "ordinary": 0o644, "authority": 0o600}.get(row.get("kind"))) for row in entries if isinstance(row, dict))
    permission_hash = hashlib.sha256(compact(entries).encode()).hexdigest()
    rebind_hash = hashlib.sha256((compact(rebind) + "\n").encode()).hexdigest()
    evidence_rebind = process_evidence.get("rebind", {}) if isinstance(process_evidence, dict) else {}
    expected_directory = str(pathlib.Path(expected_attempt) / "g3-qualified-one-byte")
    evidence_ok = not process_evidence or evidence_rebind.get("path") == "REBIND.json" and type(evidence_rebind.get("bytes")) is int and evidence_rebind["bytes"] > 0 and evidence_rebind.get("sha256") == rebind_hash
    return len(paths) == len(entries) == len(set(paths)) and all(isinstance(path, str) and not pathlib.PurePath(path).is_absolute() and ".." not in pathlib.PurePath(path).parts for path in paths) and modes and sum(row.get("kind") == "authority" for row in entries) == 1 and permissions.get("status") == "PASS" and permissions.get("classification") == "Directories0755Ordinary0644AuthoritySidecar0600NoSymlinks" and permissions.get("authority_files") == 1 and permissions.get("symlinks") == 0 and permissions.get("map_sha256") == permission_hash == clone.get("private_permission_map_sha256") and rebind.get("status") == "PASS" and rebind.get("scope") == "SealedCloneDirectoryPathRebindOnly" and rebind.get("field") == "directory" and rebind.get("changed_fields") == 1 and rebind.get("all_other_tsv_fields_byte_identical") is True and rebind.get("new_value") == expected_directory and rebind.get("old_value") != expected_directory and clone.get("rebind_receipt_sha256") == rebind_hash and evidence_ok
def pct(v, p):
    v = sorted(v)
    return v[max(0, (len(v) * p + 99) // 100 - 1)]

def raw_bundle_valid(receipt, frozen_hashes):
    entries = receipt.get("raw_artifacts")
    expected = {path.name for path in RAW_FINAL2.iterdir() if path.is_file()}
    if not isinstance(entries, list) or {entry.get("name") for entry in entries if isinstance(entry, dict)} != expected:
        return False
    for entry in entries:
        path = RAW_FINAL2 / entry["name"]
        if pathlib.PurePath(entry["name"]).name != entry["name"] or not path.is_file() or path.stat().st_size != entry.get("bytes") or hashlib.sha256(path.read_bytes()).hexdigest() != entry.get("sha256") or entry.get("sha256") not in frozen_hashes:
            return False
    return True

def input_manifest_valid(manifest, contract):
    limits = contract["compact_fixture_limits"]
    modes = contract["fixture_mode_size_bytes"]
    elapsed = manifest.get("preparation_complete_wall_ns", -1)
    if manifest.get("status") != "PASS" or manifest.get("preparation_preferred_wall_ns") != contract["preparation_preferred_wall_ns"] or manifest.get("within_preferred_wall") is not (elapsed <= contract["preparation_preferred_wall_ns"]) or manifest.get("preparation_complete_wall_limit_ns") != contract["preparation_complete_wall_limit_ns"] or not 0 <= elapsed <= contract["preparation_complete_wall_limit_ns"] or manifest.get("fixture_mode_size_bytes") != modes or manifest.get("max_input_root_bytes") != limits["max_input_root_bytes"] or max(manifest.get("input_root_apparent_bytes", limits["max_input_root_bytes"] + 1), manifest.get("input_root_allocated_bytes", limits["max_input_root_bytes"] + 1)) > limits["max_input_root_bytes"] or set(manifest.get("inputs", {})) != set(modes) or manifest.get("sealed") is not True or manifest.get("seal_reopened_verified") is not True or [manifest.get("seal_file_mode"), manifest.get("seal_directory_mode")] != [0o444, 0o555]:
        return False
    for mode, record in manifest["inputs"].items():
        inventory = record.get("inventory", {})
        files = [row for row in inventory.get("entries", []) if row.get("kind") == "file"]
        if record.get("product", {}).get("size_bytes") != modes[mode] or len(files) > limits["max_files"] or any(row.get("bytes", limits["max_file_bytes"] + 1) > limits["max_file_bytes"] or row.get("mode") != 0o444 for row in files) or any(row.get("mode") != 0o555 for row in inventory.get("entries", []) if row.get("kind") == "directory") or inventory.get("apparent_bytes", limits["max_aggregate_bytes"] + 1) > limits["max_aggregate_bytes"] or inventory.get("allocated_bytes", limits["max_aggregate_bytes"] + 1) > limits["max_aggregate_bytes"]:
            return False
    return True

def load_authority(allow_unfrozen=False):
    contract_path = HERE / "method/METHOD-CONTRACT-v3.json"
    schedule_path = HERE / "method/SCHEDULE-v3.tsv"
    contract = json.loads(contract_path.read_text())
    schedule = list(csv.DictReader(schedule_path.open(newline=""), delimiter="\t"))
    freeze_path = HERE / "method/SOURCE-FREEZE-v3.json"
    if not allow_unfrozen:
        freeze = json.loads(freeze_path.read_text())
        if freeze.get("status") != "FROZEN_BEFORE_DRY_RUN" or freeze.get("method_contract_sha256") != hashlib.sha256(contract_path.read_bytes()).hexdigest() or freeze.get("schedule_sha256") != hashlib.sha256(schedule_path.read_bytes()).hexdigest() or freeze.get("fault_matrix_sha256") != hashlib.sha256(FAULT_MATRIX.read_bytes()).hexdigest():
            raise RuntimeError("frozen method authority mismatch")
        manifest = json.loads(INPUT_MANIFEST.read_text())
        if freeze.get("input_manifest_sha256") != hashlib.sha256(INPUT_MANIFEST.read_bytes()).hexdigest() or not input_manifest_valid(manifest, contract):
            raise RuntimeError("frozen preparation/input authority mismatch")
        contract["_frozen_source_hashes"] = {item.get("sha256") for item in freeze.get("authoritative_files", []) if isinstance(item, dict)}
        contract["_executable"] = freeze.get("executable")
        contract["_self_check"] = False
        contract["_input_manifest"] = manifest
    else:
        contract["_frozen_source_hashes"] = set()
        contract["_executable"] = "synthetic-product"
        contract["_self_check"] = True
    with FAULT_MATRIX.open(newline="") as handle:
        matrix = list(csv.DictReader(handle, delimiter="\t"))
    if len(matrix) != 12 or len({row["case"] for row in matrix}) != 12 or any(row.get("decision") != "hard" for row in matrix):
        raise RuntimeError("exact fault matrix authority mismatch")
    contract["_fault_rows"] = {row["case"]: row for row in matrix}
    campaigns = contract["campaigns"]
    for row in schedule:
        authority = campaigns[row["phase"]]
        if int(row["invocations"]) != len(authority["modes"]) + 2 or int(row["complete_wall_limit_ns"]) != authority["complete_wall_limit_ns"] or row["primary_fixture_mode"] != authority["modes"][-1] or row["sentinel_modes"].split(",") != authority["modes"][:-1] or int(row["primary_exact_population"]) != authority["primary"]["exact_every_root"] or int(row["primary_latest_population"]) != authority["primary"]["latest_following"]:
            raise RuntimeError("schedule/contract mismatch")
    return contract


def rehash_process(record, evidence_root, authority, failures):
    relative = record.get("process_evidence_path")
    if not isinstance(relative, str) or pathlib.PurePath(relative).is_absolute() or ".." in pathlib.PurePath(relative).parts:
        failures.append("process-evidence-path")
        return
    root = evidence_root / relative
    receipt_path = root / "PROCESS-EVIDENCE.json"
    try:
        receipt_bytes = receipt_path.read_bytes()
        receipt = json.loads(receipt_bytes)
    except Exception:
        failures.append(f"{record.get('mode')}:process-evidence-read")
        return
    if hashlib.sha256(receipt_bytes).hexdigest() != record.get("process_evidence_sha256") or receipt.get("status") != "PASS":
        failures.append(f"{record.get('mode')}:process-evidence-binding")
    artifacts = receipt.get("artifacts")
    if not isinstance(artifacts, list) or not artifacts:
        failures.append(f"{record.get('mode')}:process-artifacts")
        return
    observed = {}
    for item in artifacts:
        name = item.get("name") if isinstance(item, dict) else None
        path = root / name if isinstance(name, str) else root / "INVALID"
        if not isinstance(name, str) or pathlib.PurePath(name).name != name or name in observed or not path.is_file() or path.stat().st_size != item.get("bytes") or hashlib.sha256(path.read_bytes()).hexdigest() != item.get("sha256"):
            failures.append(f"{record.get('mode')}:nested-artifact")
            continue
        observed[name] = path
    required = {"COMMAND.json", "CLONE.json", "STDOUT.txt", "STDERR.txt", "RETURN.json", "RSS.txt", "PARSED-RECEIPT.json"}
    if set(observed) != required:
        failures.append(f"{record.get('mode')}:nested-artifact-set")
        return
    parsed = json.loads(observed["PARSED-RECEIPT.json"].read_text())
    clone = json.loads(observed["CLONE.json"].read_text())
    returned = json.loads(observed["RETURN.json"].read_text())
    command = json.loads(observed["COMMAND.json"].read_text()).get("argv")
    stdout_lines = [line for line in observed["STDOUT.txt"].read_text().splitlines() if line.strip()]
    try:
        stdout_receipt = json.loads(stdout_lines[0]) if len(stdout_lines) == 1 else None
    except Exception:
        stdout_receipt = None
    expected_attempt = f"/synthetic/fault-{record['fault_class']}" if authority["_self_check"] and "fault_class" in record else f"/synthetic/{record.get('mode')}" if authority["_self_check"] else str(evidence_root / (f"fixture-fault-{record['fault_class']}" if "fault_class" in record else f"fixture-{record.get('mode')}"))
    expected_command = ["/usr/bin/time", "-l", "-o", str(root / "RSS.txt.pending"), authority["_executable"], authority.get("run_flag", "--g5-projection-run"), expected_attempt, record.get("mode")]
    rss_values = [int(line.split()[0]) for line in observed["RSS.txt"].read_text().splitlines() if "maximum resident set size" in line and line.split()]
    if record.get("attempt_root") != expected_attempt or parsed.get("status") != "Available" or parsed.get("receipt") != record.get("product") or stdout_receipt != parsed.get("receipt") or command != expected_command or rss_values != [record.get("maximum_resident_set_size")] or returned.get("returncode") != 0 or returned.get("timed_out") is not False or receipt.get("maximum_resident_set_size") != record.get("maximum_resident_set_size") or any(type(returned.get(name)) is not int for name in ("process_started_ns", "process_ended_ns", "process_elapsed_ns")) or returned["process_ended_ns"] - returned["process_started_ns"] != returned["process_elapsed_ns"] or any(clone.get(name) != record.get("clone", {}).get(name) for name in ("method", "inventory_equal", "source_sealed_reverified", "private_attempt_permissions")) or not attempt_clone_receipts_valid(clone, expected_attempt, receipt):
        failures.append(f"{record.get('mode')}:nested-semantics")


def validate_fault_matrix(row, authority, evidence_root, failures):
    runs = row.get("fault_runs")
    if not isinstance(runs, list) or row.get("fault_processes") != len(runs):
        failures.append("fault-processes/count")
    observed = {}
    direct_modes = {"clone-failure": "fault-clone", "after-rename-lost-ack": "fault-rename-lost-ack"}
    direct_selectors = {"clone-failure": "CloneFailure", "after-rename-lost-ack": "RenameLostAck"}
    if isinstance(runs, list):
        for record in runs:
            case = record.get("fault_class") if isinstance(record, dict) else None
            if case in observed or case not in authority["_fault_rows"]:
                failures.append("fault-run-class")
                continue
            product = record.get("product", {})
            receipt = product.get("fault_receipt")
            counters = {name: product.get(name) for name in ("cancelled", "failed", "stale", "clone_failures", "reconciliation_calls", "sqlite_write_calls", "sqlite_transactions", "sqlite_commits", "sqlite_busy_errors", "sqlite_locked_errors", "q_terminal")}
            matrix_row = authority["_fault_rows"].get(case)
            expected_mode = f"fault-{case}" if authority["_self_check"] else direct_modes.get(case)
            expected_selector = case if authority["_self_check"] else direct_selectors.get(case)
            init = type(product.get("reader_initialization_ns")) is int and product["reader_initialization_ns"] > 0 and product.get("reader_initialization_calls") == 1 and product.get("reader_initialization_bytes_requested") == 1 and all(type(product.get(name)) is int and product[name] > 0 for name in ("reader_initialization_sql_queries", "reader_initialization_authenticated_objects", "reader_initialization_authenticated_bytes", "reader_initialization_q_high_water")) and all(product.get(name) is True for name in ("reader_initialization_read_only", "reader_initialization_query_only", "reader_initialization_inside_complete_wall", "reader_initialization_excluded_from_service_samples"))
            terminal = product.get("max_buffer_bytes", authority["buffer_limit_bytes"] + 1) <= authority["buffer_limit_bytes"] and all(product.get(name) == 0 for name in TERMINAL) and record.get("maximum_resident_set_size", authority["rss_limit_bytes"] + 1) <= authority["rss_limit_bytes"]
            contention = [product.get(name) for name in ("foreground_transactions", "foreground_commits", "reader_commit_autocommit", "reader_commit_scope_live", "foreground_commit_primary_code", "foreground_commit_extended_code")] == [1, 1, 1, 0, 0, 0] and [product.get("reader_barrier_autocommit"), product.get("reader_barrier_scope_live")] == [1, 1]
            stamps = [product.get(name) for name in ("end_to_end_edit_t0_ns", "end_to_end_canonical_ack_t1_ns", "end_to_end_enqueue_t2_ns", "end_to_end_worker_start_t3_ns", "end_to_end_native_ack_t4_ns")]
            timing = all(type(value) is int and value >= 0 for value in stamps) and stamps == sorted(stamps) and [product.get("end_to_end_canonical_transactions"), product.get("end_to_end_canonical_commits")] == [1, 1]
            if record.get("mode") != expected_mode or record.get("matrix_row") != matrix_row or record.get("matrix_row_sha256") != hashlib.sha256(compact(matrix_row).encode()).hexdigest() or product.get("schema") != authority["product_schema"] or product.get("status") != "PASS" or product.get("size_bytes") != 250_000 or product.get("route_class") != authority["route_class"] or [product.get("exact_every_root_population"), product.get("latest_following_population")] != authority["mode_populations"]["self-check"] or product.get("fault_selector") != expected_selector or receipt != {"status": "ObservedCompleteApply", "complete_apply_hooks": True} or any(type(value) is not int or value < 0 for value in counters.values()) or not init or not terminal or not contention or not timing or "fault_receipts" in product:
                failures.append(f"fault-run:{case}")
            rehash_process(record, evidence_root, authority, failures)
            observed[case] = counters
    if not authority["_self_check"] and set(observed) != set(direct_modes):
        failures.append("direct-fault-modes")
    proofs = row.get("source_fault_proofs")
    if not authority["_self_check"] and SOURCE_FAULT_PROOFS.is_file() and proofs != json.loads(SOURCE_FAULT_PROOFS.read_text()).get("proofs"):
        failures.append("source-fault-proof-authority")
    proven = set()
    if isinstance(proofs, list):
        for proof in proofs:
            case = proof.get("fault_class") if isinstance(proof, dict) else None
            matrix_row = authority["_fault_rows"].get(case)
            source = REPO / proof.get("source_path", "INVALID") if isinstance(proof, dict) else REPO / "INVALID"
            execution = REPO / proof.get("execution_receipt_path", "INVALID") if isinstance(proof, dict) else REPO / "INVALID"
            try:
                execution_receipt = json.loads(execution.read_text())
                execution_cases = execution_receipt.get("cases", [execution_receipt])
                observed_case = next(item for item in execution_cases if item.get("fault_class") == case)
            except Exception:
                execution_receipt, observed_case = {}, {}
            receipt_hashes = observed_case.get("command_sha256") == hashlib.sha256(compact(observed_case.get("command")).encode()).hexdigest() and observed_case.get("stdout_sha256") == hashlib.sha256(observed_case.get("stdout", "").encode()).hexdigest() and observed_case.get("stderr_sha256") == hashlib.sha256(observed_case.get("stderr", "").encode()).hexdigest() and observed_case.get("return_sha256") == hashlib.sha256(compact({"returncode": observed_case.get("returncode")}).encode()).hexdigest()
            receipt_matches = execution_receipt.get("status") == "PASS" and execution_receipt.get("source_sha256") == proof.get("source_sha256") and raw_bundle_valid(execution_receipt, authority["_frozen_source_hashes"]) and observed_case.get("test_locator") == proof.get("test_locator") and observed_case.get("typed_outcome") == proof.get("typed_outcome") and observed_case.get("counter_claims") == proof.get("counter_claims") and isinstance(observed_case.get("command"), list) and bool(observed_case["command"]) and isinstance(observed_case.get("stdout"), str) and bool(observed_case["stdout"]) and isinstance(observed_case.get("stderr"), str) and observed_case.get("returncode") == 0 and receipt_hashes
            valid = isinstance(proof, dict) and source == PRODUCT_SOURCE and execution == FOCUSED_FAULT_EXECUTION and proof.get("classification") == "ObservedFocusedCompleteApply" and matrix_row is not None and proof.get("matrix_row") == matrix_row and proof.get("required_observation") == matrix_row["required_observation"] and proof.get("matrix_row_sha256") == hashlib.sha256(compact(matrix_row).encode()).hexdigest() and source.is_file() and execution.is_file() and hashlib.sha256(source.read_bytes()).hexdigest() == proof.get("source_sha256") and hashlib.sha256(execution.read_bytes()).hexdigest() == proof.get("execution_receipt_sha256") and proof.get("source_sha256") in authority["_frozen_source_hashes"] and proof.get("execution_receipt_sha256") in authority["_frozen_source_hashes"] and receipt_matches and isinstance(proof.get("test_locator"), str) and bool(proof["test_locator"]) and isinstance(proof.get("typed_outcome"), str) and bool(proof["typed_outcome"]) and isinstance(proof.get("counter_claims"), dict) and bool(proof["counter_claims"]) and all(type(value) is int and value >= 0 for value in proof["counter_claims"].values())
            if case in proven or case in observed or not valid:
                failures.append("source-fault-proof")
                continue
            proven.add(case)
    if set(authority["_fault_rows"]) != set(observed) | proven:
        failures.append("fault-matrix-incomplete")


def inspect(record, failures, authority, evidence_root):
    mode, p = record.get("mode"), record.get("product", {})
    def need(ok, name):
        if not ok: failures.append(f"{mode}:{name}")
    need(p.get("schema") == authority["product_schema"] and p.get("status") == "PASS" and p.get("mode") == mode and p.get("size_bytes") == 250_000 and p.get("route_class") == authority["route_class"], "schema/status/mode/size/route")
    populations = {name: tuple(values) for name, values in authority["mode_populations"].items()}
    need((p.get("exact_every_root_population"), p.get("latest_following_population")) == populations.get(mode), "populations")
    exact, latest = populations.get(mode, (-1, -1))
    expected_started = exact + min(latest, 2) + 4
    expected_submitted = exact + latest + 5
    expected_coalesced = latest - min(latest, 2) + 1
    slots = authority["slot_limits"]
    need(p.get("worker_count") == slots["workers"] and p.get("max_in_flight", slots["in_flight"] + 1) <= slots["in_flight"] and p.get("max_pending", slots["pending"] + 1) <= slots["pending"], "worker/slots")
    started = p.get("started", -1)
    need((p.get("submitted"), started, p.get("published"), p.get("coalesced")) == (expected_submitted, expected_started, expected_started, expected_coalesced) and all(p.get(k) == 0 for k in ("cancelled", "failed", "stale")), "exact-vs-latest-conservation")
    need(p.get("submitted") == p.get("coalesced", 0) + started, "request-conservation")
    need(started == sum(p.get(k, -1) for k in ("published", "cancelled", "failed", "stale")), "build-conservation")
    need(p.get("seed_rotations") == p.get("published"), "seed-rotation")
    need(p.get("projected_equals_last_requested") is True and p.get("projected_root") == p.get("last_requested_root"), "terminal-root")
    need(all(p.get(k) == 0 for k in ("sqlite_write_calls", "sqlite_transactions", "sqlite_commits", "sqlite_busy_errors", "sqlite_locked_errors")), "sqlite-read-only")
    need([p.get("foreground_transactions"), p.get("foreground_commits"), p.get("reader_commit_autocommit"), p.get("reader_commit_scope_live"), p.get("foreground_commit_primary_code"), p.get("foreground_commit_extended_code")] == [1, 1, 1, 0, 0, 0] and p.get("contention_worker_and_foreground_transaction_intervals_overlap") is True and p.get("contention_overlap_scope") == "ObservedBroadWorkerAndForegroundTransactionIntervals" and p.get("foreground_commit_within_end_to_end_t3_t4_claim") == "NotClaimedDifferentRequest", "foreground-contention-tuple")
    need(p.get("contention_worker_start_ns", 1) < p.get("contention_worker_end_ns", 0) and p.get("contention_foreground_start_ns", 1) < p.get("contention_foreground_end_ns", 0) and p.get("contention_foreground_start_ns", 1) < p.get("contention_worker_end_ns", 0) and p.get("contention_worker_start_ns", 1) < p.get("contention_foreground_end_ns", 0), "contention-equation")
    need([p.get("reader_barrier_autocommit"), p.get("reader_barrier_scope_live")] == [1, 1], "contention-barrier-state")
    need(1 <= p.get("full_fallbacks", -1) <= started and 1 <= p.get("range_fetches", -1) <= 256 * started and 0 < p.get("fetched_bytes", -1) <= 8_388_608 * started, "bounded-routes")
    need(1 <= p.get("clone_successes", -1) <= p.get("clone_calls", -1) <= started, "clone-counters")
    need(p.get("max_buffer_bytes", authority["buffer_limit_bytes"] + 1) <= authority["buffer_limit_bytes"] and all(p.get(k) == 0 for k in TERMINAL), "terminal/buffer")
    need(p.get("shutdown") == "drained" and p.get("checkpoint_outside_service_timer") is True, "shutdown/checkpoint")
    storage_fields = ("initial_descriptor_verification_bytes", "initial_storage_logical_bytes", "initial_storage_apparent_bytes", "initial_storage_allocated_bytes", "terminal_storage_logical_bytes", "terminal_storage_apparent_bytes", "terminal_storage_allocated_bytes")
    need(all(type(p.get(name)) is int and p[name] >= 0 for name in storage_fields) and p.get("initial_descriptor_verification_bytes", 0) > 0 and p.get("initial_storage_logical_bytes") == p.get("initial_descriptor_verification_bytes") and p.get("terminal_storage_logical_bytes", 0) > 0 and p.get("terminal_descriptor_classification") == "ProvenByWorkerJoinAndOwnedDescriptorDrop", "descriptor-storage")
    t0, t1, t2, t3, t4 = (p.get(name) for name in ("end_to_end_edit_t0_ns", "end_to_end_canonical_ack_t1_ns", "end_to_end_enqueue_t2_ns", "end_to_end_worker_start_t3_ns", "end_to_end_native_ack_t4_ns"))
    need(all(type(value) is int and value >= 0 for value in (t0, t1, t2, t3, t4)) and t0 <= t1 <= t2 <= t3 <= t4 and p.get("end_to_end_population") == 1 and p.get("end_to_end_scope") == "ObservedEditT0CanonicalAckT1EnqueueT2WorkerT3NativeAckT4" and p.get("end_to_end_canonical_transactions") == 1 and p.get("end_to_end_canonical_commits") == 1, "observed-t0-t4")
    latency = authority["latency_limits_ns"]
    for label, p50max, p95max in (("exact", latency["exact_p50"], latency["exact_p95"]), ("sparse", latency["sparse_p50"], latency["sparse_p95"])):
        values = p.get(f"{label}_build_ns")
        valid = isinstance(values, list) and bool(values) and all(type(x) is int and x >= 0 for x in values)
        need(valid, label + "-timers")
        if valid: need((pct(values, 50), pct(values, 95)) == (p.get(f"{label}_p50_ns"), p.get(f"{label}_p95_ns")) and p[f"{label}_p50_ns"] <= p50max and p[f"{label}_p95_ns"] <= p95max, label + "-latency")
    evidence = p.get("build_evidence")
    classified = {"exact": [], "sparse": [], "full_fallback": [], "contention_fallback": []}
    policies = {"ExactEveryRoot": [], "IsolatedSparseSentinel": [], "IsolatedOrdinaryFallback": [], "LatestFollowingSameSize": [], "LatestFollowingCountStorm": []}
    policy_latency = {name: [] for name in policies}
    valid_evidence = isinstance(evidence, list) and len(evidence) == started
    if valid_evidence:
        for value in evidence:
            valid_evidence = isinstance(value, dict) and type(value.get("contention")) is bool and value.get("plan") in ("Ranges", "FullFallback") and value.get("policy") in policies and all(type(value.get(k)) is int and value[k] >= 0 for k in ("parent_length", "target_length", "range_count", "wall_ns", "ordinal"))
            if not valid_evidence: break
            policies[value["policy"]].append(value["ordinal"])
            policy_latency[value["policy"]].append(value["wall_ns"])
            if value["plan"] == "FullFallback" or value["parent_length"] != value["target_length"]: route = "contention_fallback" if value["contention"] else "full_fallback"
            elif value["range_count"] == 0: route = "exact"
            else: route = "sparse"
            if route in ("exact", "sparse") and value["contention"]: valid_evidence = False; break
            classified[route].append(value["wall_ns"])
    need(valid_evidence, "build-evidence")
    if valid_evidence:
        need(classified["exact"] == p.get("exact_build_ns") and classified["sparse"] == p.get("sparse_build_ns") and classified["full_fallback"] == p.get("full_fallback_build_ns") and classified["contention_fallback"] == p.get("contention_full_fallback_build_ns"), "route-classification")
        expected_same_size = [] if latest == 0 else ([0] if latest == 1 else [0, latest - 1])
        expected_policies = {"ExactEveryRoot": list(range(exact)), "IsolatedSparseSentinel": [0], "IsolatedOrdinaryFallback": [0], "LatestFollowingSameSize": expected_same_size, "LatestFollowingCountStorm": [0, 2]}
        need({key: sorted(values) for key, values in policies.items()} == expected_policies, "semantic-policy-stream-ordinals")
        need(len(classified["exact"]) == 1 and len(classified["sparse"]) == exact + min(latest, 2) + 1, "execution-route-classification")
        fallback = classified["full_fallback"]
        contention_fallback = classified["contention_fallback"]
        need(bool(fallback) and len(fallback) + len(contention_fallback) == p.get("full_fallbacks") and (pct(fallback, 50), pct(fallback, 95)) == (p.get("full_fallback_p50_ns"), p.get("full_fallback_p95_ns")), "fallback-timers")
        fallback_limit = authority["fallback_isolated_limit_ns"]
        need(p.get("full_fallback_g3_bound_ns") == fallback_limit and p.get("full_fallback_within_g3_bound") is True and max(fallback) <= fallback_limit, "fallback-g3-bound")
        need(len(fallback) == 1 and len(contention_fallback) == 1 and (pct(contention_fallback, 50), pct(contention_fallback, 95)) == (p.get("contention_full_fallback_p50_ns"), p.get("contention_full_fallback_p95_ns")) and p.get("contention_full_fallback_latency_claim") == "NotClaimedDifferentConcurrentExecutionShape", "contention-fallback")
    need(type(p.get("reader_initialization_ns")) is int and p["reader_initialization_ns"] > 0 and p.get("reader_initialization_classification") == "OneTimeReadOnlyProcessInitializationInsideCompleteWallOutsideServiceSamples", "reader-initialization")
    need(p.get("reader_initialization_calls") == 1 and p.get("reader_initialization_bytes_requested") == 1 and p.get("reader_initialization_sql_queries", 0) > 0 and p.get("reader_initialization_authenticated_objects", 0) > 0 and p.get("reader_initialization_authenticated_bytes", 0) > 0 and p.get("reader_initialization_q_high_water", 0) > 0, "reader-initialization-work")
    need([p.get("reader_initialization_read_only"), p.get("reader_initialization_query_only"), p.get("reader_initialization_inside_complete_wall"), p.get("reader_initialization_excluded_from_service_samples")] == [True, True, True, True] and len(p.get("build_evidence", [])) == started, "reader-initialization-boundary")
    need(p.get("fault_selector") == "None" and p.get("fault_receipt") == {"status": "NotInjectedInPerformanceRun", "complete_apply_hooks": True} and "fault_receipts" not in p, "performance-fault-receipt-singular")
    need(record.get("maximum_resident_set_size", authority["rss_limit_bytes"] + 1) <= authority["rss_limit_bytes"], "rss")
    need(record.get("clone", {}).get("method") == "APFSCloneCpC" and record.get("clone", {}).get("inventory_equal") is True and record.get("clone", {}).get("source_sealed_reverified") is True and record.get("clone", {}).get("private_attempt_permissions") == "WritableAfterExactCloneInventory" and attempt_clone_receipts_valid(record.get("clone", {}), record.get("attempt_root")), "clone/sealed-source/private-modes/rebind")
    digest = record.get("process_evidence_sha256")
    need(isinstance(digest, str) and len(digest) == 64 and all(character in "0123456789abcdef" for character in digest), "process-evidence-binding")
    rehash_process(record, evidence_root, authority, failures)
    return {"policy_latency_ns": policy_latency, "route_latency_ns": classified}

def analyze(row, authority, evidence_root):
    failures, phase, records = [], row.get("phase"), row.get("products")
    derived = []
    plan = authority.get("campaigns", {}).get(phase, {}).get("modes")
    if row.get("schema") != "phase4-g5-2-harness-row-v3" or row.get("status") != "PASS": failures.append("row-schema/status")
    if not isinstance(records, list) or [r.get("mode") for r in records] != plan or row.get("product_processes") != len(plan or ()): failures.append("product-processes/modes")
    else:
        derived = [inspect(record, failures, authority, evidence_root) for record in records]
    all_processes = records + row.get("fault_runs", []) if isinstance(records, list) and isinstance(row.get("fault_runs"), list) else []
    if all_processes and row.get("maximum_resident_set_size") != max(r.get("maximum_resident_set_size", -1) for r in all_processes): failures.append("aggregate-rss-max")
    validate_fault_matrix(row, authority, evidence_root, failures)
    stage, terminal = row.get("analysis_stage"), row.get("terminal")
    if stage == "preliminary":
        if terminal is not None: failures.append("preliminary-terminal-present")
        complete_wall = None
    elif stage == "final":
        method_limit = authority["campaigns"][phase]["complete_wall_limit_ns"] if phase in authority["campaigns"] else -1
        expected_processes = row.get("product_processes", -1) + row.get("fault_processes", -1)
        valid_terminal = isinstance(terminal, dict) and terminal.get("schema") == "phase4-g5-2-terminal-v3" and terminal.get("status") == "PASS" and terminal.get("limit_ns") == method_limit and terminal.get("complete_wall_ns", -1) <= method_limit and terminal.get("lock_released") is True and terminal.get("terminal_fixture_roots") == 0 and terminal.get("product_processes") == expected_processes and terminal.get("product_rows") == expected_processes
        if not valid_terminal: failures.append("terminal-complete-wall")
        if isinstance(terminal, dict) and row.get("terminal_sha256") != hashlib.sha256((compact(terminal) + "\n").encode()).hexdigest(): failures.append("terminal-binding")
        terminal_path, release_path = evidence_root / "TERMINAL-v3.json", evidence_root / "LOCK-RELEASE-v3.json"
        try:
            actual_terminal, actual_release = json.loads(terminal_path.read_text()), json.loads(release_path.read_text())
            release = row.get("lock_release")
            lock_valid = isinstance(release, dict) and release == actual_release and release.get("schema") == "phase4-g5-2-lock-release-v3" and release.get("status") == "PASS" and release.get("lock_absent") is True and all(type(release.get(name)) is int and release[name] >= 0 for name in ("device", "inode")) and all(isinstance(release.get(name), str) and len(release[name]) == 64 for name in ("ownership_token_sha256", "intent_sha256"))
            if actual_terminal != terminal or hashlib.sha256(terminal_path.read_bytes()).hexdigest() != row.get("terminal_sha256") or hashlib.sha256(release_path.read_bytes()).hexdigest() != row.get("lock_release_sha256") or not lock_valid: failures.append("terminal-lock-file-custody")
        except Exception:
            failures.append("terminal-lock-file-custody")
        complete_wall = terminal.get("complete_wall_ns") if isinstance(terminal, dict) else None
    else:
        failures.append("analysis-stage")
        complete_wall = None
    if row.get("cache_state") != "WarmUnknownPreparedFixtureAPFSClone" or row.get("cold_reopen_claim") is not False: failures.append("cache-claim")
    p = records[-1].get("product", {}) if records else {}
    normalized = {"status": "PASS" if not failures else "REVISE", "analysis_stage": stage, "phase": phase, "hard_failures": sorted(failures), "product_processes": row.get("product_processes"), "sentinel_modes": plan[:-1] if plan else None, "primary_mode": plan[-1] if plan else None, "exact_population": p.get("exact_every_root_population"), "latest_population": p.get("latest_following_population"), "projected_root": p.get("projected_root"), "rss_bytes": row.get("maximum_resident_set_size"), "complete_wall_ns": complete_wall, "exact_p50_ns": p.get("exact_p50_ns"), "exact_p95_ns": p.get("exact_p95_ns"), "sparse_p50_ns": p.get("sparse_p50_ns"), "sparse_p95_ns": p.get("sparse_p95_ns"), "fallback_p50_ns": p.get("full_fallback_p50_ns"), "fallback_p95_ns": p.get("full_fallback_p95_ns"), "primary_policy_latency_ns": derived[-1]["policy_latency_ns"] if derived else None, "primary_route_latency_ns": derived[-1]["route_latency_ns"] if derived else None}
    return {"schema": "phase4-g5-2-primary-v3", "normalized": normalized, "normalized_sha256": hashlib.sha256(compact(normalized).encode()).hexdigest()}

def main():
    allow_unfrozen = len(sys.argv) == 4 and sys.argv[3] == "--self-check-authority"
    authority = load_authority(allow_unfrozen)
    rows = [json.loads(x) for x in pathlib.Path(sys.argv[1]).read_text().splitlines() if x.strip()]
    report = analyze(rows[0], authority, pathlib.Path(sys.argv[1]).parent) if len(rows) == 1 else {"schema": "phase4-g5-2-primary-v3", "normalized": {"status": "REVISE", "hard_failures": ["row-count"]}}
    pathlib.Path(sys.argv[2]).write_text(compact(report) + "\n"); print(compact(report))
    return report["normalized"]["status"] != "PASS"
if __name__ == "__main__": raise SystemExit(main())
