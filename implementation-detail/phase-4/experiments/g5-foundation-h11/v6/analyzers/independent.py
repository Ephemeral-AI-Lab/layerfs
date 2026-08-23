#!/usr/bin/env python3
import json
import pathlib
import sys

HISTORIES = (1, 10, 100, 1000)
SAMPLES = (1, 2)
OPS = ("reopen_head", "head_lookup", "range_read", "reconstruction", "first_edit_after_reopen", "materialization")
PARITY = {
    "reopen_head": ("sql_queries", "sql_rows", "row_blob_reads", "row_blob_writes", "canonical_authenticated", "q_current"),
    "head_lookup": ("sql_queries", "sql_rows", "row_blob_reads", "row_blob_writes", "canonical_authenticated", "q_current"),
    "range_read": ("sql_queries", "sql_rows", "row_blob_reads", "row_blob_writes", "canonical_authenticated", "q_current"),
    "reconstruction": ("sql_queries", "sql_rows", "row_blob_reads", "row_blob_writes", "canonical_authenticated", "q_current"),
    "materialization": ("sql_queries", "sql_rows", "row_blob_reads", "row_blob_writes", "canonical_authenticated", "write_calls", "write_bytes", "data_sync_calls", "metadata_sync_calls", "rename_calls", "directory_sync_calls", "temp_files_created", "temp_files_removed", "q_current", "max_single_buffer_bytes"),
}
POINT_FIELDS = (
    "stored_objects", "stored_canonical_bytes", "stored_mapping_bytes",
    "current_live_objects", "current_live_canonical_bytes", "current_live_mapping_bytes",
    "retained_live_objects", "retained_live_canonical_bytes", "retained_live_mapping_bytes",
    "terminal_logical_store_bytes", "terminal_apparent_store_bytes", "terminal_allocated_store_bytes",
)


def goldens(path):
    result = {}
    for line in path.read_text(encoding="utf-8").splitlines()[1:]:
        value = line.split("\t")
        result[int(value[0])] = {
            "revision": int(value[0]),
            "root": value[1],
            "transition": value[2],
            "output_digest": value[4],
        }
    return result


