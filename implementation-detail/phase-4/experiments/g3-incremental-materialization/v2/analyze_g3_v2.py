#!/usr/bin/env python3
"""Primary G3-v2 analysis; no code is shared with independent recomputation."""

import copy
import hashlib
import json
import sys
from pathlib import Path

MIB = 1 << 20
SCHEDULE = [
    (1, "qualified-noop", 10 * MIB), (2, "qualified-one-byte", 100 * MIB),
    (3, "qualified-one-mib", 10 * MIB), (4, "invalid-authority", MIB),
    (5, "external-mutation", MIB), (6, "symlink-substitution", MIB),
    (7, "count-change", MIB), (8, "before-publication-fault", MIB),
    (9, "lost-ack", MIB),
]
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
SOURCE_PATHS = [
    "Cargo.lock", "crates/layerfs-engine/Cargo.toml",
    "crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs",
    "crates/layerfs-engine/src/bin/phase4_g3_materialization.rs",
]
BINDINGS = [
    "store_instance", "validation_authority", "profile", "integrity_epoch",
    "generation", "receipt_transition", "parent_root", "target_root",
    "destination_identity", "open_serial", "mutation_serial",
    "publication_serial", "operation", "nonce", "seed_identity",
]
COUNTERS = (
    "authority_reads authority_bytes_read seed_authority_reads seed_authority_bytes_read "
    "authority_validations authority_validation_successes authority_validation_failures "
    "permit_consumptions mapping_sql_queries mapping_sql_rows object_sql_queries object_sql_rows "
    "payload_sql_queries payload_sql_rows canonical_blob_reads canonical_blob_bytes authenticated_objects "
    "canonical_bytes_authenticated source_bytes_reconstructed destination_bytes_read verification_bytes_read "
    "clone_calls clone_successes clone_failures clone_source_logical_bytes copy_calls copied_payload_bytes "
    "patch_calls patch_bytes fallback_calls fallback_write_bytes changed_ranges changed_bytes metadata_operations "
    "temp_files_created temp_files_removed seed_files_created seed_files_removed data_sync_calls metadata_sync_calls "
    "rename_calls directory_sync_calls reconciliation_calls reconciliation_sql_queries reconciliation_sql_rows "
    "reconciliation_blob_reads reconciliation_canonical_bytes_authenticated reconciliation_source_bytes_compared "
    "reconciliation_q_high_water q_high_water q_terminal temp_logical_bytes "
    "temp_apparent_bytes temp_allocated_bytes seed_logical_bytes seed_apparent_bytes seed_allocated_bytes "
    "output_length output_mode temp_residue_count seed_residue_count timer_preflight_ns timer_qualification_ns "
    "timer_payload_prepare_ns timer_data_sync_ns timer_metadata_ns timer_metadata_sync_ns timer_rename_ns "
    "timer_directory_sync_ns timer_reconciliation_ns timer_cleanup_ns attributed_wall_ns unattributed_wall_ns "
    "operation_total_ns child_timeout_seconds child_exit_code maximum_resident_set_bytes"
).split()
TIMERS = (
    "timer_preflight_ns timer_qualification_ns timer_payload_prepare_ns timer_data_sync_ns timer_metadata_ns "
    "timer_metadata_sync_ns timer_rename_ns timer_directory_sync_ns timer_reconciliation_ns timer_cleanup_ns"
).split()
TEXT = (
    "schema scenario route outcome qualification_reason parent_root target_root reconciliation_outcome "
    "output_digest expected_output_digest old_or_new physical_io_status cache_warmth_status stable_media_status "
    "label executable_sha256 source_set_sha256 methodology_set_sha256 environment_sha256"
).split()
SUMMARY = (
    "sequence scenario size_bytes route outcome qualification_reason error generation authority_validations "
    "authority_validation_successes authority_validation_failures permit_consumptions payload_sql_queries "
    "payload_sql_rows canonical_blob_reads canonical_bytes_authenticated source_bytes_reconstructed "
    "destination_bytes_read clone_calls patch_calls patch_bytes fallback_calls fallback_write_bytes changed_ranges "
    "changed_bytes temp_files_created temp_files_removed rename_calls directory_sync_calls reconciliation_calls "
    "reconciliation_outcome reconciliation_sql_queries reconciliation_sql_rows reconciliation_blob_reads "
    "reconciliation_canonical_bytes_authenticated reconciliation_source_bytes_compared reconciliation_q_high_water "
    "q_high_water q_terminal output_length output_mode byte_exact mode_exact old_or_new "
    "temp_residue_count seed_residue_count operation_total_ns maximum_resident_set_bytes executable_sha256 "
    "source_set_sha256 methodology_set_sha256"
).split()
GATES = "schedule shape source_custody authority route direct_counters fallback publication timers exactness resources cleanup custody".split()


