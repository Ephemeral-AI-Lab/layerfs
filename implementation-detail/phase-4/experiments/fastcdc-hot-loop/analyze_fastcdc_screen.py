#!/usr/bin/env python3
"""Fail-closed analysis for the one FastCDC exact-boundary mechanism screen."""

import csv
import hashlib
import json
import statistics
import sys
from pathlib import Path

EXPECTED_BYTES = 104_857_600
EXPECTED_CHUNKS = 5_284
EXPECTED_SOURCE_BLAKE3 = "bb883eecf4ea85d80432953791dcc352243da94175e7503e2c476afe9bd0bab7"
EXPECTED_ORDERS = ["AB", "BA", "AB", "BA"]


def sha256(path):
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def mean(rows, key):
    return statistics.fmean(float(row[key]) for row in rows)


def validate_boundaries(path):
    rows = list(csv.DictReader(path.open(), delimiter="\t"))
    start = 0
    minimum = EXPECTED_BYTES
    maximum = 0
    for ordinal, row in enumerate(rows):
        actual = (int(row["ordinal"]), int(row["start"]), int(row["end"]), int(row["length"]))
        number, row_start, end, length = actual
        if number != ordinal or row_start != start or length <= 0 or end - row_start != length:
            raise ValueError(f"invalid exact boundary {ordinal} in {path}")
        if length > 32_768:
            raise ValueError(f"boundary exceeds maximum in {path}")
        start = end
        minimum = min(minimum, length)
        maximum = max(maximum, length)
    if len(rows) != EXPECTED_CHUNKS or start != EXPECTED_BYTES:
        raise ValueError(f"incomplete boundary file {path}")
    return {"count": len(rows), "sum": start, "minimum": minimum, "maximum": maximum}


