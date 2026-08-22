#!/usr/bin/env python3
"""Independent G3-v6 recomputation using a predicate table and fresh parser."""

import copy
import hashlib
import json
import sys
from pathlib import Path, PurePosixPath

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[4]
MB = 1048576
PLAN = [
    (1, "qualified-noop", 10 * MB), (2, "qualified-one-byte", 100 * MB),
    (3, "qualified-one-mib", 10 * MB), (4, "invalid-authority", MB),
    (5, "external-mutation", MB), (6, "symlink-substitution", MB),
    (7, "count-change", MB), (8, "before-publication-fault", MB), (9, "lost-ack", MB),
]
DECISIONS = {
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
FILES = [
    "Cargo.lock", "crates/layerfs-engine/Cargo.toml",
    "crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs",
    "crates/layerfs-engine/src/bin/phase4_g3_materialization.rs",
]
BOUND = [
    "store_instance", "validation_authority", "profile", "integrity_epoch", "generation",
    "receipt_transition", "parent_root", "target_root", "destination_identity", "open_serial",
    "mutation_serial", "publication_serial", "operation", "nonce", "seed_identity",
]
NUMBER_KEYS = set((
    "sequence size_bytes generation authority_reads authority_bytes_read seed_authority_reads seed_authority_bytes_read "
    "authority_validations authority_validation_successes authority_validation_failures permit_consumptions "
    "mapping_sql_queries mapping_sql_rows object_sql_queries object_sql_rows payload_sql_queries payload_sql_rows "
    "canonical_blob_reads canonical_blob_bytes authenticated_objects canonical_bytes_authenticated "
    "source_bytes_reconstructed destination_bytes_read verification_bytes_read clone_calls clone_successes "
    "clone_failures clone_source_logical_bytes copy_calls copied_payload_bytes patch_calls patch_bytes fallback_calls "
    "fallback_write_bytes changed_ranges changed_bytes metadata_operations temp_files_created temp_files_removed "
    "seed_files_created seed_files_removed data_sync_calls metadata_sync_calls rename_calls directory_sync_calls "
    "reconciliation_calls reconciliation_sql_queries reconciliation_sql_rows reconciliation_blob_reads "
    "reconciliation_canonical_bytes_authenticated reconciliation_source_bytes_compared reconciliation_q_high_water "
    "q_high_water q_terminal temp_logical_bytes temp_apparent_bytes temp_allocated_bytes "
    "seed_logical_bytes seed_apparent_bytes seed_allocated_bytes output_length output_mode temp_residue_count "
    "seed_residue_count timer_preflight_ns timer_qualification_ns timer_payload_prepare_ns timer_data_sync_ns "
    "timer_metadata_ns timer_metadata_sync_ns timer_rename_ns timer_directory_sync_ns timer_reconciliation_ns "
    "timer_cleanup_ns attributed_wall_ns unattributed_wall_ns operation_total_ns child_timeout_seconds "
    "child_exit_code maximum_resident_set_bytes"
).split())
TIME_KEYS = "timer_preflight_ns timer_qualification_ns timer_payload_prepare_ns timer_data_sync_ns timer_metadata_ns timer_metadata_sync_ns timer_rename_ns timer_directory_sync_ns timer_reconciliation_ns timer_cleanup_ns".split()
STRING_KEYS = set("schema scenario route outcome qualification_reason parent_root target_root reconciliation_outcome output_digest expected_output_digest old_or_new physical_io_status cache_warmth_status stable_media_status label executable_sha256 source_set_sha256 methodology_set_sha256 environment_sha256".split())
VIEW = "sequence scenario size_bytes route outcome qualification_reason error generation authority_validations authority_validation_successes authority_validation_failures permit_consumptions payload_sql_queries payload_sql_rows canonical_blob_reads canonical_bytes_authenticated source_bytes_reconstructed destination_bytes_read clone_calls patch_calls patch_bytes fallback_calls fallback_write_bytes changed_ranges changed_bytes temp_files_created temp_files_removed rename_calls directory_sync_calls reconciliation_calls reconciliation_outcome reconciliation_sql_queries reconciliation_sql_rows reconciliation_blob_reads reconciliation_canonical_bytes_authenticated reconciliation_source_bytes_compared reconciliation_q_high_water q_high_water q_terminal output_length output_mode byte_exact mode_exact old_or_new temp_residue_count seed_residue_count operation_total_ns maximum_resident_set_bytes executable_sha256 source_set_sha256 methodology_set_sha256".split()
GATE_ORDER = "schedule shape source_custody authority route direct_counters fallback publication timers exactness resources cleanup custody".split()


def canonical_hash(data):
    return hashlib.sha256(json.dumps(data, sort_keys=True, separators=(",", ":")).encode()).hexdigest()


def hash_file(path):
    value = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1048576), b""):
            value.update(block)
    return value.hexdigest()


