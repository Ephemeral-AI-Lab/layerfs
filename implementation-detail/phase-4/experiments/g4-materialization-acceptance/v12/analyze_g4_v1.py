#!/usr/bin/env python3
import hashlib
import json
import re
import runpy
import sys
from pathlib import Path

REPO = Path("/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty")
BASE = Path("/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty/implementation-detail/phase-4/experiments/g4-materialization-acceptance/v1/analyze_g4_v1.py")
BASE_SHA256 = "5dcc5a3b9283b47c18d3565a9dda457290681807491e2cad4003d8096116ab74"
CANDIDATE = "e72988fc25e96f608d0d405e157ea8e837029595ace916f066932082a736db33"
G3_CONTROL = "535bfa178a8a569ea43d9f1d23808775c2349a29f9cdacddae508391a6e5e61e"
PROTECTED_CONTROL = "5d72b46d29a5b77494781f343cc6841a71879b5de426751afe744f27a033e8f5"
SOURCE_HASHES = {
    REPO / "crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs": "01886da1d413ce73bbeba38f1b5cbc45a939e9d50e69fa7273c1af33f65554cb",
    REPO / "crates/layerfs-engine/src/bin/phase4_g3_materialization.rs": "320ecb529c11de4464ce9a76ce97cc11f60d719d418f33a40d945e5f6dde196a",
    REPO / "crates/layerfs-core/src/canonical_v2.rs": "8fe11085d8b27b1f2a833665b4afd11f6370f3e94821f5022d67ae14cac071dc",
    REPO / "Cargo.lock": "70c7f1079b6dcff927932d6e0072e5cd169cd2f49ea51c72f7f108d950adb8d8",
}
HISTORY_SHA256 = "a8ec9eb35a1860be6bea9ef01ebb19a45efef02984444c93c5b80919fd311e06"
STATIC_CONTROLS = {
    "535bfa178a8a569ea43d9f1d23808775c2349a29f9cdacddae508391a6e5e61e": "frozen-g3-v13-source-static-bound",
    "5d72b46d29a5b77494781f343cc6841a71879b5de426751afe744f27a033e8f5": "frozen-protected-control-source-static-bound",
}
ESTIMATED = (8, 16, 17, 18, 19, 20, 22, 24, 25, 26, 27, 29, 30)
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
BUFFER_FIELDS = (
    "q_cdc_old_window_segment_max_bytes", "q_cdc_scan_input_segment_max_bytes",
    "q_cdc_old_chunk_slots_bytes", "leaf_batch_query_bytes_max", "q_report_output_bytes", "buffer_bytes",
)
LIMIT = 1_048_576
RSS_LIMIT = 20_971_520

if hashlib.sha256(BASE.read_bytes()).hexdigest() != BASE_SHA256:
    raise SystemExit("frozen v1 primary analyzer custody mismatch")
try:
    runpy.run_path(str(BASE), run_name="__main__")
except SystemExit:
    pass
results = Path(sys.argv[1])
rows = [json.loads(line) for line in (results / "ARM-RAW-v1.jsonl").read_text().splitlines() if line]
arms = {(row["sequence"], row["role"]): row for row in rows}
path = results / "PRIMARY-ANALYSIS-v1.json"
report = json.loads(path.read_text())
replaced_base_adjacent = {
    *(f"g3-adjacent-degradation-{sequence}" for sequence in ESTIMATED if 16 <= sequence <= 27),
    *(f"protected-adjacent-degradation-{sequence}" for sequence in ESTIMATED if sequence in {8, 28, 29, 30}),
}
issues = [issue for issue in report["issues"] if issue not in replaced_base_adjacent]


def elapsed(sequence, payload):
    if 16 <= sequence <= 27:
        return payload["operation_total_ns"]
    if sequence == 8:
        return payload["range_measurements"][0]["wall_ns"]
    return payload["fresh_reopen_head_wall_ns"] if sequence == 30 else payload["durable_capture_total_wall_ns"]


def measured(payload):
    if payload.get("mode") == "seed-read":
        return payload["qualified_no_digest_wall_ns"] + payload["qualified_digest_wall_ns"]
    return payload.get("operation_wall_ns", payload.get("operation_total_ns", payload.get("complete_lifecycle_total_wall_ns", 0)))


