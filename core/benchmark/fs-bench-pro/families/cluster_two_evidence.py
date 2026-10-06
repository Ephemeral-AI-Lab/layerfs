"""E1 registration and retained-gate evaluation, never a qualification runner.

Uses runner.py's read-only register entrypoint. Concrete drivers, calibrated
limits and whole-route observers are required before any selected row can be
admitted. Existing completed/failed families are not replayed or relabeled.
"""
import copy
import json
from pathlib import Path

from shared.evidence_gates import artifact, evaluate, integer, resource_phase
from shared.evidence_registration import HASH64, LIMITS, digest_json, inspect_source, validate

HERE = Path(__file__).resolve().parents[1]
REGISTRY = HERE / "registry/cluster-two-evidence-v1.json"
SCHEMA = "cluster-two-registration-v1"


def registry():
    value = json.loads(REGISTRY.read_text())
    ids = [case["id"] for case in value["cases"]]
    if value.get("schema") != "cluster-two-evidence-registry-v1" or len(ids) != 27 or len(set(ids)) != 27 or ids != value.get("case_order"):
        raise ValueError("cluster-two registration cardinality/order invalid")
    for case in value["cases"]:
        case["gates"] = copy.deepcopy(value["common_gates"]) + case["gates"]
        if case["status"] != "NOT_RUN" or case["sample_count"] != 0:
            raise ValueError("registration catalogue must not contain a measurement")
    return value


def template(selected=None):
    """An explicit incomplete input; null never signifies an observed zero."""
    catalog = registry()
    selected = catalog["case_order"] if selected is None else list(selected)
    if set(selected) - set(catalog["case_order"]):
        raise ValueError("unknown case selection")
    rows = {}
    for case in catalog["cases"]:
        rows[case["id"]] = {"selected": case["id"] in selected, "profile": None,
            "source_arm": None, "sample_id": None, "attempt_contract": None,
            "resident_spread_contract": copy.deepcopy(case["resident_spread_contract"]),
            "identity": None, "driver": None, "oracle": None, "cache": None,
            "observers": None, "sql_plan_profile": None,
            "budgets": copy.deepcopy(case["budgets"]),
            "boundary_sha256": digest_json(case["boundaries"]),
            "fixture_manifest": None, "workload_manifest": None, "schedule_manifest": None,
            "gate_specification": None,
            "limits": {name: None for name in LIMITS if name not in case.get("not_applicable_limits", [])}, "service_classes": None}
    return {"schema": SCHEMA, "registry_sha256": digest_json(catalog),
            "specification": {"commit": None, "artifact": None}, "sample_count": 0,
            "selected_order": selected, "rows": rows}


def admission(manifest, root):
    catalog = registry()
    result = validate(catalog, manifest, root)
    wanted = [name for name in manifest.get("selected_order", [])]
    actual = [case["id"] for case in catalog["cases"] if manifest.get("rows", {}).get(case["id"], {}).get("selected") is True]
    if wanted != actual:
        result["global_errors"].append("selected arm order must equal prospective registry order")
    seen = set()
    for case in result["rows"]:
        if case["registration_status"] == "NOT_RUN":
            continue
        identity = manifest["rows"][case["case"]].get("identity")
        key = digest_json(identity)
        if key not in seen:
            seen.add(key)
            result["global_errors"].extend(inspect_source(identity, root))
    if result["global_errors"]:
        result["status"] = "INCOMPLETE"
        for case in result["rows"]:
            if case["registration_status"] != "NOT_RUN":
                case["registration_status"] = "INCOMPLETE"
    return result


