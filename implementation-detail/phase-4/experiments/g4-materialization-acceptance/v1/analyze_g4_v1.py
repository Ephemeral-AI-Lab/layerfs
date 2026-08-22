#!/usr/bin/env python3
import hashlib
import json
import sys
from pathlib import Path


RSS_LIMIT = 20_971_520


def read_jsonl(path):
    return [json.loads(line) for line in Path(path).read_text().splitlines() if line]


def main(results):
    results = Path(results)
    arms = read_jsonl(results / "ARM-RAW-v1.jsonl")
    records = read_jsonl(results / "G4-RAW-v1.jsonl")
    issues = []
    if len(records) != 30 or [row["sequence"] for row in records] != list(range(1, 31)):
        issues.append("exact-30-record-chronology")
    if len(arms) != 50:
        issues.append("exact-50-arm-observations")

    def one(sequence, role):
        matches = [row for row in arms if row["sequence"] == sequence and row["role"] == role]
        if len(matches) != 1:
            raise ValueError(f"missing arm {sequence}/{role}")
        return matches[0]

    r0 = one(3, "r0-control")["payload"]
    attribution = one(3, "r1-attribution-control")["payload"]
    candidate = one(3, "r1-candidate")["payload"]
    parity = [
        "root", "output_digest", "occurrence_digest", "references",
        "total_sql_query_calls", "total_sql_rows_returned", "mapping_singleton_query_calls",
        "mapping_rows_returned", "chunk_batch_query_calls", "chunk_rows_returned",
        "leaf_batch_references", "leaf_batch_references_max", "borrowed_chunk_blob_reads",
        "borrowed_chunk_blob_bytes", "authenticated_objects", "canonical_bytes_authenticated",
        "output_digest_hashes", "output_digest_bytes_hashed", "occurrence_fold_entries",
        "occurrence_fold_bytes", "q_current",
    ]
    if any(r0.get(key) != attribution.get(key) or attribution.get(key) != candidate.get(key) for key in parity):
        issues.append("r01-proof-work-parity")
    if attribution.get("content_closure_status") != "computed" or candidate.get("content_closure_status") != "derived-not-computed":
        issues.append("closure-mode-attribution")
    if candidate.get("closure_fold_updates") != 0 or candidate.get("closure_fold_canonical_bytes") != 0:
        issues.append("closure-off-work-not-zero")
    if candidate["operation_wall_ns"] > 333_000_000:
        issues.append("warm-reconstruction-target")
    if candidate["operation_wall_ns"] * 100 > attribution["operation_wall_ns"] * 95:
        issues.append("closure-direct-improvement")
    fresh = one(4, "r1-candidate")["payload"]
    if fresh["operation_wall_ns"] > 400_000_000:
        issues.append("fresh-reconstruction-target")

    expected_shape = {
        "total_sql_query_calls": 170,
        "total_sql_rows_returned": 5371,
        "mapping_singleton_query_calls": 87,
        "mapping_rows_returned": 87,
        "chunk_batch_query_calls": 83,
        "chunk_rows_returned": 5284,
        "leaf_batch_references": 5284,
        "leaf_batch_references_max": 64,
        "borrowed_chunk_blob_reads": 5284,
        "borrowed_chunk_blob_bytes": 104926292,
        "authenticated_objects": 5371,
        "canonical_bytes_authenticated": 105122401,
        "output_digest_hashes": 1,
        "output_digest_bytes_hashed": 104857600,
        "occurrence_fold_entries": 5284,
        "occurrence_fold_bytes": 190224,
    }
    if any(candidate.get(key) != value for key, value in expected_shape.items()):
        issues.append("accepted-s1-100-reconstruction-shape")

    seed = one(7, "s1-candidate")["payload"]
    if seed["qualified_no_digest_wall_ns"] > 50_000_000 or seed["read_bytes"] != 104857600 or not seed["identity_stable"]:
        issues.append("seed-full-read-target")
    if seed["digest_read_bytes"] != seed["read_bytes"] or seed["digest"] != candidate["output_digest"]:
        issues.append("seed-digest-separation")

    m0 = one(14, "m0-candidate")["payload"]
    if m0["operation_wall_ns"] > 400_000_000:
        issues.append("first-native-target")
    if any(m0.get(key) != value for key, value in expected_shape.items() if key not in {"mapping_singleton_query_calls", "mapping_rows_returned"}):
        issues.append("m0-batched-proof-shape")
    if m0["chunk_scalar_query_calls"] != 0 or m0["native_write_bytes"] != 104857600 or m0["native_short_writes"] or m0["native_write_errors"]:
        issues.append("m0-writer-shape")
    if m0["temp_residue_count"] or m0["final_residue_count"] or m0["q_current"]:
        issues.append("m0-terminal-cleanup")

    g3_ratios = {}
    for sequence in range(16, 28):
        control = one(sequence, "g3-control")["payload"]
        protected = one(sequence, "s1-candidate")["payload"]
        if protected.get("output_digest") != control.get("output_digest") or protected.get("old_or_new") != control.get("old_or_new") or protected.get("error") != control.get("error"):
            issues.append(f"g3-semantic-parity-{sequence}")
        c_ns, p_ns = control["operation_total_ns"], protected["operation_total_ns"]
        g3_ratios[str(sequence)] = {"control_ns": c_ns, "candidate_ns": p_ns}
        if p_ns * 100 > c_ns * 105:
            issues.append(f"g3-adjacent-degradation-{sequence}")
    if one(17, "s1-candidate")["payload"]["operation_total_ns"] > 10_000_000:
        issues.append("clone-noop-target")
    if one(18, "s1-candidate")["payload"]["operation_total_ns"] > 10_000_000:
        issues.append("one-byte-target")
    if one(19, "s1-candidate")["payload"]["operation_total_ns"] > 20_000_000:
        issues.append("one-mib-target")

    def guard_ns(sequence, payload):
        if sequence == 8:
            values = payload.get("range_measurements", [])
            return values[0]["wall_ns"] if values else 0
        if sequence == 30:
            return payload["fresh_reopen_head_wall_ns"]
        return payload["durable_capture_total_wall_ns"]

    guard_ratios = {}
    for sequence in (8, 28, 29, 30):
        control = one(sequence, "protected-control")["payload"]
        protected = one(sequence, "protected-candidate")["payload"]
        c_ns, p_ns = guard_ns(sequence, control), guard_ns(sequence, protected)
        guard_ratios[str(sequence)] = {"control_ns": c_ns, "candidate_ns": p_ns}
        if not c_ns or not p_ns or p_ns * 100 > c_ns * 105:
            issues.append(f"protected-adjacent-degradation-{sequence}")
        for key in ("root_id", "transition_id", "source_fingerprint", "actual_cdc_references", "publication_status", "error"):
            if control.get(key) != protected.get(key):
                issues.append(f"protected-semantic-parity-{sequence}-{key}")

    max_rss = max(row["external"]["maximum_resident_set_bytes"] for row in arms)
    if max_rss > RSS_LIMIT:
        issues.append("rss-limit")
    if any(row["payload"].get("q_current", 0) != 0 for row in arms):
        issues.append("terminal-q")
    cold = [row for row in records if row["kind"] == "cold-unavailable"]
    if len(cold) != 2 or any(row["status"] != "Unavailable" for row in cold):
        issues.append("cold-administrative-cells")
    def measured_ns(row):
        value = row["payload"]
        if value.get("mode") == "seed-read":
            return value["qualified_no_digest_wall_ns"] + value["qualified_digest_wall_ns"]
        if "operation_wall_ns" in value:
            return value["operation_wall_ns"]
        if "operation_total_ns" in value:
            return value["operation_total_ns"]
        return value.get("complete_lifecycle_total_wall_ns", 0)
    measured_operation_sum_ns = sum(measured_ns(row) for row in arms)
    if measured_operation_sum_ns > 20_000_000_000:
        issues.append("measured-operation-sum")

    ledger = {
        "record_count": len(records),
        "arm_count": len(arms),
        "r0_control_ns": r0["operation_wall_ns"],
        "r1_attribution_control_ns": attribution["operation_wall_ns"],
        "r1_candidate_ns": candidate["operation_wall_ns"],
        "r1_fresh_ns": fresh["operation_wall_ns"],
        "r1_queries": candidate["total_sql_query_calls"],
        "r1_rows": candidate["total_sql_rows_returned"],
        "r1_leaf_batches": candidate["chunk_batch_query_calls"],
        "r1_chunk_rows": candidate["chunk_rows_returned"],
        "m0_candidate_ns": m0["operation_wall_ns"],
        "m0_native_write_calls": m0["native_write_calls"],
        "m0_data_sync_and_publish_included": True,
        "seed_no_digest_ns": seed["qualified_no_digest_wall_ns"],
        "seed_digest_ns": seed["qualified_digest_wall_ns"],
        "max_rss_bytes": max_rss,
        "measured_operation_sum_ns": measured_operation_sum_ns,
        "g3_adjacent": g3_ratios,
        "protected_adjacent": guard_ratios,
        "cold_unavailable_records": len(cold),
        "issues": sorted(set(issues)),
    }
    digest = hashlib.sha256(json.dumps(ledger, sort_keys=True, separators=(",", ":")).encode()).hexdigest()
    report = {
        "schema": "phase4-g4-primary-analysis-v1",
        "status": "PASS" if not issues else "REVISE",
        "normalized_ledger": ledger,
        "normalized_ledger_sha256": digest,
        "issues": sorted(set(issues)),
    }
    path = results / "PRIMARY-ANALYSIS-v1.json"
    path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    print(json.dumps({"status": report["status"], "ledger_sha256": digest}, sort_keys=True))
    return 0 if not issues else 2


if __name__ == "__main__":
    if len(sys.argv) != 2:
        raise SystemExit("usage: analyze_g4_v1.py RESULTS")
    raise SystemExit(main(sys.argv[1]))