def same_work(left, right, keys=None):
    selected = list(keys) if keys else sorted(set(left) & set(right) - {"status", "status_adapter", "adjacent_estimator_v12"})
    selected = [key for key in selected if not key.endswith("_ns")]
    return all(key in left and key in right and left[key] == right[key] for key in selected)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def parse_time(path):
    text = path.read_text()
    timing = re.search(r"([0-9.]+) real\s+([0-9.]+) user\s+([0-9.]+) sys", text)
    rss = re.search(r"(\d+)\s+maximum resident set size", text)
    voluntary = re.search(r"(\d+)\s+voluntary context switches", text)
    involuntary = re.search(r"(\d+)\s+involuntary context switches", text)
    if not all((timing, rss, voluntary, involuntary)):
        raise ValueError("incomplete /usr/bin/time -l evidence")
    return {
        "external_real_seconds": float(timing.group(1)),
        "external_user_seconds": float(timing.group(2)),
        "external_system_seconds": float(timing.group(3)),
        "maximum_resident_set_bytes": int(rss.group(1)),
        "voluntary_context_switches": int(voluntary.group(1)),
        "involuntary_context_switches": int(involuntary.group(1)),
    }


def expected_children():
    ordinary = {
        1: ("r0-control", "r1-attribution-control", "r1-candidate"),
        2: ("r0-control", "r1-attribution-control", "r1-candidate"),
        3: ("r0-control", "r1-attribution-control", "r1-candidate"),
        4: ("r1-candidate",), 6: ("s1-candidate",), 7: ("s1-candidate",),
        9: ("m0-control",), 10: ("m0-control",), 11: ("m0-control",),
        12: ("m0-candidate",), 13: ("m0-candidate",), 14: ("m0-candidate",),
        21: ("g3-control", "s1-candidate"), 23: ("g3-control", "s1-candidate"),
        28: ("protected-control", "protected-candidate"),
    }
    expected = []
    for sequence in range(1, 31):
        if sequence in ESTIMATED:
            control, candidate = (("g3-control", "s1-candidate") if 16 <= sequence <= 27 else ("protected-control", "protected-candidate"))
            pairs = ((control, 1), (candidate, 1), (candidate, 2), (control, 2)) if ESTIMATED.index(sequence) % 2 == 0 else ((candidate, 1), (control, 1), (control, 2), (candidate, 2))
            expected.extend((sequence, role, sample) for role, sample in pairs)
        else:
            expected.extend((sequence, role, 1) for role in ordinary.get(sequence, ()))
    return expected


def aggregate_external(values):
    return {
        "external_real_seconds": sum(value["external_real_seconds"] for value in values),
        "external_user_seconds": sum(value["external_user_seconds"] for value in values),
        "external_system_seconds": sum(value["external_system_seconds"] for value in values),
        "maximum_resident_set_bytes": max(value["maximum_resident_set_bytes"] for value in values),
        "voluntary_context_switches": sum(value["voluntary_context_switches"] for value in values),
        "involuntary_context_switches": sum(value["involuntary_context_switches"] for value in values),
    }


def logical_payload_matches(raw, logical):
    if raw.get("schema") != "phase4-g3-row-v1" or "status" in raw:
        return raw == logical
    adapted = dict(logical)
    return (
        adapted.pop("status", None) == "PASS"
        and adapted.pop("status_adapter", None)
        == "qualified-from-retained-g3-v1-exact-outcome-byte-mode-q-residue-invariants"
        and adapted == raw
    )


commands = json.loads((results / "COMMANDS-v1.json").read_text())
chronology = [json.loads(line) for line in (results / "CHRONOLOGY-v1.jsonl").read_text().splitlines() if line]
child_events = [event for event in chronology if event.get("event") == "measured-child-complete"]
if len(commands) != 76 or len(child_events) != 76 or [item.get("order") for item in commands] != list(range(1, 77)):
    issues.append("exact-76-child-command-chronology")