def latency(index, operation):
    control_history = 10 if operation == "first_edit_after_reopen" else 1
    control = [index[(control_history, sample)][operation]["wall_ns"] for sample in SAMPLES]
    candidate = [index[(1000, sample)][operation]["wall_ns"] for sample in SAMPLES]
    control_sum, candidate_sum = sum(control), sum(candidate)
    relative = candidate_sum * 100 > control_sum * 105
    absolute = candidate_sum - control_sum >= 2_000_000
    return {
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


def recompute(raw_path, expected_path):
    records = [json.loads(line) for line in raw_path.read_text(encoding="utf-8").splitlines() if line]
    lookup = {(row["history_revisions"], row["sample"]): row for row in records}
    expected = goldens(expected_path)
    keys = {(history, sample) for history in HISTORIES for sample in SAMPLES}
    failures = set()
    if len(records) != 8 or set(lookup) != keys:
        failures.add("schedule_or_row_count")
    for history, sample in sorted(keys & set(lookup)):
        row = lookup[(history, sample)]
        final = expected[history + 1]
        checks = {
            "sample_status": row.get("status") == "PASS",
            "root": row.get("final_root") == final["root"],
            "transition": row.get("final_transition") == final["transition"],
            "digest": row.get("final_output_digest") == final["output_digest"],
            "history_count": len(row.get("history_edit_samples_ns", [])) == history - 1,
            "history_transactions": row.get("history_transactions") == history - 1,
            "history_commits": row.get("history_commits") == history - 1,
            "q": row.get("q_current") == 0,
            "buffer": row.get("materialization", {}).get("max_single_buffer_bytes", 2**63) <= 1_048_576,
            "fd": row.get("descriptor_leak") is False,
            "permit": row.get("permit_leak") is False,
            "seed": row.get("seed_residue") == 0,
            "temp": row.get("temp_residue") == 0,
            "rss": row.get("external_time", {}).get("maximum_resident_set_size", 2**63) <= 20_971_520,
            "retained_reachability": row.get("retained_unreachable_objects") == 0,
        }
        failures.update(f"h{history}s{sample}:{name}" for name, passed in checks.items() if not passed)
    for operation, fields in PARITY.items():
        for field in fields:
            if len({lookup[key][operation][field] for key in keys}) != 1:
                failures.add(f"work_parity:{operation}:{field}")
    edit_fields = ("sql_queries", "sql_rows", "row_blob_reads", "row_blob_writes", "canonical_authenticated", "mapping_rewritten", "transactions", "commits", "q_current")
    for field in edit_fields:
        if len({lookup[(1, sample)]["first_edit_after_reopen"][field] for sample in SAMPLES}) != 1:
            failures.add(f"genesis_work_pair:first_edit_after_reopen:{field}")
        if len({lookup[(history, sample)]["first_edit_after_reopen"][field] for history in (10, 100, 1000) for sample in SAMPLES}) != 1:
            failures.add(f"nongenenesis_work_parity:first_edit_after_reopen:{field}")
    latency_rows = {operation: latency(lookup, operation) for operation in OPS}
    failures.update(f"latency_material:{operation}" for operation, value in latency_rows.items() if value["product_material_regression"])

    points = {}
    allocated = {}
    for history in HISTORIES:
        pair = [lookup[(history, sample)] for sample in SAMPLES]
        for field in POINT_FIELDS:
            if field != "terminal_allocated_store_bytes" and pair[0][field] != pair[1][field]:
                failures.add(f"storage_pair:{history}:{field}")
        allocated[history] = [pair[0]["terminal_allocated_store_bytes"], pair[1]["terminal_allocated_store_bytes"]]
        if max(allocated[history]) - min(allocated[history]) > 1_048_576:
            failures.add(f"allocated_pair_spread:{history}")
        points[history] = {field: pair[0][field] for field in POINT_FIELDS}
    for field in ("current_live_objects", "current_live_canonical_bytes", "current_live_mapping_bytes"):
        if len({points[history][field] for history in HISTORIES}) != 1:
            failures.add(f"current_live_growth:{field}")
    control = dict(points[1])
    candidate = dict(points[1000])
    control["terminal_allocated_store_bytes"] = min(allocated[1])
    candidate["terminal_allocated_store_bytes"] = max(allocated[1000])
    storage = {
        "control": control,
        "candidate": candidate,
        "added_revisions": 999,
        "stored_objects_per_revision": (candidate["stored_objects"] - control["stored_objects"]) / 999,
        "stored_canonical_bytes_per_revision": (candidate["stored_canonical_bytes"] - control["stored_canonical_bytes"]) / 999,
        "stored_mapping_bytes_per_revision": (candidate["stored_mapping_bytes"] - control["stored_mapping_bytes"]) / 999,
        "logical_store_bytes_per_revision": (candidate["terminal_logical_store_bytes"] - control["terminal_logical_store_bytes"]) / 999,
        "apparent_store_bytes_per_revision": (candidate["terminal_apparent_store_bytes"] - control["terminal_apparent_store_bytes"]) / 999,
        "allocated_store_bytes_per_revision": (candidate["terminal_allocated_store_bytes"] - control["terminal_allocated_store_bytes"]) / 999,
        "allocated_raw": allocated,
    }
    for name, value, ceiling in (
        ("objects", storage["stored_objects_per_revision"], 16),
        ("canonical", storage["stored_canonical_bytes_per_revision"], 65_536),
        ("mapping", storage["stored_mapping_bytes_per_revision"], 8_192),
        ("allocated", storage["allocated_store_bytes_per_revision"], 131_072),
    ):
        if value > ceiling:
            failures.add(f"storage_slope:{name}")

    q_rows, tuple_rows, reopen_rows = {}, {}, {}
    for row in records:
        history, sample = row["history_revisions"], row["sample"]
        label = f"h{history}s{sample}"
        marker = row.get("q_terminal", {})
        q_rows[label] = {
            "high_water": row.get("q_high_water"),
            "terminal": marker.get("q_current"),
            "terminal_high_water": marker.get("q_high_water"),
        }
        if not (
            row.get("q_current") == 0
            and row.get("q_high_water", 0) >= 691_675
            and marker.get("status") == "PASS"
            and marker.get("q_current") == 0
            and marker.get("q_high_water") == row.get("q_high_water")
            and row.get("reachability_entry_q_bytes") == 64
        ):
            failures.add(f"{label}:whole_harness_q")
        revisions = {1, history, history // 2}
        revisions.update(point for point in HISTORIES if point <= history)
        wanted = [expected[revision] for revision in sorted(revisions - {0})]
        tuple_rows[label] = row.get("historical_tuples", [])
        if tuple_rows[label] != wanted:
            failures.add(f"{label}:historical_tuples")
        phase = row.get("reopen_phases", {})
        reopen_rows[label] = phase
        total = sum(phase.get(name, -1) for name in ("preflight_ns", "sqlite_open_profile_ns", "cache_profile_ns", "head_lookup_ns"))
        if total != phase.get("sum_ns") or total != row.get("reopen_head", {}).get("wall_ns") or not str(phase.get("sql_counter_scope", "")).startswith("partial-logical"):
            failures.add(f"{label}:reopen_phases")
        expected_history = {
            "history_objects_created": 6 * (history - 1),
            "history_objects_reused": 0,
            "history_canonical_new_bytes": 23_030 * (history - 1),
            "history_mapping_rewritten": 2_309 * (history - 1),
            "history_transactions": history - 1,
            "history_commits": history - 1,
        }
        failures.update(f"{label}:{name}" for name, value in expected_history.items() if row.get(name) != value)

    normalized = {
        "hard_failures": sorted(failures),
        "latency": latency_rows,
        "storage": storage,
        "whole_harness_q": q_rows,
        "historical_tuples": tuple_rows,
        "reopen_phases": reopen_rows,
        "reachability_entry_q_bytes": 64,
    }
    return {
        "schema": "phase4-g5-h11-independent-recomputation-v6",
        "status": "PASS" if not failures else "REVISE",
        "row_count": len(records),
        "materiality_rule": "candidate_sum*100>control_sum*105 AND candidate_sum-control_sum>=2000000",
        "normalized": normalized,
    }


if __name__ == "__main__":
    if len(sys.argv) != 4:
        raise SystemExit("usage: independent.py RAW EXPECTED OUTPUT")
    raw, expected, output = map(pathlib.Path, sys.argv[1:])
    result = recompute(raw, expected)
    output.write_text(json.dumps(result, sort_keys=True, separators=(",", ":")) + "\n", encoding="utf-8")
    raise SystemExit(0 if result["status"] == "PASS" else 1)




