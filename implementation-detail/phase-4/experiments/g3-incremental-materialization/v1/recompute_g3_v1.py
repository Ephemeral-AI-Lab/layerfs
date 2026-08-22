#!/usr/bin/env python3
"""Independent G3-v1 recomputation; deliberately shares no analysis code."""

import copy
import hashlib
import json
import sys
from pathlib import Path

MiB = 1 << 20
ORDER = [
    (1, "qualified-noop", 10 * MiB),
    (2, "qualified-one-byte", 100 * MiB),
    (3, "qualified-one-mib", 10 * MiB),
    (4, "invalid-authority", MiB),
    (5, "external-mutation", MiB),
    (6, "symlink-substitution", MiB),
    (7, "count-change", MiB),
    (8, "before-publication-fault", MiB),
    (9, "lost-ack", MiB),
]
EXPECTED_ROUTE = {
    "qualified-noop": ["qualified-noop", "seed-hit", "success", None],
    "qualified-one-byte": ["qualified-patch", "seed-hit", "success", None],
    "qualified-one-mib": ["qualified-patch", "seed-hit", "success", None],
    "invalid-authority": ["complete-fallback", "invalid-authority", "success", None],
    "external-mutation": ["complete-fallback", "destination-invalidated", "success", None],
    "symlink-substitution": ["typed-rejection", "destination-symlink", "typed-error", "NativeDestinationSymlink"],
    "count-change": ["complete-fallback", "count-change", "success", None],
    "before-publication-fault": ["qualified-patch", "seed-hit", "typed-error", "InjectedBeforePublication"],
    "lost-ack": ["qualified-patch", "seed-hit", "success", None],
}
AUTHORITY_VECTOR = [
    "store_instance", "validation_authority", "profile", "integrity_epoch",
    "generation", "receipt_transition", "parent_root", "target_root",
    "destination_identity", "open_serial", "mutation_serial",
    "publication_serial", "operation", "nonce", "seed_identity",
]
NUMERIC = {
    "sequence", "size_bytes", "generation", "authority_reads",
    "authority_bytes_read", "seed_authority_reads", "seed_authority_bytes_read",
    "authority_validations", "authority_validation_successes",
    "authority_validation_failures", "permit_consumptions",
    "mapping_sql_queries", "mapping_sql_rows", "object_sql_queries",
    "object_sql_rows", "payload_sql_queries", "payload_sql_rows",
    "canonical_blob_reads", "canonical_blob_bytes", "authenticated_objects",
    "canonical_bytes_authenticated", "source_bytes_reconstructed",
    "destination_bytes_read", "verification_bytes_read", "clone_calls",
    "clone_successes", "clone_failures", "clone_source_logical_bytes",
    "copy_calls", "copied_payload_bytes", "patch_calls", "patch_bytes",
    "fallback_calls", "fallback_write_bytes", "changed_ranges", "changed_bytes",
    "metadata_operations", "temp_files_created", "temp_files_removed",
    "seed_files_created", "seed_files_removed", "data_sync_calls",
    "metadata_sync_calls", "rename_calls", "directory_sync_calls",
    "reconciliation_calls", "q_high_water", "q_terminal",
    "temp_logical_bytes", "temp_apparent_bytes", "temp_allocated_bytes",
    "seed_logical_bytes", "seed_apparent_bytes", "seed_allocated_bytes",
    "output_length", "output_mode", "temp_residue_count", "seed_residue_count",
    "timer_preflight_ns", "timer_qualification_ns", "timer_payload_prepare_ns",
    "timer_data_sync_ns", "timer_metadata_ns", "timer_metadata_sync_ns",
    "timer_rename_ns", "timer_directory_sync_ns", "timer_reconciliation_ns",
    "timer_cleanup_ns", "attributed_wall_ns", "unattributed_wall_ns",
    "operation_total_ns", "child_timeout_seconds", "child_exit_code",
    "maximum_resident_set_bytes",
}
TIMERS = [
    "timer_preflight_ns", "timer_qualification_ns", "timer_payload_prepare_ns",
    "timer_data_sync_ns", "timer_metadata_ns", "timer_metadata_sync_ns",
    "timer_rename_ns", "timer_directory_sync_ns", "timer_reconciliation_ns",
    "timer_cleanup_ns",
]
STRINGS = {
    "schema", "scenario", "route", "outcome", "qualification_reason",
    "parent_root", "target_root", "reconciliation_outcome", "output_digest",
    "expected_output_digest", "old_or_new", "physical_io_status",
    "cache_warmth_status", "stable_media_status", "label", "executable_sha256",
    "source_sha256", "methodology_set_sha256", "environment_sha256",
}
ROW_VIEW = (
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
GATES = (
    "schedule", "shape", "authority", "route", "direct_counters", "fallback",
    "publication", "timers", "exactness", "resources", "cleanup", "custody",
)


def digest(value):
    encoded = json.dumps(value, separators=(",", ":"), sort_keys=True).encode()
    return hashlib.sha256(encoded).hexdigest()


def recompute(rows, cleanup):
    rejected = set()

    def require(gate, sequence, name, truth):
        if not truth:
            rejected.add(f"{gate}:{sequence}:{name}")

    require("schedule", 0, "exact-order", [(r.get("sequence"), r.get("scenario"), r.get("size_bytes")) for r in rows] == ORDER)
    for expected, row in zip(ORDER, rows):
        sequence, scenario, size = expected
        required = NUMERIC | STRINGS | {"error", "authority_bindings_checked", "byte_exact", "mode_exact", "command", "external_real_seconds", "external_user_seconds", "external_system_seconds"}
        require("shape", sequence, "required-fields", required <= row.keys())
        if not required <= row.keys():
            continue
        require("shape", sequence, "schema", row["schema"] == "phase4-g3-row-v1")
        require("shape", sequence, "nonnegative-integers", all(type(row[key]) is int and row[key] >= 0 for key in NUMERIC))
        require("shape", sequence, "strings", all(type(row[key]) is str and bool(row[key]) for key in STRINGS))
        require("shape", sequence, "booleans", type(row["byte_exact"]) is bool and type(row["mode_exact"]) is bool)
        require("shape", sequence, "command", isinstance(row["command"], list) and bool(row["command"]))
        require("shape", sequence, "external-times", all(type(row[key]) in (int, float) and row[key] >= 0 for key in ("external_real_seconds", "external_user_seconds", "external_system_seconds")))

        route_observation = [row["route"], row["qualification_reason"], row["outcome"], row["error"]]
        require("route", sequence, "decision", route_observation == EXPECTED_ROUTE[scenario])
        require("authority", sequence, "binding-vector", row["authority_bindings_checked"] == ([] if scenario == "symlink-substitution" else AUTHORITY_VECTOR))
        require("authority", sequence, "validation-partition", row["authority_validations"] == row["authority_validation_successes"] + row["authority_validation_failures"])
        require("authority", sequence, "read-count", row["authority_reads"] >= row["authority_validations"])
        if scenario == "symlink-substitution":
            require("authority", sequence, "preflight-zero", sum(row[k] for k in ("authority_reads", "authority_bytes_read", "seed_authority_reads", "seed_authority_bytes_read", "authority_validations", "authority_validation_successes", "authority_validation_failures", "permit_consumptions")) == 0)
        elif scenario == "invalid-authority":
            require("authority", sequence, "invalid", row["authority_validations"] >= 1 and row["authority_validation_successes"] == 0 and row["authority_validation_failures"] >= 1 and row["permit_consumptions"] == 0)
        elif row["route"] in ("qualified-noop", "qualified-patch"):
            require("authority", sequence, "qualified", row["authority_validations"] >= 1 and row["authority_validation_successes"] == row["authority_validations"] and row["authority_validation_failures"] == 0 and row["permit_consumptions"] == 1 and row["seed_authority_reads"] >= 1 and row["seed_authority_bytes_read"] >= 1)
        else:
            require("authority", sequence, "fallback", row["authority_validations"] >= 1 and row["authority_validation_successes"] >= 1 and row["authority_validation_failures"] == 0 and row["permit_consumptions"] == 0)

        require("direct_counters", sequence, "query-sum", row["payload_sql_queries"] == row["mapping_sql_queries"] + row["object_sql_queries"])
        require("direct_counters", sequence, "row-sum", row["payload_sql_rows"] == row["mapping_sql_rows"] + row["object_sql_rows"])
        require("direct_counters", sequence, "canonical-bytes", row["canonical_blob_bytes"] == row["canonical_bytes_authenticated"])
        if scenario == "qualified-noop":
            payload_names = ("mapping_sql_queries", "mapping_sql_rows", "object_sql_queries", "object_sql_rows", "payload_sql_queries", "payload_sql_rows", "canonical_blob_reads", "canonical_blob_bytes", "authenticated_objects", "canonical_bytes_authenticated", "source_bytes_reconstructed", "copy_calls", "copied_payload_bytes", "patch_calls", "patch_bytes", "fallback_calls", "fallback_write_bytes", "changed_ranges", "changed_bytes")
            require("direct_counters", sequence, "noop", sum(row[k] for k in payload_names) == 0 and [row["clone_calls"], row["clone_successes"], row["clone_failures"], row["clone_source_logical_bytes"]] == [1, 1, 0, size])
        elif scenario in {"qualified-one-byte", "qualified-one-mib", "before-publication-fault", "lost-ack"}:
            delta = MiB if scenario == "qualified-one-mib" else 1
            require("direct_counters", sequence, "patch", row["changed_ranges"] == 1 and row["changed_bytes"] == delta and row["patch_bytes"] == delta and row["patch_calls"] >= 1 and [row["clone_calls"], row["clone_successes"], row["clone_failures"], row["clone_source_logical_bytes"]] == [1, 1, 0, size] and row["fallback_calls"] == row["source_bytes_reconstructed"] == row["copy_calls"] == row["copied_payload_bytes"] == 0 and 0 < row["canonical_bytes_authenticated"] <= delta + MiB)
        elif row["route"] == "complete-fallback":
            require("fallback", sequence, "full", row["fallback_calls"] == 1 and row["source_bytes_reconstructed"] == row["output_length"] and row["fallback_write_bytes"] == row["output_length"] and row["clone_calls"] == row["patch_calls"] == row["patch_bytes"] == row["copy_calls"] == row["copied_payload_bytes"] == 0)
        else:
            prohibited = NUMERIC - {"sequence", "size_bytes", "generation", "q_high_water", "q_terminal", "output_length", "output_mode", "verification_bytes_read", "timer_preflight_ns", "timer_qualification_ns", "timer_payload_prepare_ns", "timer_data_sync_ns", "timer_metadata_ns", "timer_metadata_sync_ns", "timer_rename_ns", "timer_directory_sync_ns", "timer_reconciliation_ns", "timer_cleanup_ns", "attributed_wall_ns", "unattributed_wall_ns", "operation_total_ns", "child_timeout_seconds", "child_exit_code", "maximum_resident_set_bytes", "temp_logical_bytes", "temp_apparent_bytes", "temp_allocated_bytes", "seed_logical_bytes", "seed_apparent_bytes", "seed_allocated_bytes", "temp_residue_count", "seed_residue_count"}
            require("direct_counters", sequence, "rejection-zero", sum(row[k] for k in prohibited) == 0)

        final_length = size + (1 if scenario == "count-change" else 0)
        final_state = "old" if scenario in {"symlink-substitution", "before-publication-fault"} else "new"
        require("exactness", sequence, "output", row["output_length"] == final_length and row["byte_exact"] and row["mode_exact"] and row["output_digest"] == row["expected_output_digest"] and row["old_or_new"] == final_state and row["temp_residue_count"] == row["seed_residue_count"] == 0)
        require("publication", sequence, "seed-cleanup", row["seed_files_created"] == row["seed_files_removed"])
        if scenario == "symlink-substitution":
            require("publication", sequence, "reconciliation", row["reconciliation_calls"] == 0 and row["reconciliation_outcome"] == "not-needed")
        elif scenario == "before-publication-fault":
            require("publication", sequence, "prepublication", row["rename_calls"] == 0 and row["temp_files_created"] == row["temp_files_removed"] and row["reconciliation_calls"] == 0 and row["reconciliation_outcome"] == "not-needed")
        else:
            require("publication", sequence, "temp-equation", row["temp_files_created"] == row["temp_files_removed"] + row["rename_calls"])
            require("publication", sequence, "durability", row["metadata_operations"] >= 1 and [row["data_sync_calls"], row["metadata_sync_calls"], row["rename_calls"], row["directory_sync_calls"]] == [1, 1, 1, 1])
            require("publication", sequence, "reconcile", [row["reconciliation_calls"], row["reconciliation_outcome"]] == ([1, "target"] if scenario == "lost-ack" else [0, "not-needed"]))

        attributed = sum(row[key] for key in TIMERS)
        require("timers", sequence, "equations", row["attributed_wall_ns"] == attributed and row["operation_total_ns"] == attributed + row["unattributed_wall_ns"] and row["operation_total_ns"] < 5_000_000_000)
        require("resources", sequence, "q-rss", row["q_terminal"] == 0 and row["q_high_water"] <= 20 * MiB and row["maximum_resident_set_bytes"] <= 20 * MiB)
        require("resources", sequence, "storage", max(row[key] for key in ("temp_logical_bytes", "temp_apparent_bytes", "temp_allocated_bytes", "seed_logical_bytes", "seed_apparent_bytes", "seed_allocated_bytes")) <= 512 * MiB)
        require("resources", sequence, "unavailable", all(row[key].startswith("Unavailable:") for key in ("physical_io_status", "cache_warmth_status", "stable_media_status")))
        require("custody", sequence, "child", row["child_exit_code"] == 0 and row["child_timeout_seconds"] == (15 if scenario == "qualified-one-byte" else 5))
        require("custody", sequence, "hashes", all(len(row[key]) == 64 for key in ("executable_sha256", "source_sha256", "methodology_set_sha256", "environment_sha256")))

    require("timers", 0, "operation-sum", sum(row.get("operation_total_ns", 5_000_000_000) for row in rows) < 20_000_000_000)
    require("cleanup", 0, "runner", cleanup.get("status") == "PASS" and cleanup.get("declared_root") == "work-v1" and cleanup.get("work_root_absent") is True and cleanup.get("broad_deletion") is False and max(cleanup.get("peak_logical_bytes", 513 * MiB), cleanup.get("peak_apparent_bytes", 513 * MiB), cleanup.get("peak_allocated_bytes", 513 * MiB)) <= 512 * MiB)

    failures = sorted(rejected)
    ledger = {
        "schema": "phase4-g3-v1-normalized-ledger-v1",
        "failures": failures,
        "gates": {gate: not any(f.startswith(gate + ":") for f in failures) for gate in GATES},
        "operation_total_ns": sum(row.get("operation_total_ns", 0) for row in rows),
        "rows": [{key: row.get(key) for key in ROW_VIEW} for row in rows],
    }
    return {"status": "PASS" if not failures else "REVISE", "normalized_ledger": ledger, "normalized_ledger_sha256": digest(ledger)}


def fixtures():
    rows = []
    for seq, scenario, size in ORDER:
        route, reason, outcome, error = EXPECTED_ROUTE[scenario]
        row = {key: 0 for key in NUMERIC}
        row.update({
            "schema": "phase4-g3-row-v1", "sequence": seq,
            "label": f"{seq:02d}-{scenario}", "scenario": scenario,
            "size_bytes": size, "route": route, "qualification_reason": reason,
            "outcome": outcome, "error": error, "generation": 7,
            "parent_root": "parent", "target_root": "target",
            "authority_bindings_checked": [] if scenario == "symlink-substitution" else AUTHORITY_VECTOR.copy(),
            "reconciliation_outcome": "target" if scenario == "lost-ack" else "not-needed",
            "output_digest": "digest", "expected_output_digest": "digest",
            "byte_exact": True, "mode_exact": True,
            "old_or_new": "old" if scenario in {"symlink-substitution", "before-publication-fault"} else "new",
            "physical_io_status": "Unavailable: fixture", "cache_warmth_status": "Unavailable: fixture",
            "stable_media_status": "Unavailable: fixture", "command": ["fixture"],
            "external_real_seconds": .01, "external_user_seconds": 0,
            "external_system_seconds": 0, "peak_memory_footprint_bytes": 1,
            "executable_sha256": "0" * 64, "source_sha256": "1" * 64,
            "methodology_set_sha256": "2" * 64, "environment_sha256": "3" * 64,
            "output_length": size + (1 if scenario == "count-change" else 0),
            "output_mode": 420, "q_high_water": 1, "maximum_resident_set_bytes": 1,
            "verification_bytes_read": 0 if scenario == "symlink-substitution" else size,
            "child_timeout_seconds": 15 if scenario == "qualified-one-byte" else 5,
        })
        for timer in TIMERS:
            row[timer] = 2
        row["attributed_wall_ns"], row["unattributed_wall_ns"], row["operation_total_ns"] = 20, 1, 21
        if scenario != "symlink-substitution":
            row.update({"authority_reads": 1, "authority_bytes_read": 32, "authority_validations": 1, "authority_validation_successes": 1, "seed_files_created": 1, "seed_files_removed": 1})
        if scenario == "invalid-authority":
            row.update({"authority_validation_successes": 0, "authority_validation_failures": 1})
        elif route in {"qualified-noop", "qualified-patch"}:
            row.update({"seed_authority_reads": 1, "seed_authority_bytes_read": 8, "permit_consumptions": 1})
        if scenario == "qualified-noop":
            row.update({"clone_calls": 1, "clone_successes": 1, "clone_source_logical_bytes": size})
        elif scenario in {"qualified-one-byte", "qualified-one-mib", "before-publication-fault", "lost-ack"}:
            amount = MiB if scenario == "qualified-one-mib" else 1
            row.update({"mapping_sql_queries": 1, "mapping_sql_rows": 1, "object_sql_queries": 1, "object_sql_rows": 1, "payload_sql_queries": 2, "payload_sql_rows": 2, "canonical_blob_reads": 1, "canonical_blob_bytes": amount, "authenticated_objects": 1, "canonical_bytes_authenticated": amount, "clone_calls": 1, "clone_successes": 1, "clone_source_logical_bytes": size, "patch_calls": 1, "patch_bytes": amount, "changed_ranges": 1, "changed_bytes": amount})
        elif route == "complete-fallback":
            amount = row["output_length"]
            row.update({"mapping_sql_queries": 1, "mapping_sql_rows": 1, "object_sql_queries": 1, "object_sql_rows": 1, "payload_sql_queries": 2, "payload_sql_rows": 2, "canonical_blob_reads": 1, "canonical_blob_bytes": amount, "authenticated_objects": 1, "canonical_bytes_authenticated": amount, "source_bytes_reconstructed": amount, "fallback_calls": 1, "fallback_write_bytes": amount})
        if scenario == "before-publication-fault":
            row.update({"temp_files_created": 1, "temp_files_removed": 1})
        elif scenario != "symlink-substitution":
            row.update({"metadata_operations": 1, "temp_files_created": 1, "data_sync_calls": 1, "metadata_sync_calls": 1, "rename_calls": 1, "directory_sync_calls": 1})
        if scenario == "lost-ack":
            row["reconciliation_calls"] = 1
        rows.append(row)
    return rows


def check_implementation():
    cleanup = {"status": "PASS", "declared_root": "work-v1", "work_root_absent": True, "broad_deletion": False, "peak_logical_bytes": 1, "peak_apparent_bytes": 1, "peak_allocated_bytes": 1}
    rows = fixtures()
    assert recompute(rows, cleanup)["status"] == "PASS"
    cases = []
    reversed_rows = copy.deepcopy(rows); reversed_rows.reverse(); cases.append((reversed_rows, cleanup))
    for index, key, replacement in ((0, "authority_bindings_checked", []), (1, "qualification_reason", "invalid-authority"), (2, "changed_bytes", 3), (3, "source_bytes_reconstructed", 0), (5, "clone_calls", 1), (7, "rename_calls", 1), (8, "reconciliation_outcome", "not-needed"), (0, "unattributed_wall_ns", 2), (0, "q_terminal", 1), (0, "mode_exact", False)):
        mutated = copy.deepcopy(rows); mutated[index][key] = replacement; cases.append((mutated, cleanup))
    cases.append((rows, dict(cleanup, broad_deletion=True)))
    assert all(recompute(candidate, cleanup_record)["status"] == "REVISE" for candidate, cleanup_record in cases)
    print(json.dumps({"status": "PASS", "mutations_rejected": len(cases)}, sort_keys=True))


def main():
    if sys.argv[1:] == ["--self-check"]:
        check_implementation()
        return
    if len(sys.argv) != 2:
        raise SystemExit("usage: recompute_g3_v1.py RESULTS | --self-check")
    root = Path(sys.argv[1])
    observations = [json.loads(line) for line in (root / "rows-v1/G3-V1-RAW.jsonl").read_text().splitlines() if line]
    cleanup = json.loads((root / "CLEANUP-v1.json").read_text())
    report = {"schema": "phase4-g3-v1-independent-recomputation-v1", **recompute(observations, cleanup)}
    print(json.dumps(report, separators=(",", ":"), sort_keys=True))


if __name__ == "__main__":
    main()