def observation_binding(manifest, case_id, observation, root):
    """Tie every retained counter/resource to its exact admitted row and raw file."""
    binding = manifest["rows"][case_id]
    identity = binding.get("identity") or {}
    expected = {"case": case_id, "registration_sha256": digest_json(manifest),
                "registration_row_sha256": digest_json(binding),
                "registry_sha256": manifest.get("registry_sha256"),
                "boundary_sha256": binding.get("boundary_sha256"),
                "source_commit": identity.get("source_commit"),
                "source_tree": identity.get("source_tree"),
                "workload_sha256": identity.get("workload_sha256"),
                "observer_sha256": identity.get("observer_sha256"),
                "source_arm": binding.get("source_arm"), "sample_id": binding.get("sample_id"),
                "binary_sha256": ((binding.get("driver") or {}).get("binary") or {}).get("sha256")}
    if not isinstance(observation, dict) or observation.get("schema") != "cluster-two-row-observations-v1" or any(value is None or observation.get(key) != value for key, value in expected.items()):
        raise ValueError("retained observations do not match admitted source/row/boundary/workload/observer/binary identity")
    raw = json.loads(artifact(observation.get("raw"), root).read_text())
    if raw != {key: value for key, value in observation.items() if key != "raw"}:
        raise ValueError("metrics/resources/identity differ from sealed raw observations")
    if not isinstance(raw.get("metrics"), dict) or not isinstance(raw.get("resources"), dict):
        raise ValueError("sealed observation metric/resource inventories missing")
    if not integer(raw.get("sample_count")) or raw["sample_count"] != 1 or raw["metrics"].get("sample_count") != 1 or type(raw["metrics"].get("sample_count")) is not int:
        raise ValueError("retained observation must contain exactly one source-arm sample")
    original_attempts(binding, raw, root)
    return raw


def original_attempts(binding, raw, root):
    """Exact single-sample original receipts, never a resampled aggregate."""
    contract = binding.get("attempt_contract") or {}
    attempts = raw.get("original_attempts")
    if contract.get("scope") != "declared-public-operation-attempts" or not integer(contract.get("count"), positive=True) or not isinstance(attempts, list) or len(attempts) != contract["count"]:
        raise ValueError("original attempt count differs from frozen public-operation contract")
    ids, receipts = set(), set()
    for index, entry in enumerate(attempts):
        if not isinstance(entry, dict):
            raise ValueError("original attempt entry schema missing")
        attempt_id = entry.get("attempt_id")
        receipt = entry.get("receipt") or {}
        receipt_id = receipt.get("sha256")
        if not isinstance(attempt_id, str) or HASH64.fullmatch(attempt_id) is None or attempt_id == "0" * 64 or attempt_id in ids or receipt_id in receipts:
            raise ValueError("duplicate/missing original attempt or receipt identity")
        recorded = json.loads(artifact(receipt, root).read_text())
        if not isinstance(recorded, dict):
            raise ValueError("original receipt schema missing")
        expected = {key: raw[key] for key in ("case", "sample_id", "source_arm", "source_commit", "source_tree", "binary_sha256")}
        expected.update(attempt_id=attempt_id, operation_index=index, scope=contract["scope"])
        if recorded.get("schema") != "cluster-two-original-attempt-v1" or any(recorded.get(key) != value for key, value in expected.items()) or type(recorded.get("sample_count")) is not int or recorded["sample_count"] != 1 or type(recorded.get("attempt_count")) is not int or recorded["attempt_count"] != 1:
            raise ValueError("original receipt does not match its single source-arm sample/attempt")
        ids.add(attempt_id)
        receipts.add(receipt_id)
    return ids, receipts


def phase_witness(resource, root):
    """Resolve a distinct-clock calibration through an immutable attested file."""
    if not isinstance(resource, dict):
        raise ValueError("phase resource witness missing")
    checked = copy.deepcopy(resource)
    if checked.get("sample_clock") != checked.get("boundary_clock"):
        receipt = json.loads(artifact(checked.get("calibration_artifact"), root).read_text())
        if not isinstance(receipt, dict) or receipt != checked.get("clock_calibration"):
            raise ValueError("phase clock calibration differs from sealed attestation")
        receipt["validated_receipt"] = True
        checked["clock_calibration"] = receipt
    return checked


