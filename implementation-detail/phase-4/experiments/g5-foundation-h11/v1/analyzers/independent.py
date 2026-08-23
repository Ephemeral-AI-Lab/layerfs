#!/usr/bin/env python3
import json
import pathlib
import sys


HISTORIES = [1, 10, 100, 1000]
OPS = ["reopen_head", "head_lookup", "range_read", "reconstruction", "first_edit_after_reopen", "materialization"]
PARITY = {
    "reopen_head": ["sql_queries", "sql_rows", "row_blob_reads", "row_blob_writes", "canonical_authenticated", "q_current"],
    "head_lookup": ["sql_queries", "sql_rows", "row_blob_reads", "row_blob_writes", "canonical_authenticated", "q_current"],
    "range_read": ["sql_queries", "sql_rows", "row_blob_reads", "row_blob_writes", "canonical_authenticated", "q_current"],
    "reconstruction": ["sql_queries", "sql_rows", "row_blob_reads", "row_blob_writes", "canonical_authenticated", "q_current"],
    "first_edit_after_reopen": ["sql_queries", "sql_rows", "row_blob_reads", "row_blob_writes", "canonical_authenticated", "mapping_rewritten", "transactions", "commits", "q_current"],
    "materialization": ["sql_queries", "sql_rows", "row_blob_reads", "row_blob_writes", "canonical_authenticated", "write_calls", "write_bytes", "data_sync_calls", "metadata_sync_calls", "rename_calls", "directory_sync_calls", "temp_files_created", "temp_files_removed", "q_current", "max_single_buffer_bytes"],
}


