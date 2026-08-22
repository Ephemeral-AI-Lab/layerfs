#!/usr/bin/env python3
"""Primary G3-v1 gate analysis; stdlib-only and independent of recomputation."""

import copy
import hashlib
import json
import sys
from pathlib import Path

MIB = 1024 * 1024
Q_RSS_LIMIT = 20 * MIB
STORAGE_LIMIT = 512 * MIB
OPERATION_LIMIT_NS = 5_000_000_000
TOTAL_LIMIT_NS = 20_000_000_000
SCHEDULE = (
    (1, "qualified-noop", 10 * MIB),
    (2, "qualified-one-byte", 100 * MIB),
    (3, "qualified-one-mib", 10 * MIB),
    (4, "invalid-authority", MIB),
    (5, "external-mutation", MIB),
    (6, "symlink-substitution", MIB),
    (7, "count-change", MIB),
    (8, "before-publication-fault", MIB),
    (9, "lost-ack", MIB),
)
ROUTES = {
    "qualified-noop": ("qualified-noop", "seed-hit", "success", None),
    "qualified-one-byte": ("qualified-patch", "seed-hit", "success", None),
    "qualified-one-mib": ("qualified-patch", "seed-hit", "success", None),
    "invalid-authority": ("complete-fallback", "invalid-authority", "success", None),
    "external-mutation": ("complete-fallback", "destination-invalidated", "success", None),
    "symlink-substitution": ("typed-rejection", "destination-symlink", "typed-error", "NativeDestinationSymlink"),
    "count-change": ("complete-fallback", "count-change", "success", None),
    "before-publication-fault": ("qualified-patch", "seed-hit", "typed-error", "InjectedBeforePublication"),
    "lost-ack": ("qualified-patch", "seed-hit", "success", None),
}
BINDINGS = [
    "store_instance", "validation_authority", "profile", "integrity_epoch",
    "generation", "receipt_transition", "parent_root", "target_root",
    "destination_identity", "open_serial", "mutation_serial",
    "publication_serial", "operation", "nonce", "seed_identity",
]
COUNTERS = (
    "authority_reads", "authority_bytes_read", "seed_authority_reads",
    "seed_authority_bytes_read", "authority_validations",
    "authority_validation_successes", "authority_validation_failures",
    "permit_consumptions", "mapping_sql_queries", "mapping_sql_rows",
    "object_sql_queries", "object_sql_rows", "payload_sql_queries",
    "payload_sql_rows", "canonical_blob_reads", "canonical_blob_bytes",
    "authenticated_objects", "canonical_bytes_authenticated",
    "source_bytes_reconstructed", "destination_bytes_read",
    "verification_bytes_read", "clone_calls", "clone_successes",
    "clone_failures", "clone_source_logical_bytes", "copy_calls",
    "copied_payload_bytes", "patch_calls", "patch_bytes", "fallback_calls",
    "fallback_write_bytes", "changed_ranges", "changed_bytes",
    "metadata_operations", "temp_files_created", "temp_files_removed",
    "seed_files_created", "seed_files_removed", "data_sync_calls",
    "metadata_sync_calls", "rename_calls", "directory_sync_calls",
    "reconciliation_calls", "q_high_water", "q_terminal",
    "temp_logical_bytes", "temp_apparent_bytes", "temp_allocated_bytes",
    "seed_logical_bytes", "seed_apparent_bytes", "seed_allocated_bytes",
    "output_length", "output_mode", "temp_residue_count",
    "seed_residue_count", "timer_preflight_ns", "timer_qualification_ns",
    "timer_payload_prepare_ns", "timer_data_sync_ns", "timer_metadata_ns",
    "timer_metadata_sync_ns", "timer_rename_ns", "timer_directory_sync_ns",
    "timer_reconciliation_ns", "timer_cleanup_ns", "attributed_wall_ns",
    "unattributed_wall_ns", "operation_total_ns", "child_timeout_seconds",
    "child_exit_code", "maximum_resident_set_bytes",
)
TIMER_FIELDS = (
    "timer_preflight_ns", "timer_qualification_ns", "timer_payload_prepare_ns",
    "timer_data_sync_ns", "timer_metadata_ns", "timer_metadata_sync_ns",
    "timer_rename_ns", "timer_directory_sync_ns", "timer_reconciliation_ns",
    "timer_cleanup_ns",
)
TEXT_FIELDS = (
    "schema", "scenario", "route", "outcome", "qualification_reason",
    "parent_root", "target_root", "reconciliation_outcome", "output_digest",
    "expected_output_digest", "old_or_new", "physical_io_status",
    "cache_warmth_status", "stable_media_status", "label",
    "executable_sha256", "source_sha256", "methodology_set_sha256",
    "environment_sha256",
)
SUMMARY_FIELDS = (
    "sequence", "scenario", "size_bytes", "route", "outcome",
    "qualification_reason", "error", "generation", "authority_validations",
    "authority_validation_successes", "authority_validation_failures",
    "permit_consumptions", "payload_sql_queries", "payload_sql_rows",
    "canonical_blob_reads", "canonical_bytes_authenticated",
    "source_bytes_reconstructed", "destination_bytes_read", "clone_calls",
    "patch_calls", "patch_bytes", "fallback_calls", "fallback_write_bytes",
    "changed_ranges", "changed_bytes", "temp_files_created",
    "temp_files_removed", "rename_calls", "directory_sync_calls",
    "reconciliation_calls", "reconciliation_outcome", "q_high_water",
    "q_terminal", "output_length", "output_mode", "byte_exact", "mode_exact",
    "old_or_new", "temp_residue_count", "seed_residue_count",
    "operation_total_ns", "maximum_resident_set_bytes", "executable_sha256",
    "source_sha256", "methodology_set_sha256",
)
GATE_NAMES = (
    "schedule", "shape", "authority", "route", "direct_counters", "fallback",
    "publication", "timers", "exactness", "resources", "cleanup", "custody",
)


