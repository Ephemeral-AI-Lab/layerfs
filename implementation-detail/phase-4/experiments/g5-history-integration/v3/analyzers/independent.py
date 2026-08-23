#!/usr/bin/env python3
import csv, hashlib, json, pathlib, sys

BASE = pathlib.Path(__file__).resolve().parents[1]
REPO = BASE.parents[4]
OP_KEYS = frozenset(("wall_ns", "sql_queries", "sql_rows", "row_blob_reads", "row_blob_writes", "canonical_authenticated", "canonical_new_bytes", "mapping_rewritten", "objects_created", "objects_reused", "transactions", "commits", "q_high_water", "q_current"))
canonical = lambda value: json.dumps(value, sort_keys=True, separators=(",", ":"))
file_hash = lambda path: hashlib.sha256(pathlib.Path(path).read_bytes()).hexdigest()
is_id = lambda value: type(value) is str and len(value) == 64 and not (set(value) - set("0123456789abcdef"))
is_count = lambda value: type(value) is int and value >= 0

def bindings(value):
    pending = [value]
    while pending:
        item = pending.pop()
        if type(item) is dict:
            if "path" in item and "sha256" in item: yield item
            pending.extend(item.values())
        elif type(item) is list: pending.extend(item)

def method(unfrozen=False):
    contract_file, reused_file = BASE / "method/METHOD-CONTRACT-v3.json", BASE / "REUSED-AUTHORITY-v3.json"
    contract, reused = json.loads(contract_file.read_text()), json.loads(reused_file.read_text())
    if reused.get("status") != "PASS" or any(not (REPO / item["path"]).is_file() or file_hash(REPO / item["path"]) != item["sha256"] for item in bindings(reused.get("authorities", {}))): raise RuntimeError("reused milestone authority")
    schedule_file, expected_file, coverage_file = BASE / "method/SCHEDULE-v3.tsv", BASE / "method/EXPECTED-OUTCOMES-v3.tsv", BASE / "COVERAGE-MAP-v3.tsv"
    with schedule_file.open(newline="") as handle: schedule = tuple(csv.DictReader(handle, delimiter="\t"))
    if tuple(sorted(row["phase"] for row in schedule)) != ("gate", "screen"): raise RuntimeError("schedule phases")
    for row in schedule:
        plan = contract["campaigns"][row["phase"]]
        found = (int(row["product_children"]), int(row["history_edits"]), tuple(map(int, row["checkpoint_revisions"].split(","))), int(row["full_reconstructions"]), int(row["complete_wall_limit_ns"]))
        wanted = (1, plan["history_edits"], tuple(plan["checkpoint_revisions"]), contract["full_reconstruction"][row["phase"] + "_count"], plan["complete_wall_hard_limit_ns"])
        if found != wanted: raise RuntimeError("schedule binding")
    if not unfrozen:
        frozen = json.loads((BASE / "method/SOURCE-FREEZE-v3.json").read_text())
        direct = (frozen.get("status") == "FROZEN_BEFORE_FORECAST", frozen.get("method_contract_sha256") == file_hash(contract_file), frozen.get("schedule_sha256") == file_hash(schedule_file), frozen.get("expected_outcomes_sha256") == file_hash(expected_file), frozen.get("coverage_map_sha256") == file_hash(coverage_file), frozen.get("reused_authority_sha256") == file_hash(reused_file))
        if not all(direct): raise RuntimeError("frozen method authority")
    return contract

def operation_receipt(value, tx, commit):
    return type(value) is dict and frozenset(value) == OP_KEYS and all(is_count(value[key]) for key in OP_KEYS) and (value["transactions"], value["commits"], value["q_current"]) == (tx, commit, 0)