def recompute(raw_name, expected_name):
    observations = [json.loads(text) for text in raw_name.read_text(encoding="utf-8").splitlines() if text.strip()]
    by_key = {(item["history_revisions"], item["sample"]): item for item in observations}
    expected = {}
    for text in expected_name.read_text(encoding="utf-8").splitlines()[1:]:
        values = text.split("\t")
        expected[int(values[0])] = (values[1], values[2], values[4])
    failures = set()
    keys = [(history, sample) for history in HISTORIES for sample in (1, 2)]
    if len(observations) != 8 or sorted(by_key) != sorted(keys):
        failures.add("schedule_or_row_count")
    for history, sample in keys:
        if (history, sample) not in by_key:
            continue
        item = by_key[(history, sample)]
        root, transition, digest = expected[history + 1]
        predicates = [
            ("sample_status", item.get("status") == "PASS"),
            ("root", item.get("final_root") == root),
            ("transition", item.get("final_transition") == transition),
            ("digest", item.get("final_output_digest") == digest),
            ("history_count", len(item.get("history_edit_samples_ns", [])) == history - 1),
            ("history_transactions", item.get("history_transactions") == history - 1),
            ("history_commits", item.get("history_commits") == history - 1),
            ("q", item.get("q_current") == 0),
            ("buffer", item["materialization"].get("max_single_buffer_bytes", 2**63) <= 1_048_576),
            ("fd", item.get("descriptor_leak") is False),
            ("permit", item.get("permit_leak") is False),
            ("seed", item.get("seed_residue") == 0),
            ("temp", item.get("temp_residue") == 0),
            ("rss", item.get("external_time", {}).get("maximum_resident_set_size", 2**63) <= 20_971_520),
            ("retained_reachability", item.get("retained_unreachable_objects") == 0),
        ]
        failures.update(f"h{history}s{sample}:{label}" for label, ok in predicates if not ok)
    for operation, names in PARITY.items():
        for name in names:
            if len({by_key[key][operation][name] for key in keys if key in by_key}) != 1:
                failures.add(f"work_parity:{operation}:{name}")
    latency = {}
    for operation in OPS:
        control = [by_key[(1, sample)][operation]["wall_ns"] for sample in (1, 2)]
        candidate = [by_key[(1000, sample)][operation]["wall_ns"] for sample in (1, 2)]
        control_sum, candidate_sum = sum(control), sum(candidate)
        relative = candidate_sum * 100 > control_sum * 105
        absolute = candidate_sum - control_sum >= 2_000_000
        latency[operation] = {
            "control_raw_ns": control,
            "candidate_raw_ns": candidate,
            "control_sum_ns": control_sum,
            "candidate_sum_ns": candidate_sum,
            "control_mean_ns": control_sum / 2,
            "candidate_mean_ns": candidate_sum / 2,
            "ratio": candidate_sum / control_sum,
            "absolute_delta_mean_ns": (candidate_sum - control_sum) / 2,
            "relative_branch_failed": relative,
            "absolute_branch_failed": absolute,
            "product_material_regression": relative and absolute,
        }
        if relative and absolute:
            failures.add(f"latency_material:{operation}")
    names = [
        "stored_objects", "stored_canonical_bytes", "stored_mapping_bytes",
        "current_live_objects", "current_live_canonical_bytes", "current_live_mapping_bytes",
        "retained_live_objects", "retained_live_canonical_bytes", "retained_live_mapping_bytes",
        "terminal_logical_store_bytes", "terminal_apparent_store_bytes", "terminal_allocated_store_bytes",
    ]
    points = {}
    for history in HISTORIES:
        first, second = by_key[(history, 1)], by_key[(history, 2)]
        for name in names:
            if first[name] != second[name]:
                failures.add(f"storage_pair:{history}:{name}")
        points[history] = {name: first[name] for name in names}
    for name in ["current_live_objects", "current_live_canonical_bytes", "current_live_mapping_bytes"]:
        if len({points[history][name] for history in HISTORIES}) != 1:
            failures.add(f"current_live_growth:{name}")
    base, last, count = points[1], points[1000], 999
    storage = {
        "control": base,
        "candidate": last,
        "added_revisions": count,
        "stored_objects_per_revision": (last["stored_objects"] - base["stored_objects"]) / count,
        "stored_canonical_bytes_per_revision": (last["stored_canonical_bytes"] - base["stored_canonical_bytes"]) / count,
        "stored_mapping_bytes_per_revision": (last["stored_mapping_bytes"] - base["stored_mapping_bytes"]) / count,
        "logical_store_bytes_per_revision": (last["terminal_logical_store_bytes"] - base["terminal_logical_store_bytes"]) / count,
        "apparent_store_bytes_per_revision": (last["terminal_apparent_store_bytes"] - base["terminal_apparent_store_bytes"]) / count,
        "allocated_store_bytes_per_revision": (last["terminal_allocated_store_bytes"] - base["terminal_allocated_store_bytes"]) / count,
    }
    for name, value, ceiling in [
        ("objects", storage["stored_objects_per_revision"], 16),
        ("canonical", storage["stored_canonical_bytes_per_revision"], 65_536),
        ("mapping", storage["stored_mapping_bytes_per_revision"], 8_192),
        ("allocated", storage["allocated_store_bytes_per_revision"], 131_072),
    ]:
        if value > ceiling:
            failures.add(f"storage_slope:{name}")
    normalized = {"hard_failures": sorted(failures), "latency": latency, "storage": storage}
    return {"schema": "phase4-g5-h11-independent-recomputation-v1", "status": "PASS" if not failures else "REVISE", "row_count": len(observations), "materiality_rule": "candidate_sum*100>control_sum*105 AND candidate_sum-control_sum>=2000000", "normalized": normalized}


if __name__ == "__main__":
    if len(sys.argv) != 4:
        raise SystemExit("usage: independent.py RAW EXPECTED OUTPUT")
    result = recompute(pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2]))
    pathlib.Path(sys.argv[3]).write_text(json.dumps(result, sort_keys=True, separators=(",", ":")) + "\n", encoding="utf-8")
    raise SystemExit(0 if result["status"] == "PASS" else 1)
