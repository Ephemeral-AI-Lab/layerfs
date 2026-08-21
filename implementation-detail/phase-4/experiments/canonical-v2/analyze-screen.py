#!/usr/bin/env python3
import json
import statistics
import sys
from pathlib import Path


EXPECTED_PROFILE = "94a03ba7b6c97b5ff37c0ec62ef1d801b9896494b45456bd3df23e2cb278d13b"
EXPECTED_ROOT = "93d1b461b5cbf88e8d122ad4e90a15a6e3029c408f0fe02ef51c561e5a94c6d1"
EXPECTED_TRANSITION = "2de8d2ce6b614373ba8fb8b29d3a3eccd535abfccc2654e7922514e5ca90fd89"
EXPECTED_COMMITMENT = "5bf63ef8adad7bb373be4e968759997b385590d076d75a1488e1958b64a3f8e2"
EXPECTED_SCHEDULE = [(0, "A", "warmup", "AB"), (0, "B", "warmup", "AB"),
                     (1, "A", "measured", "AB"), (1, "B", "measured", "AB"),
                     (2, "B", "measured", "BA"), (2, "A", "measured", "BA"),
                     (3, "A", "measured", "AB"), (3, "B", "measured", "AB")]


def load_jsonl(path):
    return [json.loads(line) for line in Path(path).read_text().splitlines() if line.strip()]


def canonical_phase(row):
    return next(phase for phase in row["phase_counters"]
                if phase["phase"] == "canonical_cas_mapping")


def check(condition, reason, reasons):
    if not condition:
        reasons.append(reason)


