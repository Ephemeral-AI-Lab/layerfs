#!/usr/bin/env python3
import hashlib
import json
import runpy
import sys
from pathlib import Path

SOURCE = Path("/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty/implementation-detail/phase-4/experiments/g4-materialization-acceptance/v1/analyze_g4_v1.py")
EXPECTED = "5dcc5a3b9283b47c18d3565a9dda457290681807491e2cad4003d8096116ab74"
CANDIDATE = "770dcfa8db17f1f9e1b90336a26923eb0530073590a9da5578e06339d85813e8"
STATIC_BUFFER_CONTROLS = {
    "535bfa178a8a569ea43d9f1d23808775c2349a29f9cdacddae508391a6e5e61e": "frozen-g3-v13-source-static-bound",
    "5d72b46d29a5b77494781f343cc6841a71879b5de426751afe744f27a033e8f5": "frozen-protected-control-source-static-bound",
}
ESTIMATED = {8, 16, 17, 18, 19, 20, 22, 24, 25, 26, 27, 29, 30}
FAST_KEYS = (
    "root_id", "transition_id", "source_fingerprint", "actual_cdc_references",
    "expected_cdc_references", "expected_cdc_sequence_fingerprint", "ordered_closure_digest",
    "publication_status", "error", "q_current", "edit_reference_count_before",
    "edit_reference_count_after", "edit_count_classification", "sql_query_calls",
    "sql_rows_returned", "row_blob_reads", "borrowed_row_blob_reads", "borrowed_row_blob_bytes",
    "objects_authenticated", "canonical_bytes_authenticated", "leaf_batch_queries",
    "leaf_batch_references", "leaf_batch_references_max", "source_bytes_read",
    "source_cdc_bytes_read", "canonical_stage_source_bytes_read", "w_bytes", "d_bytes",
)
LIMIT = 1_048_576

if hashlib.sha256(SOURCE.read_bytes()).hexdigest() != EXPECTED:
    raise SystemExit("frozen v1 primary analyzer custody mismatch")
try:
    runpy.run_path(str(SOURCE), run_name="__main__")
except SystemExit:
    pass

results = Path(sys.argv[1])
arms = [json.loads(line) for line in (results / "ARM-RAW-v1.jsonl").read_text().splitlines() if line]
index = {(row["sequence"], row["role"]): row for row in arms}
path = results / "PRIMARY-ANALYSIS-v1.json"
report = json.loads(path.read_text())
issues = list(report["issues"])


def timer(sequence, payload):
    if 16 <= sequence <= 27:
        return payload["operation_total_ns"]
    if sequence == 8:
        return payload["range_measurements"][0]["wall_ns"]
    if sequence == 30:
        return payload["fresh_reopen_head_wall_ns"]
    return payload["durable_capture_total_wall_ns"]


def raw_payload(path_value, digest):
    sample = results / path_value
    if not sample.is_file() or hashlib.sha256(sample.read_bytes()).hexdigest() != digest:
        raise ValueError(f"estimator sample custody mismatch: {path_value}")
    return json.loads([line for line in sample.read_text().splitlines() if line][-1])


def same_work(first, second, keys=None):
    if keys is None:
        keys = sorted(set(first) & set(second) - {"status", "status_adapter"})
        keys = [key for key in keys if not key.endswith("_ns") and key != "adjacent_estimator_v10"]
    return all(key in first and key in second and first[key] == second[key] for key in keys)


observations = []
estimator_ledger = {}
for sequence, row in sorted(index.items()):
    seq, role = sequence
    payload = row["payload"]
    if seq not in ESTIMATED:
        observations.append((seq, role, payload, row["binary_sha256"]))
        continue
    meta = payload.get("adjacent_estimator_v10", {})
    if meta.get("replications_per_role") != 2 or meta.get("estimator") != "equal-weight-arithmetic-mean" or meta.get("relative_limit_basis_points") != 10_500:
        issues.append(f"estimator-metadata-{seq}-{role}")
        continue
    samples = [raw_payload(name, digest) for name, digest in zip(meta.get("sample_payload_paths", []), meta.get("sample_payload_sha256", []))]
    values = [timer(seq, sample) for sample in samples]
    if len(samples) != 2 or values != meta.get("samples_ns") or sum(values) != meta.get("sum_ns") or timer(seq, payload) != (sum(values) + 1) // 2:
        issues.append(f"estimator-sample-equation-{seq}-{role}")
    keys = FAST_KEYS if seq in {8, 29, 30} else None
    if len(samples) == 2 and not same_work(samples[0], samples[1], keys):
        issues.append(f"estimator-within-role-work-parity-{seq}-{role}")
    observations.extend((seq, role, sample, row["binary_sha256"]) for sample in samples)
    estimator_ledger[f"{seq}-{role}"] = values

for sequence in range(16, 28):
    control = index[(sequence, "g3-control")]["payload"]
    candidate = index[(sequence, "s1-candidate")]["payload"]
    if not same_work(control, candidate):
        issues.append(f"g3-complete-semantic-work-parity-{sequence}")
for sequence in (8, 28, 29, 30):
    control = index[(sequence, "protected-control")]["payload"]
    candidate = index[(sequence, "protected-candidate")]["payload"]
    if not same_work(control, candidate, FAST_KEYS):
        issues.append(f"protected-complete-semantic-work-parity-{sequence}")

