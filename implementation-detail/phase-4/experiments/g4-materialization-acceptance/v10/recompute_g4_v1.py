#!/usr/bin/env python3
import hashlib
import json
import runpy
import sys
from pathlib import Path

SOURCE = Path("/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty/implementation-detail/phase-4/experiments/g4-materialization-acceptance/v1/recompute_g4_v1.py")
EXPECTED = "402980e4a4f441964086a37e37d2105e3ec8e78bc48b24ecdf8d7fbbb1de9bcc"
CANDIDATE = "770dcfa8db17f1f9e1b90336a26923eb0530073590a9da5578e06339d85813e8"
G3_CONTROL = "535bfa178a8a569ea43d9f1d23808775c2349a29f9cdacddae508391a6e5e61e"
PROTECTED_CONTROL = "5d72b46d29a5b77494781f343cc6841a71879b5de426751afe744f27a033e8f5"
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
    raise SystemExit("frozen v1 independent analyzer custody mismatch")
try:
    runpy.run_path(str(SOURCE), run_name="__main__")
except SystemExit:
    pass
results = Path(sys.argv[1])
rows = [json.loads(value) for value in (results / "ARM-RAW-v1.jsonl").read_text().splitlines() if value]
arms = {(row["sequence"], row["role"]): row for row in rows}
path = results / "INDEPENDENT-RECOMPUTATION-v1.json"
report = json.loads(path.read_text())
issues = list(report["issues"])


def elapsed(sequence, payload):
    if sequence in range(16, 28):
        return payload["operation_total_ns"]
    if sequence == 8:
        return payload["range_measurements"][0]["wall_ns"]
    return payload["fresh_reopen_head_wall_ns"] if sequence == 30 else payload["durable_capture_total_wall_ns"]


def read_sample(name, digest):
    target = results.joinpath(name)
    if hashlib.sha256(target.read_bytes()).hexdigest() != digest:
        raise RuntimeError(f"sample digest mismatch: {name}")
    return json.loads(next(line for line in reversed(target.read_text().splitlines()) if line))


def parity(left, right, exact=None):
    keys = exact or sorted(set(left).intersection(right).difference({"status", "status_adapter", "adjacent_estimator_v10"}))
    keys = [key for key in keys if not key.endswith("_ns")]
    return all(left.get(key) == right.get(key) for key in keys)


observations = []
estimator_ledger = {}
for (sequence, role), row in sorted(arms.items()):
    payload = row["payload"]
    if sequence not in ESTIMATED:
        observations.append((sequence, role, payload, row["binary_sha256"]))
        continue
    meta = payload.get("adjacent_estimator_v10")
    if not isinstance(meta, dict) or (meta.get("replications_per_role"), meta.get("estimator"), meta.get("relative_limit_basis_points")) != (2, "equal-weight-arithmetic-mean", 10_500):
        issues.append(f"estimator-metadata-{sequence}-{role}")
        continue
    names, digests = meta.get("sample_payload_paths", []), meta.get("sample_payload_sha256", [])
    samples = [read_sample(name, digest) for name, digest in zip(names, digests)]
    values = [elapsed(sequence, sample) for sample in samples]
    if len(samples) != 2 or values != meta.get("samples_ns") or sum(values) != meta.get("sum_ns") or elapsed(sequence, payload) != (sum(values) + 1) // 2:
        issues.append(f"estimator-sample-equation-{sequence}-{role}")
    if len(samples) == 2 and not parity(samples[0], samples[1], FAST_KEYS if sequence in {8, 29, 30} else None):
        issues.append(f"estimator-within-role-work-parity-{sequence}-{role}")
    observations += [(sequence, role, sample, row["binary_sha256"]) for sample in samples]
    estimator_ledger[f"{sequence}-{role}"] = values

for sequence in range(16, 28):
    if not parity(arms[(sequence, "g3-control")]["payload"], arms[(sequence, "s1-candidate")]["payload"]):
        issues.append(f"g3-complete-semantic-work-parity-{sequence}")
for sequence in (8, 28, 29, 30):
    if not parity(arms[(sequence, "protected-control")]["payload"], arms[(sequence, "protected-candidate")]["payload"], FAST_KEYS):
        issues.append(f"protected-complete-semantic-work-parity-{sequence}")

relative = {}
route_roles = [(sequence, "g3-control", "s1-candidate", "g3") for sequence in range(16, 28)]
route_roles += [(sequence, "protected-control", "protected-candidate", "protected") for sequence in (8, 28, 29, 30)]
for sequence, control_role, candidate_role, prefix in route_roles:
    control = arms[(sequence, control_role)]["payload"]
    candidate = arms[(sequence, candidate_role)]["payload"]
    control_values = control.get("adjacent_estimator_v10", {}).get("samples_ns", [elapsed(sequence, control)])
    candidate_values = candidate.get("adjacent_estimator_v10", {}).get("samples_ns", [elapsed(sequence, candidate)])
    relative[str(sequence)] = {control_role: control_values, candidate_role: candidate_values}
    if sum(candidate_values) * 100 > sum(control_values) * 105:
        issues.append(f"{prefix}-adjacent-degradation-{sequence}")