def audit(row, contract):
    bad = []
    check = lambda condition, label: bad.append(label) if not condition else None
    phase, stage = row.get("phase"), row.get("analysis_stage")
    plan, product = contract["campaigns"].get(phase, {}), row.get("product", {})
    check((row.get("schema"), row.get("status")) == (contract["envelope_schema"], "PASS") and stage in ("preliminary", "final"), "envelope")
    check(tuple(row.get(key) for key in ("product_processes", "children_started", "children_reaped", "terminal_active_children")) == (1, 1, 1, 0), "one-long-lived-child")
    check(tuple(product.get(key) for key in ("schema", "status", "phase", "source_bytes")) == (contract["product_schema"], "PASS", phase, contract["edit"]["length_bytes"]), "product")

    first = product.get("base_publication", {})
    first_ok = is_id(first.get("root")) and tuple(first.get(key) for key in ("ordinal", "length", "route", "transactions", "commits", "q_terminal", "temp_residue")) == (1, 1048576, "InitialBuild", 1, 1, 0, 0) and is_count(first.get("q_high_water"))
    check(first_ok, "base-publication")
    fill = product.get("edits")
    fill_ok = type(fill) is list and len(fill) == plan.get("history_edits", 0) - 1
    revision_roots = {1: first.get("root")}
    if fill_ok:
        for offset, receipt in enumerate(fill):
            ordinal = offset + 2
            fill_ok &= receipt.get("ordinal") == ordinal and is_id(receipt.get("root")) and tuple(receipt.get(key) for key in ("length", "route", "transactions", "commits", "q_terminal", "temp_residue")) == (1048576, contract["edit"]["route"], 1, 1, 0, 0) and is_count(receipt.get("q_high_water"))
            revision_roots[ordinal] = receipt.get("root")
        fill_ok &= len(set(revision_roots.values())) == plan.get("history_edits")
    check(fill_ok, "history-fill-edits")

    points = product.get("checkpoints")
    point_ok = type(points) is list and tuple(point.get("revision") for point in points) == tuple(plan.get("checkpoint_revisions", ()))
    if point_ok:
        for point in points:
            revision = point["revision"]
            current, successor = revision_roots.get(revision), point.get("next_root")
            projection, store, resource = point.get("projection", {}), point.get("storage", {}), point.get("resource", {})
            point_ok &= point.get("root") == current and is_id(current) and point.get("length") == 1048576 and is_id(point.get("transition")) and is_id(point.get("output_digest")) and point.get("range_bytes") == 4096 and is_id(point.get("range_digest"))
            point_ok &= point.get("edit_to_revision") == revision + 1 and is_id(successor) and successor != current and is_id(point.get("next_transition")) and (revision == plan["history_edits"] or successor == revision_roots.get(revision + 1))
            point_ok &= operation_receipt(point.get("operations", {}).get("range"), 0, 0) and operation_receipt(point.get("operations", {}).get("same_size_edit"), 1, 1)
            policy = (projection.get("classification"), projection.get("exact_policy"), projection.get("exact_revision"), projection.get("exact_requested_root"), projection.get("exact_result_root"), projection.get("latest_policy"), projection.get("latest_revision"), projection.get("latest_requested_root"), projection.get("latest_result_root"), projection.get("latest_route"))
            point_ok &= policy == ("ExactThenLatestSparsePatch", "ExactEveryRoot", revision, current, current, "LatestFollowing", revision + 1, successor, successor, "SparsePatchAuthenticatedEdge")
            counters = tuple(projection.get(key) for key in ("submitted", "started", "published", "coalesced", "full_fallbacks", "range_fetches", "q_terminal", "temp_residue"))
            point_ok &= counters == (2, 2, 2, 0, 0, 1, 0, 0) and all(is_count(projection.get(key)) for key in ("clone_calls", "seed_rotations", "q_high_water", "max_buffer_bytes")) and 0 < projection.get("written_bytes", 0) <= 1048576 and projection.get("max_buffer_bytes", 1048577) <= 1048576
            point_ok &= all(is_count(store.get(key)) for key in ("logical_bytes", "apparent_bytes", "allocated_bytes", "live_objects", "unreachable_objects")) and store.get("live_objects", 0) > 0 and is_count(resource.get("q_high_water")) and is_count(resource.get("fd_count"))
    check(point_ok, "checkpoint-operations-projection")

    rebuilds = product.get("reconstructions")
    expected_rebuilds = []
    if type(points) is list:
        expected_rebuilds.extend((point.get("revision"), point.get("root"), point.get("output_digest"), "CompleteCheckpoint") for point in points)
        if points: expected_rebuilds.append((plan.get("history_edits", 0) + 1, points[-1].get("next_root"), None, "TerminalVerifiedNative"))
    rebuild_ok = type(rebuilds) is list and len(rebuilds) == contract["full_reconstruction"].get(phase + "_count", -1) == len(expected_rebuilds)
    if rebuild_ok:
        for actual, expected in zip(rebuilds, expected_rebuilds):
            revision, expected_root, expected_digest, scope = expected
            rebuild_ok &= tuple(actual.get(key) for key in ("revision", "root", "length", "scope")) == (revision, expected_root, 1048576, scope) and is_id(actual.get("output_digest")) and (expected_digest is None or actual.get("output_digest") == expected_digest)
    check(rebuild_ok, "full-reconstruction")

    aba = product.get("aba", {})
    last = points[-1] if point_ok and points else {}
    aba_ok = is_id(aba.get("root_a")) and (aba.get("root_a"), aba.get("root_b"), aba.get("final_root")) == (last.get("root"), last.get("next_root"), last.get("root")) and aba.get("root_a") != aba.get("root_b") and is_id(aba.get("final_transition")) and aba.get("identity_reused") is True and tuple(aba.get(key) for key in ("a_to_b_transactions", "a_to_b_commits", "b_to_a_transactions", "b_to_a_commits", "transactions", "commits", "q_terminal")) == (1, 1, 1, 1, 2, 2, 0) and all(is_count(aba.get(key)) for key in ("objects_created", "objects_reused", "logical_store_bytes_before", "logical_store_bytes_after", "q_high_water"))
    check(aba_ok, "aba-publications")
    historical = product.get("historical_read", {})
    check((historical.get("requested_root"), historical.get("bytes")) == (aba.get("root_b"), 4096) and is_id(historical.get("digest")), "historical-root-read")

    concurrent = product.get("concurrency", {})
    before_after = (concurrent.get("reader_one_before"), concurrent.get("reader_two_before"), concurrent.get("reader_one_current_head_after"), concurrent.get("reader_two_current_head_after"))
    concurrent_ok = concurrent.get("source_bytes") == 10485760 and concurrent.get("reader_model") == "OpenImmutableReadersBoundedScopesBeforeAndAfterCommitNoLiveStatementOrBlobAcrossCommit" and is_id(concurrent.get("prior_root")) and is_id(concurrent.get("new_root")) and concurrent.get("prior_root") != concurrent.get("new_root") and before_after == (concurrent.get("prior_root"), concurrent.get("prior_root"), concurrent.get("new_root"), concurrent.get("new_root")) and tuple(concurrent.get(key) for key in ("historical_range_bytes_after_commit", "writer_transactions", "writer_commits", "busy_errors", "locked_errors", "q_terminal")) == (4096, 1, 1, 0, 0, 0) and concurrent.get("sqlite_error_observation") == "ObservedNoSqliteErrorReturn" and is_count(concurrent.get("q_high_water"))
    check(concurrent_ok, "concurrency-sentinel")
    cache=contract["cache_profile"]; observed_pages=tuple(concurrent.get(key) for key in ("writer_cache_size_pages","reader_one_cache_size_pages","reader_two_cache_size_pages")); expected_pages=(cache["writer_pages"],cache["reader_one_pages"],cache["reader_two_pages"]); page_bytes=concurrent.get("sqlite_page_size_bytes"); reduction=concurrent.get("configured_cache_reduction_bytes"); connection_counts=(concurrent.get("simultaneous_connections_high_water"),concurrent.get("simultaneous_connections_terminal"))
    cache_ok=observed_pages==expected_pages and page_bytes==cache["page_bytes"] and sum(observed_pages)*page_bytes==cache["aggregate_cache_ceiling_bytes"] and (3*cache["default_pages"]-sum(observed_pages))*page_bytes==reduction==cache["aggregate_reduction_bytes"] and connection_counts==(cache["active_connections_high_water"],cache["active_connections_terminal"])
    check(cache_ok,"cache-profile-equation")

    end = product.get("terminal", {})
    end_counts = all(is_count(end.get(key)) for key in ("stored_objects", "current_live_objects", "current_unreachable_objects", "retained_live_objects", "retained_unreachable_objects", "logical_store_bytes", "apparent_store_bytes", "allocated_store_bytes", "q_high_water", "fd_before", "fd_after_store_close", "fd_after_cleanup"))
    end_ok = end.get("revision") == plan.get("history_edits", -2) + 2 and end.get("root") == aba.get("final_root") and is_id(end.get("transition")) and end.get("output_digest") == last.get("output_digest") and end.get("reachability") == "ReadOnlyNoGc" and end_counts and end.get("stored_objects", 0) > 0 and end.get("current_live_objects", 0) <= end.get("stored_objects", -1) and end.get("retained_live_objects", 0) <= end.get("stored_objects", -1) and end.get("q_terminal") == 0 and end.get("fd_before") == end.get("fd_after_cleanup") and end.get("descriptor_leak") is False and tuple(end.get(key) for key in ("seed_residue", "temp_residue", "work_root_residue")) == (0, 0, 0)
    check(end_ok, "terminal-product")
    check(is_count(product.get("max_buffer_bytes")) and product.get("max_buffer_bytes") <= contract["limits"]["buffer_bytes"] and type(row.get("maximum_resident_set_size")) is int and 0 < row["maximum_resident_set_size"] <= contract["limits"]["rss_bytes"], "resources")

    elapsed = row.get("complete_wall_ns")
    if stage == "preliminary": check((elapsed, row.get("lock_released"), row.get("terminal_work_roots")) == (None, None, None), "preliminary-scope")
    elif stage == "final": check(type(elapsed) is int and 0 <= elapsed <= plan.get("complete_wall_hard_limit_ns", -1) and row.get("lock_released") is True and row.get("terminal_work_roots") == 0, "complete-wall-through-lock-release")
    return {"status": "PASS" if not bad else "REVISE", "phase": phase, "analysis_stage": stage, "hard_failures": sorted(bad), "product_processes": row.get("product_processes"), "history_revisions": 1 + len(fill) if type(fill) is list else None, "checkpoint_revisions": [point.get("revision") for point in points] if type(points) is list else None, "terminal_revision": end.get("revision"), "aba_root": aba.get("final_root"), "cache_profile":{"writer_pages":observed_pages[0],"reader_one_pages":observed_pages[1],"reader_two_pages":observed_pages[2],"page_bytes":page_bytes,"aggregate_reduction_bytes":reduction,"connections_high":connection_counts[0],"connections_terminal":connection_counts[1]}, "rss_bytes": row.get("maximum_resident_set_size"), "complete_wall_ns": elapsed}

def main():
    source, target = map(pathlib.Path, sys.argv[1:3])
    normalized = audit(json.loads(source.read_text()), method("--self-check-authority" in sys.argv[3:]))
    report = {"schema": "phase4-g5-3-independent-v3", "normalized": normalized, "normalized_sha256": hashlib.sha256(canonical(normalized).encode()).hexdigest()}
    target.write_text(canonical(report) + "\n")

if __name__ == "__main__": main()
