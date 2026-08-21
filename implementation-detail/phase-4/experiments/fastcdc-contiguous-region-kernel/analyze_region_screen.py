#!/usr/bin/env python3
"""Fail-closed analysis for the corrected contiguous-region CDC screen."""

import csv
import hashlib
import json
import statistics
import sys
from pathlib import Path

EXPECTED_BYTES = 104_857_600
EXPECTED_CHUNKS = 5_284
EXPECTED_SOURCE_BLAKE3 = "bb883eecf4ea85d80432953791dcc352243da94175e7503e2c476afe9bd0bab7"
ORDERS = ["AB", "BA", "AB", "BA"]


def sha256(path):
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def average(rows, field):
    return statistics.fmean(float(row[field]) for row in rows)


def validate_authority(path):
    rows = list(csv.DictReader(path.open(), delimiter="\t"))
    start = 0
    minimum = EXPECTED_BYTES
    maximum = 0
    for ordinal, row in enumerate(rows):
        number = int(row["ordinal"])
        row_start = int(row["start"])
        end = int(row["end"])
        length = int(row["length"])
        if number != ordinal or row_start != start or length <= 0 or end - row_start != length:
            raise ValueError(f"invalid boundary authority row {ordinal}")
        if length > 32_768:
            raise ValueError(f"boundary {ordinal} exceeds maximum")
        start = end
        minimum = min(minimum, length)
        maximum = max(maximum, length)
    if len(rows) != EXPECTED_CHUNKS or start != EXPECTED_BYTES:
        raise ValueError("boundary authority is incomplete")
    return {"count": len(rows), "sum": start, "minimum": minimum, "maximum": maximum}