def main(raw_path, smoke_path, output_path):
    rows = load_jsonl(raw_path)
    smoke = load_jsonl(smoke_path)
    reasons = []

    schedule = [(r["screen_pair"], r["screen_arm"], r["screen_sample_kind"],
                 r["screen_order"]) for r in rows]
    check(schedule == EXPECTED_SCHEDULE, "schedule mismatch", reasons)
    check(len(smoke) == 7, "protected smoke count mismatch", reasons)
    check([r["screen_smoke_operation"] for r in smoke] ==
          ["same-middle", "plus1-early", "plus1-middle", "materialize-warm",
           "materialize-fresh", "read-range-1m", "reopen"],
          "protected smoke order mismatch", reasons)

    for row in rows:
        arm = row["screen_arm"]
        check(row["status"] == "PASS" and row["error"] is None,
              f"row {row['screen_pair']}{arm} failed", reasons)
        check(row["q_current"] == 0 and row["screen_residue"] == [],
              f"row {row['screen_pair']}{arm} cleanup mismatch", reasons)
        check((row["transactions"], row["commits"], row["commit_dispatches"],
               row["commit_returns"], row["commit_return_successes"]) == (1, 1, 1, 1, 1),
              f"row {row['screen_pair']}{arm} transaction mismatch", reasons)
        check(row["durable_phase_sum_matches"] and row["commit_timer_equation_matches"],
              f"row {row['screen_pair']}{arm} timer equation mismatch", reasons)
        phase = canonical_phase(row)
        if arm == "B":
            exact = {
                "profile_id": EXPECTED_PROFILE,
                "root_id": EXPECTED_ROOT,
                "transition_id": EXPECTED_TRANSITION,
                "screen_canonical_commitment": EXPECTED_COMMITMENT,
                "source_bytes_read": 104_857_600,
                "source_cdc_bytes_read": 104_857_600,
                "references": 5_284,
                "chunks": 5_284,
                "raw_bytes_hashed": 0,
                "raw_hashes": 0,
                "mapping_bytes_rewritten": 196_174,
                "canonical_new_write_bytes": 105_122_466,
                "objects_created": 5_372,
                "objects_reused": 0,
                "pages": 83,
                "branches": 2,
            }
            for field, expected in exact.items():
                check(row[field] == expected,
                      f"candidate pair {row['screen_pair']} {field}: {row[field]} != {expected}",
                      reasons)
            phase_exact = {
                "construction_source_hash_bytes": 0,
                "construction_source_hashes": 0,
                "construction_canonical_commitment_bytes": 190_224,
                "construction_canonical_commitment_entries": 5_284,
                "construction_canonical_commitment_hashes": 1,
                "construction_cdc_entries": 5_284,
            }
            for field, expected in phase_exact.items():
                check(phase[field] == expected,
                      f"candidate pair {row['screen_pair']} phase {field} mismatch", reasons)
            check(row["physical_store_allocated_bytes"] * 4 <=
                  row["sqlite_post_apparent_store_bytes"] * 5,
                  f"candidate pair {row['screen_pair']} allocation exceeds 125%", reasons)
        else:
            check(row["profile_id"] ==
                  "b0ebb845409ef995a5fa454bb23d10a80c6ecf44deb7832ca2ce1213eb0f4ba1",
                  f"control pair {row['screen_pair']} profile mismatch", reasons)
            check((row["raw_bytes_hashed"], row["raw_hashes"],
                   row["mapping_bytes_rewritten"], row["canonical_new_write_bytes"]) ==
                  (104_857_600, 5_284, 365_262, 105_291_554),
                  f"control pair {row['screen_pair']} work mismatch", reasons)

    for row in smoke:
        mutation = row["screen_smoke_operation"] in {
            "same-middle", "plus1-early", "plus1-middle"
        }
        check(row["status"] == "PASS" and row["error"] is None,
              f"smoke {row['screen_smoke_operation']} failed", reasons)
        check(row["profile_id"] == EXPECTED_PROFILE and row["q_current"] == 0
              and row["screen_residue"] == [],
              f"smoke {row['screen_smoke_operation']} authority/cleanup mismatch", reasons)
        check((row["transactions"], row["commits"]) == ((1, 1) if mutation else (0, 0)),
              f"smoke {row['screen_smoke_operation']} transaction mismatch", reasons)
        check(row["raw_bytes_hashed"] == 0 and row["raw_hashes"] == 0,
              f"smoke {row['screen_smoke_operation']} performed legacy raw hashing", reasons)

    measured = [row for row in rows if row["screen_sample_kind"] == "measured"]
    pairs = []
    for pair in (1, 2, 3):
        a = next(row for row in measured if row["screen_pair"] == pair and row["screen_arm"] == "A")
        b = next(row for row in measured if row["screen_pair"] == pair and row["screen_arm"] == "B")
        improvement = (a["durable_capture_total_wall_ns"] - b["durable_capture_total_wall_ns"]) * 100 / a["durable_capture_total_wall_ns"]
        pairs.append({
            "pair": pair,
            "order": a["screen_order"],
            "control_ns": a["durable_capture_total_wall_ns"],
            "candidate_ns": b["durable_capture_total_wall_ns"],
            "improvement_percent": improvement,
            "mapping_improvement_percent":
                (a["canonical_cas_mapping_stage_wall_ns"] - b["canonical_cas_mapping_stage_wall_ns"]) * 100 / a["canonical_cas_mapping_stage_wall_ns"],
            "allocated_store_delta_bytes":
                b["physical_store_allocated_bytes"] - a["physical_store_allocated_bytes"],
        })
    improvements = [pair["improvement_percent"] for pair in pairs]
    wins = sum(value > 0 for value in improvements)
    paired_median = statistics.median(improvements)
    if paired_median >= 15 and wins == 3:
        classification = "BREAKTHROUGH"
    elif paired_median >= 5 and wins >= 2:
        classification = "STRONG"
    elif paired_median > 0 and wins >= 2:
        classification = "PROMISING"
    else:
        classification = "STOP"

    a_values = [row["durable_capture_total_wall_ns"] for row in measured if row["screen_arm"] == "A"]
    b_values = [row["durable_capture_total_wall_ns"] for row in measured if row["screen_arm"] == "B"]
    a_median = statistics.median(a_values)
    b_median = statistics.median(b_values)
    b_ms = b_median / 1_000_000
    summary = {
        "status": "PASS" if not reasons else "FAIL",
        "reasons": reasons,
        "classification": classification,
        "candidate_wins": wins,
        "paired_median_improvement_percent": paired_median,
        "pairs": pairs,
        "control_median_ns": a_median,
        "candidate_median_ns": b_median,
        "candidate_min_ns": min(b_values),
        "candidate_max_ns": max(b_values),
        "candidate_throughput_mib_s": 100_000 / b_ms,
        "evidence_gap_ms": {
            "to_500": b_ms - 500,
            "to_400": b_ms - 400,
            "to_333_333": b_ms - 333.333,
        },
        "mapping_reduction_bytes": 169_088,
        "apparent_store_reduction_bytes": 109_269_024 - 109_199_392,
        "candidate_q_high_water": sorted({row["q_high_water"] for row in rows if row["screen_arm"] == "B"}),
        "control_q_high_water": sorted({row["q_high_water"] for row in rows if row["screen_arm"] == "A"}),
        "candidate_allocated_store_values": sorted({row["physical_store_allocated_bytes"] for row in rows if row["screen_arm"] == "B"}),
        "control_allocated_store_values": sorted({row["physical_store_allocated_bytes"] for row in rows if row["screen_arm"] == "A"}),
        "protected_smoke": {
            row["screen_smoke_operation"]: {
                "elapsed_ns": row["elapsed_wall_ns"],
                "durable_edit_ns": row["durable_capture_total_wall_ns"],
                "authority_ns": row["same_open_authority_establishment_wall_ns"],
                "reconstruction_ns": row["reconstruction_wall_ns"],
                "range_ns": row["range_verification_wall_ns"],
                "reopen_ns": row["fresh_reopen_head_wall_ns"],
            }
            for row in smoke
        },
    }
    Path(output_path).write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n")
    print(json.dumps(summary, sort_keys=True, separators=(",", ":")))
    raise SystemExit(bool(reasons))


if __name__ == "__main__":
    if len(sys.argv) != 4:
        raise SystemExit("usage: analyze-screen.py RAW_JSONL SMOKE_JSONL OUTPUT_JSON")
    main(*sys.argv[1:])