def digest(value):
    return hashlib.sha256(json.dumps(value, separators=(",", ":"), sort_keys=True).encode()).hexdigest()


def generic_precedence(row):
    if row.get("qualification_reason") in {"destination-symlink", "destination-wrong-kind"}:
        zero = [name for name in COUNTERS if name.startswith(("authority_", "seed_authority_")) or name in {
            "permit_consumptions", "mapping_sql_queries", "mapping_sql_rows", "object_sql_queries", "object_sql_rows",
            "payload_sql_queries", "payload_sql_rows", "canonical_blob_reads", "canonical_blob_bytes",
            "authenticated_objects", "canonical_bytes_authenticated", "source_bytes_reconstructed", "clone_calls",
            "copy_calls", "patch_calls", "fallback_calls", "temp_files_created", "data_sync_calls", "rename_calls",
            "reconciliation_calls",
        }]
        return row.get("route") == "typed-rejection" and all(row.get(name) == 0 for name in zero)
    if row.get("qualification_reason") in {"clone-failed", "clone-unsupported", "cross-volume"}:
        return row.get("route") == "complete-fallback" and row.get("permit_consumptions") == 0 and row.get("fallback_calls") == 1
    return True


def source_custody_ok(custody):
    rows = custody.get("sources", [])
    return (
        custody.get("schema") == "phase4-g3-v2-source-custody-v1"
        and custody.get("status") == "PASS"
        and len(custody.get("source_set_sha256", "")) == 64
        and [row.get("path") for row in rows] == SOURCE_PATHS
        and all(row.get("source_mode") == "0644" and row.get("copy_mode") == "0400" and row.get("sha256") == row.get("copy_sha256") and row.get("size_bytes", 0) > 0 for row in rows)
        and custody.get("source_set_sha256") == digest([{key: row[key] for key in ("path", "sha256", "size_bytes", "source_mode")} for row in rows])
    )


