#!/usr/bin/env python3
import hashlib
import json
import sys

PROFILE = "b0ebb845409ef995a5fa454bb23d10a80c6ecf44deb7832ca2ce1213eb0f4ba1"
OPS = (
    "write", "edit-same", "materialize-warm", "materialize-fresh",
    "read-range", "read-range-1m", "reopen",
)


def stats(values):
    values = sorted(values)
    return {
        "count": len(values), "median_ns": values[len(values) // 2],
        "min_ns": values[0], "max_ns": values[-1],
        "spread_ns": values[-1] - values[0],
    }


def main(path):
    raw = open(path, "rb").read()
    rows = [json.loads(line) for line in raw.decode().splitlines() if line.strip()]
    reasons = []
    counts = {kind: sum(row.get("sample_kind") == kind for row in rows)
              for kind in ("smoke", "warmup", "measured", "structural-guard")}
    if len(rows) != 42 or counts != {"smoke": 12, "warmup": 7, "measured": 21, "structural-guard": 2}:
        reasons.append("schedule")
    for index, row in enumerate(rows):
        mutation = row["operation"] == "write" or row["operation"].startswith("edit-")
        valid = (
            row["schema"] == "phase4-current-baseline-v1"
            and row["purpose"] == "product_workflow_baseline"
            and row["milestone"] == "CURRENT-BASELINE-V1"
            and row["acceptance_scope"] == "baseline"
            and row["candidate_comparison"] is False and row["promotion"] is False
            and row["status"] == "PASS" and row["error"] is None
            and row["profile_id"] == PROFILE and row["q_current"] == 0
            and (row["transactions"], row["commits"]) == ((1, 1) if mutation else (0, 0))
        )
        if not valid:
            reasons.append(f"row:{index}")
        if row["operation"] == "read-range-1m":
            ranges = row["range_measurements"]
            if not (len(ranges) == 1 and ranges[0]["label"] == "sequential-1m"
                    and ranges[0]["returned_bytes"] == 1_048_576
                    and ranges[0]["canonical_bytes_authenticated"] > 1_048_576
                    and ranges[0]["objects_authenticated"] > 0):
                reasons.append(f"sequential-range:{index}")
    measured = [row for row in rows if row["sample_kind"] == "measured"]
    arms = []
    for operation in OPS:
        group = [row for row in measured if row["operation"] == operation]
        if len(group) != 3:
            reasons.append(f"samples:{operation}")
            continue
        for key in ("root_id", "transition_id", "ordered_closure_digest"):
            if len({row[key] for row in group}) != 1:
                reasons.append(f"identity:{operation}:{key}")
        primary = (
            [row["capture_publish_wall_ns"] for row in group]
            if operation in ("write", "edit-same")
            else [row["complete_lifecycle_total_wall_ns"] for row in group]
        )
        arm = {
            "operation": operation,
            "primary": stats(primary),
            "q_high_water": stats([row["q_high_water"] for row in group]),
            "rss_bytes": stats([row["external_time"]["maximum_resident_set_bytes"] for row in group]),
            "peak_footprint_bytes": stats([row["external_time"]["peak_memory_footprint_bytes"] for row in group]),
            "user_cpu_seconds": stats([int(row["external_time"]["user_seconds"] * 1_000_000_000) for row in group]),
            "system_cpu_seconds": stats([int(row["external_time"]["system_seconds"] * 1_000_000_000) for row in group]),
        }
        if operation == "write":
            arm.update({
                "mapping": stats([row["canonical_cas_mapping_stage_wall_ns"] for row in group]),
                "proof": stats([row["precommit_closure_validation_wall_ns"] for row in group]),
                "commit": stats([row["sqlite_commit_durability_wall_ns"] for row in group]),
                "canonical_new_bytes": group[0]["canonical_new_write_bytes"],
                "mapping_bytes": group[0]["mapping_bytes_rewritten"],
            })
        elif operation == "edit-same":
            arm["authority"] = stats([row["same_open_authority_establishment_wall_ns"] for row in group])
        elif operation.startswith("materialize-"):
            arm["reconstruction"] = stats([row["reconstruction_wall_ns"] for row in group])
        elif operation.startswith("read-range"):
            arm["range"] = stats([row["range_verification_wall_ns"] for row in group])
            if operation == "read-range-1m":
                arm["returned_bytes"] = 1_048_576
                arm["returned_throughput_mib_s"] = stats([
                    int(row["range_measurements"][0]["throughput_mib_s"] * 1_000_000)
                    for row in group
                ])
                arm["authenticated_bytes"] = group[0]["range_measurements"][0]["canonical_bytes_authenticated"]
                arm["authenticated_objects"] = group[0]["range_measurements"][0]["objects_authenticated"]
        elif operation == "reopen":
            arm["reopen"] = stats([row["fresh_reopen_head_wall_ns"] for row in group])
        arms.append(arm)
    structural = []
    for row in rows:
        if row["sample_kind"] == "structural-guard":
            structural.append({
                "operation": row["operation"],
                "publication_ns": row["capture_publish_wall_ns"],
                "authority_ns": row["same_open_authority_establishment_wall_ns"],
                "first_edit_ns": row["capture_publish_wall_ns"] + row["same_open_authority_establishment_wall_ns"],
                "suffix_references": row["suffix_references"],
                "mapping_bytes": row["mapping_bytes_rewritten"],
            })
    result = {
        "schema": "phase4-current-baseline-analysis-v1",
        "status": "PASS" if not reasons else "FAIL",
        "reasons": sorted(set(reasons)),
        "raw_sha256": hashlib.sha256(raw).hexdigest(),
        "row_counts": counts,
        "arms": arms,
        "structural_guards": structural,
        "cache_scope": "OS/filesystem cache warm-or-unknown",
        "candidate_comparison": False,
        "control_use": "next candidate must run adjacent balanced A/B; do not subtract historical absolute medians",
    }
    print(json.dumps(result, sort_keys=True, separators=(",", ":")))
    return result["status"] != "PASS"


if __name__ == "__main__":
    if len(sys.argv) != 2:
        raise SystemExit(f"usage: {sys.argv[0]} RAW.jsonl")
    raise SystemExit(main(sys.argv[1]))
