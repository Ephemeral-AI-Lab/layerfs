#!/usr/bin/env python3
"""Independent raw-row recomputation for Phase-4 G1."""

import json
import statistics
import sys
from pathlib import Path


def balanced(rows, arm, key):
    return statistics.mean(
        statistics.mean(row[key] for row in rows if row["arm"] == arm and row["position"] == position)
        for position in (1, 2)
    )


def main():
    root = Path(sys.argv[1])
    rows = [json.loads(line) for line in (root / "rows-v1/G1-RAW-v1.jsonl").read_text().splitlines() if line]
    failures = []
    nogo = []
    expected = [("warmup", 0, "AB", 1, "A"), ("warmup", 0, "AB", 2, "B")]
    for pair, order in enumerate(("AB", "BA", "AB", "BA"), 1):
        expected.extend(("measured", pair, order, position, arm) for position, arm in enumerate(order, 1))
    actual = [(r.get("kind"), r.get("pair"), r.get("order"), r.get("position"), r.get("arm")) for r in rows]
    if actual != expected or [r.get("sequence") for r in rows] != list(range(1, 11)):
        failures.append("chronology or schedule mismatch")
    measured = [r for r in rows if r.get("kind") == "measured"]
    if len(rows) != 10 or len(measured) != 8:
        failures.append("row count mismatch")

    constants = {
        "status": {"PASS"}, "profile_id": {"94a03ba7b6c97b5ff37c0ec62ef1d801b9896494b45456bd3df23e2cb278d13b"},
        "size_bytes": {104857600}, "input_size_bytes": {104857600}, "actual_cdc_references": {5284},
        "objects_created": {5372}, "objects_reused": {0}, "canonical_bytes_written": {105122466},
        "mapping_bytes_rewritten": {196174}, "sql_calls": {5381}, "row_blob_writes": {10748},
        "transactions": {1}, "commits": {1}, "commit_dispatches": {1}, "commit_returns": {1},
        "commit_return_successes": {1}, "commit_return_errors": {0}, "q_high_water": {86181},
        "q_current": {0}, "sqlite_page_size_bytes": {4096}, "sqlite_runtime_journal_mode": {"delete"},
        "sqlite_runtime_synchronous": {2}, "sqlite_runtime_temp_store": {1}, "sqlite_runtime_mmap_size": {0},
        "sqlite_post_logical_database_bytes": {109199360}, "sqlite_post_apparent_database_bytes": {109199360},
        "sqlite_post_logical_store_bytes": {109199392}, "sqlite_post_apparent_store_bytes": {109199392},
        "publication_status": {"Committed"}, "error": {None},
    }
    for key, allowed in constants.items():
        observed = {row.get(key) for row in rows}
        if observed != allowed:
            failures.append(f"{key} invariant mismatch")
    identities = [
        ("source_fingerprint", "bb883eecf4ea85d80432953791dcc352243da94175e7503e2c476afe9bd0bab7"),
        ("expected_cdc_sequence_fingerprint", "5bf63ef8adad7bb373be4e968759997b385590d076d75a1488e1958b64a3f8e2"),
        ("root_id", "93d1b461b5cbf88e8d122ad4e90a15a6e3029c408f0fe02ef51c561e5a94c6d1"),
        ("transition_id", "2de8d2ce6b614373ba8fb8b29d3a3eccd535abfccc2654e7922514e5ca90fd89"),
        ("ordered_closure_digest", "29233d6018b031f6035c8e2c8175f1ab86fb721808f21a8858bc965567e9c0c1"),
    ]
    for key, value in identities:
        if any(row.get(key) != value for row in rows):
            failures.append(f"{key} identity mismatch")
    for row in rows:
        if not all(row.get(key) is True for key in ("durable_phase_sum_matches", "lifecycle_phase_sum_matches", "commit_timer_equation_matches")):
            failures.append(f"{row.get('label')}: timer mismatch")
        if row.get("sqlite_main_db_pager_write_bytes") != row.get("sqlite_main_db_dirty_pages_written", -1) * 4096:
            failures.append(f"{row.get('label')}: pager equation mismatch")
        if row.get("residue_files") or row.get("post_run_file_modes_octal") != {"database": "0600", "authority": "0600", "expectations": "0400"}:
            failures.append(f"{row.get('label')}: residue or mode mismatch")
        custody = row.get("common_base_custody", {})
        if len(custody) != 3 or not all(v.get("bytes_unchanged") and v.get("distinct_inode") for v in custody.values()):
            failures.append(f"{row.get('label')}: custody mismatch")

    stats = {}
    if len(measured) == 8:
        keys = {
            "wall_ratio": "durable_capture_total_wall_ns", "cache_ratio": "sqlite_page_cache_snapshot_max_bytes",
            "rss_ratio": "maximum_resident_set_bytes", "footprint_ratio": "peak_memory_footprint_bytes",
            "dirty_write_ratio": "sqlite_main_db_dirty_pages_written", "spill_ratio": "sqlite_cache_spill_pages",
            "allocated_store_ratio": "sqlite_post_allocated_store_bytes",
        }
        stats = {name: balanced(measured, "B", field) / balanced(measured, "A", field) for name, field in keys.items()}
        pairs = []
        allocated = []
        for pair in range(1, 5):
            arms = {row["arm"]: row for row in measured if row["pair"] == pair}
            pairs.append(arms["B"]["durable_capture_total_wall_ns"] / arms["A"]["durable_capture_total_wall_ns"])
            allocated.append(arms["B"]["sqlite_post_allocated_store_bytes"] / arms["A"]["sqlite_post_allocated_store_bytes"])
        positions = []
        allocated_positions = []
        for position in (1, 2):
            a = statistics.mean(r["durable_capture_total_wall_ns"] for r in measured if r["arm"] == "A" and r["position"] == position)
            b = statistics.mean(r["durable_capture_total_wall_ns"] for r in measured if r["arm"] == "B" and r["position"] == position)
            positions.append(b / a)
            aa = statistics.mean(r["sqlite_post_allocated_store_bytes"] for r in measured if r["arm"] == "A" and r["position"] == position)
            bb = statistics.mean(r["sqlite_post_allocated_store_bytes"] for r in measured if r["arm"] == "B" and r["position"] == position)
            allocated_positions.append(bb / aa)
        drift_pass = True
        for arm in "AB":
            values = [next(r["durable_capture_total_wall_ns"] for r in measured if r["pair"] == pair and r["arm"] == arm) for pair in range(1, 5)]
            monotonic = all(x < y for x, y in zip(values, values[1:])) or all(x > y for x, y in zip(values, values[1:]))
            if monotonic and abs(values[-1] - values[0]) / statistics.mean(values) > 0.05:
                drift_pass = False
        if stats["wall_ratio"] > 1.05 or sum(value <= 1.05 for value in pairs) < 3 or any(value > 1.05 for value in positions) or not drift_pass:
            nogo.append("durable wall protection failed")
        if stats["cache_ratio"] > 0.5:
            nogo.append("cache reduction failed")
        if stats["rss_ratio"] > 0.5:
            nogo.append("RSS reduction failed")
        if stats["footprint_ratio"] >= 1 or abs(stats["footprint_ratio"] - stats["rss_ratio"]) > 0.10:
            nogo.append("footprint contradiction")
        if stats["dirty_write_ratio"] > 1.10:
            nogo.append("dirty-write amplification")
        if stats["spill_ratio"] <= 1 or stats["cache_ratio"] >= 1:
            nogo.append("mechanism direction failed")
        if stats["allocated_store_ratio"] > 1.05 or sum(value <= 1.05 for value in allocated) < 3 or any(value > 1.05 for value in allocated_positions) or all(value > 1 for value in allocated):
            failures.append("allocated-store protection failed")

    if failures:
        status, disposition = "FAIL", "G1 FAILURE"
    elif nogo:
        status, disposition = "PASS", "G1 NO-GO / RETAIN PREDECESSOR"
    else:
        status, disposition = "PASS", "G1 MEASURED PASS / STATIC CLOSURE REQUIRED"
    output = {"schema": "phase4-g1-writer-memory-independent-recomputation-v1", "status": status, "disposition": disposition, "rows": len(rows), "measured_rows": len(measured), "hard_failures": failures, "nogo_reasons": nogo, "statistics": stats}
    (root / "INDEPENDENT-RECOMPUTATION-v1.json").write_text(json.dumps(output, indent=2, sort_keys=True) + "\n")
    print(json.dumps({"status": status, "disposition": disposition}, sort_keys=True))
    return 0 if status == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