def canonical_sha256(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(",", ":")).encode()).hexdigest()


def add(failures, gate, sequence, detail):
    failures.append(f"{gate}:{sequence}:{detail}")


def validate(rows, cleanup):
    failures = []
    expected_shape = list(SCHEDULE)
    actual_shape = [(row.get("sequence"), row.get("scenario"), row.get("size_bytes")) for row in rows]
    if actual_shape != expected_shape:
        add(failures, "schedule", 0, "exact-order")

    for expected, row in zip(SCHEDULE, rows):
        sequence, scenario, size = expected
        missing = [name for name in (*TEXT_FIELDS, *COUNTERS, "sequence", "size_bytes", "generation", "error", "authority_bindings_checked", "byte_exact", "mode_exact", "command", "external_real_seconds", "external_user_seconds", "external_system_seconds") if name not in row]
        if missing:
            add(failures, "shape", sequence, "missing-" + ",".join(missing))
            continue
        if row["schema"] != "phase4-g3-row-v1":
            add(failures, "shape", sequence, "schema")
        if any(type(row[name]) is not int or row[name] < 0 for name in COUNTERS):
            add(failures, "shape", sequence, "counter-type")
        if type(row["generation"]) is not int or row["generation"] < 0:
            add(failures, "shape", sequence, "generation")
        if any(type(row[name]) is not str or not row[name] for name in TEXT_FIELDS):
            add(failures, "shape", sequence, "text")
        if type(row["byte_exact"]) is not bool or type(row["mode_exact"]) is not bool:
            add(failures, "shape", sequence, "boolean")
        if not isinstance(row["command"], list) or not row["command"]:
            add(failures, "shape", sequence, "command")
        if any(type(row[name]) not in (int, float) or row[name] < 0 for name in ("external_real_seconds", "external_user_seconds", "external_system_seconds")):
            add(failures, "shape", sequence, "external-time")

        route_tuple = (row["route"], row["qualification_reason"], row["outcome"], row["error"])
        if route_tuple != ROUTES[scenario]:
            add(failures, "route", sequence, "route-reason-outcome-error")
        expected_bindings = [] if scenario == "symlink-substitution" else BINDINGS
        if row["authority_bindings_checked"] != expected_bindings:
            add(failures, "authority", sequence, "bindings")
        if row["authority_validations"] != row["authority_validation_successes"] + row["authority_validation_failures"]:
            add(failures, "authority", sequence, "partition")
        if scenario == "symlink-substitution":
            if any(row[name] for name in ("authority_reads", "authority_bytes_read", "seed_authority_reads", "seed_authority_bytes_read", "authority_validations", "authority_validation_successes", "authority_validation_failures", "permit_consumptions")):
                add(failures, "authority", sequence, "preflight-did-authority-work")
        elif scenario == "invalid-authority":
            if not (row["authority_validations"] >= 1 and row["authority_validation_successes"] == 0 and row["authority_validation_failures"] >= 1 and row["permit_consumptions"] == 0):
                add(failures, "authority", sequence, "invalid-authority-gate")
        elif row["route"].startswith("qualified"):
            if not (row["authority_validations"] >= 1 and row["authority_validation_successes"] == row["authority_validations"] and row["authority_validation_failures"] == 0 and row["permit_consumptions"] == 1 and row["seed_authority_reads"] >= 1 and row["seed_authority_bytes_read"] >= 1):
                add(failures, "authority", sequence, "qualified-gate")
        elif not (row["authority_validations"] >= 1 and row["authority_validation_successes"] >= 1 and row["authority_validation_failures"] == 0 and row["permit_consumptions"] == 0):
            add(failures, "authority", sequence, "fallback-gate")

        if row["payload_sql_queries"] != row["mapping_sql_queries"] + row["object_sql_queries"] or row["payload_sql_rows"] != row["mapping_sql_rows"] + row["object_sql_rows"]:
            add(failures, "direct_counters", sequence, "sql-equation")
        if row["canonical_blob_bytes"] != row["canonical_bytes_authenticated"]:
            add(failures, "direct_counters", sequence, "blob-auth-equation")
        if row["authority_reads"] < row["authority_validations"]:
            add(failures, "authority", sequence, "reads")

        if scenario == "qualified-noop":
            zero = ("mapping_sql_queries", "mapping_sql_rows", "object_sql_queries", "object_sql_rows", "payload_sql_queries", "payload_sql_rows", "canonical_blob_reads", "canonical_blob_bytes", "authenticated_objects", "canonical_bytes_authenticated", "source_bytes_reconstructed", "copy_calls", "copied_payload_bytes", "patch_calls", "patch_bytes", "fallback_calls", "fallback_write_bytes", "changed_ranges", "changed_bytes")
            if any(row[name] for name in zero) or (row["clone_calls"], row["clone_successes"], row["clone_failures"]) != (1, 1, 0) or row["clone_source_logical_bytes"] != size:
                add(failures, "direct_counters", sequence, "noop-work")
        elif scenario in ("qualified-one-byte", "qualified-one-mib", "before-publication-fault", "lost-ack"):
            changed = MIB if scenario == "qualified-one-mib" else 1
            if not (row["changed_ranges"] == 1 and row["changed_bytes"] == changed and row["patch_bytes"] == changed and row["patch_calls"] >= 1 and (row["clone_calls"], row["clone_successes"], row["clone_failures"]) == (1, 1, 0) and row["clone_source_logical_bytes"] == size and row["fallback_calls"] == 0 and row["source_bytes_reconstructed"] == 0 and row["copy_calls"] == 0 and row["copied_payload_bytes"] == 0 and 0 < row["canonical_bytes_authenticated"] <= changed + MIB):
                add(failures, "direct_counters", sequence, "bounded-patch-work")
        elif row["route"] == "complete-fallback":
            if not (row["fallback_calls"] == 1 and row["source_bytes_reconstructed"] == row["output_length"] and row["fallback_write_bytes"] == row["output_length"] and row["clone_calls"] == 0 and row["patch_calls"] == 0 and row["patch_bytes"] == 0 and row["copy_calls"] == 0 and row["copied_payload_bytes"] == 0):
                add(failures, "fallback", sequence, "complete-work")
        else:
            mutation_fields = ("mapping_sql_queries", "mapping_sql_rows", "object_sql_queries", "object_sql_rows", "payload_sql_queries", "payload_sql_rows", "canonical_blob_reads", "canonical_blob_bytes", "authenticated_objects", "canonical_bytes_authenticated", "source_bytes_reconstructed", "destination_bytes_read", "clone_calls", "clone_successes", "clone_failures", "clone_source_logical_bytes", "copy_calls", "copied_payload_bytes", "patch_calls", "patch_bytes", "fallback_calls", "fallback_write_bytes", "changed_ranges", "changed_bytes", "metadata_operations", "temp_files_created", "temp_files_removed", "seed_files_created", "seed_files_removed", "data_sync_calls", "metadata_sync_calls", "rename_calls", "directory_sync_calls", "reconciliation_calls")
            if any(row[name] for name in mutation_fields):
                add(failures, "direct_counters", sequence, "symlink-work")

        expected_length = size + 1 if scenario == "count-change" else size
        expected_state = "old" if scenario in ("symlink-substitution", "before-publication-fault") else "new"
        if row["output_length"] != expected_length or not row["byte_exact"] or not row["mode_exact"] or row["output_digest"] != row["expected_output_digest"] or row["old_or_new"] != expected_state or row["temp_residue_count"] != 0 or row["seed_residue_count"] != 0:
            add(failures, "exactness", sequence, "output-mode-residue")
        if row["seed_files_created"] != row["seed_files_removed"]:
            add(failures, "publication", sequence, "seed-cleanup")
        if scenario == "symlink-substitution":
            if row["reconciliation_outcome"] != "not-needed":
                add(failures, "publication", sequence, "symlink-reconciliation")
        elif scenario == "before-publication-fault":
            if row["rename_calls"] != 0 or row["temp_files_created"] != row["temp_files_removed"] or row["reconciliation_calls"] != 0 or row["reconciliation_outcome"] != "not-needed":
                add(failures, "publication", sequence, "before-publication")
        else:
            if row["temp_files_created"] != row["temp_files_removed"] + row["rename_calls"] or row["metadata_operations"] < 1 or (row["data_sync_calls"], row["metadata_sync_calls"], row["rename_calls"], row["directory_sync_calls"]) != (1, 1, 1, 1):
                add(failures, "publication", sequence, "durable-publish")
            expected_reconciliation = (1, "target") if scenario == "lost-ack" else (0, "not-needed")
            if (row["reconciliation_calls"], row["reconciliation_outcome"]) != expected_reconciliation:
                add(failures, "publication", sequence, "reconciliation")

        timer_sum = sum(row[name] for name in TIMER_FIELDS)
        if row["attributed_wall_ns"] != timer_sum or row["operation_total_ns"] != row["attributed_wall_ns"] + row["unattributed_wall_ns"] or row["operation_total_ns"] >= OPERATION_LIMIT_NS:
            add(failures, "timers", sequence, "equation-or-ceiling")
        if row["q_terminal"] != 0 or row["q_high_water"] > Q_RSS_LIMIT or row["maximum_resident_set_bytes"] > Q_RSS_LIMIT:
            add(failures, "resources", sequence, "q-rss")
        if max(row[name] for name in ("temp_logical_bytes", "temp_apparent_bytes", "temp_allocated_bytes", "seed_logical_bytes", "seed_apparent_bytes", "seed_allocated_bytes")) > STORAGE_LIMIT:
            add(failures, "resources", sequence, "storage")
        if any(not row[name].startswith("Unavailable:") for name in ("physical_io_status", "cache_warmth_status", "stable_media_status")):
            add(failures, "resources", sequence, "unsupported-status")
        if row["child_exit_code"] != 0 or row["child_timeout_seconds"] != (15 if scenario == "qualified-one-byte" else 5):
            add(failures, "custody", sequence, "child")
        if any(len(row[name]) != 64 for name in ("executable_sha256", "source_sha256", "methodology_set_sha256", "environment_sha256")):
            add(failures, "custody", sequence, "hash-shape")

    if sum(row.get("operation_total_ns", OPERATION_LIMIT_NS) for row in rows) >= TOTAL_LIMIT_NS:
        add(failures, "timers", 0, "operation-sum")
    if not (cleanup.get("status") == "PASS" and cleanup.get("declared_root") == "work-v1" and cleanup.get("work_root_absent") is True and cleanup.get("broad_deletion") is False and cleanup.get("peak_logical_bytes", STORAGE_LIMIT + 1) <= STORAGE_LIMIT and cleanup.get("peak_apparent_bytes", STORAGE_LIMIT + 1) <= STORAGE_LIMIT and cleanup.get("peak_allocated_bytes", STORAGE_LIMIT + 1) <= STORAGE_LIMIT):
        add(failures, "cleanup", 0, "runner-cleanup")

    failures = sorted(set(failures))
    gates = {name: not any(item.startswith(name + ":") for item in failures) for name in GATE_NAMES}
    normalized = {
        "schema": "phase4-g3-v1-normalized-ledger-v1",
        "failures": failures,
        "gates": gates,
        "operation_total_ns": sum(row.get("operation_total_ns", 0) for row in rows),
        "rows": [{name: row.get(name) for name in SUMMARY_FIELDS} for row in rows],
    }
    return {"status": "PASS" if not failures else "REVISE", "normalized_ledger": normalized, "normalized_ledger_sha256": canonical_sha256(normalized)}


