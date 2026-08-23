#!/usr/bin/env python3
import json
import pathlib
import sys


HISTORIES = (1, 10, 100, 1000)
SAMPLES = (1, 2)
OPERATIONS = (
    "reopen_head",
    "head_lookup",
    "range_read",
    "reconstruction",
    "first_edit_after_reopen",
    "materialization",
)
WORK_FIELDS = {
    "reopen_head": ("sql_queries", "sql_rows", "row_blob_reads", "row_blob_writes", "canonical_authenticated", "q_current"),
    "head_lookup": ("sql_queries", "sql_rows", "row_blob_reads", "row_blob_writes", "canonical_authenticated", "q_current"),
    "range_read": ("sql_queries", "sql_rows", "row_blob_reads", "row_blob_writes", "canonical_authenticated", "q_current"),
    "reconstruction": ("sql_queries", "sql_rows", "row_blob_reads", "row_blob_writes", "canonical_authenticated", "q_current"),
    "first_edit_after_reopen": ("sql_queries", "sql_rows", "row_blob_reads", "row_blob_writes", "canonical_authenticated", "mapping_rewritten", "transactions", "commits", "q_current"),
    "materialization": ("sql_queries", "sql_rows", "row_blob_reads", "row_blob_writes", "canonical_authenticated", "write_calls", "write_bytes", "data_sync_calls", "metadata_sync_calls", "rename_calls", "directory_sync_calls", "temp_files_created", "temp_files_removed", "q_current", "max_single_buffer_bytes"),
}


def load_rows(raw_path):
    rows = [json.loads(line) for line in raw_path.read_text(encoding="utf-8").splitlines() if line]
    return rows, {(row["history_revisions"], row["sample"]): row for row in rows}


def expected_rows(path):
    rows = {}
    for line in path.read_text(encoding="utf-8").splitlines()[1:]:
        fields = line.split("\t")
        rows[int(fields[0])] = {"root": fields[1], "transition": fields[2], "output_digest": fields[4]}
    return rows


def compare_latency(index, operation):
    control = [index[(1, sample)][operation]["wall_ns"] for sample in SAMPLES]
    candidate = [index[(1000, sample)][operation]["wall_ns"] for sample in SAMPLES]
    control_sum = sum(control)
    candidate_sum = sum(candidate)
    relative_failed = candidate_sum * 100 > control_sum * 105
    absolute_failed = candidate_sum - control_sum >= 2_000_000
    return {
        "control_raw_ns": control,
        "candidate_raw_ns": candidate,
        "control_sum_ns": control_sum,
        "candidate_sum_ns": candidate_sum,
        "control_mean_ns": control_sum / 2,
        "candidate_mean_ns": candidate_sum / 2,
        "ratio": candidate_sum / control_sum,
        "absolute_delta_mean_ns": (candidate_sum - control_sum) / 2,
        "relative_branch_failed": relative_failed,
        "absolute_branch_failed": absolute_failed,
        "product_material_regression": relative_failed and absolute_failed,
    }