def checked_resource_metrics(case, raw, phases):
    """Numeric peak gates use their checked witness value, never a second claim."""
    metrics = copy.deepcopy(raw["metrics"])
    for domain in case["resident_domains"]:
        key = domain + "_phase_peak_bytes"
        peak = phases[domain].get("peak_bytes") if phases[domain].get("status") == "PASS" else None
        if peak is None or not integer(metrics.get(key)) or metrics[key] != peak:
            raise ValueError("gated phase peak disagrees with checked witness: " + domain)
        metrics[key] = peak
    return metrics


def resident_spread(manifest, observations, case, root, admitted):
    """Derive max-minus-min of the exact frozen matched cohort owner peaks."""
    contract = case["resident_spread_contract"]
    if contract.get("id") != "matched-cohort-aggregate-phase-peak-range-v1":
        raise ValueError("resident spread definition unsupported")
    aggregates, match = [], None
    for case_id in contract["operands"]:
        binding = manifest.get("rows", {}).get(case_id) or {}
        if binding.get("selected") is not True or binding.get("resident_spread_contract") != contract:
            raise ValueError("resident spread operand unselected or differently frozen: " + case_id)
        if admitted.get(case_id, {}).get("registration_status") != "PASS":
            raise ValueError("resident spread operand registration is not PASS: " + case_id)
        raw = observation_binding(manifest, case_id, observations.get("rows", {}).get(case_id), root)
        identity = binding.get("identity") or {}
        control = {key: raw[key] for key in ("source_arm", "source_commit", "source_tree", "observer_sha256", "binary_sha256")}
        control.update(profile=binding.get("profile"), product_sha256=identity.get("product_sha256"), cache_sha256=identity.get("cache_sha256"))
        if any(value is None for value in control.values()):
            raise ValueError("resident spread cohort matching identities incomplete")
        if match is not None and match != control:
            raise ValueError("resident spread cohort arm/profile/source/cache/observer/binary mismatch")
        match = control
        total = 0
        for domain in contract["domains"]:
            limits = admitted.get(case_id, {}).get("resolved_limits")
            # The caller's admission supplies resolved numerical limits; the
            # registration JSON itself never supplies trusted resolved values.
            if not isinstance(limits, dict):
                raise ValueError("resident spread operand limits have not been admitted")
            phase = resource_phase(phase_witness(raw["resources"].get(domain), root),
                sample_interval_ns=limits.get("observer_interval_ns"),
                maximum_gap_ns=limits.get("observer_gap_ns"), clock_error_ns=limits.get("clock_error_ns"))
            if not integer(raw["metrics"].get(domain + "_phase_peak_bytes")) or raw["metrics"][domain + "_phase_peak_bytes"] != phase["peak_bytes"]:
                raise ValueError("resident spread operand peak differs from its witness")
            total += phase["peak_bytes"]
        aggregates.append({"case": case_id, "aggregate_phase_peak_bytes": total})
    if not aggregates:
        raise ValueError("resident spread has no declared operands")
    values = [row["aggregate_phase_peak_bytes"] for row in aggregates]
    return {"status": "PASS", "value": max(values) - min(values), "operands": aggregates,
            "meaning": contract["id"], "singleton": len(aggregates) == 1}