static = {G3_CONTROL: "frozen-g3-v13-source-static-bound", PROTECTED_CONTROL: "frozen-protected-control-source-static-bound"}
buffer_evidence = []
for sequence, role, payload, binary in observations:
    if binary == CANDIDATE:
        maximum = payload.get("max_single_buffer_bytes")
        valid = payload.get("buffer_evidence_complete") is True and payload.get("full_file_buffer_bytes") == 0 and isinstance(maximum, int) and maximum <= LIMIT
        if not valid:
            issues.append(f"candidate-buffer-evidence-{sequence}-{role}")
        for key in ("q_cdc_old_window_bytes", "q_cdc_scan_input_bytes", "q_cdc_old_chunk_slots_bytes", "leaf_batch_query_bytes_max", "q_report_output_bytes", "buffer_bytes"):
            if isinstance(payload.get(key), int) and payload[key] > LIMIT:
                issues.append(f"candidate-buffer-field-{sequence}-{role}-{key}")
        if max([item.get("returned_bytes", 0) for item in payload.get("range_measurements", [])] or [0]) > LIMIT:
            issues.append(f"candidate-range-buffer-{sequence}-{role}")
        buffer_evidence.append({"sequence": sequence, "role": role, "kind": "direct", "max_single_buffer_bytes": maximum})
    elif binary in static:
        buffer_evidence.append({"sequence": sequence, "role": role, "kind": static[binary], "max_single_buffer_bytes": LIMIT})
    else:
        issues.append(f"missing-buffer-authority-{sequence}-{role}")

def measured(payload):
    if payload.get("mode") == "seed-read":
        return payload["qualified_no_digest_wall_ns"] + payload["qualified_digest_wall_ns"]
    return payload.get("operation_wall_ns", payload.get("operation_total_ns", payload.get("complete_lifecycle_total_wall_ns", 0)))

operation_sum = sum(measured(payload) for _, _, payload, _ in observations)
if operation_sum > 20_000_000_000:
    issues.append("measured-operation-sum")
m0 = arms[(14, "m0-candidate")]["payload"]
seed = arms[(7, "s1-candidate")]["payload"]
durability = {"data_sync_calls": 1, "metadata_operations": 1, "metadata_sync_calls": 1, "rename_calls": 1, "directory_sync_calls": 2, "reconciliation_calls": 0, "reconciliation_outcome": "not-needed", "publication_status": "committed", "publication_diagnostic": None, "temp_files_created": 1, "temp_files_removed": 1}
if any(m0.get(key) != value for key, value in durability.items()):
    issues.append("m0-direct-durability-counters")
if seed.get("cache_class") != "same-open-protected-seed-warm-or-unknown":
    issues.append("seed-cache-class")
cache = {"r0_control": arms[(3, "r0-control")]["payload"].get("sqlite_cache_size_pages"), "r1_attribution": arms[(3, "r1-attribution-control")]["payload"].get("sqlite_cache_size_pages"), "r1_candidate": arms[(3, "r1-candidate")]["payload"].get("sqlite_cache_size_pages"), "r1_fresh": arms[(4, "r1-candidate")]["payload"].get("sqlite_cache_size_pages"), "m0_control": arms[(11, "m0-control")]["payload"].get("sqlite_cache_size_pages"), "m0_candidate": m0.get("sqlite_cache_size_pages")}
if cache != {"r0_control": 2000, "r1_attribution": 1500, "r1_candidate": 1500, "r1_fresh": 1500, "m0_control": 2000, "m0_candidate": 1500}:
    issues.append("g4-read-cache-profile")

issues = sorted(set(issues))
ledger = report["normalized_ledger"]
ledger.update({"issues": issues, "adjacent_estimator_policy": "fixed-two-sample-mean-exact-five-percent", "adjacent_estimator_sequences": sorted(ESTIMATED), "adjacent_estimator_samples": estimator_ledger, "all_protected_relative_samples": relative, "measured_payload_observations": len(observations), "measured_operation_sum_ns": operation_sum, "campaign_buffer_limit_bytes": LIMIT, "campaign_buffer_evidence": buffer_evidence, "campaign_max_single_buffer_bytes": max(item["max_single_buffer_bytes"] for item in buffer_evidence), "m0_durability": {key: m0.get(key) for key in durability}, "seed_cache_class": seed.get("cache_class"), "g4_read_cache_pages": cache})
digest = hashlib.sha256(json.dumps(ledger, sort_keys=True, separators=(",", ":")).encode()).hexdigest()
report.update({"schema": "phase4-g4-independent-recomputation-v10", "status": "PASS" if not issues else "REVISE", "issues": issues, "normalized_ledger": ledger, "normalized_ledger_sha256": digest})
path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
print(json.dumps({"status": report["status"], "ledger_sha256": digest}, sort_keys=True))
raise SystemExit(0 if not issues else 2)
