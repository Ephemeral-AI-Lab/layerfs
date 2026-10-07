"""Integer gates and prospective control calibration for cluster-two evidence.

No function launches product work. Missing values remain unavailable; booleans,
negative counters and lifetime resource peaks cannot become numeric evidence.
"""
import hashlib
import json
from pathlib import Path

LIMIT_DEFINITIONS = {
    "operation_ns": ("ns", "acknowledged-operation-elapsed-v1", "operation_ns"),
    "request_ns": ("ns", "maximum-original-request-elapsed-v1", "request_max_ns"),
    "queue_wait_ns": ("ns", "maximum-original-queue-wait-v1", "queue_wait_max_ns"),
    "service_ns": ("ns", "maximum-original-service-elapsed-v1", "service_max_ns"),
    "resource_wait_ns": ("ns", "maximum-original-resource-wait-v1", "resource_wait_max_ns"),
    "runnable_wait_ns": ("ns", "maximum-runnable-class-wait-v1", "runnable_wait_max_ns"),
    "drain_ns": ("ns", "automatic-last-owner-drain-elapsed-v1", "automatic_drain_ns"),
    "host_resident_bytes": ("bytes", "aggregate-host-phase-peak-v1", "host_phase_peak_bytes"),
    "linux_resident_bytes": ("bytes", "aggregate-linux-phase-peak-v1", "linux_phase_peak_bytes"),
    "resident_spread_bytes": ("bytes", "matched-cohort-aggregate-phase-peak-range-v1", "phase_resident_spread_bytes"),
    "allocated_peak_bytes": ("bytes", "whole-owner-allocated-peak-v1", "allocated_peak_bytes"),
    "allocated_final_bytes": ("bytes", "whole-owner-allocated-final-v1", "allocated_final_bytes"),
    "eligible_debt_bytes": ("bytes", "maximum-eligible-reclamation-debt-v1", "eligible_debt_peak_bytes"),
    "headroom_bytes": ("bytes", "minimum-protected-cleanup-capacity-v1", "protected_headroom_bytes"),
    "observer_interval_ns": ("ns", "nominal-phase-observer-sampling-interval-v1", "observer_interval_ns"),
    "observer_gap_ns": ("ns", "uncertainty-inclusive-maximum-observer-gap-v1", "observer_gap_ns"),
    "clock_error_ns": ("ns", "maximum-attested-clock-uncertainty-v1", "clock_error_ns"),
    "observer_overhead_ns": ("ns", "measured-observer-overhead-v1", "observer_overhead_ns"),
    "sql_vm_steps": ("steps", "whole-original-operation-actual-vm-steps-v1", "sql_vm_steps"),
    "sql_statement_attempts": ("attempts", "whole-original-operation-sql-attempts-v1", "sql_statement_attempts"),
    "physical_read_bytes": ("bytes", "whole-owner-physical-read-volume-v1", "physical_read_bytes"),
    "physical_write_bytes": ("bytes", "whole-owner-physical-write-volume-v1", "physical_write_bytes"),
    "copy_bytes": ("bytes", "whole-original-operation-copy-volume-v1", "whole_operation_copy_bytes"),
    "queue_jobs": ("jobs", "aggregate-admitted-queue-peak-v1", "queue_peak_jobs"),
    "queue_bytes": ("bytes", "aggregate-admitted-queue-byte-peak-v1", "queue_peak_bytes"),
}


def numerical_scope(context):
    if context.get("limit") not in LIMIT_DEFINITIONS or any(not isinstance(context.get(key), str) or not context[key] for key in ("case", "profile", "route")):
        raise ValueError("numerical target case/profile/route/limit scope missing")
    return {key: context[key] for key in ("case", "profile", "route", "limit")}


def integer(value, *, positive=False):
    return type(value) is int and value >= (1 if positive else 0)


