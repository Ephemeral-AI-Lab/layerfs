#!/usr/bin/env python3
import csv, hashlib, json, pathlib, sys

HERE = pathlib.Path(__file__).resolve().parents[1]
REPO = HERE.parents[4]
OPERATION_FIELDS = {"wall_ns", "sql_queries", "sql_rows", "row_blob_reads", "row_blob_writes", "canonical_authenticated", "canonical_new_bytes", "mapping_rewritten", "objects_created", "objects_reused", "transactions", "commits", "q_high_water", "q_current"}

def compact(value): return json.dumps(value, sort_keys=True, separators=(",", ":"))
def digest(path): return hashlib.sha256(pathlib.Path(path).read_bytes()).hexdigest()
def object_id(value): return isinstance(value, str) and len(value) == 64 and not (set(value) - set("0123456789abcdef"))
def natural(value): return type(value) is int and value >= 0

def bound_items(value):
    if isinstance(value, dict):
        if {"path", "sha256"} <= set(value): yield value
        for child in value.values(): yield from bound_items(child)
    elif isinstance(value, list):
        for child in value: yield from bound_items(child)

def authority(self_check=False):
    contract_path, reused_path = HERE / "method/METHOD-CONTRACT-v3.json", HERE / "REUSED-AUTHORITY-v3.json"
    contract, reused = json.loads(contract_path.read_text()), json.loads(reused_path.read_text())
    if reused.get("status") != "PASS": raise RuntimeError("reused authority unavailable")
    for item in bound_items(reused.get("authorities", {})):
        path = REPO / item["path"]
        if not path.is_file() or digest(path) != item["sha256"]: raise RuntimeError("reused authority hash mismatch")
    schedule_path, expected_path, coverage_path = HERE / "method/SCHEDULE-v3.tsv", HERE / "method/EXPECTED-OUTCOMES-v3.tsv", HERE / "COVERAGE-MAP-v3.tsv"
    with schedule_path.open(newline="") as handle: rows = list(csv.DictReader(handle, delimiter="\t"))
    if {row["phase"] for row in rows} != {"screen", "gate"}: raise RuntimeError("schedule phases")
    for row in rows:
        plan = contract["campaigns"][row["phase"]]
        observed = (int(row["product_children"]), int(row["history_edits"]), [int(value) for value in row["checkpoint_revisions"].split(",")], int(row["full_reconstructions"]), int(row["complete_wall_limit_ns"]))
        expected = (1, plan["history_edits"], plan["checkpoint_revisions"], contract["full_reconstruction"][row["phase"] + "_count"], plan["complete_wall_hard_limit_ns"])
        if observed != expected: raise RuntimeError("schedule contract")
    if not self_check:
        freeze = json.loads((HERE / "method/SOURCE-FREEZE-v3.json").read_text())
        if freeze.get("status") != "FROZEN_BEFORE_FORECAST" or freeze.get("method_contract_sha256") != digest(contract_path) or freeze.get("schedule_sha256") != digest(schedule_path) or freeze.get("expected_outcomes_sha256") != digest(expected_path) or freeze.get("coverage_map_sha256") != digest(coverage_path) or freeze.get("reused_authority_sha256") != digest(reused_path): raise RuntimeError("frozen method authority mismatch")
    return contract

def operation(value, transactions, commits):
    return isinstance(value, dict) and set(value) == OPERATION_FIELDS and all(natural(value[name]) for name in OPERATION_FIELDS) and value["transactions"] == transactions and value["commits"] == commits and value["q_current"] == 0