def analyze(raw_path, expected_path):
    rows, index = load_rows(raw_path)
    golden = expected_rows(expected_path)
    failures = []
    expected_keys = {(history, sample) for history in HISTORIES for sample in SAMPLES}
    if len(rows) != 8 or set(index) != expected_keys:
        failures.append("schedule_or_row_count")
    for key in sorted(expected_keys & set(index)):
        history, _sample = key
        row = index[key]
        gate = golden[history + 1]
        checks = {
            "sample_status": row.get("status") == "PASS",
            "root": row.get("final_root") == gate["root"],
            "transition": row.get("final_transition") == gate["transition"],
            "digest": row.get("final_output_digest") == gate["output_digest"],
            "history_count": len(row.get("history_edit_samples_ns", [])) == history - 1,
            "history_transactions": row.get("history_transactions") == history - 1,
            "history_commits": row.get("history_commits") == history - 1,
            "q": row.get("q_current") == 0,
            "buffer": row["materialization"].get("max_single_buffer_bytes", 2**63) <= 1_048_576,
            "fd": row.get("descriptor_leak") is False,
            "permit": row.get("permit_leak") is False,
            "seed": row.get("seed_residue") == 0,
            "temp": row.get("temp_residue") == 0,
            "rss": row.get("external_time", {}).get("maximum_resident_set_size", 2**63) <= 20_971_520,
            "retained_reachability": row.get("retained_unreachable_objects") == 0,
        }
        failures.extend(f"h{history}s{key[1]}:{name}" for name, passed in checks.items() if not passed)
    for operation, fields in WORK_FIELDS.items():
        for field in fields:
            values = {index[key][operation][field] for key in expected_keys if key in index}
            if len(values) != 1:
                failures.append(f"work_parity:{operation}:{field}")
    latency = {operation: compare_latency(index, operation) for operation in OPERATIONS}
    failures.extend(f"latency_material:{operation}" for operation, result in latency.items() if result["product_material_regression"])
    points = {}
    for history in HISTORIES:
        pair = [index[(history, sample)] for sample in SAMPLES]
        for field in (
            "stored_objects",
            "stored_canonical_bytes",
            "stored_mapping_bytes",
            "current_live_objects",
            "current_live_canonical_bytes",
            "current_live_mapping_bytes",
            "retained_live_objects",
            "retained_live_canonical_bytes",
            "retained_live_mapping_bytes",
            "terminal_logical_store_bytes",
            "terminal_apparent_store_bytes",
            "terminal_allocated_store_bytes",
        ):
            if pair[0][field] != pair[1][field]:
                failures.append(f"storage_pair:{history}:{field}")
        points[history] = {field: pair[0][field] for field in (
            "stored_objects", "stored_canonical_bytes", "stored_mapping_bytes",
            "current_live_objects", "current_live_canonical_bytes", "current_live_mapping_bytes",
            "retained_live_objects", "retained_live_canonical_bytes", "retained_live_mapping_bytes",
            "terminal_logical_store_bytes", "terminal_apparent_store_bytes", "terminal_allocated_store_bytes",
        )}
    for field in ("current_live_objects", "current_live_canonical_bytes", "current_live_mapping_bytes"):
        if len({points[history][field] for history in HISTORIES}) != 1:
            failures.append(f"current_live_growth:{field}")
    denominator = 999
    storage = {
        "control": points[1],
        "candidate": points[1000],
        "added_revisions": denominator,
        "stored_objects_per_revision": (points[1000]["stored_objects"] - points[1]["stored_objects"]) / denominator,
        "stored_canonical_bytes_per_revision": (points[1000]["stored_canonical_bytes"] - points[1]["stored_canonical_bytes"]) / denominator,
        "stored_mapping_bytes_per_revision": (points[1000]["stored_mapping_bytes"] - points[1]["stored_mapping_bytes"]) / denominator,
        "logical_store_bytes_per_revision": (points[1000]["terminal_logical_store_bytes"] - points[1]["terminal_logical_store_bytes"]) / denominator,
        "apparent_store_bytes_per_revision": (points[1000]["terminal_apparent_store_bytes"] - points[1]["terminal_apparent_store_bytes"]) / denominator,
        "allocated_store_bytes_per_revision": (points[1000]["terminal_allocated_store_bytes"] - points[1]["terminal_allocated_store_bytes"]) / denominator,
    }
    if storage["stored_objects_per_revision"] > 16:
        failures.append("storage_slope:objects")
    if storage["stored_canonical_bytes_per_revision"] > 65_536:
        failures.append("storage_slope:canonical")
    if storage["stored_mapping_bytes_per_revision"] > 8_192:
        failures.append("storage_slope:mapping")
    if storage["allocated_store_bytes_per_revision"] > 131_072:
        failures.append("storage_slope:allocated")
    normalized = {"hard_failures": sorted(set(failures)), "latency": latency, "storage": storage}
    return {
        "schema": "phase4-g5-h11-primary-analysis-v1",
        "status": "PASS" if not failures else "REVISE",
        "row_count": len(rows),
        "materiality_rule": "candidate_sum*100>control_sum*105 AND candidate_sum-control_sum>=2000000",
        "normalized": normalized,
    }


if __name__ == "__main__":
    if len(sys.argv) != 4:
        raise SystemExit("usage: primary.py RAW EXPECTED OUTPUT")
    result = analyze(pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2]))
    pathlib.Path(sys.argv[3]).write_text(json.dumps(result, sort_keys=True, separators=(",", ":")) + "\n", encoding="utf-8")
    raise SystemExit(0 if result["status"] == "PASS" else 1)