def permissions(path):
    return f"{path.stat().st_mode & 0o7777:04o}"


def custody_valid(custody, results=None):
    sources = custody.get("sources", [])
    identity = [{field: source.get(field) for field in ("path", "sha256", "size_bytes", "source_mode")} for source in sources]
    predicates = [
        custody.get("schema") == "phase4-g3-v6-source-custody-v1",
        custody.get("status") == "PASS",
        [source.get("path") for source in sources] == FILES,
        len(custody.get("source_set_sha256", "")) == 64,
        custody.get("source_set_sha256") == canonical_hash(identity),
        all(source.get("source_mode") == "0644" for source in sources),
        all(source.get("copy_mode") == "0400" for source in sources),
        all(source.get("sha256") == source.get("copy_sha256") and source.get("copy_size_bytes") == source.get("size_bytes") and source.get("size_bytes", 0) > 0 for source in sources),
    ]
    if not all(predicates):
        return False
    result_root = (results or Path("/__g3_v6_independent_synthetic__")).resolve()
    copy_root = (result_root / "source-custody-v6").resolve()
    for source in sources:
        relative = source.get("copy_path")
        parts = PurePosixPath(relative) if isinstance(relative, str) else None
        if parts is None or parts.is_absolute() or any(part in ("", ".", "..") for part in parts.parts) or relative != f"source-custody-v6/{source['path']}":
            return False
        copied = result_root.joinpath(*parts.parts)
        original = REPO / source["path"]
        if copy_root not in copied.resolve().parents or copied.resolve() == original.resolve():
            return False
        if results is not None and (copied.is_symlink() or not copied.is_file() or (copied.stat().st_dev, copied.stat().st_ino) == (original.stat().st_dev, original.stat().st_ino) or permissions(copied) != "0400" or copied.stat().st_size != source["copy_size_bytes"] or hash_file(copied) != source["copy_sha256"] or permissions(original) != "0644" or original.stat().st_size != source["size_bytes"] or hash_file(original) != source["sha256"]):
            return False
    return True


def precedence_valid(row):
    reason = row.get("qualification_reason")
    if reason in ("destination-symlink", "destination-wrong-kind"):
        names = "authority_reads authority_bytes_read seed_authority_reads seed_authority_bytes_read authority_validations authority_validation_successes authority_validation_failures permit_consumptions mapping_sql_queries mapping_sql_rows object_sql_queries object_sql_rows payload_sql_queries payload_sql_rows canonical_blob_reads canonical_blob_bytes authenticated_objects canonical_bytes_authenticated source_bytes_reconstructed clone_calls copy_calls patch_calls fallback_calls temp_files_created data_sync_calls rename_calls reconciliation_calls".split()
        return row.get("route") == "typed-rejection" and all(row.get(name) == 0 for name in names)
    if reason in ("clone-failed", "clone-unsupported", "cross-volume"):
        return row.get("route") == "complete-fallback" and row.get("permit_consumptions") == 0 and row.get("fallback_calls") == 1
    return True