def analyze(root):
    raw_path = root / "SCREEN-RAW-v1.jsonl"
    custody = json.loads((root / "CUSTODY-v1.json").read_text())
    acquisition = json.loads((root / "SCREEN-ACQUISITION-CUSTODY-v1.json").read_text())
    rows = [json.loads(line) for line in raw_path.read_text().splitlines() if line]
    reasons = []
    expected_schedule = []
    for arm in "AB":
        expected_schedule.append(("warmup", 0, "AB", arm))
    for pair, order in enumerate(EXPECTED_ORDERS, 1):
        for arm in order:
            expected_schedule.append(("measured", pair, order, arm))
    actual_schedule = [(row.get("kind"), row.get("pair"), row.get("order"), row.get("arm")) for row in rows]
    if actual_schedule != expected_schedule:
        reasons.append("schedule-or-chronology")

    exact = []
    for row in rows:
        required = (
            row.get("status") == "PASS"
            and row.get("input_bytes_consumed") == EXPECTED_BYTES
            and row.get("bytes_scanned") == EXPECTED_BYTES
            and row.get("source_read_bytes") == EXPECTED_BYTES
            and row.get("output_occurrences") == EXPECTED_CHUNKS
            and row.get("callback_count") == EXPECTED_CHUNKS
            and row.get("sum_occurrence_lengths") == EXPECTED_BYTES
            and row.get("terminal_occurrence_sum") == EXPECTED_BYTES
            and row.get("reconstructed_source_blake3") == EXPECTED_SOURCE_BLAKE3
            and row.get("scanner_chunk_buffer_capacity") == 32_768
            and row.get("boundary_record_capacity") == EXPECTED_CHUNKS
            and 0 < row.get("minimum_occurrence_length", 0) <= row.get("maximum_occurrence_length", 0) <= 32_768
            and row.get("scan_wall_ns", 0) > 0
        )
        if not required:
            reasons.append(f"row-contract:{row.get('label')}")
            continue
        boundary = root / row["boundary_file"]
        try:
            summary = validate_boundaries(boundary)
        except Exception as error:
            reasons.append(f"boundary-file:{row.get('label')}:{error}")
            continue
        if sha256(boundary) != row.get("boundary_file_sha256"):
            reasons.append(f"boundary-custody:{row.get('label')}")
        if summary != {
            "count": row["output_occurrences"],
            "sum": row["sum_occurrence_lengths"],
            "minimum": row["minimum_occurrence_length"],
            "maximum": row["maximum_occurrence_length"],
        }:
            reasons.append(f"boundary-summary:{row.get('label')}")
        exact.append(summary)

    parity_fields = [
        "ordered_boundary_transcript_blake3",
        "reconstructed_source_blake3",
        "boundary_file_sha256",
        "minimum_occurrence_length",
        "maximum_occurrence_length",
        "source_read_calls",
        "source_nonempty_read_calls",
        "source_read_bytes",
        "scanner_chunk_buffer_capacity",
        "boundary_record_capacity",
    ]
    for field in parity_fields:
        if len({row.get(field) for row in rows}) != 1:
            reasons.append(f"parity:{field}")
    if any(row.get("binary_sha256") != custody[f"{row.get('arm')}_screen_binary_sha256"] for row in rows):
        reasons.append("binary-custody")
    if sha256(raw_path) != acquisition.get("screen_raw_sha256"):
        reasons.append("raw-custody")

    measured = [row for row in rows if row.get("kind") == "measured"]
    pair_results = []
    for pair, order in enumerate(EXPECTED_ORDERS, 1):
        selected = [row for row in measured if row["pair"] == pair]
        by_arm = {row["arm"]: row for row in selected}
        if set(by_arm) != {"A", "B"}:
            reasons.append(f"pair-shape:{pair}")
            continue
        control = by_arm["A"]
        candidate = by_arm["B"]
        pair_results.append({
            "pair": pair,
            "order": order,
            "control_ms": control["scan_wall_ns"] / 1_000_000,
            "candidate_ms": candidate["scan_wall_ns"] / 1_000_000,
            "candidate_saved_ms": (control["scan_wall_ns"] - candidate["scan_wall_ns"]) / 1_000_000,
            "candidate_faster": candidate["scan_wall_ns"] < control["scan_wall_ns"],
            "control_position": control["position"],
            "candidate_position": candidate["position"],
            "rss_ratio": candidate["maximum_resident_set_bytes"] / control["maximum_resident_set_bytes"],
        })

    controls = [row for row in measured if row.get("arm") == "A"]
    candidates = [row for row in measured if row.get("arm") == "B"]
    control_ns = mean(controls, "scan_wall_ns") if len(controls) == 4 else float("nan")
    candidate_ns = mean(candidates, "scan_wall_ns") if len(candidates) == 4 else float("nan")
    saved_ns = control_ns - candidate_ns
    relative = saved_ns / control_ns if control_ns > 0 else float("-inf")
    pair_wins = sum(result["candidate_faster"] for result in pair_results)
    position_results = []
    for position in (1, 2):
        position_control = [row for row in controls if row["position"] == position]
        position_candidate = [row for row in candidates if row["position"] == position]
        if len(position_control) != 2 or len(position_candidate) != 2:
            reasons.append(f"position-shape:{position}")
            continue
        a = mean(position_control, "scan_wall_ns")
        b = mean(position_candidate, "scan_wall_ns")
        position_results.append({"position": position, "control_ms": a / 1_000_000,
                                 "candidate_ms": b / 1_000_000, "candidate_faster": b < a})

    cpu_ok = bool(controls and candidates) and (
        mean(candidates, "user_seconds") <= mean(controls, "user_seconds") * 1.05
        and mean(candidates, "system_seconds") <= mean(controls, "system_seconds") * 1.05
    )
    rss_ratios = [result["rss_ratio"] for result in pair_results]
    rss_ok = len(rss_ratios) == 4 and statistics.median(rss_ratios) <= 1.05 and sum(ratio <= 1.05 for ratio in rss_ratios) >= 3
    allocation_ok = all(row.get("scanner_chunk_buffer_capacity") == 32_768 for row in rows)
    parity_ok = not reasons
    signal_ok = (
        parity_ok
        and saved_ns >= 15_000_000
        and relative >= 0.10
        and pair_wins >= 3
        and len(position_results) == 2
        and all(result["candidate_faster"] for result in position_results)
        and cpu_ok
        and rss_ok
        and allocation_ok
    )
    if not parity_ok:
        disposition = "FASTCDC EXACT HOT LOOP PARITY FAIL / REVERT"
    elif signal_ok:
        disposition = "FASTCDC EXACT HOT LOOP SCREEN GO / ADVANCE"
    else:
        disposition = "FASTCDC EXACT HOT LOOP NO-GO / REVERT"

    return {
        "status": "PASS" if parity_ok else "FAIL",
        "disposition": disposition,
        "advance_to_durable": signal_ok,
        "reasons": sorted(set(reasons)),
        "rows": len(rows),
        "measured_rows": len(measured),
        "pair_results": pair_results,
        "position_results": position_results,
        "control_position_balanced_ms": control_ns / 1_000_000,
        "candidate_position_balanced_ms": candidate_ns / 1_000_000,
        "candidate_saved_ms": saved_ns / 1_000_000,
        "relative_improvement": relative,
        "pairs_favoring_candidate": pair_wins,
        "cpu_gate_pass": cpu_ok,
        "rss_gate_pass": rss_ok,
        "allocation_gate_pass": allocation_ok,
        "parity_gate_pass": parity_ok,
        "performance_signal_pass": signal_ok,
        "ordered_boundary_transcript_blake3": rows[0].get("ordered_boundary_transcript_blake3") if rows else None,
        "boundary_file_sha256": rows[0].get("boundary_file_sha256") if rows else None,
        "minimum_occurrence_length": rows[0].get("minimum_occurrence_length") if rows else None,
        "maximum_occurrence_length": rows[0].get("maximum_occurrence_length") if rows else None,
        "source_read_calls": rows[0].get("source_read_calls") if rows else None,
        "source_nonempty_read_calls": rows[0].get("source_nonempty_read_calls") if rows else None,
    }


def write_outputs(root, result):
    (root / "SCREEN-ANALYSIS-v1.json").write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
    lines = [
        "# FastCDC exact hot-loop mechanism screen v1",
        "",
        f"Disposition: **{result['disposition']}**",
        "",
        "| Pair | Order | Control ms | Candidate ms | Saved ms | Candidate faster |",
        "|---:|:---:|---:|---:|---:|:---:|",
    ]
    for row in result["pair_results"]:
        lines.append(f"| {row['pair']} | {row['order']} | {row['control_ms']:.6f} | {row['candidate_ms']:.6f} | {row['candidate_saved_ms']:.6f} | {str(row['candidate_faster']).lower()} |")
    lines.extend([
        "",
        f"Position-balanced control/candidate: {result['control_position_balanced_ms']:.6f} / {result['candidate_position_balanced_ms']:.6f} ms.",
        f"Saved: {result['candidate_saved_ms']:.6f} ms ({result['relative_improvement'] * 100:.6f}%).",
        f"Pair wins: {result['pairs_favoring_candidate']}/4; parity: {result['parity_gate_pass']}; CPU: {result['cpu_gate_pass']}; RSS: {result['rss_gate_pass']}.",
        "",
    ])
    (root / "SCREEN-REPORT-v1.md").write_text("\n".join(lines))


def main():
    root = Path(sys.argv[1]).resolve()
    result = analyze(root)
    write_outputs(root, result)
    print(json.dumps(result, sort_keys=True))
    return 0 if result["parity_gate_pass"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