def sha256(path):
    digest = hashlib.sha256()
    with Path(path).open("rb") as stream:
        for block in iter(lambda: stream.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def artifact(value, root):
    """Inspect one sealed first-party input, never execute its contents."""
    if not isinstance(value, dict) or not isinstance(value.get("path"), str):
        raise ValueError("missing artifact path/hash")
    path = (Path(root) / value["path"]).resolve()
    if not path.is_relative_to(Path(root).resolve()) or not path.is_file():
        raise ValueError("artifact outside owning checkout or absent")
    if sha256(path) != value.get("sha256"):
        raise ValueError("artifact hash mismatch: " + value["path"])
    return path


def calibration(value, root, context):
    """Derive a limit from a previously eligible matched control, with integers.

The owning specification chooses numerator/denominator/offset prospectively.
This helper supplies no tolerance, new baseline or candidate-derived constant.
The registration layer also verifies its specification's committed identity.
"""
    receipt = json.loads(artifact(value.get("control"), root).read_text())
    if not isinstance(receipt, dict):
        raise ValueError("calibration control receipt schema missing")
    if receipt.get("kind") != "control" or not integer(receipt.get("sample_count")) or receipt["sample_count"] != 1:
        raise ValueError("calibration needs one untouched control sample")
    if receipt.get("admission_eligible") is not True or any(
        receipt.get(name) != "PASS"
        for name in ("status", "cache_status", "correctness_status", "resource_status", "cleanup_status")
    ):
        raise ValueError("calibration control is not eligible")
    match = receipt.get("match")
    if not isinstance(match, dict):
        raise ValueError("calibration matching scope missing")
    for key in ("profile", "route", "workload_sha256", "cache_sha256", "observer_sha256"):
        if context.get(key) is None or match.get(key) != context[key]:
            raise ValueError("calibration mismatch: " + key)
    scope = numerical_scope(context)
    unit, meaning, metric_name = LIMIT_DEFINITIONS[scope["limit"]]
    descriptors = receipt.get("metrics")
    descriptor = descriptors.get(value.get("metric")) if isinstance(descriptors, dict) else None
    if not isinstance(descriptor, dict) or not integer(descriptor.get("value")):
        raise ValueError("calibration metric/formula incomplete")
    if value.get("metric") != metric_name or value.get("unit") != unit or descriptor.get("unit") != unit or descriptor.get("meaning") != meaning or descriptor.get("scope") != scope:
        raise ValueError("calibration metric meaning/unit/scope mismatch")
    metric = descriptor["value"]
    numerator, denominator = value.get("numerator"), value.get("denominator")
    offset = value.get("offset", 0)
    if not integer(metric) or not integer(numerator, positive=True) or not integer(denominator, positive=True) or not integer(offset):
        raise ValueError("calibration metric/formula incomplete")
    if value.get("candidate_sample_taken") is not False or value.get("untouched_control") is not True:
        raise ValueError("calibration must precede candidate sampling/treatment")
    return (metric * numerator + denominator - 1) // denominator + offset


def resolve_limit(value, root, context):
    if not isinstance(value, dict):
        raise ValueError("missing numerical limit")
    if value.get("kind") == "calibration":
        return calibration(value, root, context)
    if value.get("kind") == "contract" and integer(value.get("value")):
        authority = json.loads(artifact(value.get("authority"), root).read_text())
        if not isinstance(value.get("anchor"), str) or not value["anchor"] or not isinstance(authority, dict) or authority.get("schema") != "cluster-two-numerical-authority-v1":
            raise ValueError("numerical contract needs its owning anchor")
        scope = numerical_scope(context)
        unit, meaning, _ = LIMIT_DEFINITIONS[scope["limit"]]
        entries = authority.get("limits")
        entry = entries.get(value["anchor"]) if isinstance(entries, dict) else None
        if not isinstance(entry, dict) or not integer(entry.get("value")) or entry["value"] != value["value"] or entry.get("unit") != unit or value.get("unit") != unit or entry.get("meaning") != meaning or entry.get("scope") != scope:
            raise ValueError("numerical authority key/value/unit/meaning/scope mismatch")
        return entry["value"]
    raise ValueError("limit needs a numeric owning contract or eligible calibration")


def evaluate(gates, observations, limits):
    """Re-derive each separate gate; do not sum nested/inclusive durations."""
    rows = []
    for gate in gates:
        name, metric = gate["id"], gate["metric"]
        observed = observations.get(metric)
        bound = gate.get("value") if "value" in gate else limits.get(gate.get("limit"))
        reason = None
        if not integer(observed):
            reason = "missing/non-numeric observation: " + metric
        elif not integer(bound):
            reason = "missing numerical bound: " + str(gate.get("limit"))
        elif gate.get("operator") not in ("eq", "le", "ge"):
            reason = "unsupported gate operator"
        if reason:
            status = "INCOMPLETE"
        else:
            passed = {"eq": observed == bound, "le": observed <= bound, "ge": observed >= bound}[gate["operator"]]
            status = "PASS" if passed else "FAIL"
        rows.append({"gate": name, "metric": metric, "observed": observed,
                     "bound": bound, "status": status, "reason": reason})
    status = "FAIL" if any(row["status"] == "FAIL" for row in rows) else "INCOMPLETE" if any(row["status"] == "INCOMPLETE" for row in rows) else "PASS"
    return {"status": status, "gates": rows}


def resource_phase(value, *, sample_interval_ns, maximum_gap_ns, clock_error_ns):
    """Validate a finite sampled inventory, never a continuous or lifetime peak."""
    if not isinstance(value, dict) or value.get("scope") != "phase":
        raise ValueError("phase resource witness required; lifetime peak forbidden")
    if value.get("peak_kind") != "sampled":
        raise ValueError("sampled inventory required; continuous peak source is not implemented")
    fields = ("baseline_bytes", "peak_bytes", "final_bytes", "opened_ns", "closed_ns", "first_ns", "last_ns", "largest_gap_ns")
    if any(not integer(value.get(key)) for key in fields):
        raise ValueError("missing phase resource boundary/value")
    if value["peak_bytes"] < max(value["baseline_bytes"], value["final_bytes"]):
        raise ValueError("phase peak below baseline/final")
    uncertainty = value.get("time_uncertainty_ns")
    if not integer(clock_error_ns) or not integer(uncertainty) or uncertainty > clock_error_ns:
        raise ValueError("clock uncertainty missing or above frozen bound")
    sample_clock, boundary_clock = value.get("sample_clock"), value.get("boundary_clock")
    if not isinstance(sample_clock, str) or not sample_clock or not isinstance(boundary_clock, str) or not boundary_clock:
        raise ValueError("sample/boundary clock identities missing")
    timestamps = value.get("sample_timestamps_ns")
    if not isinstance(timestamps, list) or len(timestamps) < 3 or any(not integer(t) for t in timestamps):
        raise ValueError("phase needs baseline, guaranteed interior and final timestamps")
    if value.get("samples") != len(timestamps) or type(value.get("samples")) is not int:
        raise ValueError("phase sample inventory/count mismatch")
    byte_values = value.get("sample_byte_values")
    if not isinstance(byte_values, list) or len(byte_values) != len(timestamps) or any(not integer(b) for b in byte_values):
        raise ValueError("phase byte inventory missing/mismatched")
    derived_peak = max(byte_values)
    if byte_values[0] != value["baseline_bytes"] or byte_values[-1] != value["final_bytes"] or derived_peak != value["peak_bytes"]:
        raise ValueError("phase baseline/peak/final disagree with actual byte inventory")
    if any(a >= b for a, b in zip(timestamps, timestamps[1:])):
        raise ValueError("phase timestamp inventory must be strictly increasing")
    if sample_clock != boundary_clock:
        calibration_ = value.get("clock_calibration", {})
        offset = calibration_.get("offset_ns")
        source_timestamps = value.get("source_timestamps_ns")
        if calibration_.get("schema") != "cluster-two-clock-calibration-v1" or calibration_.get("status") != "PASS" or calibration_.get("source_clock") != sample_clock or calibration_.get("target_clock") != boundary_clock or type(offset) is not int:
            raise ValueError("different clocks require attested bounded calibration")
        error = calibration_.get("maximum_error_ns")
        if not integer(error) or error > uncertainty or calibration_.get("validated_receipt") is not True:
            raise ValueError("clock calibration error/attestation missing")
        if not isinstance(source_timestamps, list) or len(source_timestamps) != len(timestamps) or any(not integer(t) for t in source_timestamps) or [t + offset for t in source_timestamps] != timestamps:
            raise ValueError("calibrated timestamp conversion mismatch")
    gap = max(b - a for a, b in zip(timestamps, timestamps[1:]))
    if timestamps[0] != value["first_ns"] or timestamps[-1] != value["last_ns"] or gap != value["largest_gap_ns"]:
        raise ValueError("phase timestamp boundaries/gap disagree with inventory")
    if not value["first_ns"] + uncertainty <= value["opened_ns"] - uncertainty <= value["closed_ns"] - uncertainty or value["last_ns"] - uncertainty < value["closed_ns"] + uncertainty:
        raise ValueError("phase observer missed a guaranteed boundary under clock uncertainty")
    if not any(value["opened_ns"] + 2 * uncertainty < t < value["closed_ns"] - 2 * uncertainty for t in timestamps):
        raise ValueError("phase observer has no guaranteed interior sample under clock uncertainty")
    if not integer(sample_interval_ns, positive=True) or not integer(maximum_gap_ns, positive=True):
        raise ValueError("observer precision not frozen")
    if value["largest_gap_ns"] + 2 * uncertainty > maximum_gap_ns or value.get("sample_interval_ns") != sample_interval_ns:
        raise ValueError("observer gap/interval outside frozen contract")
    return {"status": "PASS", "peak_kind": value["peak_kind"], "peak_bytes": derived_peak,
            "time_uncertainty_ns": uncertainty, "sample_count": len(timestamps)}