def analyze(root):
    custody = json.loads((root / "CUSTODY-v1.json").read_text())
    acquisition = json.loads((root / "SCREEN-ACQUISITION-CUSTODY-v1.json").read_text())
    raw_path = root / "SCREEN-RAW-v1.jsonl"
    rows = [json.loads(line) for line in raw_path.read_text().splitlines() if line]
    reasons = []
    expected = [("warmup", 0, "AB", arm) for arm in "AB"]
    for pair, order in enumerate(ORDERS, 1):
        expected.extend(("measured", pair, order, arm) for arm in order)
    actual = [(row.get("kind"), row.get("pair"), row.get("order"), row.get("arm")) for row in rows]
    if actual != expected:
        reasons.append("schedule-or-chronology")

    for row in rows:
        if not (
            row.get("status") == "PASS"
            and row.get("input_bytes_consumed") == EXPECTED_BYTES
            and row.get("bytes_scanned") == EXPECTED_BYTES
            and row.get("output_occurrences") == EXPECTED_CHUNKS
            and row.get("callback_count") == EXPECTED_CHUNKS
            and row.get("sum_occurrence_lengths") == EXPECTED_BYTES
            and row.get("terminal_end") == EXPECTED_BYTES
            and row.get("reconstructed_source_blake3") == EXPECTED_SOURCE_BLAKE3
            and row.get("scanner_chunk_buffer_capacity") == 32_768
            and row.get("boundary_record_capacity") == EXPECTED_CHUNKS
            and 0 < row.get("minimum_occurrence_length", 0) <= row.get("maximum_occurrence_length", 0) <= 32_768
            and row.get("boundary_wall_ns", 0) > 0
        ):
            reasons.append(f"row-contract:{row.get('label')}")
    parity_fields = [
        "ordered_boundary_transcript_blake3", "reconstructed_source_blake3",
        "minimum_occurrence_length", "maximum_occurrence_length", "scanner_chunk_buffer_capacity",
        "boundary_record_capacity", "sum_occurrence_lengths", "terminal_end",
    ]
    for field in parity_fields:
        if len({row.get(field) for row in rows}) != 1:
            reasons.append(f"parity:{field}")

    authority = root / "SCREEN-BOUNDARY-AUTHORITY-v1.tsv"
    try:
        authority_summary = validate_authority(authority)
    except Exception as error:
        authority_summary = None
        reasons.append(f"boundary-authority:{error}")
    if sha256(authority) != acquisition.get("boundary_authority_sha256"):
        reasons.append("boundary-authority-custody")
    if sum(bool(row.get("boundary_authority_written")) for row in rows) != 1:
        reasons.append("boundary-authority-count")
    if authority_summary and rows and authority_summary != {
        "count": rows[0]["output_occurrences"], "sum": rows[0]["sum_occurrence_lengths"],
        "minimum": rows[0]["minimum_occurrence_length"], "maximum": rows[0]["maximum_occurrence_length"],
    }:
        reasons.append("boundary-authority-summary")

    if sha256(raw_path) != acquisition.get("screen_raw_sha256"):
        reasons.append("raw-custody")
    if any(row.get("binary_sha256") != custody[f"{row.get('arm')}_screen_binary_sha256"] for row in rows):
        reasons.append("binary-custody")
    codegen = json.loads((root.parent.parent / "codegen-v1/CODEGEN-PREFLIGHT-v1.json").read_text())
    if codegen.get("status") != "PASS" or not all(codegen.get("checks", {}).values()):
        reasons.append("machine-code-preflight")
    if sha256(root.parent.parent / "codegen-v1/CODEGEN-PREFLIGHT-v1.json") != custody["codegen_preflight_sha256"]:
        reasons.append("machine-code-custody")

    measured = [row for row in rows if row.get("kind") == "measured"]
    controls = [row for row in measured if row.get("arm") == "A"]
    candidates = [row for row in measured if row.get("arm") == "B"]
    pair_results = []
    for pair, order in enumerate(ORDERS, 1):
        selected = {row["arm"]: row for row in measured if row.get("pair") == pair}
        if set(selected) != {"A", "B"}:
            reasons.append(f"pair-shape:{pair}")
            continue
        control = selected["A"]
        candidate = selected["B"]
        pair_results.append({
            "pair": pair, "order": order,
            "control_ms": control["boundary_wall_ns"] / 1_000_000,
            "candidate_ms": candidate["boundary_wall_ns"] / 1_000_000,
            "saved_ms": (control["boundary_wall_ns"] - candidate["boundary_wall_ns"]) / 1_000_000,
            "candidate_faster": candidate["boundary_wall_ns"] < control["boundary_wall_ns"],
            "control_position": control["position"], "candidate_position": candidate["position"],
            "rss_ratio": candidate["maximum_resident_set_bytes"] / control["maximum_resident_set_bytes"],
        })

    control_ns = average(controls, "boundary_wall_ns") if len(controls) == 4 else float("nan")
    candidate_ns = average(candidates, "boundary_wall_ns") if len(candidates) == 4 else float("nan")
    saved_ns = control_ns - candidate_ns
    relative = saved_ns / control_ns if control_ns > 0 else float("-inf")
    pair_wins = sum(row["candidate_faster"] for row in pair_results)
    positions = []
    for position in (1, 2):
        a = [row for row in controls if row.get("position") == position]
        b = [row for row in candidates if row.get("position") == position]
        if len(a) != 2 or len(b) != 2:
            reasons.append(f"position-shape:{position}")
            continue
        a_ns = average(a, "boundary_wall_ns")
        b_ns = average(b, "boundary_wall_ns")
        positions.append({"position": position, "control_ms": a_ns / 1_000_000,
                          "candidate_ms": b_ns / 1_000_000, "candidate_faster": b_ns < a_ns})

    temporal_center_a = average(controls, "sequence") if controls else float("nan")
    temporal_center_b = average(candidates, "sequence") if candidates else float("nan")
    temporal_ok = temporal_center_a == temporal_center_b
    cpu_ok = bool(controls and candidates) and (
        average(candidates, "user_seconds") <= average(controls, "user_seconds") * 1.05
        and average(candidates, "system_seconds") <= average(controls, "system_seconds") * 1.05
    )
    rss_ratios = [row["rss_ratio"] for row in pair_results]
    rss_ok = len(rss_ratios) == 4 and statistics.median(rss_ratios) <= 1.05 and sum(ratio <= 1.05 for ratio in rss_ratios) >= 3
    capacity_ok = all(row.get("scanner_chunk_buffer_capacity") == 32_768 and row.get("boundary_record_capacity") == EXPECTED_CHUNKS for row in rows)
    parity_ok = not reasons
    signal_ok = (
        parity_ok and saved_ns >= 8_000_000 and pair_wins >= 3
        and len(positions) == 2 and all(row["candidate_faster"] for row in positions)
        and temporal_ok and cpu_ok and rss_ok and capacity_ok
    )
    disposition = (
        "FASTCDC CONTIGUOUS REGION KERNEL SCREEN GO / ADVANCE" if signal_ok
        else "FASTCDC CONTIGUOUS REGION KERNEL NO-GO / REVERT"
    )
    return {
        "status": "PASS" if parity_ok else "FAIL", "disposition": disposition,
        "advance_to_durable": signal_ok, "reasons": sorted(set(reasons)),
        "rows": len(rows), "measured_rows": len(measured), "pair_results": pair_results,
        "position_results": positions, "control_position_balanced_ms": control_ns / 1_000_000,
        "candidate_position_balanced_ms": candidate_ns / 1_000_000,
        "candidate_saved_ms": saved_ns / 1_000_000, "relative_improvement": relative,
        "pairs_favoring_candidate": pair_wins, "temporal_center_control": temporal_center_a,
        "temporal_center_candidate": temporal_center_b, "temporal_balance_pass": temporal_ok,
        "cpu_gate_pass": cpu_ok, "rss_gate_pass": rss_ok, "capacity_gate_pass": capacity_ok,
        "parity_gate_pass": parity_ok, "performance_signal_pass": signal_ok,
        "ordered_boundary_transcript_blake3": rows[0].get("ordered_boundary_transcript_blake3") if rows else None,
        "boundary_authority_sha256": acquisition.get("boundary_authority_sha256"),
        "minimum_occurrence_length": rows[0].get("minimum_occurrence_length") if rows else None,
        "maximum_occurrence_length": rows[0].get("maximum_occurrence_length") if rows else None,
    }


def write_outputs(root, result):
    (root / "SCREEN-ANALYSIS-v1.json").write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
    lines = ["# FastCDC contiguous-region screen v2", "", f"Disposition: **{result['disposition']}**", "",
             "| Pair | Order | Control ms | Candidate ms | Saved ms | Candidate faster |",
             "|---:|:---:|---:|---:|---:|:---:|"]
    for row in result["pair_results"]:
        lines.append(f"| {row['pair']} | {row['order']} | {row['control_ms']:.6f} | {row['candidate_ms']:.6f} | {row['saved_ms']:.6f} | {str(row['candidate_faster']).lower()} |")
    lines.extend(["", f"Position-balanced control/candidate: {result['control_position_balanced_ms']:.6f} / {result['candidate_position_balanced_ms']:.6f} ms.",
                  f"Saved: {result['candidate_saved_ms']:.6f} ms ({result['relative_improvement'] * 100:.6f}% descriptive).", ""])
    (root / "SCREEN-REPORT-v1.md").write_text("\n".join(lines))


def main():
    root = Path(sys.argv[1]).resolve()
    result = analyze(root)
    write_outputs(root, result)
    print(json.dumps(result, sort_keys=True))
    return 0 if result["parity_gate_pass"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