def retained_gates(manifest, observations, root):
    """Validate retained integer metrics without launching or resampling work."""
    result = admission(manifest, root)
    result["registration_status"] = result.pop("status")
    result["schema"] = "cluster-two-retained-numeric-v1"
    result["qualification_status"] = "NOT_EVALUATED"
    result["qualification_reason"] = "numeric comparisons do not prove completed outcomes, acknowledged timing, or bound oracle/cache/lifecycle results"
    cases = {case["id"]: case for case in registry()["cases"]}
    observation_globals = []
    if observations.get("schema") != "cluster-two-observations-v1" or observations.get("registration_sha256") != digest_json(manifest) or observations.get("registry_sha256") != manifest.get("registry_sha256"):
        observation_globals.append("retained observation registry/registration schema or identity mismatch")
    if set(observations.get("rows", {})) - set(cases):
        observation_globals.append("retained observations contain unknown case IDs")
    result["observation_errors"] = observation_globals
    admitted = {row["case"]: row for row in result["rows"]}
    sample_ids, attempt_ids, receipt_ids = set(), set(), set()
    for row in result["rows"]:
        if row["registration_status"] == "NOT_RUN":
            continue
        row["observation_errors"] = list(observation_globals)
        try:
            raw = observation_binding(manifest, row["case"], observations.get("rows", {}).get(row["case"]), root)
            attempts, receipts = original_attempts(manifest["rows"][row["case"]], raw, root)
            if raw["sample_id"] in sample_ids or attempts & attempt_ids or receipts & receipt_ids:
                raise ValueError("sample/original attempt/receipt identity reused across selected rows")
            sample_ids.add(raw["sample_id"])
            attempt_ids.update(attempts)
            receipt_ids.update(receipts)
        except (ValueError, OSError, TypeError) as error:
            row["observation_errors"].append(str(error))
            raw = {"metrics": {}, "resources": {}}
        metrics = raw["metrics"]
        # Phase witnesses are distinct from peak numbers; all owner domains are
        # checked, so a heap-only or lifetime peak cannot pass resource admission.
        row["phase_resources"] = {}
        for domain in cases[row["case"]]["resident_domains"]:
            try:
                row["phase_resources"][domain] = resource_phase(
                    phase_witness(raw["resources"].get(domain), root),
                    sample_interval_ns=row["resolved_limits"].get("observer_interval_ns"),
                    maximum_gap_ns=row["resolved_limits"].get("observer_gap_ns"),
                    clock_error_ns=row["resolved_limits"].get("clock_error_ns"))
            except (ValueError, OSError) as error:
                row["phase_resources"][domain] = {"status": "INCOMPLETE", "reason": str(error)}
        try:
            metrics = checked_resource_metrics(cases[row["case"]], raw, row["phase_resources"])
        except (ValueError, KeyError, TypeError) as error:
            row["observation_errors"].append(str(error))
            # Keep known witness-derived failures even if the independent
            # reported scalar is wrong or a different resource is unavailable.
            for domain in cases[row["case"]]["resident_domains"]:
                key = domain + "_phase_peak_bytes"
                phase = row["phase_resources"][domain]
                if phase.get("status") == "PASS":
                    metrics[key] = phase["peak_bytes"]
                else:
                    metrics.pop(key, None)
        try:
            row["resident_spread"] = resident_spread(manifest, observations, cases[row["case"]], root, admitted)
            if not integer(metrics.get("phase_resident_spread_bytes")) or metrics["phase_resident_spread_bytes"] != row["resident_spread"]["value"]:
                raise ValueError("gated resident spread differs from checked cohort operands")
            metrics["phase_resident_spread_bytes"] = row["resident_spread"]["value"]
        except (ValueError, OSError, KeyError, TypeError) as error:
            row["observation_errors"].append(str(error))
            metrics.pop("phase_resident_spread_bytes", None)
        row["gate_evaluation"] = evaluate(row["gates"], metrics, row["resolved_limits"])
        if row["gate_evaluation"]["status"] == "FAIL":
            row["numeric_gate_status"] = "FAIL"
        else:
            row["numeric_gate_status"] = row["gate_evaluation"]["status"] if row["registration_status"] == "PASS" and not row["observation_errors"] and all(v["status"] == "PASS" for v in row["phase_resources"].values()) else "INCOMPLETE"
        row["qualification_status"] = "NOT_EVALUATED"
    selected = [row for row in result["rows"] if row["registration_status"] != "NOT_RUN"]
    verdicts = [row["numeric_gate_status"] for row in selected]
    result["retained_numeric_status"] = "FAIL" if "FAIL" in verdicts else "PASS" if verdicts and all(verdict == "PASS" for verdict in verdicts) else "INCOMPLETE"
    return result


def list_rows():
    return [(case["id"], case["route"]["entrypoint"], case["budgets"]["complete_command_ns"])
            for case in registry()["cases"]]
