#!/usr/bin/env python3
"""Independent narrow recomputation of the sealed G2 raw rows."""

import json
import statistics
import sys
from pathlib import Path

FAMILIES = (
    "sqlite_blob_acquisition_wall_ns",
    "canonical_authentication_wall_ns",
    "mapping_validation_wall_ns",
    "closure_commitment_wall_ns",
    "occurrence_commitment_wall_ns",
    "source_fingerprint_wall_ns",
    "secondary_bytes_decode_wall_ns",
)


def main():
    results = Path(sys.argv[1])
    rows = [json.loads(line) for line in (results / "rows-v1/G2-RAW-v1.jsonl").read_text().splitlines() if line]
    primary = [row for row in rows if row["workload"] == "primary"]
    measured = [row for row in primary if row["kind"] == "measured"]
    b_rows = [row for row in measured if row["arm"] == "B"]
    failures = []
    for row in rows:
        if row["status"] != "PASS" or row["q_current"] != 0 or row["residue_files"]:
            failures.append(f"{row['label']}:semantic-or-resource")
    for row in b_rows:
        g2 = row["g2_decomposition"]
        if sum(g2[name] for name in FAMILIES) != g2["direct_timer_sum_wall_ns"]:
            failures.append(f"{row['label']}:direct-sum")
        if g2["direct_timer_sum_wall_ns"] + g2["raw_residual_wall_ns"] != row["reconstruction_wall_ns"]:
            failures.append(f"{row['label']}:parent-equation")
        if g2["sqlite_cache_writes"] or g2["sqlite_cache_spills"] or g2["sqlite_status_errors"]:
            failures.append(f"{row['label']}:sqlite-read-status")
    pairs = []
    for pair_id in range(1, 5):
        pair = {row["arm"]: row for row in measured if row["pair"] == pair_id}
        if set(pair) != {"A", "B"}:
            failures.append(f"pair-{pair_id}:shape")
            continue
        ratio = pair["B"]["reconstruction_wall_ns"] / pair["A"]["reconstruction_wall_ns"]
        pairs.append(ratio)
        if ratio > 1.05:
            failures.append(f"pair-{pair_id}:ratio")
    centers = {
        arm: statistics.mean(row["reconstruction_wall_ns"] for row in measured if row["arm"] == arm)
        for arm in "AB"
    }
    eligible = []
    family_medians = {}
    for name in FAMILIES:
        values = [row["g2_decomposition"][name] for row in b_rows]
        family_medians[name] = statistics.median(values) if values else None
        # The only statically removable lane is the second decode.
        if name == "secondary_bytes_decode_wall_ns" and len(values) == 4 and min(values) >= 33_000_000:
            eligible.append(name)
    disposition = (
        "G2 REVISE"
        if failures
        else f"G2 PASS / SELECT {eligible[0]}"
        if len(eligible) == 1
        else "G2 PASS / INSUFFICIENT_EVIDENCE FOR A CONSTANT-FACTOR CANDIDATE"
    )
    result = {
        "schema": "phase4-g2-materialization-decomposition-independent-recomputation-v1",
        "status": "REVISE" if failures else "PASS",
        "disposition": disposition,
        "failures": sorted(set(failures)),
        "rows": len(rows),
        "primary_rows": len(primary),
        "measured_primary_rows": len(measured),
        "pair_ratios": pairs,
        "position_balanced_ratio": centers["B"] / centers["A"],
        "timer_regions": sorted({row["g2_decomposition"]["timer_regions"] for row in b_rows}),
        "family_medians_ns": family_medians,
        "eligible_families": eligible,
    }
    (results / "INDEPENDENT-RECOMPUTATION-v1.json").write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
    print(json.dumps({"status": result["status"], "disposition": disposition}, sort_keys=True))
    return 0 if not failures else 2


if __name__ == "__main__":
    raise SystemExit(main())
