#!/usr/bin/env python3
"""Fail-closed analysis for the conditional four-pair durable FastCDC A/B."""

import json
import statistics
import sys
from pathlib import Path

ORDERS = ["AB", "BA", "AB", "BA"]
PROFILE = "94a03ba7b6c97b5ff37c0ec62ef1d801b9896494b45456bd3df23e2cb278d13b"
SOURCE = "bb883eecf4ea85d80432953791dcc352243da94175e7503e2c476afe9bd0bab7"
COMMITMENT = "5bf63ef8adad7bb373be4e968759997b385590d076d75a1488e1958b64a3f8e2"
ROOT_ID = "93d1b461b5cbf88e8d122ad4e90a15a6e3029c408f0fe02ef51c561e5a94c6d1"
TRANSITION = "2de8d2ce6b614373ba8fb8b29d3a3eccd535abfccc2654e7922514e5ca90fd89"
CLOSURE = "29233d6018b031f6035c8e2c8175f1ab86fb721808f21a8858bc965567e9c0c1"


def average(rows, field):
    return statistics.fmean(float(row[field]) for row in rows)


def ratio_gate(pairs, field):
    ratios = [pair["B"][field] / pair["A"][field] for pair in pairs]
    return statistics.median(ratios) <= 1.05 and sum(ratio <= 1.05 for ratio in ratios) >= 3, ratios


def semantic_row_ok(row):
    phases = {phase["phase"]: phase for phase in row.get("phase_counters", [])}
    commit = phases.get("sqlite_commit", {})
    graph_fields = [
        "identity_bytes_hashed", "raw_bytes_hashed", "raw_hashes", "canonical_id_bytes_hashed",
        "canonical_id_hashes", "canonical_bytes_authenticated", "canonical_new_write_bytes",
        "canonical_authenticated_nonnew_bytes", "objects_created", "objects_reused",
        "references", "pages", "branches", "construction_put_evidences",
        "construction_edges_covered", "construction_leaf_summaries", "construction_branch_summaries",
        "construction_file_summaries", "construction_workspace_summaries",
        "construction_transition_summaries", "construction_proof_consumptions",
    ]
    return (
        row.get("status") == "PASS"
        and row.get("size_bytes") == row.get("input_size_bytes") == 104_857_600
        and row.get("profile_id") == PROFILE
        and row.get("source_fingerprint") == SOURCE
        and row.get("expected_cdc_sequence_fingerprint") == COMMITMENT
        and row.get("actual_cdc_references") == row.get("chunks") == row.get("references") == 5_284
        and row.get("ordered_closure_digest") == CLOSURE
        and row.get("root_id") == ROOT_ID
        and row.get("transition_id") == TRANSITION
        and row.get("objects_created") == 5_372
        and row.get("objects_reused") == 0
        and row.get("canonical_bytes_written") == 105_122_466
        and row.get("mapping_bytes_rewritten") == 196_174
        and row.get("sql_calls") == 5_381
        and row.get("row_blob_writes") == 10_748
        and row.get("transactions") == row.get("commits") == 1
        and row.get("commit_dispatches") == row.get("commit_returns") == row.get("commit_return_successes") == 1
        and row.get("commit_return_errors") == 0
        and row.get("sqlite_runtime_journal_mode") == "delete"
        and row.get("sqlite_runtime_synchronous") == 2
        and row.get("sqlite_runtime_temp_store") == 1
        and row.get("sqlite_runtime_mmap_size") == 0
        and row.get("durable_phase_sum_matches") is True
        and row.get("capture_publish_wall_ns") == row.get("durable_phase_sum_ns")
        and row.get("q_high_water", 10**9) <= 86_181
        and row.get("q_current") == 0
        and row.get("sqlite_post_logical_database_bytes") == 109_199_360
        and row.get("sqlite_post_apparent_database_bytes") == 109_199_360
        and row.get("sqlite_post_logical_store_bytes") == 109_199_392
        and row.get("sqlite_post_apparent_store_bytes") == 109_199_392
        and row.get("physical_journal_apparent_bytes") == 0
        and row.get("residue_files") == []
        and row.get("publication_status") == "Committed"
        and commit.get("commits") == 1
        and all(commit.get(field) == 0 for field in graph_fields)
    )


