#!/usr/bin/env python3
"""Deterministic primary analysis for the prospective G2 diagnostic."""

import argparse
import json
import statistics
from pathlib import Path

ROOT = "93d1b461b5cbf88e8d122ad4e90a15a6e3029c408f0fe02ef51c561e5a94c6d1"
TRANSITION = "2de8d2ce6b614373ba8fb8b29d3a3eccd535abfccc2654e7922514e5ca90fd89"
CLOSURE = "29233d6018b031f6035c8e2c8175f1ab86fb721808f21a8858bc965567e9c0c1"
SOURCE = "bb883eecf4ea85d80432953791dcc352243da94175e7503e2c476afe9bd0bab7"
SEQUENCE = "5bf63ef8adad7bb373be4e968759997b385590d076d75a1488e1958b64a3f8e2"
FAMILIES = (
    "sqlite_blob_acquisition_wall_ns",
    "canonical_authentication_wall_ns",
    "mapping_validation_wall_ns",
    "closure_commitment_wall_ns",
    "occurrence_commitment_wall_ns",
    "source_fingerprint_wall_ns",
    "secondary_bytes_decode_wall_ns",
)
# Only the second Bytes framing decode is statically removable without weakening
# identity or output authority. A following candidate still requires a new preregistration.
REMOVABLE = {name: name == "secondary_bytes_decode_wall_ns" for name in FAMILIES}


def read_rows(results):
    raw = results / "rows-v1/G2-RAW-v1.jsonl"
    return [json.loads(line) for line in raw.read_text().splitlines() if line]


def phase(row, name):
    return next(item for item in row["phase_counters"] if item["phase"] == name)


def parity_failures(row):
    failures = []
    expected = {
        "status": "PASS",
        "source_fingerprint": SOURCE,
        "q_current": 0,
    }
    if row["operation"] != "same-middle":
        expected.update({
            "root_id": ROOT,
            "transition_id": TRANSITION,
            "ordered_closure_digest": CLOSURE,
            "expected_cdc_sequence_fingerprint": SEQUENCE,
            "actual_cdc_references": 5284,
        })
    for key, value in expected.items():
        if row.get(key) != value:
            failures.append(f"{row['label']}:{key}")
    expected_transactions = 1 if row["operation"] == "same-middle" else 0
    if row.get("transactions") != expected_transactions or row.get("commits") != expected_transactions:
        failures.append(f"{row['label']}:read-transaction-shape")
    if row.get("allocated_store_delta_bytes") != 0:
        failures.append(f"{row['label']}:allocated-store-delta")
    if row.get("residue_files"):
        failures.append(f"{row['label']}:residue")
    if row["operation"] != "same-middle" and row.get("pre_edit_database_sha256") != row.get("post_database_sha256"):
        failures.append(f"{row['label']}:database-bytes-changed")
    if row.get("pre_edit_authority_sha256") != row.get("post_authority_sha256"):
        failures.append(f"{row['label']}:authority-bytes-changed")
    if row.get("pre_edit_expectations_sha256") != row.get("post_expectations_sha256"):
        failures.append(f"{row['label']}:expectations-bytes-changed")
    return failures


