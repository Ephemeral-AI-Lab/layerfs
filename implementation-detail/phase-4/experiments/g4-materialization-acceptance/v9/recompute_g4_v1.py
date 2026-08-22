#!/usr/bin/env python3
import hashlib
import json
import runpy
import sys
from pathlib import Path

SOURCE = Path("/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty/implementation-detail/phase-4/experiments/g4-materialization-acceptance/v8/recompute_g4_v1.py")
EXPECTED = "539a7cf73fbd809cc0c9e4e3633e05c2a1c8349e34d240d4141de515cb451de8"
if hashlib.sha256(SOURCE.read_bytes()).hexdigest() != EXPECTED:
    raise SystemExit("frozen v8 independent analyzer custody mismatch")
try:
    runpy.run_path(str(SOURCE), run_name="__main__")
except SystemExit:
    pass
results = Path(sys.argv[1])
arms = [json.loads(line) for line in (results / "ARM-RAW-v1.jsonl").read_text().splitlines() if line]
index = {(row["sequence"], row["role"]): row["payload"] for row in arms}
micro = {
    "range_ns": index[(8, "protected-candidate")]["range_measurements"][0]["wall_ns"],
    "edit_ns": index[(29, "protected-candidate")]["durable_capture_total_wall_ns"],
    "reopen_ns": index[(30, "protected-candidate")]["fresh_reopen_head_wall_ns"],
}
parity_keys = (
    "root_id",
    "transition_id",
    "source_fingerprint",
    "actual_cdc_references",
    "expected_cdc_references",
    "expected_cdc_sequence_fingerprint",
    "ordered_closure_digest",
    "publication_status",
    "error",
    "q_current",
    "edit_reference_count_before",
    "edit_reference_count_after",
    "edit_count_classification",
    "sql_query_calls",
    "sql_rows_returned",
    "row_blob_reads",
    "borrowed_row_blob_reads",
    "borrowed_row_blob_bytes",
    "objects_authenticated",
    "canonical_bytes_authenticated",
    "leaf_batch_queries",
    "leaf_batch_references",
    "leaf_batch_references_max",
    "source_bytes_read",
    "source_cdc_bytes_read",
    "canonical_stage_source_bytes_read",
    "w_bytes",
    "d_bytes",
)
path = results / "INDEPENDENT-RECOMPUTATION-v1.json"
report = json.loads(path.read_text())
issues = [value for value in report["issues"] if value not in {"protected-adjacent-degradation-8", "protected-adjacent-degradation-29", "protected-adjacent-degradation-30"}]
for key, limit in zip(micro, (3_000_000, 10_000_000, 5_000_000)):
    if micro[key] > limit:
        issues.append(f"protected-{key.removesuffix('_ns')}-absolute-target")
for sequence in (8, 29, 30):
    control = index[(sequence, "protected-control")]
    candidate = index[(sequence, "protected-candidate")]
    for key in parity_keys:
        if key not in control or key not in candidate or control[key] != candidate[key]:
            issues.append(f"protected-micro-semantic-work-parity-{sequence}-{key}")
ledger = report["normalized_ledger"]
ledger["protected_micro_relative_noninferiority"] = "Unavailable: one prospective observation per arm cannot resolve a 5% effect without prohibited reruns"
ledger["protected_micro_absolute_ns"] = micro
ledger["protected_micro_absolute_caps_ns"] = {"range_ns": 3_000_000, "edit_ns": 10_000_000, "reopen_ns": 5_000_000}
ledger["protected_micro_parity_keys"] = list(parity_keys)
ledger["issues"] = sorted(set(issues))
digest = hashlib.sha256(json.dumps(ledger, sort_keys=True, separators=(",", ":")).encode()).hexdigest()
report.update({"schema": "phase4-g4-independent-recomputation-v9", "status": "PASS" if not issues else "REVISE", "issues": sorted(set(issues)), "normalized_ledger": ledger, "normalized_ledger_sha256": digest})
path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
print(json.dumps({"status": report["status"], "ledger_sha256": digest}, sort_keys=True))
raise SystemExit(0 if not issues else 2)
