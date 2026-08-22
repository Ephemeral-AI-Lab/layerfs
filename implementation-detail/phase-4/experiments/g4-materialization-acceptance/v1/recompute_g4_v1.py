#!/usr/bin/env python3
import hashlib
import json
import sys
from pathlib import Path


def lines(path):
    return [json.loads(value) for value in Path(path).read_text().splitlines() if value]


def recompute(results):
    results = Path(results)
    arms = lines(results / "ARM-RAW-v1.jsonl")
    records = lines(results / "G4-RAW-v1.jsonl")
    index = {(row["sequence"], row["role"]): row for row in arms}
    payload = lambda sequence, role: index[(sequence, role)]["payload"]
    problems = []

    r0 = payload(3, "r0-control")
    on = payload(3, "r1-attribution-control")
    off = payload(3, "r1-candidate")
    fresh = payload(4, "r1-candidate")
    seed = payload(7, "s1-candidate")
    m0 = payload(14, "m0-candidate")
    parity = (
        "root", "output_digest", "occurrence_digest", "references", "total_sql_query_calls",
        "total_sql_rows_returned", "mapping_singleton_query_calls", "mapping_rows_returned",
        "chunk_batch_query_calls", "chunk_rows_returned", "leaf_batch_references",
        "leaf_batch_references_max", "borrowed_chunk_blob_reads", "borrowed_chunk_blob_bytes",
        "authenticated_objects", "canonical_bytes_authenticated", "output_digest_hashes",
        "output_digest_bytes_hashed", "occurrence_fold_entries", "occurrence_fold_bytes", "q_current",
    )
    if any(len({json.dumps(item.get(key), sort_keys=True) for item in (r0, on, off)}) != 1 for key in parity):
        problems.append("r01-proof-work-parity")
    if on.get("content_closure_status") != "computed" or off.get("content_closure_status") != "derived-not-computed":
        problems.append("closure-mode-attribution")
    if off.get("closure_fold_updates") or off.get("closure_fold_canonical_bytes"):
        problems.append("closure-off-work-not-zero")
    if off["operation_wall_ns"] > 333_000_000:
        problems.append("warm-reconstruction-target")
    if off["operation_wall_ns"] * 100 > on["operation_wall_ns"] * 95:
        problems.append("closure-direct-improvement")
    if fresh["operation_wall_ns"] > 400_000_000:
        problems.append("fresh-reconstruction-target")

    shape = (170, 5371, 87, 87, 83, 5284, 5284, 64, 5284, 104926292, 5371, 105122401, 1, 104857600, 5284, 190224)
    keys = ("total_sql_query_calls", "total_sql_rows_returned", "mapping_singleton_query_calls", "mapping_rows_returned", "chunk_batch_query_calls", "chunk_rows_returned", "leaf_batch_references", "leaf_batch_references_max", "borrowed_chunk_blob_reads", "borrowed_chunk_blob_bytes", "authenticated_objects", "canonical_bytes_authenticated", "output_digest_hashes", "output_digest_bytes_hashed", "occurrence_fold_entries", "occurrence_fold_bytes")
    if tuple(off.get(key) for key in keys) != shape:
        problems.append("accepted-s1-100-reconstruction-shape")
    if seed["qualified_no_digest_wall_ns"] > 50_000_000 or seed["read_bytes"] != 104857600 or seed["identity_stable"] is not True:
        problems.append("seed-full-read-target")
    if seed["digest_read_bytes"] != seed["read_bytes"] or seed["digest"] != off["output_digest"]:
        problems.append("seed-digest-separation")
    if m0["operation_wall_ns"] > 400_000_000:
        problems.append("first-native-target")
    m0_keys = tuple(key for key in keys if key not in {"mapping_singleton_query_calls", "mapping_rows_returned"})
    shape_by_key = dict(zip(keys, shape))
    if any(m0.get(key) != shape_by_key[key] for key in m0_keys):
        problems.append("m0-batched-proof-shape")
    if m0["chunk_scalar_query_calls"] or m0["native_write_bytes"] != 104857600 or m0["native_short_writes"] or m0["native_write_errors"]:
        problems.append("m0-writer-shape")
    if m0["temp_residue_count"] or m0["final_residue_count"] or m0["q_current"]:
        problems.append("m0-terminal-cleanup")

    g3 = {}
    for sequence in range(16, 28):
        control, candidate = payload(sequence, "g3-control"), payload(sequence, "s1-candidate")
        if (control.get("output_digest"), control.get("old_or_new"), control.get("error")) != (candidate.get("output_digest"), candidate.get("old_or_new"), candidate.get("error")):
            problems.append(f"g3-semantic-parity-{sequence}")
        g3[str(sequence)] = {"control_ns": control["operation_total_ns"], "candidate_ns": candidate["operation_total_ns"]}
        if candidate["operation_total_ns"] * 100 > control["operation_total_ns"] * 105:
            problems.append(f"g3-adjacent-degradation-{sequence}")
    if payload(17, "s1-candidate")["operation_total_ns"] > 10_000_000:
        problems.append("clone-noop-target")
    if payload(18, "s1-candidate")["operation_total_ns"] > 10_000_000:
        problems.append("one-byte-target")
    if payload(19, "s1-candidate")["operation_total_ns"] > 20_000_000:
        problems.append("one-mib-target")

    def protected_ns(sequence, item):
        if sequence == 8:
            return item["range_measurements"][0]["wall_ns"] if item.get("range_measurements") else 0
        return item["fresh_reopen_head_wall_ns"] if sequence == 30 else item["durable_capture_total_wall_ns"]

    guards = {}
    for sequence in (8, 28, 29, 30):
        control, candidate = payload(sequence, "protected-control"), payload(sequence, "protected-candidate")
        c_ns, p_ns = protected_ns(sequence, control), protected_ns(sequence, candidate)
        guards[str(sequence)] = {"control_ns": c_ns, "candidate_ns": p_ns}
        if not c_ns or not p_ns or p_ns * 100 > c_ns * 105:
            problems.append(f"protected-adjacent-degradation-{sequence}")
        for key in ("root_id", "transition_id", "source_fingerprint", "actual_cdc_references", "publication_status", "error"):
            if control.get(key) != candidate.get(key):
                problems.append(f"protected-semantic-parity-{sequence}-{key}")

    if len(records) != 30 or [item["sequence"] for item in records] != list(range(1, 31)):
        problems.append("exact-30-record-chronology")
    if len(arms) != 50:
        problems.append("exact-50-arm-observations")
    max_rss = max(item["external"]["maximum_resident_set_bytes"] for item in arms)
    if max_rss > 20_971_520:
        problems.append("rss-limit")
    if any(item["payload"].get("q_current", 0) for item in arms):
        problems.append("terminal-q")
    cold = [item for item in records if item["kind"] == "cold-unavailable"]
    if len(cold) != 2 or any(item["status"] != "Unavailable" for item in cold):
        problems.append("cold-administrative-cells")
    def operation_ns(item):
        value = item["payload"]
        if value.get("mode") == "seed-read":
            return value["qualified_no_digest_wall_ns"] + value["qualified_digest_wall_ns"]
        return value.get("operation_wall_ns", value.get("operation_total_ns", value.get("complete_lifecycle_total_wall_ns", 0)))
    measured_operation_sum_ns = sum(operation_ns(item) for item in arms)
    if measured_operation_sum_ns > 20_000_000_000:
        problems.append("measured-operation-sum")

    ledger = {
        "record_count": len(records), "arm_count": len(arms),
        "r0_control_ns": r0["operation_wall_ns"],
        "r1_attribution_control_ns": on["operation_wall_ns"],
        "r1_candidate_ns": off["operation_wall_ns"], "r1_fresh_ns": fresh["operation_wall_ns"],
        "r1_queries": off["total_sql_query_calls"], "r1_rows": off["total_sql_rows_returned"],
        "r1_leaf_batches": off["chunk_batch_query_calls"], "r1_chunk_rows": off["chunk_rows_returned"],
        "m0_candidate_ns": m0["operation_wall_ns"], "m0_native_write_calls": m0["native_write_calls"],
        "m0_data_sync_and_publish_included": True,
        "seed_no_digest_ns": seed["qualified_no_digest_wall_ns"], "seed_digest_ns": seed["qualified_digest_wall_ns"],
        "max_rss_bytes": max_rss, "measured_operation_sum_ns": measured_operation_sum_ns, "g3_adjacent": g3, "protected_adjacent": guards,
        "cold_unavailable_records": len(cold), "issues": sorted(set(problems)),
    }
    digest = hashlib.sha256(json.dumps(ledger, sort_keys=True, separators=(",", ":")).encode()).hexdigest()
    report = {"schema": "phase4-g4-independent-recomputation-v1", "status": "PASS" if not problems else "REVISE", "normalized_ledger": ledger, "normalized_ledger_sha256": digest, "issues": sorted(set(problems))}
    (results / "INDEPENDENT-RECOMPUTATION-v1.json").write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    print(json.dumps({"status": report["status"], "ledger_sha256": digest}, sort_keys=True))
    return 0 if not problems else 2


if __name__ == "__main__":
    if len(sys.argv) != 2:
        raise SystemExit("usage: recompute_g4_v1.py RESULTS")
    raise SystemExit(recompute(sys.argv[1]))
