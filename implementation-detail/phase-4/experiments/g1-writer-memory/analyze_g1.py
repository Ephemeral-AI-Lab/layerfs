#!/usr/bin/env python3
"""Primary analysis for the one-shot Phase-4 G1 writer-memory screen."""

import json
import statistics
import sys
from pathlib import Path

CONTROL = "454bc2f3deacd8581a3cc352c8b7495215cdc103a85580606246ea12bb25eba8"
CANDIDATE = "42e3ddeb15df298c978b14639690e366fbb26ee55851524d42c6c3e9c0e8bd55"
PROFILE = "94a03ba7b6c97b5ff37c0ec62ef1d801b9896494b45456bd3df23e2cb278d13b"
SOURCE = "bb883eecf4ea85d80432953791dcc352243da94175e7503e2c476afe9bd0bab7"
SEQUENCE = "5bf63ef8adad7bb373be4e968759997b385590d076d75a1488e1958b64a3f8e2"
ROOT = "93d1b461b5cbf88e8d122ad4e90a15a6e3029c408f0fe02ef51c561e5a94c6d1"
TRANSITION = "2de8d2ce6b614373ba8fb8b29d3a3eccd535abfccc2654e7922514e5ca90fd89"
CLOSURE = "29233d6018b031f6035c8e2c8175f1ab86fb721808f21a8858bc965567e9c0c1"
ORDERS = ("AB", "BA", "AB", "BA")


def schedule():
    rows = []
    for position, arm in enumerate("AB", 1):
        rows.append(("warmup", 0, "AB", position, arm))
    for pair, order in enumerate(ORDERS, 1):
        for position, arm in enumerate(order, 1):
            rows.append(("measured", pair, order, position, arm))
    return rows


def center(rows, arm, field):
    positions = []
    for position in (1, 2):
        values = [row[field] for row in rows if row["arm"] == arm and row["position"] == position]
        positions.append(statistics.mean(values))
    return statistics.mean(positions)


def ratio(candidate, control):
    return candidate / control


def monotonic_drift(rows, arm, field):
    values = [
        next(row[field] for row in rows if row["pair"] == pair and row["arm"] == arm)
        for pair in range(1, 5)
    ]
    monotonic = all(a < b for a, b in zip(values, values[1:])) or all(
        a > b for a, b in zip(values, values[1:])
    )
    material = abs(values[-1] - values[0]) / statistics.mean(values) > 0.05
    return {"values": values, "monotonic": monotonic, "material": material, "pass": not (monotonic and material)}