def audit(rows, cleanup, custody, results=None, cleanup_rows=None):
    failures = set()

    def check(gate, sequence, label, predicate):
        if not predicate:
            failures.add(f"{gate}:{sequence}:{label}")

    check("schedule", 0, "exact-order", [(row.get("sequence"), row.get("scenario"), row.get("size_bytes")) for row in rows] == PLAN)
    check("source_custody", 0, "source-set", custody_valid(custody, results))
    source_set = custody.get("source_set_sha256")
    required = NUMBER_KEYS | STRING_KEYS | {"error", "authority_bindings_checked", "byte_exact", "mode_exact", "command", "external_real_seconds", "external_user_seconds", "external_system_seconds"}
    for (sequence, scenario, size), row in zip(PLAN, rows):
        complete = required <= row.keys()
        check("shape", sequence, "required-fields", complete)
        if not complete:
            continue
        checks = [
            ("shape", "types", row["schema"] == "phase4-g3-row-v1" and all(type(row[key]) is int and row[key] >= 0 for key in NUMBER_KEYS) and all(type(row[key]) is str and bool(row[key]) for key in STRING_KEYS) and type(row["byte_exact"]) is bool and type(row["mode_exact"]) is bool),
            ("route", "decision-precedence", [row["route"], row["qualification_reason"], row["outcome"], row["error"]] == DECISIONS[scenario] and precedence_valid(row)),
            ("source_custody", "row-source-set", row["source_set_sha256"] == source_set and len(row["source_set_sha256"]) == 64),
            ("authority", "binding-vector", row["authority_bindings_checked"] == ([] if scenario == "symlink-substitution" else BOUND)),
            ("authority", "partition", row["authority_validations"] == row["authority_validation_successes"] + row["authority_validation_failures"] and row["authority_reads"] >= row["authority_validations"]),
            ("direct_counters", "sql", row["payload_sql_queries"] == row["mapping_sql_queries"] + row["object_sql_queries"] and row["payload_sql_rows"] == row["mapping_sql_rows"] + row["object_sql_rows"]),
            ("direct_counters", "blob", row["canonical_blob_bytes"] == row["canonical_bytes_authenticated"]),
            ("timers", "equations", row["attributed_wall_ns"] == sum(row[key] for key in TIME_KEYS) and row["operation_total_ns"] == row["attributed_wall_ns"] + row["unattributed_wall_ns"] and row["operation_total_ns"] < 5_000_000_000),
            ("resources", "q-rss", row["q_terminal"] == 0 and row["q_high_water"] <= 20 * MB and row["reconciliation_q_high_water"] <= 20 * MB and row["maximum_resident_set_bytes"] <= 20 * MB),
            ("resources", "storage-status", max(row[key] for key in "temp_logical_bytes temp_apparent_bytes temp_allocated_bytes seed_logical_bytes seed_apparent_bytes seed_allocated_bytes".split()) <= 512 * MB and all(row[key].startswith("Unavailable:") for key in ("physical_io_status", "cache_warmth_status", "stable_media_status"))),
            ("custody", "child-hashes", row["child_exit_code"] == 0 and row["child_timeout_seconds"] == (15 if scenario == "qualified-one-byte" else 5) and all(len(row[key]) == 64 for key in ("executable_sha256", "source_set_sha256", "methodology_set_sha256", "environment_sha256"))),
        ]
        for gate, label, predicate in checks:
            check(gate, sequence, label, predicate)
        if scenario == "symlink-substitution":
            check("authority", sequence, "preflight", sum(row[key] for key in "authority_reads authority_bytes_read seed_authority_reads seed_authority_bytes_read authority_validations authority_validation_successes authority_validation_failures permit_consumptions".split()) == 0)
        elif scenario == "invalid-authority":
            check("authority", sequence, "invalid", row["authority_validations"] >= 1 and row["authority_validation_successes"] == 0 and row["authority_validation_failures"] >= 1 and row["permit_consumptions"] == 0)
        elif row["route"].startswith("qualified"):
            check("authority", sequence, "qualified", row["authority_validations"] >= 1 and row["authority_validation_successes"] == row["authority_validations"] and row["authority_validation_failures"] == 0 and row["permit_consumptions"] == 1 and min(row["seed_authority_reads"], row["seed_authority_bytes_read"]) >= 1)
        else:
            check("authority", sequence, "fallback", row["authority_validation_successes"] >= 1 and row["authority_validation_failures"] == 0 and row["permit_consumptions"] == 0)
        if scenario == "qualified-noop":
            zero = "mapping_sql_queries mapping_sql_rows object_sql_queries object_sql_rows payload_sql_queries payload_sql_rows canonical_blob_reads canonical_blob_bytes authenticated_objects canonical_bytes_authenticated source_bytes_reconstructed copy_calls copied_payload_bytes patch_calls patch_bytes fallback_calls fallback_write_bytes changed_ranges changed_bytes".split()
            check("direct_counters", sequence, "noop", sum(row[key] for key in zero) == 0 and [row["clone_calls"], row["clone_successes"], row["clone_failures"], row["clone_source_logical_bytes"]] == [1, 1, 0, size])
        elif scenario in ("qualified-one-byte", "qualified-one-mib", "before-publication-fault", "lost-ack"):
            delta = MB if scenario == "qualified-one-mib" else 1
            check("direct_counters", sequence, "patch", row["changed_ranges"] == 1 and row["changed_bytes"] == row["patch_bytes"] == delta and row["patch_calls"] >= 1 and [row["clone_calls"], row["clone_successes"], row["clone_failures"], row["clone_source_logical_bytes"]] == [1, 1, 0, size] and row["fallback_calls"] == row["source_bytes_reconstructed"] == row["copy_calls"] == row["copied_payload_bytes"] == 0 and 0 < row["canonical_bytes_authenticated"] <= delta + MB)
        elif row["route"] == "complete-fallback":
            check("fallback", sequence, "complete", row["permit_consumptions"] == 0 and row["fallback_calls"] == 1 and row["source_bytes_reconstructed"] == row["fallback_write_bytes"] == row["output_length"] and row["clone_calls"] == row["copy_calls"] == row["patch_calls"] == row["patch_bytes"] == row["copied_payload_bytes"] == 0)
        length = size + (1 if scenario == "count-change" else 0)
        lost_pair = (row["reconciliation_outcome"], row["old_or_new"]) in {("target", "new"), ("prior", "old")}
        state = lost_pair if scenario == "lost-ack" else row["old_or_new"] == ("old" if scenario in ("symlink-substitution", "before-publication-fault") else "new")
        check("exactness", sequence, "output", row["output_length"] == length and row["byte_exact"] and row["mode_exact"] and row["output_digest"] == row["expected_output_digest"] and state and row["temp_residue_count"] == row["seed_residue_count"] == 0)
        check("publication", sequence, "seed-cleanup", row["seed_files_created"] == row["seed_files_removed"])
        reconciliation_fields = "reconciliation_sql_queries reconciliation_sql_rows reconciliation_blob_reads reconciliation_canonical_bytes_authenticated reconciliation_source_bytes_compared reconciliation_q_high_water".split()
        if scenario == "lost-ack":
            compared = row["destination_bytes_read"] == row["output_length"] and row["reconciliation_source_bytes_compared"] == row["output_length"] if row["reconciliation_outcome"] == "target" else row["destination_bytes_read"] >= row["output_length"] and row["reconciliation_source_bytes_compared"] >= row["output_length"]
            check("publication", sequence, "reconciliation-work", all(row[key] > 0 for key in reconciliation_fields) and compared and row["reconciliation_canonical_bytes_authenticated"] >= row["output_length"] and row["q_high_water"] >= row["reconciliation_q_high_water"])
        else:
            check("publication", sequence, "unexpected-reconciliation-work", all(row[key] == 0 for key in reconciliation_fields))
        if scenario == "symlink-substitution":
            check("publication", sequence, "preflight", row["reconciliation_calls"] == 0 and row["reconciliation_outcome"] == "not-needed")
        elif scenario == "before-publication-fault":
            check("publication", sequence, "before", row["rename_calls"] == 0 and row["temp_files_created"] == row["temp_files_removed"] and row["reconciliation_calls"] == 0 and row["reconciliation_outcome"] == "not-needed")
        else:
            check("publication", sequence, "durability", row["temp_files_created"] == row["temp_files_removed"] + row["rename_calls"] and row["metadata_operations"] >= 1 and [row["data_sync_calls"], row["metadata_sync_calls"], row["rename_calls"], row["directory_sync_calls"]] == [1, 1, 1, 1])
            check("publication", sequence, "reconciliation", row["reconciliation_calls"] == (1 if scenario == "lost-ack" else 0) and (lost_pair if scenario == "lost-ack" else row["reconciliation_outcome"] == "not-needed"))
    check("timers", 0, "sum", sum(row.get("operation_total_ns", 5_000_000_000) for row in rows) < 20_000_000_000)
    cleanup_rows = cleanup_rows or []
    order = [(sequence, f"{sequence:02d}-{scenario}") for sequence, scenario, _ in PLAN]
    labels = [label for _, label in order]; expected_pairs = [(sequence, label, event) for sequence, label in order for event in ("PREPARE", "COMPLETE")]
    check("cleanup", 0, "prepare-complete-order", [(record.get("sequence"), record.get("label"), record.get("event")) for record in cleanup_rows] == expected_pairs)
    dimensions = ("logical_bytes", "apparent_bytes", "allocated_bytes"); prepares, completes = cleanup_rows[::2], cleanup_rows[1::2]
    method = "descriptor-relative-openat-fstatat-unlinkat-rmdir-no-follow-exact-inventory-v1"
    for sequence, (label, prepare, complete, raw) in enumerate(zip(labels, prepares, completes, rows), 1):
        entries = prepare.get("inventory", []); paths = [entry.get("path") for entry in entries]
        valid_inventory = isinstance(entries, list) and entries == sorted(entries, key=lambda entry: entry.get("path", "")) and paths == sorted(set(paths)) and all(isinstance(path, str) and path and not PurePosixPath(path).is_absolute() and all(part not in ("", ".", "..") for part in PurePosixPath(path).parts) and entry.get("kind") in ("regular", "directory", "symlink", "other") and all(type(entry.get(key)) is int for key in ("device", "inode", "mode", "nlink", "size_bytes", "mtime_ns", "ctime_ns", "allocated_bytes")) for path, entry in zip(paths, entries))
        row_identity = prepare.get("row_identity", {}); valid_row_identity = row_identity.get("kind") == "directory" and all(type(row_identity.get(key)) is int for key in ("device", "inode", "mode", "nlink", "size_bytes", "mtime_ns", "ctime_ns", "allocated_bytes"))
        exactness = {key: raw.get(key) for key in ("byte_exact", "mode_exact", "temp_residue_count", "seed_residue_count", "old_or_new", "output_digest", "expected_output_digest")}
        check("cleanup", sequence, "prepare-fields", prepare.get("schema") == "phase4-g3-v6-row-cleanup-v1" and prepare.get("row_root") == f"work-v6/{label}" and valid_inventory and valid_row_identity and prepare.get("inventory_count") == len(entries) and prepare.get("inventory_sha256") == canonical_hash(entries) and prepare.get("deletion_method") == method and prepare.get("anchored_work_dirfd") is True and prepare.get("anchored_row_dirfd") is True and prepare.get("row_fd_retained_prepare_through_delete") is True and prepare.get("enumeration_followed_symlinks") is False and prepare.get("private_namespace_process_custody") is True and prepare.get("candidate_exactness") == exactness)
        before, work_before = prepare.get("pre_delete_row", {}), prepare.get("pre_delete_work", {})
        check("cleanup", sequence, "prepare-usage", all(type(before.get(key)) is int and 0 <= before[key] <= 512 * MB and work_before.get(key) == before[key] for key in dimensions))
        check("cleanup", sequence, "complete-fields", complete.get("schema") == "phase4-g3-v6-row-cleanup-v1" and complete.get("row_root") == prepare.get("row_root") and complete.get("prepare_sha256") == canonical_hash(prepare) and complete.get("inventory_count") == len(entries) and complete.get("inventory_sha256") == canonical_hash(entries) and complete.get("deleted_count") == len(entries) and complete.get("deleted_sha256") == canonical_hash(entries) and complete.get("deletion_method") == method and complete.get("row_root_absent") is True and all(complete.get("post_delete_work", {}).get(key) == 0 for key in (*dimensions, "files", "directories", "symlinks")))
        if results is not None: check("cleanup", sequence, "root-absent", not (results / prepare.get("row_root", "unsafe")).exists())
    peak = {key: max((record.get("pre_delete_row", {}).get(key, 513 * MB) for record in prepares), default=513 * MB) for key in dimensions}
    cumulative = {key: sum(record.get("pre_delete_row", {}).get(key, 0) for record in prepares) for key in dimensions}
    check("cleanup", 0, "exact-root", cleanup.get("status") == "PASS" and cleanup.get("declared_root") == "work-v6" and cleanup.get("work_root_absent") is True and cleanup.get("all_row_roots_absent") is True and cleanup.get("broad_deletion") is False and cleanup.get("deletion_method") == method and cleanup.get("prepare_records") == cleanup.get("complete_records") == 9 and cleanup.get("row_cleanup_records") == 18 and cleanup.get("row_cleanup_labels") == labels and cleanup.get("durable_prepare_complete") is True and len(cleanup.get("row_cleanup_sha256", "")) == 64 and cleanup.get("peak_equation") == "max_individual_PREPARE_pre_delete_row_not_cumulative_sum" and all(cleanup.get(f"peak_{key}") == peak[key] and cleanup.get(f"cumulative_{key}") == cumulative[key] for key in dimensions) and max(peak.values()) <= 512 * MB)
    if results is not None: check("cleanup", 0, "artifact-hash-work-root", cleanup.get("row_cleanup_sha256") == hash_file(results / "ROW-CLEANUP-v6.jsonl") and not (results / "work-v6").exists())
    ordered_failures = sorted(failures)
    ledger = {"schema": "phase4-g3-v6-normalized-ledger-v1", "failures": ordered_failures, "gates": {gate: not any(item.startswith(gate + ":") for item in ordered_failures) for gate in GATE_ORDER}, "source_set_sha256": source_set, "operation_total_ns": sum(row.get("operation_total_ns", 0) for row in rows), "storage": {"peak": peak, "cumulative": cumulative, "row_cleanup_records": len(cleanup_rows)}, "rows": [{key: row.get(key) for key in VIEW} for row in rows]}
    return {"status": "PASS" if not ordered_failures else "REVISE", "normalized_ledger": ledger, "normalized_ledger_sha256": canonical_hash(ledger)}