def analyze(root):
    rows = [json.loads(line) for line in (root / "DURABLE-RAW-v1.jsonl").read_text().splitlines() if line]
    screen = json.loads((root.parent / "screen-v1/SCREEN-ANALYSIS-v1.json").read_text())
    reasons = []
    expected = [("warmup", 0, "AB", arm) for arm in "AB"]
    for pair, order in enumerate(ORDERS, 1):
        expected.extend(("measured", pair, order, arm) for arm in order)
    if [(row.get("kind"), row.get("pair"), row.get("order"), row.get("arm")) for row in rows] != expected:
        reasons.append("schedule-or-chronology")
    for row in rows:
        if not semantic_row_ok(row):
            reasons.append(f"semantic-or-durability:{row.get('label')}")

    measured = [row for row in rows if row.get("kind") == "measured"]
    pairs = []
    pair_table = []
    for pair, order in enumerate(ORDERS, 1):
        selected = {row["arm"]: row for row in measured if row.get("pair") == pair}
        if set(selected) != {"A", "B"}:
            reasons.append(f"pair-shape:{pair}")
            continue
        pairs.append(selected)
        pair_table.append({
            "pair": pair,
            "order": order,
            "control_total_ms": selected["A"]["capture_publish_wall_ns"] / 1_000_000,
            "candidate_total_ms": selected["B"]["capture_publish_wall_ns"] / 1_000_000,
            "saved_ms": (selected["A"]["capture_publish_wall_ns"] - selected["B"]["capture_publish_wall_ns"]) / 1_000_000,
            "candidate_faster": selected["B"]["capture_publish_wall_ns"] < selected["A"]["capture_publish_wall_ns"],
            "control_mapping_ms": selected["A"]["canonical_cas_mapping_stage_wall_ns"] / 1_000_000,
            "candidate_mapping_ms": selected["B"]["canonical_cas_mapping_stage_wall_ns"] / 1_000_000,
            "control_proof_ms": selected["A"]["precommit_closure_validation_wall_ns"] / 1_000_000,
            "candidate_proof_ms": selected["B"]["precommit_closure_validation_wall_ns"] / 1_000_000,
            "control_commit_ms": selected["A"]["sqlite_commit_durability_wall_ns"] / 1_000_000,
            "candidate_commit_ms": selected["B"]["sqlite_commit_durability_wall_ns"] / 1_000_000,
        })

    controls = [row for row in measured if row.get("arm") == "A"]
    candidates = [row for row in measured if row.get("arm") == "B"]
    control_total = average(controls, "capture_publish_wall_ns") if len(controls) == 4 else float("nan")
    candidate_total = average(candidates, "capture_publish_wall_ns") if len(candidates) == 4 else float("nan")
    saved = control_total - candidate_total
    relative = saved / control_total if control_total > 0 else float("-inf")
    control_mapping = average(controls, "canonical_cas_mapping_stage_wall_ns") if controls else float("nan")
    candidate_mapping = average(candidates, "canonical_cas_mapping_stage_wall_ns") if candidates else float("nan")
    control_proof = average(controls, "precommit_closure_validation_wall_ns") if controls else float("nan")
    candidate_proof = average(candidates, "precommit_closure_validation_wall_ns") if candidates else float("nan")
    control_commit = average(controls, "sqlite_commit_durability_wall_ns") if controls else float("nan")
    candidate_commit = average(candidates, "sqlite_commit_durability_wall_ns") if candidates else float("nan")
    pair_wins = sum(row["candidate_faster"] for row in pair_table)
    positions = []
    for position in (1, 2):
        a = [row for row in controls if row.get("position") == position]
        b = [row for row in candidates if row.get("position") == position]
        if len(a) != 2 or len(b) != 2:
            reasons.append(f"position-shape:{position}")
            continue
        a_wall = average(a, "capture_publish_wall_ns")
        b_wall = average(b, "capture_publish_wall_ns")
        positions.append({"position": position, "control_ms": a_wall / 1_000_000,
                          "candidate_ms": b_wall / 1_000_000, "candidate_faster": b_wall < a_wall})

    resource_gates = {}
    for field in ("user_seconds", "system_seconds", "maximum_resident_set_bytes",
                  "sqlite_post_allocated_store_bytes"):
        if len(pairs) == 4:
            resource_gates[field], _ = ratio_gate(pairs, field)
        else:
            resource_gates[field] = False
    semantic_ok = not reasons
    performance_ok = (
        semantic_ok
        and screen.get("advance_to_durable") is True
        and pair_wins >= 3
        and len(positions) == 2 and all(row["candidate_faster"] for row in positions)
        and saved >= 10_000_000
        and relative >= 0.02
        and candidate_mapping < control_mapping
        and all(resource_gates.values())
    )
    disposition = "FASTCDC EXACT HOT LOOP PASS / RETAIN" if performance_ok else "FASTCDC EXACT HOT LOOP NO-GO / REVERT"
    return {
        "status": "PASS" if semantic_ok else "FAIL",
        "disposition": disposition,
        "retain_candidate": performance_ok,
        "reasons": sorted(set(reasons)),
        "rows": len(rows),
        "pair_results": pair_table,
        "position_results": positions,
        "pairs_favoring_candidate": pair_wins,
        "control_position_balanced_total_ms": control_total / 1_000_000,
        "candidate_position_balanced_total_ms": candidate_total / 1_000_000,
        "candidate_saved_ms": saved / 1_000_000,
        "relative_improvement": relative,
        "control_mapping_ms": control_mapping / 1_000_000,
        "candidate_mapping_ms": candidate_mapping / 1_000_000,
        "control_proof_ms": control_proof / 1_000_000,
        "candidate_proof_ms": candidate_proof / 1_000_000,
        "control_commit_ms": control_commit / 1_000_000,
        "candidate_commit_ms": candidate_commit / 1_000_000,
        "resource_gates": resource_gates,
        "semantic_gate_pass": semantic_ok,
        "performance_gate_pass": performance_ok,
        "crossed_ms": {str(value): candidate_total / 1_000_000 < value for value in (500, 400, 333.333, 250)},
    }


def write(root, result):
    (root / "DURABLE-ANALYSIS-v1.json").write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
    lines = ["# Conditional FastCDC durable A/B v1", "", f"Disposition: **{result['disposition']}**", "",
             "| Pair | Order | Control ms | Candidate ms | Saved ms | Candidate faster |",
             "|---:|:---:|---:|---:|---:|:---:|"]
    for row in result["pair_results"]:
        lines.append(f"| {row['pair']} | {row['order']} | {row['control_total_ms']:.6f} | {row['candidate_total_ms']:.6f} | {row['saved_ms']:.6f} | {str(row['candidate_faster']).lower()} |")
    lines.extend(["", f"Position-balanced control/candidate: {result['control_position_balanced_total_ms']:.6f} / {result['candidate_position_balanced_total_ms']:.6f} ms.",
                  f"Saved: {result['candidate_saved_ms']:.6f} ms ({result['relative_improvement'] * 100:.6f}%).", ""])
    (root / "DURABLE-REPORT-v1.md").write_text("\n".join(lines))


def main():
    root = Path(sys.argv[1]).resolve()
    result = analyze(root)
    write(root, result)
    print(json.dumps(result, sort_keys=True))
    return 0 if result["semantic_gate_pass"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