elif any(
    command.get(key) != event.get(key)
    for command, event in zip(commands, child_events)
    for key in ("order", "sequence", "role", "sample", "label", "command", "binary_sha256", "stdout_sha256", "stderr_sha256", "external")
):
    issues.append("child-command-chronology-custody")
if [(item.get("sequence"), item.get("role"), item.get("sample")) for item in commands] != expected_children():
    issues.append("exact-76-child-execution-order")

child_payloads = {}
child_external = {}
child_resource_evidence = []
for command in commands:
    order = command.get("order")
    role = command.get("role")
    expected_binary = G3_CONTROL if role == "g3-control" else PROTECTED_CONTROL if role == "protected-control" else CANDIDATE if role in {"r0-control", "r1-attribution-control", "r1-candidate", "s1-candidate", "m0-control", "m0-candidate", "protected-candidate"} else None
    try:
        executable = Path(command["command"][0])
        stdout = results / f"arm-raw-v1/{command['label']}.stdout"
        stderr = results / f"arm-raw-v1/{command['label']}.stderr"
        external = parse_time(stderr)
        payload = json.loads(next(line for line in reversed(stdout.read_text().splitlines()) if line))
        if expected_binary is None or command.get("binary_sha256") != expected_binary or digest(executable) != expected_binary:
            issues.append(f"child-role-binary-{order}")
        if digest(stdout) != command.get("stdout_sha256") or digest(stderr) != command.get("stderr_sha256"):
            issues.append(f"child-output-custody-{order}")
        if external != command.get("external"):
            issues.append(f"child-stderr-derivation-{order}")
        child_payloads[order] = payload
        child_external[order] = external
        child_resource_evidence.append({"order": order, "sequence": command.get("sequence"), "role": role, "sample": command.get("sample"), **external})
        if command.get("sequence") not in ESTIMATED:
            arm = arms.get((command.get("sequence"), role), {})
            if not logical_payload_matches(payload, arm.get("payload", {})) or arm.get("external") != external or arm.get("stdout_sha256") != command.get("stdout_sha256") or arm.get("stderr_sha256") != command.get("stderr_sha256"):
                issues.append(f"child-logical-arm-binding-{order}")
    except (KeyError, OSError, StopIteration, ValueError, json.JSONDecodeError):
        issues.append(f"child-raw-evidence-{order}")
if len(child_resource_evidence) != 76:
    issues.append("exact-76-child-resource-observations")
if any(item["maximum_resident_set_bytes"] > RSS_LIMIT for item in child_resource_evidence):
    issues.append("rss-limit")

observations = []
samples_by_role = {}
estimator_ledger = {}
for (sequence, role), arm in sorted(arms.items()):
    payload = arm["payload"]
    if sequence not in ESTIMATED:
        observations.append((sequence, role, payload, arm["binary_sha256"]))
        continue
    meta = payload.get("adjacent_estimator_v12", {})
    expected_order = "ABBA" if ESTIMATED.index(sequence) % 2 == 0 else "BAAB"
    if (meta.get("replications_per_role"), meta.get("estimator"), meta.get("relative_limit_basis_points"), meta.get("balanced_order")) != (2, "equal-weight-arithmetic-mean", 10_500, expected_order):
        issues.append(f"estimator-metadata-{sequence}-{role}")
        continue
    fields = ("sample_payload_paths", "sample_payload_sha256", "samples_ns", "sample_order", "sample_commands", "sample_external")
    role_commands = [command for command in commands if command.get("sequence") == sequence and command.get("role") == role]
    if any(not isinstance(meta.get(field), list) or len(meta[field]) != 2 for field in fields) or len(role_commands) != 2:
        issues.append(f"estimator-sample-cardinality-{sequence}-{role}")
        continue
    orders = meta["sample_order"]
    samples = [child_payloads.get(order) for order in orders]
    if any(sample is None for sample in samples):
        issues.append(f"estimator-sample-custody-{sequence}-{role}")
        continue
    for index, command in enumerate(role_commands):
        expected_path = f"arm-raw-v1/{command['label']}.stdout"
        if (orders[index] != command["order"] or meta["sample_payload_paths"][index] != expected_path or meta["sample_payload_sha256"][index] != command["stdout_sha256"] or meta["sample_commands"][index] != command["command"] or meta["sample_external"][index] != child_external.get(command["order"])):
            issues.append(f"estimator-sample-binding-{sequence}-{role}-{index + 1}")
    values = [elapsed(sequence, sample) for sample in samples]
    if any(type(value) is not int or value <= 0 for value in values) or values != meta.get("samples_ns") or sum(values) != meta.get("sum_ns") or elapsed(sequence, payload) != (sum(values) + 1) // 2:
        issues.append(f"estimator-sample-equation-{sequence}-{role}")
    derived_external = aggregate_external([child_external[order] for order in orders])
    if arm.get("external") != derived_external:
        issues.append(f"estimator-external-aggregate-{sequence}-{role}")
    if len(samples) == 2 and not same_work(samples[0], samples[1], FAST_KEYS if sequence in {8, 29, 30} else None):
        issues.append(f"estimator-within-role-work-parity-{sequence}-{role}")
    samples_by_role[(sequence, role)] = samples
    observations.extend((sequence, role, sample, arm["binary_sha256"]) for sample in samples)
    estimator_ledger[f"{sequence}-{role}"] = values