def validate(rows, cleanup, custody):
    failures = []
    reject = lambda gate, seq, name: failures.append(f"{gate}:{seq}:{name}")
    if [(r.get("sequence"), r.get("scenario"), r.get("size_bytes")) for r in rows] != SCHEDULE:
        reject("schedule", 0, "exact-order")
    if not source_custody_ok(custody):
        reject("source_custody", 0, "source-set")
    source_set = custody.get("source_set_sha256")
    required = set(COUNTERS + TEXT + ["sequence", "size_bytes", "generation", "error", "authority_bindings_checked", "byte_exact", "mode_exact", "command", "external_real_seconds", "external_user_seconds", "external_system_seconds"])
    for (sequence, scenario, size), row in zip(SCHEDULE, rows):
        if not required <= row.keys():
            reject("shape", sequence, "missing-fields")
            continue
        if row["schema"] != "phase4-g3-row-v1" or any(type(row[name]) is not int or row[name] < 0 for name in COUNTERS) or any(type(row[name]) is not str or not row[name] for name in TEXT) or type(row["byte_exact"]) is not bool or type(row["mode_exact"]) is not bool:
            reject("shape", sequence, "types")
        if (row["route"], row["qualification_reason"], row["outcome"], row["error"]) != ROUTES[scenario] or not generic_precedence(row):
            reject("route", sequence, "decision-precedence")
        if row["source_set_sha256"] != source_set or len(row["source_set_sha256"]) != 64:
            reject("source_custody", sequence, "row-source-set")
        if row["authority_bindings_checked"] != ([] if scenario == "symlink-substitution" else BINDINGS) or row["authority_validations"] != row["authority_validation_successes"] + row["authority_validation_failures"] or row["authority_reads"] < row["authority_validations"]:
            reject("authority", sequence, "binding-equation")
        if scenario == "symlink-substitution":
            if any(row[name] for name in ("authority_reads", "authority_bytes_read", "seed_authority_reads", "seed_authority_bytes_read", "authority_validations", "authority_validation_successes", "authority_validation_failures", "permit_consumptions")):
                reject("authority", sequence, "preflight-work")
        elif scenario == "invalid-authority":
            if not (row["authority_validations"] >= 1 and row["authority_validation_successes"] == 0 and row["authority_validation_failures"] >= 1 and row["permit_consumptions"] == 0):
                reject("authority", sequence, "invalid-gate")
        elif row["route"].startswith("qualified"):
            if not (row["authority_validations"] >= 1 and row["authority_validation_successes"] == row["authority_validations"] and row["authority_validation_failures"] == 0 and row["permit_consumptions"] == 1 and row["seed_authority_reads"] >= 1 and row["seed_authority_bytes_read"] >= 1):
                reject("authority", sequence, "qualified-gate")
        elif not (row["authority_validation_successes"] >= 1 and row["authority_validation_failures"] == 0 and row["permit_consumptions"] == 0):
            reject("authority", sequence, "fallback-gate")
        if row["payload_sql_queries"] != row["mapping_sql_queries"] + row["object_sql_queries"] or row["payload_sql_rows"] != row["mapping_sql_rows"] + row["object_sql_rows"] or row["canonical_blob_bytes"] != row["canonical_bytes_authenticated"]:
            reject("direct_counters", sequence, "equations")
        if scenario == "qualified-noop":
            names = "mapping_sql_queries mapping_sql_rows object_sql_queries object_sql_rows payload_sql_queries payload_sql_rows canonical_blob_reads canonical_blob_bytes authenticated_objects canonical_bytes_authenticated source_bytes_reconstructed copy_calls copied_payload_bytes patch_calls patch_bytes fallback_calls fallback_write_bytes changed_ranges changed_bytes".split()
            if any(row[name] for name in names) or (row["clone_calls"], row["clone_successes"], row["clone_failures"], row["clone_source_logical_bytes"]) != (1, 1, 0, size):
                reject("direct_counters", sequence, "noop")
        elif scenario in {"qualified-one-byte", "qualified-one-mib", "before-publication-fault", "lost-ack"}:
            amount = MIB if scenario == "qualified-one-mib" else 1
            if not (row["changed_ranges"] == 1 and row["changed_bytes"] == row["patch_bytes"] == amount and row["patch_calls"] >= 1 and (row["clone_calls"], row["clone_successes"], row["clone_failures"], row["clone_source_logical_bytes"]) == (1, 1, 0, size) and row["fallback_calls"] == row["source_bytes_reconstructed"] == row["copy_calls"] == row["copied_payload_bytes"] == 0 and 0 < row["canonical_bytes_authenticated"] <= amount + MIB):
                reject("direct_counters", sequence, "patch")
        elif row["route"] == "complete-fallback":
            if not (row["permit_consumptions"] == 0 and row["fallback_calls"] == 1 and row["source_bytes_reconstructed"] == row["fallback_write_bytes"] == row["output_length"] and row["clone_calls"] == row["copy_calls"] == row["patch_calls"] == row["patch_bytes"] == row["copied_payload_bytes"] == 0):
                reject("fallback", sequence, "complete")
        expected_length = size + (scenario == "count-change")
        expected_state = "old" if scenario in {"symlink-substitution", "before-publication-fault"} else "new"
        state_ok = (row["reconciliation_outcome"], row["old_or_new"]) in {("target", "new"), ("prior", "old")} if scenario == "lost-ack" else row["old_or_new"] == expected_state
        if row["output_length"] != expected_length or not row["byte_exact"] or not row["mode_exact"] or row["output_digest"] != row["expected_output_digest"] or not state_ok or row["temp_residue_count"] or row["seed_residue_count"]:
            reject("exactness", sequence, "output")
        if row["seed_files_created"] != row["seed_files_removed"]:
            reject("publication", sequence, "seed-cleanup")
        reconciliation_fields = ("reconciliation_sql_queries", "reconciliation_sql_rows", "reconciliation_blob_reads", "reconciliation_canonical_bytes_authenticated", "reconciliation_source_bytes_compared", "reconciliation_q_high_water")
        if scenario == "lost-ack":
            comparison_exact = row["destination_bytes_read"] == row["output_length"] and row["reconciliation_source_bytes_compared"] == row["output_length"] if row["reconciliation_outcome"] == "target" else row["destination_bytes_read"] >= row["output_length"] and row["reconciliation_source_bytes_compared"] >= row["output_length"]
            if any(row[name] <= 0 for name in reconciliation_fields) or not comparison_exact or row["reconciliation_canonical_bytes_authenticated"] < row["output_length"] or row["q_high_water"] < row["reconciliation_q_high_water"]:
                reject("publication", sequence, "reconciliation-work")
        elif any(row[name] for name in reconciliation_fields):
            reject("publication", sequence, "unexpected-reconciliation-work")
        if scenario == "symlink-substitution":
            if row["reconciliation_calls"] or row["reconciliation_outcome"] != "not-needed": reject("publication", sequence, "preflight")
        elif scenario == "before-publication-fault":
            if row["rename_calls"] or row["temp_files_created"] != row["temp_files_removed"] or row["reconciliation_calls"] or row["reconciliation_outcome"] != "not-needed": reject("publication", sequence, "before")
        else:
            if row["temp_files_created"] != row["temp_files_removed"] + row["rename_calls"] or row["metadata_operations"] < 1 or (row["data_sync_calls"], row["metadata_sync_calls"], row["rename_calls"], row["directory_sync_calls"]) != (1, 1, 1, 1): reject("publication", sequence, "durability")
            if scenario == "lost-ack":
                if row["reconciliation_calls"] != 1 or not state_ok: reject("publication", sequence, "lost-ack")
            elif row["reconciliation_calls"] or row["reconciliation_outcome"] != "not-needed": reject("publication", sequence, "unexpected-reconciliation")
        if row["attributed_wall_ns"] != sum(row[name] for name in TIMERS) or row["operation_total_ns"] != row["attributed_wall_ns"] + row["unattributed_wall_ns"] or row["operation_total_ns"] >= 5_000_000_000:
            reject("timers", sequence, "equation-ceiling")
        if row["q_terminal"] or row["q_high_water"] > 20 * MIB or row["reconciliation_q_high_water"] > 20 * MIB or row["maximum_resident_set_bytes"] > 20 * MIB or max(row[name] for name in "temp_logical_bytes temp_apparent_bytes temp_allocated_bytes seed_logical_bytes seed_apparent_bytes seed_allocated_bytes".split()) > 512 * MIB or any(not row[name].startswith("Unavailable:") for name in ("physical_io_status", "cache_warmth_status", "stable_media_status")):
            reject("resources", sequence, "bounds-status")
        if row["child_exit_code"] or row["child_timeout_seconds"] != (15 if scenario == "qualified-one-byte" else 5) or any(len(row[name]) != 64 for name in ("executable_sha256", "source_set_sha256", "methodology_set_sha256", "environment_sha256")):
            reject("custody", sequence, "child-hash")
    if sum(row.get("operation_total_ns", 5_000_000_000) for row in rows) >= 20_000_000_000:
        reject("timers", 0, "sum")
    if not (cleanup.get("status") == "PASS" and cleanup.get("declared_root") == "work-v2" and cleanup.get("work_root_absent") is True and cleanup.get("broad_deletion") is False and max(cleanup.get("peak_logical_bytes", 513 * MIB), cleanup.get("peak_apparent_bytes", 513 * MIB), cleanup.get("peak_allocated_bytes", 513 * MIB)) <= 512 * MIB):
        reject("cleanup", 0, "exact-root")
    failures = sorted(set(failures))
    ledger = {
        "schema": "phase4-g3-v2-normalized-ledger-v1", "failures": failures,
        "gates": {gate: not any(item.startswith(gate + ":") for item in failures) for gate in GATES},
        "source_set_sha256": source_set,
        "operation_total_ns": sum(row.get("operation_total_ns", 0) for row in rows),
        "rows": [{name: row.get(name) for name in SUMMARY} for row in rows],
    }
    return {"status": "PASS" if not failures else "REVISE", "normalized_ledger": ledger, "normalized_ledger_sha256": digest(ledger)}