relative = {}
for sequence in range(16, 28):
    roles = ("g3-control", "s1-candidate")
    values = {}
    for role in roles:
        payload = index[(sequence, role)]["payload"]
        values[role] = payload.get("adjacent_estimator_v10", {}).get("samples_ns", [timer(sequence, payload)])
    relative[str(sequence)] = values
    if sum(values["s1-candidate"]) * 100 > sum(values["g3-control"]) * 105:
        issues.append(f"g3-adjacent-degradation-{sequence}")
for sequence in (8, 28, 29, 30):
    roles = ("protected-control", "protected-candidate")
    values = {}
    for role in roles:
        payload = index[(sequence, role)]["payload"]
        values[role] = payload.get("adjacent_estimator_v10", {}).get("samples_ns", [timer(sequence, payload)])
    relative[str(sequence)] = values
    if sum(values["protected-candidate"]) * 100 > sum(values["protected-control"]) * 105:
        issues.append(f"protected-adjacent-degradation-{sequence}")

buffer_evidence = []
for sequence, role, payload, binary in observations:
    if binary == CANDIDATE:
        maximum = payload.get("max_single_buffer_bytes")
        if payload.get("buffer_evidence_complete") is not True or payload.get("full_file_buffer_bytes") != 0 or not isinstance(maximum, int) or maximum > LIMIT:
            issues.append(f"candidate-buffer-evidence-{sequence}-{role}")
        for key in ("q_cdc_old_window_bytes", "q_cdc_scan_input_bytes", "q_cdc_old_chunk_slots_bytes", "leaf_batch_query_bytes_max", "q_report_output_bytes", "buffer_bytes"):
            value = payload.get(key)
            if isinstance(value, int) and value > LIMIT:
                issues.append(f"candidate-buffer-field-{sequence}-{role}-{key}")
        if any(item.get("returned_bytes", 0) > LIMIT for item in payload.get("range_measurements", [])):
            issues.append(f"candidate-range-buffer-{sequence}-{role}")
        buffer_evidence.append({"sequence": sequence, "role": role, "kind": "direct", "max_single_buffer_bytes": maximum})
    elif binary in STATIC_BUFFER_CONTROLS:
        buffer_evidence.append({"sequence": sequence, "role": role, "kind": STATIC_BUFFER_CONTROLS[binary], "max_single_buffer_bytes": LIMIT})
    else:
        issues.append(f"missing-buffer-authority-{sequence}-{role}")

def measured_ns(payload):
    if payload.get("mode") == "seed-read":
        return payload["qualified_no_digest_wall_ns"] + payload["qualified_digest_wall_ns"]
    if "operation_wall_ns" in payload:
        return payload["operation_wall_ns"]
    if "operation_total_ns" in payload:
        return payload["operation_total_ns"]
    return payload.get("complete_lifecycle_total_wall_ns", 0)

measured_sum = sum(measured_ns(payload) for _, _, payload, _ in observations)
if measured_sum > 20_000_000_000:
    issues.append("measured-operation-sum")
m0 = index[(14, "m0-candidate")]["payload"]
seed = index[(7, "s1-candidate")]["payload"]
expected_durability = {"data_sync_calls": 1, "metadata_operations": 1, "metadata_sync_calls": 1, "rename_calls": 1, "directory_sync_calls": 2, "reconciliation_calls": 0, "reconciliation_outcome": "not-needed", "publication_status": "committed", "publication_diagnostic": None, "temp_files_created": 1, "temp_files_removed": 1}
if any(m0.get(key) != value for key, value in expected_durability.items()):
    issues.append("m0-direct-durability-counters")
if seed.get("cache_class") != "same-open-protected-seed-warm-or-unknown":
    issues.append("seed-cache-class")
cache = {"r0_control": index[(3, "r0-control")]["payload"].get("sqlite_cache_size_pages"), "r1_attribution": index[(3, "r1-attribution-control")]["payload"].get("sqlite_cache_size_pages"), "r1_candidate": index[(3, "r1-candidate")]["payload"].get("sqlite_cache_size_pages"), "r1_fresh": index[(4, "r1-candidate")]["payload"].get("sqlite_cache_size_pages"), "m0_control": index[(11, "m0-control")]["payload"].get("sqlite_cache_size_pages"), "m0_candidate": m0.get("sqlite_cache_size_pages")}
if cache != {"r0_control": 2000, "r1_attribution": 1500, "r1_candidate": 1500, "r1_fresh": 1500, "m0_control": 2000, "m0_candidate": 1500}:
    issues.append("g4-read-cache-profile")

issues = sorted(set(issues))
ledger = report["normalized_ledger"]
ledger.update({"issues": issues, "adjacent_estimator_policy": "fixed-two-sample-mean-exact-five-percent", "adjacent_estimator_sequences": sorted(ESTIMATED), "adjacent_estimator_samples": estimator_ledger, "all_protected_relative_samples": relative, "measured_payload_observations": len(observations), "measured_operation_sum_ns": measured_sum, "campaign_buffer_limit_bytes": LIMIT, "campaign_buffer_evidence": buffer_evidence, "campaign_max_single_buffer_bytes": max(item["max_single_buffer_bytes"] for item in buffer_evidence), "m0_durability": {key: m0.get(key) for key in expected_durability}, "seed_cache_class": seed.get("cache_class"), "g4_read_cache_pages": cache})
digest = hashlib.sha256(json.dumps(ledger, sort_keys=True, separators=(",", ":")).encode()).hexdigest()
report.update({"schema": "phase4-g4-primary-analysis-v10", "status": "PASS" if not issues else "REVISE", "issues": issues, "normalized_ledger": ledger, "normalized_ledger_sha256": digest})
path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
print(json.dumps({"status": report["status"], "ledger_sha256": digest}, sort_keys=True))
raise SystemExit(0 if not issues else 2)
