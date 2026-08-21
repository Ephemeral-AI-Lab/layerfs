#!/usr/bin/env python3
"""Independent arithmetic, invariant, and manifest recomputation for v2."""

import csv
import hashlib
import json
import statistics
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[4]
TARGET = REPO / "target/phase4-fastcdc-contiguous-region-kernel-20260821-v2"


def sha256(path):
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def verify_manifest(path):
    rows = list(csv.DictReader(path.open(), delimiter="\t"))
    for row in rows:
        target = REPO / row["path"]
        if not target.is_file() or target.stat().st_size != int(row["size_bytes"]) or sha256(target) != row["sha256"]:
            raise RuntimeError(f"manifest mismatch: {row['path']}")
    return len(rows)


def arm_mean(rows, arm, field):
    selected = [row[field] for row in rows if row["kind"] == "measured" and row["arm"] == arm]
    if len(selected) != 4:
        raise RuntimeError(f"wrong measured row count for {arm}")
    return statistics.fmean(selected)


def recompute(raw_path, field):
    rows = [json.loads(line) for line in raw_path.read_text().splitlines() if line]
    control = arm_mean(rows, "A", field)
    candidate = arm_mean(rows, "B", field)
    pairs = []
    for pair in range(1, 5):
        a = next(row for row in rows if row["kind"] == "measured" and row["pair"] == pair and row["arm"] == "A")
        b = next(row for row in rows if row["kind"] == "measured" and row["pair"] == pair and row["arm"] == "B")
        pairs.append(a[field] - b[field])
    positions = {}
    for position in (1, 2):
        a = statistics.fmean(row[field] for row in rows if row["kind"] == "measured" and row["arm"] == "A" and row["position"] == position)
        b = statistics.fmean(row[field] for row in rows if row["kind"] == "measured" and row["arm"] == "B" and row["position"] == position)
        positions[position] = {"control": a, "candidate": b, "candidate_faster": b < a}
    centers = {arm: statistics.fmean(row["sequence"] for row in rows if row["kind"] == "measured" and row["arm"] == arm) for arm in "AB"}
    return rows, {"control": control, "candidate": candidate, "saved": control - candidate,
                  "relative": (control - candidate) / control, "pair_savings": pairs,
                  "pair_wins": sum(value > 0 for value in pairs), "positions": positions,
                  "temporal_centers": centers}


def main():
    output = Path(sys.argv[1]).resolve()
    output.mkdir(parents=True, exist_ok=False)
    screen_root = TARGET / "results-v1/screen-v1"
    durable_root = TARGET / "results-v1/durable-v2"
    screen_rows, screen = recompute(screen_root / "SCREEN-RAW-v1.jsonl", "boundary_wall_ns")
    durable_rows, durable = recompute(durable_root / "DURABLE-RAW-v1.jsonl", "capture_publish_wall_ns")
    screen_analysis = json.loads((screen_root / "SCREEN-ANALYSIS-v1.json").read_text())
    durable_analysis = json.loads((durable_root / "DURABLE-ANALYSIS-v1.json").read_text())

    if len({(row["bytes_scanned"], row["output_occurrences"], row["ordered_boundary_transcript_blake3"],
             row["reconstructed_source_blake3"], row["minimum_occurrence_length"], row["maximum_occurrence_length"],
             row["terminal_end"]) for row in screen_rows}) != 1:
        raise RuntimeError("screen parity differs")
    expected_durable = (
        104_857_600, 104_857_600, 5_284, 5_372, 0, 105_122_466, 196_174, 5_381,
        10_748, 1, 1, "93d1b461b5cbf88e8d122ad4e90a15a6e3029c408f0fe02ef51c561e5a94c6d1",
        "2de8d2ce6b614373ba8fb8b29d3a3eccd535abfccc2654e7922514e5ca90fd89",
        "29233d6018b031f6035c8e2c8175f1ab86fb721808f21a8858bc965567e9c0c1",
    )
    for row in durable_rows:
        actual = (row["source_bytes_read"], row["source_cdc_bytes_read"], row["actual_cdc_references"],
                  row["objects_created"], row["objects_reused"], row["canonical_bytes_written"],
                  row["mapping_bytes_rewritten"], row["sql_calls"], row["row_blob_writes"],
                  row["transactions"], row["commits"], row["root_id"], row["transition_id"],
                  row["ordered_closure_digest"])
        if actual != expected_durable or row["q_high_water"] > 86_181 or row["q_current"] != 0 or row["residue_files"]:
            raise RuntimeError(f"durable invariant mismatch: {row['label']}")

    checks = {
        "screen_control_equal": screen["control"] / 1_000_000 == screen_analysis["control_position_balanced_ms"],
        "screen_candidate_equal": screen["candidate"] / 1_000_000 == screen_analysis["candidate_position_balanced_ms"],
        "screen_saved_equal": screen["saved"] / 1_000_000 == screen_analysis["candidate_saved_ms"],
        "screen_pair_wins": screen["pair_wins"] == screen_analysis["pairs_favoring_candidate"] == 4,
        "screen_positions": all(value["candidate_faster"] for value in screen["positions"].values()),
        "screen_temporal": screen["temporal_centers"] == {"A": 6.5, "B": 6.5},
        "durable_control_equal": durable["control"] / 1_000_000 == durable_analysis["control_position_balanced_total_ms"],
        "durable_candidate_equal": durable["candidate"] / 1_000_000 == durable_analysis["candidate_position_balanced_total_ms"],
        "durable_saved_equal": durable["saved"] / 1_000_000 == durable_analysis["candidate_saved_ms"],
        "durable_pair_wins": durable["pair_wins"] == durable_analysis["pairs_favoring_candidate"] == 4,
        "durable_positions": all(value["candidate_faster"] for value in durable["positions"].values()),
        "durable_temporal": durable["temporal_centers"] == {"A": 6.5, "B": 6.5},
        "screen_manifest": verify_manifest(screen_root / "SCREEN-MANIFEST-v1.tsv") == 30,
        "durable_manifest": verify_manifest(durable_root / "DURABLE-MANIFEST-v1.tsv") == 79,
        "codegen": json.loads((TARGET / "codegen-v1/CODEGEN-PREFLIGHT-v1.json").read_text())["status"] == "PASS",
        "static": json.loads((TARGET / "static-v1/STATIC-CLOSURE-v1.json").read_text())["status"] == "PASS",
        "candidate_source": sha256(REPO / "crates/layerfs-core/src/cdc/mod.rs") == "bc0346eec113914943d046a4ab4742420acfff570d6b00115082c40bdf8e58b6",
        "v1_manifest_unchanged": sha256(REPO / "target/phase4-fastcdc-exact-hot-loop-20260821-v1/TERMINAL-MANIFEST-v1.tsv") == "4252eff3cd8d72c2ceefff0e92f8992f53c7df27ffb7dc373a9ed5ad7748177e",
    }
    if not all(checks.values()):
        raise RuntimeError("independent recomputation failed: " + ",".join(key for key, value in checks.items() if not value))
    report = {"status": "PASS", "checks": checks, "screen": screen, "durable": durable}
    (output / "INDEPENDENT-RECOMPUTATION-v1.json").write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    print(json.dumps(report, sort_keys=True))


if __name__ == "__main__":
    main()