def primary_analysis(rows, observer):
    primary = [row for row in rows if row["workload"] == "primary"]
    measured = [row for row in primary if row["kind"] == "measured"]
    failures = []
    if len(primary) != 10 or len(measured) != 8:
        failures.append("primary-row-count")
    for row in primary:
        failures.extend(parity_failures(row))

    pairs = []
    for pair_id in range(1, 5):
        pair = [row for row in measured if row["pair"] == pair_id]
        if len(pair) != 2 or {row["arm"] for row in pair} != {"A", "B"}:
            failures.append(f"pair-{pair_id}:shape")
            continue
        by_arm = {row["arm"]: row for row in pair}
        ratio = by_arm["B"]["reconstruction_wall_ns"] / by_arm["A"]["reconstruction_wall_ns"]
        if ratio > 1.05:
            failures.append(f"pair-{pair_id}:observer-perturbation")
        pairs.append({
            "pair": pair_id,
            "order": by_arm["A"]["order"],
            "control_ns": by_arm["A"]["reconstruction_wall_ns"],
            "instrumented_ns": by_arm["B"]["reconstruction_wall_ns"],
            "ratio": ratio,
        })

    b_rows = [row for row in measured if row["arm"] == "B"]
    timer_counts = {row["g2_decomposition"]["timer_regions"] for row in b_rows}
    if len(timer_counts) != 1 or not timer_counts or next(iter(timer_counts), 0) == 0:
        failures.append("instrumented-timer-count-drift")
    for row in b_rows:
        g2 = row["g2_decomposition"]
        if not g2["enabled"]:
            failures.append(f"{row['label']}:g2-disabled")
        if g2["direct_timer_sum_wall_ns"] + g2["raw_residual_wall_ns"] != row["reconstruction_wall_ns"]:
            failures.append(f"{row['label']}:timer-equation")
        if g2["sqlite_cache_writes"] != 0 or g2["sqlite_cache_spills"] != 0:
            failures.append(f"{row['label']}:read-pager-write")
        if g2["sqlite_status_errors"] != 0:
            failures.append(f"{row['label']}:sqlite-status")
        read = phase(row, "read_operation")
        exact = {
            "objects_authenticated": 5371,
            "canonical_bytes_authenticated": 105122401,
            "sql_query_calls": 170,
            "sql_rows_returned": 5371,
            "borrowed_row_blob_reads": 5284,
            "borrowed_row_blob_bytes": 104926292,
            "leaf_batch_queries": 83,
            "leaf_batch_references": 5284,
        }
        for key, value in exact.items():
            if read.get(key) != value:
                failures.append(f"{row['label']}:read-operation:{key}")
        if g2["operation_q_high_water"] <= 0 or g2["operation_q_high_water"] > 1_048_576:
            failures.append(f"{row['label']}:operation-q-bound")
        if row.get("maximum_resident_set_bytes", 0) > 20 * 1024 * 1024:
            failures.append(f"{row['label']}:rss-bound")

    observer_max = max((item["probe_wall_ns"] for item in observer), default=0)
    warmup_b = next((row for row in primary if row["kind"] == "warmup" and row["arm"] == "B"), None)
    observer_gate = min(5_000_000, warmup_b["reconstruction_wall_ns"] // 100) if warmup_b else 0
    if len(observer) != 5 or observer_max > observer_gate:
        failures.append("timer-observer-ceiling")
    if warmup_b and any(item["regions"] != warmup_b["g2_decomposition"]["timer_regions"] for item in observer):
        failures.append("timer-observer-region-count")

    by_position = {}
    for position in (1, 2):
        arm_values = {
            arm: statistics.median(
                row["reconstruction_wall_ns"]
                for row in measured
                if row["position"] == position and row["arm"] == arm
            )
            for arm in "AB"
        }
        by_position[str(position)] = {**arm_values, "ratio": arm_values["B"] / arm_values["A"]}
        if by_position[str(position)]["ratio"] > 1.05:
            failures.append(f"position-{position}:observer-perturbation")
    centers = {
        arm: statistics.mean(row["reconstruction_wall_ns"] for row in measured if row["arm"] == arm)
        for arm in "AB"
    }
    center_ratio = centers["B"] / centers["A"]
    if center_ratio > 1.05:
        failures.append("position-balanced-observer-perturbation")

    family_stats = {}
    eligible = []
    for family in FAMILIES:
        values = [row["g2_decomposition"][family] for row in b_rows]
        positions = {
            str(position): statistics.median(
                row["g2_decomposition"][family]
                for row in b_rows
                if row["position"] == position
            )
            for position in (1, 2)
        }
        passes_wall = len(values) == 4 and min(values) >= 33_000_000 and min(positions.values()) >= 33_000_000
        family_stats[family] = {
            "values_ns": values,
            "median_ns": statistics.median(values) if values else None,
            "positions_ns": positions,
            "at_least_33ms_everywhere": passes_wall,
            "statically_removable": REMOVABLE[family],
        }
        if passes_wall and REMOVABLE[family]:
            eligible.append(family)
    if len(eligible) > 1:
        failures.append("multiple-eligible-families")

    status = "PASS" if not failures else "REVISE"
    disposition = (
        f"G2 PASS / SELECT {eligible[0]}"
        if status == "PASS" and len(eligible) == 1
        else "G2 PASS / INSUFFICIENT_EVIDENCE FOR A CONSTANT-FACTOR CANDIDATE"
        if status == "PASS"
        else "G2 REVISE"
    )
    return {
        "schema": "phase4-g2-materialization-decomposition-primary-analysis-v1",
        "status": status,
        "disposition": disposition,
        "failures": sorted(set(failures)),
        "primary_rows": len(primary),
        "measured_rows": len(measured),
        "pairs": pairs,
        "position_balanced": {"control_ns": centers.get("A"), "instrumented_ns": centers.get("B"), "ratio": center_ratio},
        "positions": by_position,
        "observer": {"values": observer, "max_wall_ns": observer_max, "gate_ns": observer_gate},
        "timer_regions": sorted(timer_counts),
        "families": family_stats,
        "eligible_families": eligible,
    }


def final_analysis(rows, primary):
    failures = list(primary["failures"])
    guards = [row for row in rows if row["workload"] == "guard"]
    if len(guards) != 8:
        failures.append("guard-row-count")
    comparisons = []
    for operation in ("materialize-fresh", "read-range-1m", "reopen", "same-middle"):
        pair = [row for row in guards if row["operation"] == operation]
        if len(pair) != 2 or {row["arm"] for row in pair} != {"A", "B"}:
            failures.append(f"guard-{operation}:shape")
            continue
        by_arm = {row["arm"]: row for row in pair}
        for row in pair:
            failures.extend(parity_failures(row))
        for key in (
            "root_id", "transition_id", "ordered_closure_digest", "actual_cdc_references",
            "objects_created", "objects_reused", "objects_authenticated",
            "canonical_bytes_authenticated", "canonical_bytes_written",
            "mapping_bytes_rewritten", "transactions", "commits", "q_current",
            "sqlite_post_logical_database_bytes", "sqlite_post_apparent_database_bytes",
            "sqlite_post_allocated_database_bytes", "post_database_sha256",
        ):
            if by_arm["A"].get(key) != by_arm["B"].get(key):
                failures.append(f"guard-{operation}:parity:{key}")
        field = "reconstruction_wall_ns" if operation.startswith("materialize-") else "range_verification_wall_ns" if operation.startswith("read-range") else "fresh_reopen_head_wall_ns" if operation == "reopen" else "durable_capture_total_wall_ns"
        control = by_arm["A"][field]
        candidate = by_arm["B"][field]
        allowed = control * 1.05
        if operation in ("read-range-1m", "reopen"):
            allowed = max(allowed, control + 200_000)
        if candidate > allowed:
            failures.append(f"guard-{operation}:wall")
        comparisons.append({"operation": operation, "field": field, "control_ns": control, "instrumented_ns": candidate, "allowed_ns": allowed})
    status = "PASS" if not failures else "REVISE"
    disposition = primary["disposition"] if status == "PASS" else "G2 REVISE"
    return {
        "schema": "phase4-g2-materialization-decomposition-final-analysis-v1",
        "status": status,
        "disposition": disposition,
        "failures": sorted(set(failures)),
        "rows": len(rows),
        "measured_rows": sum(not row["warmup"] for row in rows),
        "primary": primary,
        "guards": comparisons,
        "concurrency": "NotRun(diagnostic-only scalar observation; actual later mechanism required)",
    }


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("results", type=Path)
    parser.add_argument("--final", action="store_true")
    args = parser.parse_args()
    rows = read_rows(args.results)
    observer_path = args.results / "OBSERVER-PROBES-v1.json"
    observer = json.loads(observer_path.read_text()) if observer_path.is_file() else []
    primary = primary_analysis(rows, observer)
    output = final_analysis(rows, primary) if args.final else primary
    name = "G2-ANALYSIS-v1.json" if args.final else "G2-PRIMARY-ANALYSIS-v1.json"
    (args.results / name).write_text(json.dumps(output, indent=2, sort_keys=True) + "\n")
    print(json.dumps({"status": output["status"], "disposition": output["disposition"]}, sort_keys=True))
    return 0 if output["status"] == "PASS" else 2


if __name__ == "__main__":
    raise SystemExit(main())