for sequence in ESTIMATED:
    control_role, candidate_role = (("g3-control", "s1-candidate") if 16 <= sequence <= 27 else ("protected-control", "protected-candidate"))
    control_meta = arms[(sequence, control_role)]["payload"]["adjacent_estimator_v12"]
    candidate_meta = arms[(sequence, candidate_role)]["payload"]["adjacent_estimator_v12"]
    actual_order = [item["role"] for item in commands if item["sequence"] == sequence]
    expected_order = [control_role, candidate_role, candidate_role, control_role] if ESTIMATED.index(sequence) % 2 == 0 else [candidate_role, control_role, control_role, candidate_role]
    if actual_order != expected_order or sorted(control_meta["sample_order"] + candidate_meta["sample_order"]) != [item["order"] for item in commands if item["sequence"] == sequence]:
        issues.append(f"balanced-order-{sequence}")
    for index in range(2):
        keys = FAST_KEYS if sequence in {8, 29, 30} else None
        if not same_work(samples_by_role[(sequence, control_role)][index], samples_by_role[(sequence, candidate_role)][index], keys):
            issues.append(f"estimator-cross-role-work-parity-{sequence}-{index + 1}")

for sequence in range(16, 28):
    if not same_work(arms[(sequence, "g3-control")]["payload"], arms[(sequence, "s1-candidate")]["payload"]):
        issues.append(f"g3-complete-semantic-work-parity-{sequence}")
for sequence in (8, 28, 29, 30):
    if not same_work(arms[(sequence, "protected-control")]["payload"], arms[(sequence, "protected-candidate")]["payload"], FAST_KEYS):
        issues.append(f"protected-complete-semantic-work-parity-{sequence}")

relative = {}
routes = [(sequence, "g3-control", "s1-candidate", "g3") for sequence in range(16, 28)]
routes += [(sequence, "protected-control", "protected-candidate", "protected") for sequence in (8, 28, 29, 30)]
for sequence, control_role, candidate_role, prefix in routes:
    control = arms[(sequence, control_role)]["payload"]
    candidate = arms[(sequence, candidate_role)]["payload"]
    control_values = control.get("adjacent_estimator_v12", {}).get("samples_ns", [elapsed(sequence, control)])
    candidate_values = candidate.get("adjacent_estimator_v12", {}).get("samples_ns", [elapsed(sequence, candidate)])
    relative[str(sequence)] = {control_role: control_values, candidate_role: candidate_values}
    if sum(candidate_values) * 100 > sum(control_values) * 105:
        issues.append(f"{prefix}-adjacent-degradation-{sequence}")

source_custody = {}
for source, expected in SOURCE_HASHES.items():
    actual = digest(source) if source.is_file() else None
    source_custody[str(source.relative_to(REPO))] = actual
    if actual != expected:
        issues.append(f"source-custody-{source.name}")