def fixtures():
    source_rows = [{"path": path, "sha256": f"{index + 1:x}" * 64, "size_bytes": index + 1, "source_mode": "0644", "copy_sha256": f"{index + 1:x}" * 64, "copy_mode": "0400"} for index, path in enumerate(SOURCE_PATHS)]
    source_set = digest([{key: row[key] for key in ("path", "sha256", "size_bytes", "source_mode")} for row in source_rows])
    custody = {"schema": "phase4-g3-v2-source-custody-v1", "status": "PASS", "source_set_sha256": source_set, "sources": source_rows}
    rows = []
    for sequence, scenario, size in SCHEDULE:
        route, reason, outcome, error = ROUTES[scenario]
        row = {name: 0 for name in COUNTERS}
        row.update({"schema": "phase4-g3-row-v1", "sequence": sequence, "label": f"{sequence:02d}-{scenario}", "scenario": scenario, "size_bytes": size, "route": route, "qualification_reason": reason, "outcome": outcome, "error": error, "generation": 1, "parent_root": "p", "target_root": "t", "authority_bindings_checked": [] if scenario == "symlink-substitution" else BINDINGS.copy(), "reconciliation_outcome": "target" if scenario == "lost-ack" else "not-needed", "output_digest": "d", "expected_output_digest": "d", "byte_exact": True, "mode_exact": True, "old_or_new": "old" if scenario in {"symlink-substitution", "before-publication-fault"} else "new", "physical_io_status": "Unavailable: test", "cache_warmth_status": "Unavailable: test", "stable_media_status": "Unavailable: test", "command": ["test"], "external_real_seconds": .1, "external_user_seconds": 0, "external_system_seconds": 0, "peak_memory_footprint_bytes": 1, "executable_sha256": "a" * 64, "source_set_sha256": source_set, "methodology_set_sha256": "b" * 64, "environment_sha256": "c" * 64, "output_length": size + (scenario == "count-change"), "output_mode": 420, "q_high_water": 1, "maximum_resident_set_bytes": 1, "verification_bytes_read": size, "child_timeout_seconds": 15 if scenario == "qualified-one-byte" else 5})
        for name in TIMERS: row[name] = 1
        row.update({"attributed_wall_ns": 10, "unattributed_wall_ns": 1, "operation_total_ns": 11})
        if scenario != "symlink-substitution": row.update({"authority_reads": 1, "authority_bytes_read": 32, "authority_validations": 1, "authority_validation_successes": 1, "seed_files_created": 1, "seed_files_removed": 1})
        if scenario == "invalid-authority": row.update({"authority_validation_successes": 0, "authority_validation_failures": 1})
        elif route.startswith("qualified"): row.update({"seed_authority_reads": 1, "seed_authority_bytes_read": 8, "permit_consumptions": 1})
        if scenario == "qualified-noop": row.update({"clone_calls": 1, "clone_successes": 1, "clone_source_logical_bytes": size})
        elif scenario in {"qualified-one-byte", "qualified-one-mib", "before-publication-fault", "lost-ack"}:
            amount = MIB if scenario == "qualified-one-mib" else 1
            row.update({"mapping_sql_queries": 1, "mapping_sql_rows": 1, "object_sql_queries": 1, "object_sql_rows": 1, "payload_sql_queries": 2, "payload_sql_rows": 2, "canonical_blob_reads": 1, "canonical_blob_bytes": amount, "authenticated_objects": 1, "canonical_bytes_authenticated": amount, "clone_calls": 1, "clone_successes": 1, "clone_source_logical_bytes": size, "patch_calls": 1, "patch_bytes": amount, "changed_ranges": 1, "changed_bytes": amount})
        elif route == "complete-fallback":
            amount = row["output_length"]
            row.update({"mapping_sql_queries": 1, "mapping_sql_rows": 1, "object_sql_queries": 1, "object_sql_rows": 1, "payload_sql_queries": 2, "payload_sql_rows": 2, "canonical_blob_reads": 1, "canonical_blob_bytes": amount, "authenticated_objects": 1, "canonical_bytes_authenticated": amount, "source_bytes_reconstructed": amount, "fallback_calls": 1, "fallback_write_bytes": amount})
        if scenario == "before-publication-fault": row.update({"temp_files_created": 1, "temp_files_removed": 1})
        elif scenario != "symlink-substitution": row.update({"metadata_operations": 1, "temp_files_created": 1, "data_sync_calls": 1, "metadata_sync_calls": 1, "rename_calls": 1, "directory_sync_calls": 1})
        if scenario == "lost-ack":
            row.update({"reconciliation_calls": 1, "reconciliation_sql_queries": 1, "reconciliation_sql_rows": 1, "reconciliation_blob_reads": 1, "reconciliation_canonical_bytes_authenticated": size, "reconciliation_source_bytes_compared": size, "reconciliation_q_high_water": 1, "destination_bytes_read": size})
        rows.append(row)
    cleanup = {"status": "PASS", "declared_root": "work-v2", "work_root_absent": True, "broad_deletion": False, "peak_logical_bytes": 1, "peak_apparent_bytes": 1, "peak_allocated_bytes": 1}
    return rows, cleanup, custody


