#!/usr/bin/env python3
import hashlib
import json
import sys

SIZES = (1_048_576, 10_485_760, 104_857_600, 524_288_000)
OPS = ("plus1-early", "plus1-middle")
PROFILE = "b0ebb845409ef995a5fa454bb23d10a80c6ecf44deb7832ca2ce1213eb0f4ba1"


def median(values):
    values = sorted(values)
    return values[len(values) // 2]


def changed_topology(old, position):
    leaves_total = (old + 64) // 64
    first = position // 64
    leaves = leaves_total - first
    branches = 0
    while leaves_total > 64:
        leaves_total = (leaves_total + 63) // 64
        first //= 64
        branches += leaves_total - first
    return leaves, branches


def main(path):
    raw = open(path, "rb").read()
    rows = [json.loads(line) for line in raw.decode().splitlines() if line.strip()]
    reasons = []
    measured = [row for row in rows if row.get("row_kind") == "measured"]
    warmups = [row for row in rows if row.get("row_kind") == "warmup"]
    roundtrips = [row for row in rows if row.get("row_kind") == "roundtrip"]
    if (len(rows), len(measured), len(warmups), len(roundtrips)) != (34, 24, 8, 2):
        reasons.append("schedule")
    arms = []
    for size in SIZES:
        for operation in OPS:
            group = [row for row in measured if row["size_bytes"] == size and row["operation"] == operation]
            if len(group) != 3:
                reasons.append(f"samples:{size}:{operation}")
                continue
            old = group[0]["old_references"]
            position = 0 if operation == "plus1-early" else old // 2
            leaves, branches = changed_topology(old, position)
            stable = (
                "root_id", "transition_id", "ordered_closure_digest", "old_references",
                "suffix_references", "pages", "branches", "mapping_bytes_rewritten",
                "construction_put_evidences",
            )
            if any(len({row[key] for row in group}) != 1 for key in stable):
                reasons.append(f"unstable:{size}:{operation}")
            for row in group:
                pre = next(phase for phase in row["phase_counters"] if phase["phase"] == "precommit_closure")
                mapping = next(phase for phase in row["phase_counters"] if phase["phase"] == "canonical_cas_mapping")
                valid = (
                    row["status"] == "PASS" and row["error"] is None and row["profile_id"] == PROFILE
                    and row["transactions"] == row["commits"] == row["construction_proof_consumptions"] == 1
                    and row["q_current"] == 0 and row["source_bytes_read"] == 1
                    and row["suffix_references"] == old - position
                    and row["pages"] == leaves and row["branches"] == branches
                    and row["construction_put_evidences"] == leaves + branches + 5
                    and mapping["incremental_receipt_covered_edges"] == old
                    and pre["objects_authenticated"] == pre["canonical_bytes_authenticated"] == 0
                    and pre["construction_proof_consumptions"] == 1
                )
                if not valid:
                    reasons.append(f"row:{size}:{operation}:{row['sample_index']}")
            arms.append({
                "size_bytes": size,
                "operation": operation,
                "old_references": old,
                "suffix_references": old - position,
                "changed_leaves": leaves,
                "changed_branches": branches,
                "mapping_bytes": group[0]["mapping_bytes_rewritten"],
                "construction_puts": group[0]["construction_put_evidences"],
                "q_high_water": median([row["q_high_water"] for row in group]),
                "capture_median_ns": median([row["capture_publish_wall_ns"] for row in group]),
                "mapping_median_ns": median([row["canonical_cas_mapping_stage_wall_ns"] for row in group]),
                "proof_median_ns": median([row["precommit_closure_validation_wall_ns"] for row in group]),
                "commit_median_ns": median([row["sqlite_commit_durability_wall_ns"] for row in group]),
                "authority_median_ns": median([row["same_open_authority_establishment_wall_ns"] for row in group]),
                "first_edit_median_ns": median([
                    row["same_open_authority_establishment_wall_ns"] + row["capture_publish_wall_ns"]
                    for row in group
                ]),
                "capture_min_ns": min(row["capture_publish_wall_ns"] for row in group),
                "capture_max_ns": max(row["capture_publish_wall_ns"] for row in group),
                "rss_median_bytes": median([row["external_time"]["maximum_resident_set_bytes"] for row in group]),
                "peak_median_bytes": median([row["external_time"]["peak_memory_footprint_bytes"] for row in group]),
            })
    for row in roundtrips:
        if not (row["size_bytes"] == SIZES[-1] and row["operation"] in OPS and row["status"] == "PASS"
                and row["error"] is None and row["fresh_full_scrub_wall_ns"] > 0
                and row["reconstruction_wall_ns"] > 0 and row["range_verification_wall_ns"] > 0):
            reasons.append(f"roundtrip:{row.get('operation')}")
    slopes = []
    for operation in OPS:
        selected = [arm for arm in arms if arm["operation"] == operation]
        for before, after in zip(selected, selected[1:]):
            slopes.append({
                "operation": operation,
                "from_size_bytes": before["size_bytes"],
                "to_size_bytes": after["size_bytes"],
                "size_ratio": after["size_bytes"] / before["size_bytes"],
                "suffix_ratio": after["suffix_references"] / before["suffix_references"],
                "mapping_bytes_ratio": after["mapping_bytes"] / before["mapping_bytes"],
                "mapping_wall_ratio": after["mapping_median_ns"] / before["mapping_median_ns"],
                "capture_wall_ratio": after["capture_median_ns"] / before["capture_median_ns"],
                "authority_wall_ratio": after["authority_median_ns"] / before["authority_median_ns"],
            })
    result = {
        "schema": "phase4-count-change-scale-analysis-v1",
        "status": "PASS" if not reasons else "FAIL",
        "reasons": sorted(set(reasons)),
        "raw_sha256": hashlib.sha256(raw).hexdigest(),
        "rows": {"total": len(rows), "warmup": len(warmups), "measured": len(measured), "roundtrip": len(roundtrips)},
        "arms": arms,
        "slopes": slopes,
        "classification": "O(suffix), worst-case Theta(N)",
    }
    print(json.dumps(result, sort_keys=True, separators=(",", ":")))
    return result["status"] != "PASS"


if __name__ == "__main__":
    if len(sys.argv) != 2:
        raise SystemExit(f"usage: {sys.argv[0]} RAW.jsonl")
    raise SystemExit(main(sys.argv[1]))