history_path = results / "methodology-v1/PRE-EXEC-HISTORY-v11.json"
history_sha256 = digest(history_path) if history_path.is_file() else None
if results.name != "results-v12" or history_sha256 != HISTORY_SHA256:
    issues.append("v12-pre-execution-lineage")

buffer_evidence = []
for sequence, role, payload, binary in observations:
    if binary == CANDIDATE:
        maximum = payload.get("max_single_buffer_bytes")
        if payload.get("buffer_evidence_complete") is not True or payload.get("full_file_buffer_bytes") != 0 or type(maximum) is not int or not 0 <= maximum <= LIMIT:
            issues.append(f"candidate-buffer-evidence-{sequence}-{role}")
        for key in BUFFER_FIELDS:
            if key in payload and (type(payload[key]) is not int or not 0 <= payload[key] <= LIMIT):
                issues.append(f"candidate-buffer-field-{sequence}-{role}-{key}")
        if "measurement_status_schema" in payload:
            for key in BUFFER_FIELDS[:-1]:
                if type(payload.get(key)) is not int or not 0 <= payload[key] <= LIMIT:
                    issues.append(f"candidate-buffer-required-{sequence}-{role}-{key}")
        if max([item.get("returned_bytes", 0) for item in payload.get("range_measurements", [])] or [0]) > LIMIT:
            issues.append(f"candidate-range-buffer-{sequence}-{role}")
        buffer_evidence.append({"sequence": sequence, "role": role, "kind": "direct", "max_single_buffer_bytes": maximum})
    elif binary in STATIC_CONTROLS:
        buffer_evidence.append({"sequence": sequence, "role": role, "kind": STATIC_CONTROLS[binary], "max_single_buffer_bytes": LIMIT})
    else:
        issues.append(f"missing-buffer-authority-{sequence}-{role}")

operation_sum = sum(measured(payload) for _, _, payload, _ in observations)
if len(observations) != 76:
    issues.append("exact-76-measured-payload-observations")
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
valid_buffer_maxima = [item["max_single_buffer_bytes"] for item in buffer_evidence if type(item.get("max_single_buffer_bytes")) is int]
resource_totals = {
    "external_real_seconds": sum(item["external_real_seconds"] for item in child_resource_evidence),
    "external_user_seconds": sum(item["external_user_seconds"] for item in child_resource_evidence),
    "external_system_seconds": sum(item["external_system_seconds"] for item in child_resource_evidence),
    "maximum_resident_set_bytes": max((item["maximum_resident_set_bytes"] for item in child_resource_evidence), default=0),
    "voluntary_context_switches": sum(item["voluntary_context_switches"] for item in child_resource_evidence),
    "involuntary_context_switches": sum(item["involuntary_context_switches"] for item in child_resource_evidence),
}
ledger.update({"issues": issues, "adjacent_estimator_policy": "balanced-fixed-two-sample-mean-exact-five-percent", "replaced_base_adjacent_decisions": sorted(replaced_base_adjacent), "adjacent_estimator_sequences": list(ESTIMATED), "adjacent_estimator_samples": estimator_ledger, "all_protected_relative_samples": relative, "measured_payload_observations": len(observations), "measured_child_commands": len(commands), "measured_child_resource_evidence": child_resource_evidence, "measured_child_resource_totals": resource_totals, "measured_operation_sum_ns": operation_sum, "campaign_buffer_limit_bytes": LIMIT, "campaign_buffer_evidence": buffer_evidence, "campaign_max_single_buffer_bytes": max(valid_buffer_maxima, default=0), "source_custody": source_custody, "pre_execution_history_sha256": history_sha256, "m0_durability": {key: m0.get(key) for key in durability}, "seed_cache_class": seed.get("cache_class"), "g4_read_cache_pages": cache})
digest = hashlib.sha256(json.dumps(ledger, sort_keys=True, separators=(",", ":")).encode()).hexdigest()
report.update({"schema": "phase4-g4-primary-analysis-v12", "status": "PASS" if not issues else "REVISE", "issues": issues, "normalized_ledger": ledger, "normalized_ledger_sha256": digest})
path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
print(json.dumps({"status": report["status"], "ledger_sha256": digest}, sort_keys=True))
raise SystemExit(0 if not issues else 2)