def analyze(envelope, contract):
    failures, phase = [], envelope.get("phase")
    need = lambda condition, label: failures.append(label) if not condition else None
    plan = contract["campaigns"].get(phase, {})
    product = envelope.get("product", {})
    stage = envelope.get("analysis_stage")
    need(envelope.get("schema") == contract["envelope_schema"] and envelope.get("status") == "PASS" and stage in ("preliminary", "final"), "envelope")
    need(tuple(envelope.get(name) for name in ("product_processes", "children_started", "children_reaped", "terminal_active_children")) == (1, 1, 1, 0), "one-long-lived-child")
    need((product.get("schema"), product.get("status"), product.get("phase"), product.get("source_bytes")) == (contract["product_schema"], "PASS", phase, contract["edit"]["length_bytes"]), "product")

    base = product.get("base_publication", {})
    base_ok = object_id(base.get("root")) and tuple(base.get(name) for name in ("ordinal", "length", "route", "transactions", "commits", "q_terminal", "temp_residue")) == (1, contract["edit"]["length_bytes"], contract["base_publication"]["route"], 1, 1, 0, 0) and natural(base.get("q_high_water"))
    need(base_ok, "base-publication")
    edits = product.get("edits")
    edit_ok = isinstance(edits, list) and len(edits) == plan.get("history_edits", -1) - 1
    if edit_ok:
        for ordinal, edit in enumerate(edits, 2):
            edit_ok &= edit.get("ordinal") == ordinal and object_id(edit.get("root")) and tuple(edit.get(name) for name in ("length", "route", "transactions", "commits", "q_terminal", "temp_residue")) == (contract["edit"]["length_bytes"], contract["edit"]["route"], 1, 1, 0, 0) and natural(edit.get("q_high_water"))
        edit_ok &= len({base.get("root"), *(edit.get("root") for edit in edits)}) == plan.get("history_edits")
    need(edit_ok, "history-fill-edits")
    roots = {1: base.get("root")}
    if isinstance(edits, list): roots.update({edit.get("ordinal"): edit.get("root") for edit in edits})

    checkpoints = product.get("checkpoints")
    revisions = plan.get("checkpoint_revisions", [])
    checkpoint_ok = isinstance(checkpoints, list) and [point.get("revision") for point in checkpoints] == revisions
    if checkpoint_ok:
        for point in checkpoints:
            revision, current, target = point["revision"], point.get("root"), point.get("next_root")
            range_operation, edit_operation = point.get("operations", {}).get("range"), point.get("operations", {}).get("same_size_edit")
            projection = point.get("projection", {})
            checkpoint_ok &= current == roots.get(revision) and object_id(current) and point.get("length") == contract["edit"]["length_bytes"] and object_id(point.get("transition")) and object_id(point.get("output_digest")) and point.get("range_bytes") == contract["checkpoint_operations"]["exact_range_bytes"] and object_id(point.get("range_digest"))
            checkpoint_ok &= point.get("edit_to_revision") == revision + 1 and object_id(target) and target != current and object_id(point.get("next_transition")) and (revision == plan["history_edits"] or target == roots.get(revision + 1))
            checkpoint_ok &= operation(range_operation, 0, 0) and operation(edit_operation, 1, 1)
            checkpoint_ok &= tuple(projection.get(name) for name in ("classification", "exact_policy", "exact_revision", "exact_requested_root", "exact_result_root", "latest_policy", "latest_revision", "latest_requested_root", "latest_result_root", "latest_route")) == (contract["projection"]["classification"], contract["projection"]["exact_policy"], revision, current, current, contract["projection"]["latest_policy"], revision + 1, target, target, contract["projection"]["latest_route"])
            checkpoint_ok &= tuple(projection.get(name) for name in ("submitted", "started", "published", "coalesced", "full_fallbacks", "range_fetches", "q_terminal", "temp_residue")) == (2, 2, 2, 0, 0, 1, 0, 0) and natural(projection.get("clone_calls")) and natural(projection.get("seed_rotations")) and natural(projection.get("q_high_water")) and type(projection.get("written_bytes")) is int and 0 < projection["written_bytes"] <= contract["limits"]["buffer_bytes"] and natural(projection.get("max_buffer_bytes")) and projection["max_buffer_bytes"] <= contract["limits"]["buffer_bytes"]
            storage, resource = point.get("storage", {}), point.get("resource", {})
            checkpoint_ok &= all(natural(storage.get(name)) for name in ("logical_bytes", "apparent_bytes", "allocated_bytes", "live_objects", "unreachable_objects")) and storage.get("live_objects", 0) > 0 and natural(resource.get("q_high_water")) and natural(resource.get("fd_count"))
    need(checkpoint_ok, "checkpoint-operations-projection")

    reconstructions = product.get("reconstructions")
    reconstruction_ok = isinstance(reconstructions, list) and len(reconstructions) == contract["full_reconstruction"].get(phase + "_count", -1)
    if reconstruction_ok:
        expected_rows = [(point["revision"], point["root"], point["output_digest"], "CompleteCheckpoint") for point in checkpoints]
        if checkpoints: expected_rows.append((plan["history_edits"] + 1, checkpoints[-1]["next_root"], None, "TerminalVerifiedNative"))
        reconstruction_ok &= len(reconstructions) == len(expected_rows)
        for row, expected in zip(reconstructions, expected_rows):
            revision, root_value, output_digest, scope = expected
            reconstruction_ok &= (row.get("revision"), row.get("root"), row.get("length"), row.get("scope")) == (revision, root_value, contract["edit"]["length_bytes"], scope) and object_id(row.get("output_digest")) and (output_digest is None or row.get("output_digest") == output_digest)
    need(reconstruction_ok, "full-reconstruction")

    aba = product.get("aba", {})
    final_checkpoint = checkpoints[-1] if checkpoint_ok and checkpoints else {}
    aba_ok = object_id(aba.get("root_a")) and aba.get("root_a") == final_checkpoint.get("root") and aba.get("root_b") == final_checkpoint.get("next_root") and aba.get("final_root") == aba.get("root_a") and aba.get("root_a") != aba.get("root_b") and object_id(aba.get("final_transition")) and aba.get("identity_reused") is True and tuple(aba.get(name) for name in ("a_to_b_transactions", "a_to_b_commits", "b_to_a_transactions", "b_to_a_commits", "transactions", "commits", "q_terminal")) == (1, 1, 1, 1, 2, 2, 0) and all(natural(aba.get(name)) for name in ("objects_created", "objects_reused", "logical_store_bytes_before", "logical_store_bytes_after", "q_high_water"))
    need(aba_ok, "aba-publications")
    historical = product.get("historical_read", {})
    need(historical.get("requested_root") == aba.get("root_b") and historical.get("bytes") == contract["historical_read"]["bytes"] and object_id(historical.get("digest")), "historical-root-read")

    concurrency = product.get("concurrency", {})
    concurrency_ok = concurrency.get("source_bytes") == contract["concurrency_sentinel"]["size_bytes"] and concurrency.get("reader_model") == "OpenImmutableReadersBoundedScopesBeforeAndAfterCommitNoLiveStatementOrBlobAcrossCommit" and object_id(concurrency.get("prior_root")) and object_id(concurrency.get("new_root")) and concurrency.get("prior_root") != concurrency.get("new_root") and tuple(concurrency.get(name) for name in ("reader_one_before", "reader_two_before", "reader_one_current_head_after", "reader_two_current_head_after")) == (concurrency.get("prior_root"), concurrency.get("prior_root"), concurrency.get("new_root"), concurrency.get("new_root")) and tuple(concurrency.get(name) for name in ("historical_range_bytes_after_commit", "writer_transactions", "writer_commits", "busy_errors", "locked_errors", "q_terminal")) == (4096, 1, 1, 0, 0, 0) and concurrency.get("sqlite_error_observation") == "ObservedNoSqliteErrorReturn" and natural(concurrency.get("q_high_water"))
    need(concurrency_ok, "concurrency-sentinel")
    cache=contract["cache_profile"]; pages=(concurrency.get("writer_cache_size_pages"),concurrency.get("reader_one_cache_size_pages"),concurrency.get("reader_two_cache_size_pages")); page_bytes=concurrency.get("sqlite_page_size_bytes"); reduction=concurrency.get("configured_cache_reduction_bytes"); connection_counts=(concurrency.get("simultaneous_connections_high_water"),concurrency.get("simultaneous_connections_terminal"))
    cache_ok = pages==(cache["writer_pages"],cache["reader_one_pages"],cache["reader_two_pages"]) and page_bytes==cache["page_bytes"] and reduction==cache["aggregate_reduction_bytes"]==(3*cache["default_pages"]-sum(pages))*page_bytes and sum(pages)*page_bytes==cache["aggregate_cache_ceiling_bytes"] and connection_counts==(cache["active_connections_high_water"],cache["active_connections_terminal"])
    need(cache_ok, "cache-profile-equation")

    terminal = product.get("terminal", {})
    terminal_ok = terminal.get("revision") == plan.get("history_edits", -2) + 2 and terminal.get("root") == aba.get("final_root") and object_id(terminal.get("transition")) and terminal.get("output_digest") == final_checkpoint.get("output_digest") and terminal.get("reachability") == "ReadOnlyNoGc" and all(natural(terminal.get(name)) for name in ("stored_objects", "current_live_objects", "current_unreachable_objects", "retained_live_objects", "retained_unreachable_objects", "logical_store_bytes", "apparent_store_bytes", "allocated_store_bytes", "q_high_water", "fd_before", "fd_after_store_close", "fd_after_cleanup")) and terminal.get("stored_objects", 0) > 0 and terminal.get("current_live_objects", 0) <= terminal.get("stored_objects", -1) and terminal.get("retained_live_objects", 0) <= terminal.get("stored_objects", -1) and terminal.get("q_terminal") == 0 and terminal.get("fd_before") == terminal.get("fd_after_cleanup") and terminal.get("descriptor_leak") is False and tuple(terminal.get(name) for name in ("seed_residue", "temp_residue", "work_root_residue")) == (0, 0, 0)
    need(terminal_ok, "terminal-product")
    need(natural(product.get("max_buffer_bytes")) and product.get("max_buffer_bytes") <= contract["limits"]["buffer_bytes"] and type(envelope.get("maximum_resident_set_size")) is int and 0 < envelope["maximum_resident_set_size"] <= contract["limits"]["rss_bytes"], "resources")

    wall = envelope.get("complete_wall_ns")
    if stage == "preliminary": need(wall is None and envelope.get("lock_released") is None and envelope.get("terminal_work_roots") is None, "preliminary-scope")
    elif stage == "final": need(type(wall) is int and 0 <= wall <= plan.get("complete_wall_hard_limit_ns", -1) and envelope.get("lock_released") is True and envelope.get("terminal_work_roots") == 0, "complete-wall-through-lock-release")
    return {"status": "PASS" if not failures else "REVISE", "phase": phase, "analysis_stage": stage, "hard_failures": sorted(failures), "product_processes": envelope.get("product_processes"), "history_revisions": 1 + len(edits) if isinstance(edits, list) else None, "checkpoint_revisions": [point.get("revision") for point in checkpoints] if isinstance(checkpoints, list) else None, "terminal_revision": terminal.get("revision"), "aba_root": aba.get("final_root"), "cache_profile":{"writer_pages":pages[0],"reader_one_pages":pages[1],"reader_two_pages":pages[2],"page_bytes":page_bytes,"aggregate_reduction_bytes":reduction,"connections_high":connection_counts[0],"connections_terminal":connection_counts[1]}, "rss_bytes": envelope.get("maximum_resident_set_size"), "complete_wall_ns": wall}

def main():
    raw, output = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
    normalized = analyze(json.loads(raw.read_text()), authority("--self-check-authority" in sys.argv[3:]))
    report = {"schema": "phase4-g5-3-primary-v3", "normalized": normalized, "normalized_sha256": hashlib.sha256(compact(normalized).encode()).hexdigest()}
    output.write_text(compact(report) + "\n")

if __name__ == "__main__": main()