def synthetic_rows():
    rows = []
    for sequence, scenario, size in SCHEDULE:
        route, reason, outcome, error = ROUTES[scenario]
        row = {name: 0 for name in COUNTERS}
        row.update({
            "schema": "phase4-g3-row-v1", "sequence": sequence,
            "label": f"{sequence:02d}-{scenario}", "scenario": scenario,
            "size_bytes": size, "route": route, "qualification_reason": reason,
            "outcome": outcome, "error": error, "generation": 1,
            "parent_root": "p", "target_root": "t",
            "authority_bindings_checked": [] if scenario == "symlink-substitution" else BINDINGS.copy(),
            "reconciliation_outcome": "target" if scenario == "lost-ack" else "not-needed",
            "output_digest": "d", "expected_output_digest": "d",
            "byte_exact": True, "mode_exact": True,
            "old_or_new": "old" if scenario in ("symlink-substitution", "before-publication-fault") else "new",
            "physical_io_status": "Unavailable: synthetic", "cache_warmth_status": "Unavailable: synthetic",
            "stable_media_status": "Unavailable: synthetic", "command": ["synthetic"],
            "external_real_seconds": 0.01, "external_user_seconds": 0.0,
            "external_system_seconds": 0.0, "peak_memory_footprint_bytes": 1,
            "executable_sha256": "a" * 64, "source_sha256": "b" * 64,
            "methodology_set_sha256": "c" * 64, "environment_sha256": "e" * 64,
        })
        row.update({"authority_reads": 1, "authority_bytes_read": 32, "authority_validations": 1, "authority_validation_successes": 1, "seed_files_created": 1, "seed_files_removed": 1, "output_length": size + (scenario == "count-change"), "output_mode": 0o644, "verification_bytes_read": size, "q_high_water": 1, "maximum_resident_set_bytes": 1, "child_timeout_seconds": 15 if scenario == "qualified-one-byte" else 5})
        for name in TIMER_FIELDS:
            row[name] = 1
        row.update({"attributed_wall_ns": len(TIMER_FIELDS), "unattributed_wall_ns": 1, "operation_total_ns": len(TIMER_FIELDS) + 1})
        if scenario == "symlink-substitution":
            for name in ("authority_reads", "authority_bytes_read", "authority_validations", "authority_validation_successes", "seed_files_created", "seed_files_removed", "verification_bytes_read"):
                row[name] = 0
        elif scenario == "invalid-authority":
            row.update({"authority_validation_successes": 0, "authority_validation_failures": 1})
        elif route.startswith("qualified"):
            row.update({"seed_authority_reads": 1, "seed_authority_bytes_read": 16, "permit_consumptions": 1})
        if scenario == "qualified-noop":
            row.update({"clone_calls": 1, "clone_successes": 1, "clone_source_logical_bytes": size})
        elif scenario in ("qualified-one-byte", "qualified-one-mib", "before-publication-fault", "lost-ack"):
            changed = MIB if scenario == "qualified-one-mib" else 1
            row.update({"mapping_sql_queries": 1, "mapping_sql_rows": 1, "object_sql_queries": 1, "object_sql_rows": 1, "payload_sql_queries": 2, "payload_sql_rows": 2, "canonical_blob_reads": 1, "canonical_blob_bytes": changed, "authenticated_objects": 1, "canonical_bytes_authenticated": changed, "clone_calls": 1, "clone_successes": 1, "clone_source_logical_bytes": size, "patch_calls": 1, "patch_bytes": changed, "changed_ranges": 1, "changed_bytes": changed})
        elif route == "complete-fallback":
            length = row["output_length"]
            row.update({"mapping_sql_queries": 1, "mapping_sql_rows": 1, "object_sql_queries": 1, "object_sql_rows": 1, "payload_sql_queries": 2, "payload_sql_rows": 2, "canonical_blob_reads": 1, "canonical_blob_bytes": length, "authenticated_objects": 1, "canonical_bytes_authenticated": length, "source_bytes_reconstructed": length, "fallback_calls": 1, "fallback_write_bytes": length})
        if scenario not in ("symlink-substitution", "before-publication-fault"):
            row.update({"metadata_operations": 1, "temp_files_created": 1, "data_sync_calls": 1, "metadata_sync_calls": 1, "rename_calls": 1, "directory_sync_calls": 1})
        elif scenario == "before-publication-fault":
            row.update({"temp_files_created": 1, "temp_files_removed": 1})
        if scenario == "lost-ack":
            row["reconciliation_calls"] = 1
        rows.append(row)
    return rows