def main():
    result_root = Path(sys.argv[1])
    raw_path = result_root / "rows-v1/G1-RAW-v1.jsonl"
    rows = [json.loads(line) for line in raw_path.read_text().splitlines() if line]
    hard_failures = []
    nogo_reasons = []
    expected_schedule = schedule()

    if len(rows) != 10:
        hard_failures.append(f"row count {len(rows)} != 10")
    for index, expected in enumerate(expected_schedule):
        if index >= len(rows):
            break
        row = rows[index]
        actual = (row.get("kind"), row.get("pair"), row.get("order"), row.get("position"), row.get("arm"))
        if actual != expected or row.get("sequence") != index + 1:
            hard_failures.append(f"schedule row {index + 1}: {actual} != {expected}")

    exact = {
        "status": "PASS",
        "size_bytes": 104_857_600,
        "input_size_bytes": 104_857_600,
        "source_fingerprint": SOURCE,
        "expected_cdc_references": 5_284,
        "actual_cdc_references": 5_284,
        "expected_cdc_sequence_fingerprint": SEQUENCE,
        "root_id": ROOT,
        "transition_id": TRANSITION,
        "ordered_closure_digest": CLOSURE,
        "profile_id": PROFILE,
        "objects_created": 5_372,
        "objects_reused": 0,
        "canonical_bytes_written": 105_122_466,
        "mapping_bytes_rewritten": 196_174,
        "sql_calls": 5_381,
        "row_blob_writes": 10_748,
        "transactions": 1,
        "commits": 1,
        "commit_dispatches": 1,
        "commit_returns": 1,
        "commit_return_successes": 1,
        "commit_return_errors": 0,
        "q_high_water": 86_181,
        "q_current": 0,
        "sqlite_runtime_journal_mode": "delete",
        "sqlite_runtime_synchronous": 2,
        "sqlite_runtime_temp_store": 1,
        "sqlite_runtime_mmap_size": 0,
        "sqlite_page_size_bytes": 4_096,
        "sqlite_post_logical_database_bytes": 109_199_360,
        "sqlite_post_apparent_database_bytes": 109_199_360,
        "sqlite_post_logical_store_bytes": 109_199_392,
        "sqlite_post_apparent_store_bytes": 109_199_392,
        "publication_status": "Committed",
        "error": None,
    }
    for row in rows:
        label = row.get("label", "unknown")
        for field, expected in exact.items():
            if row.get(field) != expected:
                hard_failures.append(f"{label}: {field}={row.get(field)!r} != {expected!r}")
        expected_binary = CONTROL if row.get("arm") == "A" else CANDIDATE
        if row.get("binary_sha256") != expected_binary or row.get("executable_sha256") != expected_binary:
            hard_failures.append(f"{label}: executable custody mismatch")
        for field in ("durable_phase_sum_matches", "lifecycle_phase_sum_matches", "commit_timer_equation_matches"):
            if row.get(field) is not True:
                hard_failures.append(f"{label}: {field} failed")
        if row.get("durable_capture_total_wall_ns") != row.get("durable_phase_sum_ns"):
            hard_failures.append(f"{label}: durable timer equation differs")
        if row.get("sqlite_main_db_pager_write_bytes") != row.get("sqlite_main_db_dirty_pages_written", -1) * 4_096:
            hard_failures.append(f"{label}: pager-byte equation differs")
        if row.get("residue_files") != []:
            hard_failures.append(f"{label}: SQLite residue {row.get('residue_files')}")
        custody = row.get("common_base_custody", {})
        if not custody or not all(value.get("bytes_unchanged") and value.get("distinct_inode") for value in custody.values()):
            hard_failures.append(f"{label}: common-base custody failed")
        if row.get("post_run_file_modes_octal") != {"database": "0600", "authority": "0600", "expectations": "0400"}:
            hard_failures.append(f"{label}: post-run modes differ")

    measured = [row for row in rows if row.get("kind") == "measured"]
    if len(measured) != 8:
        hard_failures.append(f"measured row count {len(measured)} != 8")

    fields = [
        "durable_capture_total_wall_ns",
        "canonical_cas_mapping_stage_wall_ns",
        "precommit_closure_validation_wall_ns",
        "sqlite_commit_durability_wall_ns",
        "commit_dispatch_to_return_wall_ns",
        "maximum_resident_set_bytes",
        "peak_memory_footprint_bytes",
        "sqlite_page_cache_snapshot_max_bytes",
        "sqlite_main_db_dirty_pages_written",
        "sqlite_cache_spill_pages",
        "sqlite_main_db_pager_write_bytes",
        "sqlite_post_allocated_store_bytes",
        "user_seconds",
        "system_seconds",
    ]
    centers = {
        field: {arm: center(measured, arm, field) for arm in "AB"}
        for field in fields
    } if len(measured) == 8 else {}

    pair_results = []
    for pair in range(1, 5):
        pair_rows = {row["arm"]: row for row in measured if row["pair"] == pair}
        if set(pair_rows) != {"A", "B"}:
            hard_failures.append(f"pair {pair}: missing arm")
            continue
        control = pair_rows["A"]["durable_capture_total_wall_ns"]
        candidate = pair_rows["B"]["durable_capture_total_wall_ns"]
        pair_results.append({
            "pair": pair,
            "order": pair_rows["A"]["order"],
            "control_ns": control,
            "candidate_ns": candidate,
            "ratio": ratio(candidate, control),
            "within_5_percent": ratio(candidate, control) <= 1.05,
        })

    position_results = []
    for position in (1, 2):
        control = statistics.mean(row["durable_capture_total_wall_ns"] for row in measured if row["arm"] == "A" and row["position"] == position)
        candidate = statistics.mean(row["durable_capture_total_wall_ns"] for row in measured if row["arm"] == "B" and row["position"] == position)
        position_results.append({"position": position, "control_ns": control, "candidate_ns": candidate, "ratio": ratio(candidate, control), "within_5_percent": ratio(candidate, control) <= 1.05})

    stats = {}
    if centers:
        stats = {
            "wall_ratio": ratio(centers["durable_capture_total_wall_ns"]["B"], centers["durable_capture_total_wall_ns"]["A"]),
            "cache_ratio": ratio(centers["sqlite_page_cache_snapshot_max_bytes"]["B"], centers["sqlite_page_cache_snapshot_max_bytes"]["A"]),
            "rss_ratio": ratio(centers["maximum_resident_set_bytes"]["B"], centers["maximum_resident_set_bytes"]["A"]),
            "footprint_ratio": ratio(centers["peak_memory_footprint_bytes"]["B"], centers["peak_memory_footprint_bytes"]["A"]),
            "dirty_write_ratio": ratio(centers["sqlite_main_db_dirty_pages_written"]["B"], centers["sqlite_main_db_dirty_pages_written"]["A"]),
            "spill_ratio": ratio(centers["sqlite_cache_spill_pages"]["B"], centers["sqlite_cache_spill_pages"]["A"]),
            "allocated_store_ratio": ratio(centers["sqlite_post_allocated_store_bytes"]["B"], centers["sqlite_post_allocated_store_bytes"]["A"]),
        }
        if stats["wall_ratio"] > 1.05:
            nogo_reasons.append("position-balanced durable wall exceeds 1.05")
        if sum(result["within_5_percent"] for result in pair_results) < 3:
            nogo_reasons.append("fewer than 3/4 wall pairs are within 5%")
        if not all(result["within_5_percent"] for result in position_results):
            nogo_reasons.append("an execution position exceeds 5% wall")
        drift = {arm: monotonic_drift(measured, arm, "durable_capture_total_wall_ns") for arm in "AB"}
        if not all(value["pass"] for value in drift.values()):
            nogo_reasons.append("material monotonic wall drift")
        if stats["cache_ratio"] > 0.5:
            nogo_reasons.append("SQLite cache snapshot reduction is below 50%")
        if stats["rss_ratio"] > 0.5:
            nogo_reasons.append("maximum RSS reduction is below 50%")
        if stats["footprint_ratio"] >= 1 or abs(stats["footprint_ratio"] - stats["rss_ratio"]) > 0.10:
            nogo_reasons.append("peak footprint contradicts RSS movement")
        if stats["dirty_write_ratio"] > 1.10:
            nogo_reasons.append("dirty-page writes increased by more than 10%")
        if not (stats["spill_ratio"] > 1 and stats["cache_ratio"] < 1):
            nogo_reasons.append("expected spill-up/cache-down mechanism was not observed")
        allocated_pairs = []
        for pair in range(1, 5):
            pair_rows = {row["arm"]: row for row in measured if row["pair"] == pair}
            allocated_pairs.append(ratio(pair_rows["B"]["sqlite_post_allocated_store_bytes"], pair_rows["A"]["sqlite_post_allocated_store_bytes"]))
        allocated_positions = []
        for position in (1, 2):
            control = statistics.mean(row["sqlite_post_allocated_store_bytes"] for row in measured if row["arm"] == "A" and row["position"] == position)
            candidate = statistics.mean(row["sqlite_post_allocated_store_bytes"] for row in measured if row["arm"] == "B" and row["position"] == position)
            allocated_positions.append(ratio(candidate, control))
        if stats["allocated_store_ratio"] > 1.05 or sum(value <= 1.05 for value in allocated_pairs) < 3 or any(value > 1.05 for value in allocated_positions) or all(value > 1 for value in allocated_pairs):
            hard_failures.append("paired allocated-store gate failed")
    else:
        drift = {}

    if hard_failures:
        status = "FAIL"
        disposition = "G1 FAILURE"
        retain = False
    elif nogo_reasons:
        status = "PASS"
        disposition = "G1 NO-GO / RETAIN PREDECESSOR"
        retain = False
    else:
        status = "PASS"
        disposition = "G1 MEASURED PASS / STATIC CLOSURE REQUIRED"
        retain = True

    output = {
        "schema": "phase4-g1-writer-memory-analysis-v1",
        "status": status,
        "disposition": disposition,
        "retain_candidate_after_static_closure": retain,
        "rows": len(rows),
        "measured_rows": len(measured),
        "hard_failures": hard_failures,
        "nogo_reasons": nogo_reasons,
        "centers": centers,
        "statistics": stats,
        "pair_results": pair_results,
        "position_results": position_results,
        "drift": drift,
    }
    path = result_root / "G1-ANALYSIS-v1.json"
    path.write_text(json.dumps(output, indent=2, sort_keys=True) + "\n")
    print(json.dumps({"status": status, "disposition": disposition}, sort_keys=True))
    return 0 if status == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