def sample_data():
    sources = [{"path": path, "sha256": str(index + 1) * 64, "size_bytes": index + 1, "source_mode": "0644", "copy_path": f"source-custody-v6/{path}", "copy_sha256": str(index + 1) * 64, "copy_size_bytes": index + 1, "copy_mode": "0400"} for index, path in enumerate(FILES)]
    source_set = canonical_hash([{key: row[key] for key in ("path", "sha256", "size_bytes", "source_mode")} for row in sources])
    custody = {"schema": "phase4-g3-v6-source-custody-v1", "status": "PASS", "source_set_sha256": source_set, "sources": sources}
    rows = []
    for seq, scenario, size in PLAN:
        route, reason, outcome, error = DECISIONS[scenario]
        row = {key: 0 for key in NUMBER_KEYS}
        row.update({"schema": "phase4-g3-row-v1", "sequence": seq, "label": f"{seq:02d}-{scenario}", "scenario": scenario, "size_bytes": size, "route": route, "qualification_reason": reason, "outcome": outcome, "error": error, "generation": 1, "parent_root": "p", "target_root": "t", "authority_bindings_checked": [] if scenario == "symlink-substitution" else BOUND.copy(), "reconciliation_outcome": "target" if scenario == "lost-ack" else "not-needed", "output_digest": "d", "expected_output_digest": "d", "byte_exact": True, "mode_exact": True, "old_or_new": "old" if scenario in ("symlink-substitution", "before-publication-fault") else "new", "physical_io_status": "Unavailable: test", "cache_warmth_status": "Unavailable: test", "stable_media_status": "Unavailable: test", "command": ["test"], "external_real_seconds": .1, "external_user_seconds": 0, "external_system_seconds": 0, "peak_memory_footprint_bytes": 1, "executable_sha256": "a" * 64, "source_set_sha256": source_set, "methodology_set_sha256": "b" * 64, "environment_sha256": "c" * 64, "output_length": size + (1 if scenario == "count-change" else 0), "output_mode": 420, "q_high_water": 1, "maximum_resident_set_bytes": 1, "verification_bytes_read": size, "child_timeout_seconds": 15 if scenario == "qualified-one-byte" else 5})
        for key in TIME_KEYS: row[key] = 1
        row.update({"attributed_wall_ns": 10, "unattributed_wall_ns": 1, "operation_total_ns": 11})
        if scenario != "symlink-substitution": row.update({"authority_reads": 1, "authority_bytes_read": 32, "authority_validations": 1, "authority_validation_successes": 1, "seed_files_created": 1, "seed_files_removed": 1})
        if scenario == "invalid-authority": row.update({"authority_validation_successes": 0, "authority_validation_failures": 1})
        elif route.startswith("qualified"): row.update({"seed_authority_reads": 1, "seed_authority_bytes_read": 8, "permit_consumptions": 1})
        if scenario == "qualified-noop": row.update({"clone_calls": 1, "clone_successes": 1, "clone_source_logical_bytes": size})
        elif scenario in ("qualified-one-byte", "qualified-one-mib", "before-publication-fault", "lost-ack"):
            delta = MB if scenario == "qualified-one-mib" else 1
            row.update({"mapping_sql_queries": 1, "mapping_sql_rows": 1, "object_sql_queries": 1, "object_sql_rows": 1, "payload_sql_queries": 2, "payload_sql_rows": 2, "canonical_blob_reads": 1, "canonical_blob_bytes": delta, "authenticated_objects": 1, "canonical_bytes_authenticated": delta, "clone_calls": 1, "clone_successes": 1, "clone_source_logical_bytes": size, "patch_calls": 1, "patch_bytes": delta, "changed_ranges": 1, "changed_bytes": delta})
        elif route == "complete-fallback":
            delta = row["output_length"]
            row.update({"mapping_sql_queries": 1, "mapping_sql_rows": 1, "object_sql_queries": 1, "object_sql_rows": 1, "payload_sql_queries": 2, "payload_sql_rows": 2, "canonical_blob_reads": 1, "canonical_blob_bytes": delta, "authenticated_objects": 1, "canonical_bytes_authenticated": delta, "source_bytes_reconstructed": delta, "fallback_calls": 1, "fallback_write_bytes": delta})
        if scenario == "before-publication-fault": row.update({"temp_files_created": 1, "temp_files_removed": 1})
        elif scenario != "symlink-substitution": row.update({"metadata_operations": 1, "temp_files_created": 1, "data_sync_calls": 1, "metadata_sync_calls": 1, "rename_calls": 1, "directory_sync_calls": 1})
        if scenario == "lost-ack": row.update({"reconciliation_calls": 1, "reconciliation_sql_queries": 1, "reconciliation_sql_rows": 1, "reconciliation_blob_reads": 1, "reconciliation_canonical_bytes_authenticated": size, "reconciliation_source_bytes_compared": size, "reconciliation_q_high_water": 1, "destination_bytes_read": size})
        rows.append(row)
    cleanup_rows = []
    for sequence, scenario, _ in PLAN:
        label = f"{sequence:02d}-{scenario}"
        usage = {"logical_bytes": sequence, "apparent_bytes": sequence + 10, "allocated_bytes": sequence + 20, "files": 1, "directories": 1, "symlinks": 0}
        entries = [{"path": "artifact", "kind": "regular", "device": 1, "inode": sequence, "mode": 0o600, "nlink": 1, "size_bytes": sequence, "mtime_ns": 1, "ctime_ns": 1, "allocated_bytes": sequence + 20}]
        method = "descriptor-relative-openat-fstatat-unlinkat-rmdir-no-follow-exact-inventory-v1"
        prepare = {"schema": "phase4-g3-v6-row-cleanup-v1", "event": "PREPARE", "sequence": sequence, "label": label, "row_root": f"work-v6/{label}", "row_identity": {"kind": "directory", "device": 1, "inode": 100 + sequence, "mode": 0o700, "nlink": 2, "size_bytes": 0, "mtime_ns": 1, "ctime_ns": 1, "allocated_bytes": 0}, "inventory": entries, "inventory_count": 1, "inventory_sha256": canonical_hash(entries), "pre_delete_row": usage, "pre_delete_work": dict(usage), "deletion_method": method, "anchored_work_dirfd": True, "anchored_row_dirfd": True, "row_fd_retained_prepare_through_delete": True, "enumeration_followed_symlinks": False, "private_namespace_process_custody": True, "candidate_exactness": {key: rows[sequence - 1][key] for key in ("byte_exact", "mode_exact", "temp_residue_count", "seed_residue_count", "old_or_new", "output_digest", "expected_output_digest")}}
        complete = {"schema": "phase4-g3-v6-row-cleanup-v1", "event": "COMPLETE", "sequence": sequence, "label": label, "row_root": prepare["row_root"], "prepare_sha256": canonical_hash(prepare), "inventory_count": 1, "inventory_sha256": canonical_hash(entries), "deleted_count": 1, "deleted_sha256": canonical_hash(entries), "deletion_method": method, "row_root_absent": True, "post_delete_work": {key: 0 for key in usage}}
        cleanup_rows.extend((prepare, complete))
    dimensions = ("logical_bytes", "apparent_bytes", "allocated_bytes"); prepares = cleanup_rows[::2]; peak = {key: max(row["pre_delete_row"][key] for row in prepares) for key in dimensions}; sums = {key: sum(row["pre_delete_row"][key] for row in prepares) for key in dimensions}
    cleanup = {"status": "PASS", "declared_root": "work-v6", "work_root_absent": True, "all_row_roots_absent": True, "broad_deletion": False, "deletion_method": method, "prepare_records": 9, "complete_records": 9, "row_cleanup_records": 18, "row_cleanup_labels": [row["label"] for row in prepares], "row_cleanup_sha256": "d" * 64, "durable_prepare_complete": True, "peak_equation": "max_individual_PREPARE_pre_delete_row_not_cumulative_sum", **{f"peak_{key}": peak[key] for key in dimensions}, **{f"cumulative_{key}": sums[key] for key in dimensions}}
    return rows, cleanup, custody, cleanup_rows