def self_check():
    cleanup = {"status": "PASS", "declared_root": "work-v1", "work_root_absent": True, "broad_deletion": False, "peak_logical_bytes": 1, "peak_apparent_bytes": 1, "peak_allocated_bytes": 1}
    rows = synthetic_rows()
    assert validate(rows, cleanup)["status"] == "PASS"
    mutations = []
    swapped = copy.deepcopy(rows); swapped[0], swapped[1] = swapped[1], swapped[0]; mutations.append((swapped, cleanup))
    for index, field, value in ((0, "authority_bindings_checked", []), (1, "route", "complete-fallback"), (1, "patch_bytes", 2), (3, "fallback_calls", 0), (7, "old_or_new", "new"), (8, "reconciliation_calls", 0), (0, "operation_total_ns", 99), (0, "q_terminal", 1), (0, "byte_exact", False)):
        changed = copy.deepcopy(rows); changed[index][field] = value; mutations.append((changed, cleanup))
    bad_cleanup = dict(cleanup, work_root_absent=False); mutations.append((rows, bad_cleanup))
    assert all(validate(candidate, state)["status"] == "REVISE" for candidate, state in mutations)
    print(json.dumps({"status": "PASS", "mutations_rejected": len(mutations)}, sort_keys=True))


def main():
    if sys.argv[1:] == ["--self-check"]:
        self_check()
        return
    if len(sys.argv) != 2:
        raise SystemExit("usage: analyze_g3_v1.py RESULTS | --self-check")
    results = Path(sys.argv[1])
    raw = results / "rows-v1/G3-V1-RAW.jsonl"
    rows = [json.loads(line) for line in raw.read_text().splitlines() if line]
    cleanup = json.loads((results / "CLEANUP-v1.json").read_text())
    report = {"schema": "phase4-g3-v1-primary-analysis-v1", **validate(rows, cleanup)}
    print(json.dumps(report, separators=(",", ":"), sort_keys=True))


if __name__ == "__main__":
    main()