def self_check():
    rows, cleanup, custody = fixtures()
    assert validate(rows, cleanup, custody)["status"] == "PASS"
    prior = copy.deepcopy(rows); prior[8].update({"reconciliation_outcome": "prior", "old_or_new": "old"}); assert validate(prior, cleanup, custody)["status"] == "PASS"
    mutations = []
    for index, key, value in ((0, "source_set_sha256", "0" * 64), (8, "old_or_new", "old"), (8, "reconciliation_source_bytes_compared", 0), (5, "qualification_reason", "count-change"), (1, "patch_bytes", 2), (3, "fallback_calls", 0), (0, "operation_total_ns", 99), (0, "q_terminal", 1), (0, "byte_exact", False)):
        changed = copy.deepcopy(rows); changed[index][key] = value; mutations.append((changed, cleanup, custody))
    swapped = copy.deepcopy(rows); swapped[0], swapped[1] = swapped[1], swapped[0]; mutations.append((swapped, cleanup, custody))
    mutations.append((rows, dict(cleanup, broad_deletion=True), custody))
    bad_custody = copy.deepcopy(custody); bad_custody["sources"][0]["copy_sha256"] = "f" * 64; mutations.append((rows, cleanup, bad_custody))
    assert all(validate(*case)["status"] == "REVISE" for case in mutations)
    clone = copy.deepcopy(rows[3]); clone.update({"qualification_reason": "clone-failed", "route": "complete-fallback", "permit_consumptions": 0, "fallback_calls": 1})
    assert generic_precedence(clone); clone["permit_consumptions"] = 1; assert not generic_precedence(clone)
    wrong_kind = copy.deepcopy(rows[5]); wrong_kind.update({"qualification_reason": "destination-wrong-kind", "error": "NativeDestinationWrongKind"}); assert generic_precedence(wrong_kind); wrong_kind["authority_reads"] = 1; assert not generic_precedence(wrong_kind)
    print(json.dumps({"status": "PASS", "mutations_rejected": len(mutations) + 2}, sort_keys=True))


def main():
    if sys.argv[1:] == ["--self-check"]: self_check(); return
    if len(sys.argv) != 2: raise SystemExit("usage: analyze_g3_v2.py RESULTS | --self-check")
    root = Path(sys.argv[1])
    rows = [json.loads(line) for line in (root / "rows-v2/G3-V2-RAW.jsonl").read_text().splitlines() if line]
    report = {"schema": "phase4-g3-v2-primary-analysis-v1", **validate(rows, json.loads((root / "CLEANUP-v2.json").read_text()), json.loads((root / "SOURCE-CUSTODY-v2.json").read_text()))}
    print(json.dumps(report, separators=(",", ":"), sort_keys=True))


if __name__ == "__main__": main()