def self_test():
    rows, cleanup, custody, cleanup_rows = sample_data()
    assert audit(rows, cleanup, custody, cleanup_rows=cleanup_rows)["status"] == "PASS"
    prior = copy.deepcopy(rows); prior[8].update({"reconciliation_outcome": "prior", "old_or_new": "old"}); prior_cleanup = copy.deepcopy(cleanup_rows); prior_cleanup[16]["candidate_exactness"].update({"old_or_new": "old"}); prior_cleanup[17]["prepare_sha256"] = canonical_hash(prior_cleanup[16]); assert audit(prior, cleanup, custody, cleanup_rows=prior_cleanup)["status"] == "PASS"
    cases = []
    for index, key, value in ((0, "source_set_sha256", "f" * 64), (8, "reconciliation_outcome", "prior"), (8, "reconciliation_blob_reads", 0), (5, "route", "complete-fallback"), (1, "changed_bytes", 2), (3, "permit_consumptions", 1), (0, "operation_total_ns", 50), (0, "q_terminal", 1), (0, "mode_exact", False)):
        changed = copy.deepcopy(rows); changed[index][key] = value; cases.append((changed, cleanup, custody))
    reverse = copy.deepcopy(rows); reverse.reverse(); cases.append((reverse, cleanup, custody))
    cases.append((rows, dict(cleanup, work_root_absent=False), custody))
    broken = copy.deepcopy(custody); broken["sources"][1]["copy_mode"] = "0644"; cases.append((rows, cleanup, broken))
    missing = copy.deepcopy(custody); missing["sources"][0].pop("copy_path"); cases.append((rows, cleanup, missing))
    escaped = copy.deepcopy(custody); escaped["sources"][0]["copy_path"] = "/tmp/outside"; cases.append((rows, cleanup, escaped))
    wrong_size = copy.deepcopy(custody); wrong_size["sources"][0]["copy_size_bytes"] += 1; cases.append((rows, cleanup, wrong_size))
    assert all(audit(*case, cleanup_rows=cleanup_rows)["status"] == "REVISE" for case in cases)
    missing_cleanup = cleanup_rows[:-1]; assert audit(rows, cleanup, custody, cleanup_rows=missing_cleanup)["status"] == "REVISE"
    wrong_hash = copy.deepcopy(cleanup_rows); wrong_hash[0]["inventory_sha256"] = "0" * 64; assert audit(rows, cleanup, custody, cleanup_rows=wrong_hash)["status"] == "REVISE"
    misordered = copy.deepcopy(cleanup_rows); misordered[1], misordered[2] = misordered[2], misordered[1]; assert audit(rows, cleanup, custody, cleanup_rows=misordered)["status"] == "REVISE"
    no_method = copy.deepcopy(cleanup_rows); no_method[0].pop("deletion_method"); assert audit(rows, cleanup, custody, cleanup_rows=no_method)["status"] == "REVISE"
    late = copy.deepcopy(cleanup_rows); late[0]["inventory"].append({**late[0]["inventory"][0], "path": "late", "inode": 999}); late[0]["inventory_count"] = 2; late[0]["inventory_sha256"] = canonical_hash(late[0]["inventory"]); assert audit(rows, cleanup, custody, cleanup_rows=late)["status"] == "REVISE"
    ancestor = copy.deepcopy(cleanup_rows); ancestor[0]["anchored_work_dirfd"] = False; assert audit(rows, cleanup, custody, cleanup_rows=ancestor)["status"] == "REVISE"
    cumulative_peak = copy.deepcopy(cleanup); cumulative_peak["peak_allocated_bytes"] = cumulative_peak["cumulative_allocated_bytes"]; assert audit(rows, cumulative_peak, custody, cleanup_rows=cleanup_rows)["status"] == "REVISE"
    clone = copy.deepcopy(rows[3]); clone.update({"qualification_reason": "clone-unsupported", "route": "complete-fallback", "permit_consumptions": 0, "fallback_calls": 1})
    assert precedence_valid(clone); clone["permit_consumptions"] = 1; assert not precedence_valid(clone)
    wrong_kind = copy.deepcopy(rows[5]); wrong_kind.update({"qualification_reason": "destination-wrong-kind", "error": "NativeDestinationWrongKind"}); assert precedence_valid(wrong_kind); wrong_kind["fallback_calls"] = 1; assert not precedence_valid(wrong_kind)
    print(json.dumps({"status": "PASS", "mutations_rejected": len(cases) + 9}, sort_keys=True))


def main():
    if sys.argv[1:] == ["--self-check"]: self_test(); return
    if len(sys.argv) != 2: raise SystemExit("usage: recompute_g3_v6.py RESULTS | --self-check")
    root = Path(sys.argv[1])
    rows = [json.loads(line) for line in (root / "rows-v6/G3-V6-RAW.jsonl").read_text().splitlines() if line]
    cleanup_rows = [json.loads(line) for line in (root / "ROW-CLEANUP-v6.jsonl").read_text().splitlines() if line]
    result = {"schema": "phase4-g3-v6-independent-recomputation-v1", **audit(rows, json.loads((root / "CLEANUP-v6.json").read_text()), json.loads((root / "SOURCE-CUSTODY-v6.json").read_text()), root, cleanup_rows)}
    print(json.dumps(result, sort_keys=True, separators=(",", ":")))


if __name__ == "__main__": main()
