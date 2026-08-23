#!/usr/bin/env python3
import csv
import hashlib
import json
import pathlib
import statistics
import sys


OP_SCHEMA = "phase4-g5-1-operation-v17"
TERMINAL_SCHEMA = "phase4-g5-trusted-child-terminal-v10"
RSS_LIMIT = 20_971_520
TIMER_FIELDS = (
    "store_preflight_ns", "sqlite_open_and_profile_ns", "visible_head_and_transition_ns",
    "edit_base_scope_ns", "mapping_and_construction_ns", "proof_ns",
    "publication_commit_ns", "reconciliation_ns",
)
S07_G4_SHA256 = "e72988fc25e96f608d0d405e157ea8e837029595ace916f066932082a736db33"
S07_FIXTURE_SHA256 = "4a3acf60f044bbae8ed0d0a8aa8fabd8b4cee74216dbccc36255b9c6fbe50a2a"
S07_PRODUCT_BASE_COPY_METHOD = "regenerated-isolated-database"
S07_COMMON = {
    "source_fingerprint": "f79de600cf44b20c4443e06d2e2b9e8819e956ba5a7bcc9cab4ffd8a08059cf8",
    "expected_cdc_references": 53,
    "actual_cdc_references": 53,
    "expected_cdc_sequence_fingerprint": "c2b4a92188569d206717210b596dde9b8aeade1c9c81b87f02b8d0d6ebda1112",
    "root_id": "18f33e3ca6030e966cf8ed41c0b43f4769de8b02247f453fae447627bee4b77c",
    "transition_id": "60d191810b303b26d12453add0b9e1718b1f1b654473615d9323f0ee477a9b7d",
    "ordered_closure_digest": "7e806f7023c3e33914c59d2b0d0d84bca8859fdbd7663b55f5f5c99313252d42",
    "q_current": 0,
}
S07_FULL = {
    **S07_COMMON, "operation": "full", "canonical_bytes_written": 1_051_409,
    "canonical_new_write_bytes": 1_051_409, "canonical_bytes_authenticated": 3_259_186,
    "objects_created": 57, "objects_authenticated": 186, "objects_reused": 0,
    "mapping_bytes_rewritten": 2_144, "source_bytes_read": 1_048_576,
    "raw_bytes_hashed": 0, "payload_io_bytes": 2_097_156, "d_bytes": 1_048_580,
    "sqlite_pre_logical_database_bytes": 20_480,
    "sqlite_post_logical_database_bytes": 1_105_920, "transactions": 1, "commits": 1,
    "commit_dispatches": 1, "commit_returns": 1, "commit_return_successes": 1,
    "commit_return_errors": 0, "commit_reconciliation_calls": 0,
    "publication_status": "Committed",
}
S07_RANGE_MEASUREMENT = {
    "label": "sequential-1m", "start": 0, "end": 1_048_576,
    "returned_bytes": 1_048_576, "canonical_bytes_authenticated": 1_051_290,
    "objects_authenticated": 55,
}
S07_RANGE = {
    **S07_COMMON, "operation": "read-range-1m", "canonical_bytes_authenticated": 1_051_433,
    "objects_authenticated": 57, "canonical_bytes_written": 0,
    "canonical_new_write_bytes": 0, "objects_created": 0, "objects_reused": 0,
    "mapping_bytes_rewritten": 0, "payload_io_bytes": 1_048_576, "d_bytes": 1_048_576,
    "sqlite_pre_logical_database_bytes": 1_105_920,
    "sqlite_post_logical_database_bytes": 1_105_920, "transactions": 0, "commits": 0,
    "commit_dispatches": 0, "commit_returns": 0, "commit_return_successes": 0,
    "commit_return_errors": 0, "commit_reconciliation_calls": 0,
    "publication_status": "Unavailable",
}
S07_FULL_RANGE_SHAPES = [
    {"label": "zero", "start": 0, "end": 0, "returned_bytes": 0, "canonical_bytes_authenticated": 89, "objects_authenticated": 1},
    {"label": "first-byte", "start": 0, "end": 1, "returned_bytes": 1, "canonical_bytes_authenticated": 34_806, "objects_authenticated": 3},
    {"label": "cross-chunk", "start": 32_767, "end": 32_769, "returned_bytes": 2, "canonical_bytes_authenticated": 52_174, "objects_authenticated": 4},
    {"label": "last-byte", "start": 1_048_575, "end": 1_048_576, "returned_bytes": 1, "canonical_bytes_authenticated": 17_580, "objects_authenticated": 3},
    {"label": "eof", "start": 1_048_576, "end": 1_048_576, "returned_bytes": 0, "canonical_bytes_authenticated": 89, "objects_authenticated": 1},
]
S07_WORK_FIELDS = (
    "canonical_new_write_bytes", "canonical_bytes_written", "mapping_bytes_rewritten",
    "objects_created", "objects_reused", "transactions", "commits", "publication_status",
)
COMMON_PARITY_FIELDS = (
    "canonical_bytes_authenticated", "objects_authenticated",
    "canonical_authentication_hash_bytes", "canonical_authentication_hashes",
    "reused_object_id_authentications", "reused_object_id_authentication_bytes",
    "statement_cache_acquisitions", "sql_calls", "sql_rows_returned",
    "sql_query_calls", "sql_execute_calls", "sql_rows_changed",
    "row_blob_reads", "row_blob_writes", "row_blob_copy_bytes",
    "borrowed_row_blob_reads", "borrowed_row_blob_bytes",
    "blob_opens", "blob_reads", "blob_writes",
)
MUTATION_PARITY_FIELDS = (
    "canonical_new_write_bytes", "canonical_bytes_written", "mapping_bytes_rewritten",
    "objects_created", "objects_reused", "transactions", "commits", "publication_status",
    "sql_execute_calls", "sql_rows_changed", "row_blob_writes", "blob_writes",
)


def comparison_parity_fields(comparison):
    return (
        COMMON_PARITY_FIELDS
        if comparison == "g4-verified-vs-g5-verified"
        else MUTATION_PARITY_FIELDS
    )


def lifecycle_equation_valid(role, product):
    return role == "g4_verified" or product.get("lifecycle_phase_sum_matches") is True


def paired_rooted_state_equal(left, right):
    return left.get("rooted_logical_state") == right.get("rooted_logical_state")
SECONDARY_BA_OPERATIONS = {"one-byte-early", "one-byte-late", "plus1-middle"}
COMMON_SECONDARY_TIMER_FIELDS = (
    "edit_base_scope_ns", "mapping_and_construction_ns", "proof_ns",
    "publication_commit_ns", "reconciliation_ns",
)
FULL_INTERVAL_CLASSIFICATION = "full-first-edit-equation"
COMMON_INTERVAL_CLASSIFICATION = "common-edit-through-reconciliation"
LIFECYCLE_SCHEMA = "phase4-g5-1-product-child-lifecycle-v17"
RSS_CLASSIFICATION = "PerProductChildRetainedPeak"
TOUCHED_ERRORS = {
    "missing-object": "MissingObject",
    "identity-mismatch": "IdentityMismatch",
    "wrong-logical-role": "WrongLogicalRole",
    "malformed-logical-record": "UnexpectedEof",
}
CLONE_SCHEMA = "g5-v17-native-clone-receipt-v1"
CLONE_FIELDS = {
    "schema", "method", "copy_content", "sealed_input_manifest_sha256",
    "inventory_equal", "dispatch_modes_exact", "entries",
}
CLONE_ENTRY_FIELDS = {
    "path", "bytes", "master_manifest_sha256", "clonefile_success",
    "source_device", "source_inode", "source_mode", "destination_device",
    "destination_inode", "clone_destination_mode", "dispatch_mode",
    "mode_transition", "same_device", "distinct_inode", "size_equal",
    "source_unchanged",
}
ROOTED_STATE_SCHEMA = "g5-v17-rooted-logical-state-v1"
ROOTED_STATE_SEMANTICS = (
    "product-authenticated-root-transition-ordered-closure-not-all-object-table"
)
ROOTED_STATE_FIELDS = {
    "schema", "semantics", "query_only", "autocommit", "head_generation",
    "head_root_id", "head_transition_id", "head_receipt_bytes",
    "head_receipt_sha256", "head_receipt_semantics", "ordered_closure_digest",
    "closure_provenance", "reachable_published_result_parity",
    "all_object_table_catalog_parity", "rollback_freshness",
}
PHYSICAL_SCHEMA = "g5-v17-physical-allocation-observation-v1"
PHYSICAL_FIELDS = {
    "schema", "classification", "database_file_bytes", "sqlite_page_size",
    "sqlite_page_count", "sqlite_freelist_count", "sqlite_allocated_bytes",
    "sqlite_freelist_bytes", "sqlite_schema_rootpages",
}
LIFECYCLE_FIELDS = {
    "schema", "status", "lifecycle_scope", "started_product_children",
    "reaped_product_children", "max_simultaneous_product_children",
    "active_product_children_terminal", "construction_failures", "request_failures",
    "close_failures", "rss_classification", "rss_limit_bytes_per_product_child",
    "aggregate_product_children_rss_claim", "rss_observations_complete",
    "per_product_child_rss", "pair_scopes",
}
PAIR_SCOPE_FIELDS = {
    "sequence_id", "pair", "expected_roles", "required_g5_children", "observed_roles",
    "max_simultaneous_product_children", "row_q_zero", "pair_status",
    "active_product_children_after_pair", "sequence_terminal_records_present",
    "sequence_terminal_status_pass", "sequence_terminal_q_zero",
    "sequence_terminal_owners_zero",
    "failure_cleanup_complete", "active_product_children_after_sequence",
}
RSS_FIELDS = {
    "label", "kind", "classification", "maximum_resident_set_size",
    "limit_bytes_per_product_child", "within_per_product_child_limit",
    "aggregate_rss_claim",
}
ARM_CLEANUP_SCHEMA = "phase4-g5-1-arm-cleanup-receipt-v17"
ARM_CLEANUP_FIELDS = {
    "schema", "status", "classification", "ordinal", "sequence_id", "pair", "role",
    "row_root_name", "inventory_entries_removed", "inventory_sha256",
    "row_root_absent", "parent_directory_fsynced", "active_row_roots_after_cleanup",
}
WORK_ROOT_SCHEMA = "phase4-g5-1-work-root-lifecycle-v17"
WORK_ROOT_FIELDS = {
    "schema", "status", "lifecycle_scope", "started_row_roots", "cleaned_row_roots",
    "max_active_row_roots", "active_row_roots_terminal", "cleanup_failures", "receipts",
}
PREARM_INITIALIZATION_ACTIONS = {
    "database_discovery_enumerations": 1,
    "published_visible_state_invocations": 1,
    "published_visible_head_rows": 1,
    "published_visible_head_receipt_bytes": 216,
    "query_only_pragma_queries": 2,
    "physical_pragma_queries": 3,
    "sqlite_schema_rootpage_queries": 1,
    "sqlite_schema_rootpage_rows": 3,
    "ordered_object_all_row_scans": 0,
    "initialization_evidence_write_calls": 1,
    "initialization_evidence_file_fsync_calls": 1,
    "initialization_evidence_directory_fsync_calls": 1,
    "store_opens": 0,
    "product_children_started": 0,
    "product_rows": 0,
    "locks_acquired": 0,
}
INPUT_MANIFEST = pathlib.Path(__file__).resolve().parents[1] / "method/INPUT-MANIFEST-v17.tsv"
DRY_RUN = pathlib.Path(__file__).resolve().parents[1] / "DRY-RUN-v17.json"


def compact(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":"))


def read_tsv(path):
    with pathlib.Path(path).open(newline="", encoding="utf-8") as handle:
        return list(csv.DictReader(handle, delimiter="\t"))


def percentile(values, numerator, denominator):
    ordered = sorted(values)
    index = max(0, (len(ordered) * numerator + denominator - 1) // denominator - 1)
    return ordered[index]


def required(product, name, failures, label):
    if name not in product:
        failures.append(f"{label}:missing-{name}")
        return None
    return product[name]


def is_sha256(value):
    return (
        isinstance(value, str)
        and len(value) == 64
        and all(character in "0123456789abcdef" for character in value)
    )


def semantic_error_matches(fault_case, value):
    if fault_case == "missing-object":
        prefix = "MissingObject(ObjectId("
        return (
            isinstance(value, str)
            and value.startswith(prefix)
            and value.endswith("))")
            and is_sha256(value[len(prefix):-2])
        )
    return value == f"Core({TOUCHED_ERRORS.get(fault_case)})"


def validate_clone(wrapper, input_manifest_sha256, input_manifest, failures, label):
    receipt = wrapper.get("clone_receipt")
    custody = wrapper.get("pre_dispatch_custody")
    if not isinstance(receipt, dict) or set(receipt) != CLONE_FIELDS:
        failures.append(f"{label}:clone-receipt-shape")
        return
    entries = receipt.get("entries")
    if (
        receipt.get("schema") != CLONE_SCHEMA
        or receipt.get("method") != "darwin-clonefile"
        or receipt.get("copy_content") != "NotRehashedPerFastLaw"
        or receipt.get("sealed_input_manifest_sha256") != input_manifest_sha256
        or receipt.get("inventory_equal") is not True
        or receipt.get("dispatch_modes_exact") is not True
        or not isinstance(entries, list)
        or not entries
    ):
        failures.append(f"{label}:clone-receipt-hard")
        return
    paths = [entry.get("path") for entry in entries if isinstance(entry, dict)]
    if len(paths) != len(entries) or paths != sorted(paths) or len(paths) != len(set(paths)):
        failures.append(f"{label}:clone-receipt-order")
    for entry in entries:
        if (
            set(entry) != CLONE_ENTRY_FIELDS
            or not isinstance(entry.get("path"), str)
            or not entry["path"]
            or type(entry.get("bytes")) is not int
            or entry["bytes"] < 0
            or not is_sha256(entry.get("master_manifest_sha256"))
            or any(
                entry.get(name) is not True
                for name in ("clonefile_success", "same_device", "distinct_inode", "size_equal", "source_unchanged")
            )
            or any(
                type(entry.get(name)) is not int or entry[name] < 0
                for name in ("source_device", "source_inode", "destination_device", "destination_inode")
            )
            or entry.get("source_mode") != "-r--r--r--"
            or entry.get("clone_destination_mode") != "-r--r--r--"
            or entry.get("dispatch_mode") != "-rw-------"
            or entry.get("mode_transition") != "sealed-0444-to-private-0600"
            or entry.get("source_device") != entry.get("destination_device")
            or entry.get("source_inode") == entry.get("destination_inode")
            or any("copy" in name and "hash" in name for name in entry)
        ):
            failures.append(f"{label}:clone-entry-hard")
            break
    prefix = f"bases/{wrapper.get('operation')}-{wrapper.get('size_bytes')}/"
    expected_entries = sorted(
        (
            row["input_relative_path"][len(prefix):],
            int(row["bytes"]),
            row["sha256"],
        )
        for row in input_manifest
        if row["input_relative_path"].startswith(prefix)
    )
    observed_entries = sorted(
        (entry.get("path"), entry.get("bytes"), entry.get("master_manifest_sha256"))
        for entry in entries
        if isinstance(entry, dict)
    )
    if not expected_entries or observed_entries != expected_entries:
        failures.append(f"{label}:clone-manifest-binding")
    derived = {}
    for entry in entries:
        path = entry.get("path", "")
        if path.endswith(".sqlite"):
            derived["database_sha256"] = entry.get("master_manifest_sha256")
        elif path.endswith(".sqlite.authority"):
            derived["authority_sha256"] = entry.get("master_manifest_sha256")
        elif path.endswith(".sqlite.expectations"):
            derived["expectations_sha256"] = entry.get("master_manifest_sha256")
    if (
        not isinstance(custody, dict)
        or set(custody) != {"database_sha256", "authority_sha256", "expectations_sha256", "proof"}
        or custody.get("proof") != "preverified-sealed-master-plus-native-clone-receipt"
        or derived != {name: custody.get(name) for name in derived}
        or set(derived) != {"database_sha256", "authority_sha256", "expectations_sha256"}
    ):
        failures.append(f"{label}:manifest-derived-custody")


def validate_rooted_state(wrapper, product, failures, label):
    state = wrapper.get("rooted_logical_state")
    if not isinstance(state, dict) or set(state) != ROOTED_STATE_FIELDS:
        failures.append(f"{label}:rooted-state-shape")
        return
    expected_provenance = (
        "ObservedVerifiedCompleteRoundTrip"
        if wrapper.get("validation_scope", "CompleteRoundTrip") == "CompleteRoundTrip"
        else "PreparedGoldenBoundByExactRootTransitionAndProductQualification"
    )
    if (
        state.get("schema") != ROOTED_STATE_SCHEMA
        or state.get("semantics") != ROOTED_STATE_SEMANTICS
        or state.get("query_only") is not True
        or state.get("autocommit") is not True
        or type(state.get("head_generation")) is not int
        or state["head_generation"] < 1
        or state.get("head_receipt_bytes") != 216
        or state.get("head_root_id") != product.get("root_id")
        or state.get("head_transition_id") != product.get("transition_id")
        or state.get("ordered_closure_digest") != product.get("ordered_closure_digest")
        or any(
            not is_sha256(state.get(name))
            for name in (
                "head_root_id", "head_transition_id", "head_receipt_sha256",
                "ordered_closure_digest",
            )
        )
        or state.get("head_receipt_semantics")
        != "ProductAuthenticatedHeadTupleOpaqueHashNotClosureOrFreshness"
        or state.get("closure_provenance") != expected_provenance
        or state.get("reachable_published_result_parity") != "ClaimedHardGated"
        or state.get("all_object_table_catalog_parity")
        != "NotClaimedSeparateFutureAllRowCasAudit"
        or state.get("rollback_freshness") != "NotProtected"
    ):
        failures.append(f"{label}:rooted-state-hard")
    if "post_database_sha256" in wrapper:
        failures.append(f"{label}:physical-database-claim")
    if wrapper.get("post_database_hash_semantics") != ROOTED_STATE_SEMANTICS:
        failures.append(f"{label}:database-hash-semantics")
    physical = wrapper.get("physical_allocation_observation")
    if not isinstance(physical, dict) or set(physical) != PHYSICAL_FIELDS:
        failures.append(f"{label}:physical-allocation-shape")
        return
    page_size = physical.get("sqlite_page_size", 0)
    rootpages = physical.get("sqlite_schema_rootpages")
    rootpage_keys = {"type", "name", "table_name", "rootpage"}
    rootpage_order = [
        (entry.get("type"), entry.get("name"))
        for entry in rootpages
        if isinstance(entry, dict)
    ] if isinstance(rootpages, list) else []
    if (
        physical.get("schema") != PHYSICAL_SCHEMA
        or physical.get("classification") != "NotLogicalParity"
        or any(
            type(physical.get(name)) is not int or physical[name] < 0
            for name in PHYSICAL_FIELDS
            - {"schema", "classification", "sqlite_schema_rootpages"}
        )
        or physical.get("database_file_bytes", 0) <= 0
        or not 512 <= page_size <= 65_536
        or page_size & (page_size - 1) != 0
        or physical.get("sqlite_page_count", 0) <= 0
        or physical.get("sqlite_freelist_count", 0) > physical.get("sqlite_page_count", 0)
        or physical.get("sqlite_allocated_bytes") != page_size * physical.get("sqlite_page_count")
        or physical.get("sqlite_freelist_bytes") != page_size * physical.get("sqlite_freelist_count")
        or not isinstance(rootpages, list)
        or not rootpages
        or len(rootpage_order) != len(rootpages)
        or rootpage_order != sorted(rootpage_order)
        or len(rootpage_order) != len(set(rootpage_order))
        or any(
            set(entry) != rootpage_keys
            or any(not isinstance(entry.get(name), str) or not entry[name] for name in (
                "type", "name", "table_name"
            ))
            or type(entry.get("rootpage")) is not int
            or entry["rootpage"] < 0
            for entry in rootpages
        )
    ):
        failures.append(f"{label}:physical-allocation-hard")


def validate_comparison_interval(row, wrapper, timers, failures, label):
    common = (
        wrapper.get("comparison") == "g4-verified-vs-g5-verified"
        and wrapper.get("operation") != "first-edit-after-reopen"
    )
    fields = COMMON_SECONDARY_TIMER_FIELDS if common else TIMER_FIELDS
    classification = COMMON_INTERVAL_CLASSIFICATION if common else FULL_INTERVAL_CLASSIFICATION
    interval = row.get("comparison_interval_ns")
    if wrapper.get("comparison") == "g4-g5-triple":
        named_intervals = {}
        named_classes = {}
        role = wrapper.get("role")
        if role in ("g4_verified", "g5_verified"):
            fields_for_g4 = (
                COMMON_SECONDARY_TIMER_FIELDS
                if wrapper.get("operation") != "first-edit-after-reopen"
                else TIMER_FIELDS
            )
            named_intervals["g4_verified_vs_g5_verified"] = sum(
                timers.get(name, -1) for name in fields_for_g4
            )
            named_classes["g4_verified_vs_g5_verified"] = (
                COMMON_INTERVAL_CLASSIFICATION
                if fields_for_g4 == COMMON_SECONDARY_TIMER_FIELDS
                else FULL_INTERVAL_CLASSIFICATION
            )
        if role in ("g5_verified", "g5_trusted"):
            named_intervals["g5_verified_vs_g5_trusted"] = row.get("decision_ns")
            named_classes["g5_verified_vs_g5_trusted"] = FULL_INTERVAL_CLASSIFICATION
    else:
        key = str(wrapper.get("comparison")).replace("-", "_")
        named_intervals = {key: interval}
        named_classes = {key: classification}
    if (
        row.get("comparison_interval_classification") != classification
        or row.get("comparison_interval_components") != list(fields)
        or type(interval) is not int
        or interval < 0
        or interval != sum(timers.get(name, -1) for name in fields)
        or (not common and interval != row.get("decision_ns"))
        or row.get("comparison_intervals_ns") != named_intervals
        or row.get("comparison_interval_classifications") != named_classes
        or wrapper.get("comparison_intervals_ns") != named_intervals
        or wrapper.get("comparison_interval_classifications") != named_classes
    ):
        failures.append(f"{label}:comparison-interval")


def expected_gate_shape(schedule):
    expected = []
    ordinal = 0
    for sequence in schedule:
        if sequence.get("campaign") != "gate":
            continue
        comparison = sequence["comparison"]
        base_roles = (
            ["g4_verified", "g5_verified"]
            if comparison == "g4-verified-vs-g5-verified"
            else ["g5_verified", "g5_trusted"]
        )
        for pair in range(1, int(sequence["pairs"]) + 1):
            roles = list(base_roles)
            flipped_secondary = int(sequence["pairs"]) == 5 and sequence["operation"] in SECONDARY_BA_OPERATIONS
            if (pair % 2 == 0) != flipped_secondary:
                roles.reverse()
            for role in roles:
                ordinal += 1
                expected.append(
                    (
                        ordinal, sequence["sequence_id"], comparison, sequence["operation"],
                        pair, role, "trusted-local-dev" if role == "g5_trusted" else "verified",
                        int(sequence["size_bytes"]), sequence["expectation_id"],
                    )
                )
    return expected


def expected_screen_shape(schedule):
    expected = []
    ordinal = 0
    roles_by_comparison = {
        "same-g5": ["g5_verified", "g5_trusted"],
        "g4-g5-triple": ["g4_verified", "g5_verified", "g5_trusted"],
    }
    for sequence in schedule:
        if sequence.get("sequence_id") not in {"S01", "S05", "S06"}:
            continue
        for role in roles_by_comparison[sequence["comparison"]]:
            ordinal += 1
            expected.append(
                (
                    ordinal, sequence["sequence_id"], sequence["comparison"],
                    sequence["operation"], 1, role,
                    "trusted-local-dev" if role == "g5_trusted" else "verified",
                    int(sequence["size_bytes"]), sequence["expectation_id"],
                )
            )
    return expected


def expected_child_lifecycle(operations, campaign):
    pairs = {}
    for row in operations:
        wrapper = row.get("wrapper", {})
        pairs.setdefault((wrapper.get("sequence_id"), wrapper.get("pair")), []).append(row)
    active_by_sequence = {}
    scopes = []
    rss_kinds = {}
    terminal_labels = set()
    for (sequence_id, pair), rows in pairs.items():
        active = active_by_sequence.setdefault(sequence_id, set())
        roles = [row.get("wrapper", {}).get("role") for row in rows]
        required = [f"{sequence_id}-{role}" for role in roles if str(role).startswith("g5_")]
        high_water = len(active)
        for row, role in zip(rows, roles):
            wrapper = row.get("wrapper", {})
            if role == "g4_verified":
                label = f"{int(wrapper['ordinal']):03d}-{sequence_id}-{role}"
                rss_kinds[label] = "synchronous-one-shot"
                high_water = max(high_water, len(active) + 1)
            else:
                label = f"{sequence_id}-{role}"
                active.add(label)
                terminal_labels.add(label)
                rss_kinds[label] = "persistent-g5-row-transport"
                high_water = max(high_water, len(active))
        scopes.append(
            {
                "sequence_id": sequence_id,
                "pair": pair,
                "expected_roles": roles,
                "required_g5_children": required,
                "observed_roles": roles,
                "max_simultaneous_product_children": high_water,
                "row_q_zero": True,
                "pair_status": "PASS",
                "active_product_children_after_pair": len(active),
                "sequence_terminal_records_present": True,
                "sequence_terminal_status_pass": True,
                "sequence_terminal_q_zero": True,
                "sequence_terminal_owners_zero": True,
                "failure_cleanup_complete": True,
                "active_product_children_after_sequence": 0,
            }
        )
    if campaign == "screen":
        for case in (
            "touched-error-matrix", "unrelated-corruption",
            "trusted-verified-reopen", "reconciliation",
        ):
            rss_kinds[f"semantic-{case}"] = "semantic-one-shot"
        for label in (
            "s07-01-fixture", "s07-02-full-create-prepare", "s07-03-full-create-row",
            "s07-04-range-prepare", "s07-05-range-row",
        ):
            rss_kinds[label] = "s07-one-shot"
    return scopes, rss_kinds, terminal_labels


def validate_child_lifecycle(
    records, operations, terminals, semantic_terminals, sentinel_rows,
    campaign, artifact_path, failures,
):
    if len(records) != 1 or not isinstance(records[0], dict):
        failures.append(f"child-lifecycle-cardinality:{len(records)}")
        return
    value = records[0]
    try:
        artifact_bytes = pathlib.Path(artifact_path).read_bytes()
        artifact = json.loads(artifact_bytes)
    except (OSError, json.JSONDecodeError):
        failures.append("child-lifecycle-artifact")
        artifact = None
        artifact_bytes = b""
    if artifact != value or artifact_bytes != (compact(value) + "\n").encode():
        failures.append("child-lifecycle-artifact")
    expected_scopes, expected_rss, expected_terminals = expected_child_lifecycle(
        operations, campaign
    )
    expected_rss_bytes = {}
    for row in operations:
        wrapper = row.get("wrapper", {})
        if wrapper.get("role") == "g4_verified":
            label = f"{int(wrapper['ordinal']):03d}-{wrapper['sequence_id']}-g4_verified"
            expected_rss_bytes[label] = row.get("external_time", {}).get(
                "maximum_resident_set_size"
            )
    for terminal in terminals:
        expected_rss_bytes[terminal.get("product_child_label")] = terminal.get(
            "external_time", {}
        ).get("maximum_resident_set_size")
    if campaign == "screen":
        for terminal in semantic_terminals:
            expected_rss_bytes[f"semantic-{terminal.get('case')}"] = terminal.get(
                "external_time", {}
            ).get("maximum_resident_set_size")
        for row in sentinel_rows:
            route = row.get("route")
            times = row.get("command_external_times", {})
            expected_rss_bytes["s07-01-fixture"] = times.get("fixture", {}).get(
                "maximum_resident_set_size"
            )
            prefix = "s07-02-full-create" if route == "full-create" else "s07-04-range"
            expected_rss_bytes[f"{prefix}-prepare"] = times.get("prepare", {}).get(
                "maximum_resident_set_size"
            )
            row_prefix = "s07-03-full-create" if route == "full-create" else "s07-05-range"
            expected_rss_bytes[f"{row_prefix}-row"] = times.get("row", {}).get(
                "maximum_resident_set_size"
            )
    if set(value) != LIFECYCLE_FIELDS:
        failures.append("child-lifecycle-shape")
        return
    observations = value.get("per_product_child_rss")
    observed_rss = {
        item.get("label"): item
        for item in observations
        if isinstance(item, dict)
    } if isinstance(observations, list) else {}
    rss_hard = (
        not isinstance(observations, list)
        or len(observed_rss) != len(observations)
        or set(observed_rss) != set(expected_rss)
        or any(
            set(item) != RSS_FIELDS
            or item.get("kind") != expected_rss[label]
            or item.get("maximum_resident_set_size") != expected_rss_bytes.get(label)
            or item.get("classification") != RSS_CLASSIFICATION
            or type(item.get("maximum_resident_set_size")) is not int
            or not 0 < item["maximum_resident_set_size"] <= RSS_LIMIT
            or item.get("limit_bytes_per_product_child") != RSS_LIMIT
            or item.get("within_per_product_child_limit") is not True
            or item.get("aggregate_rss_claim") != "NotClaimed"
            for label, item in observed_rss.items()
        )
    )
    terminal_labels = {terminal.get("product_child_label") for terminal in terminals}
    expected_maximum = max(
        (scope["max_simultaneous_product_children"] for scope in expected_scopes),
        default=1 if expected_rss else 0,
    )
    if (
        value.get("schema") != LIFECYCLE_SCHEMA
        or value.get("status") != "PASS"
        or value.get("lifecycle_scope") != "SequenceScopedMatchedPairs"
        or value.get("started_product_children") != len(expected_rss)
        or value.get("reaped_product_children") != len(expected_rss)
        or value.get("max_simultaneous_product_children") != expected_maximum
        or expected_maximum > 2
        or value.get("active_product_children_terminal") != 0
        or any(value.get(name) != 0 for name in (
            "construction_failures", "request_failures", "close_failures"
        ))
        or value.get("rss_classification") != RSS_CLASSIFICATION
        or value.get("rss_limit_bytes_per_product_child") != RSS_LIMIT
        or value.get("aggregate_product_children_rss_claim") != "NotClaimed"
        or value.get("rss_observations_complete") is not True
        or value.get("pair_scopes") != expected_scopes
        or terminal_labels != expected_terminals
        or rss_hard
    ):
        failures.append("child-lifecycle-hard")


def validate_work_root_lifecycle(records, operations, artifact_path, failures):
    receipts = []
    for row in operations:
        wrapper = row.get("wrapper", {})
        receipt = wrapper.get("arm_cleanup_receipt")
        inventory = wrapper.get("post_inventory")
        label = f"{wrapper.get('sequence_id')}:{wrapper.get('pair')}:{wrapper.get('role')}"
        if (
            not isinstance(receipt, dict)
            or set(receipt) != ARM_CLEANUP_FIELDS
            or receipt.get("schema") != ARM_CLEANUP_SCHEMA
            or receipt.get("status") != "PASS"
            or receipt.get("classification") != "ImmediatePostEvidence"
            or receipt.get("ordinal") != wrapper.get("ordinal")
            or receipt.get("sequence_id") != wrapper.get("sequence_id")
            or receipt.get("pair") != wrapper.get("pair")
            or receipt.get("role") != wrapper.get("role")
            or receipt.get("row_root_name")
            != f"{int(wrapper.get('ordinal', -1)):03d}-{wrapper.get('sequence_id')}-{wrapper.get('role')}"
            or not isinstance(inventory, list)
            or receipt.get("inventory_entries_removed") != len(inventory or [])
            or receipt.get("inventory_sha256")
            != hashlib.sha256(compact(inventory).encode()).hexdigest()
            or receipt.get("row_root_absent") is not True
            or receipt.get("parent_directory_fsynced") is not True
            or receipt.get("active_row_roots_after_cleanup") != 0
        ):
            failures.append(f"{label}:arm-cleanup-receipt")
        receipts.append(receipt)
    if len(records) != 1 or not isinstance(records[0], dict):
        failures.append(f"work-root-lifecycle-cardinality:{len(records)}")
        return
    value = records[0]
    try:
        artifact_bytes = pathlib.Path(artifact_path).read_bytes()
        artifact = json.loads(artifact_bytes)
    except (OSError, json.JSONDecodeError):
        artifact = None
        artifact_bytes = b""
        failures.append("work-root-lifecycle-artifact")
    if artifact != value or artifact_bytes != (compact(value) + "\n").encode():
        failures.append("work-root-lifecycle-artifact")
    if (
        set(value) != WORK_ROOT_FIELDS
        or value.get("schema") != WORK_ROOT_SCHEMA
        or value.get("status") != "PASS"
        or value.get("lifecycle_scope") != "ImmediatePerArmPostEvidenceCleanup"
        or value.get("started_row_roots") != len(operations)
        or value.get("cleaned_row_roots") != len(operations)
        or value.get("max_active_row_roots") != 1
        or value.get("active_row_roots_terminal") != 0
        or value.get("cleanup_failures") != 0
        or value.get("receipts") != receipts
    ):
        failures.append("work-root-lifecycle-hard")


def validate_prearm_initialization(
    initialization,
    artifact_bytes,
    evidence_bytes,
    dry,
    input_manifest,
    input_manifest_sha256,
    failures,
):
    manifest = {
        row["input_relative_path"]: {
            "bytes": int(row["bytes"]),
            "sha256": row["sha256"],
        }
        for row in input_manifest
    }
    try:
        evidence = json.loads(evidence_bytes)
    except (UnicodeDecodeError, json.JSONDecodeError):
        evidence = None
    evidence_fields = (
        "classification", "master", "database", "database_manifest",
        "input_manifest_sha256", "plan_sha256", "action_counts",
        "rooted_state", "physical_allocation_observation",
    )
    database = initialization.get("database") if isinstance(initialization, dict) else None
    state = initialization.get("rooted_state", {}) if isinstance(initialization, dict) else {}
    physical = (
        initialization.get("physical_allocation_observation", {})
        if isinstance(initialization, dict)
        else {}
    )
    rootpages = physical.get("sqlite_schema_rootpages")
    dry_wrapper = dry.get("wrapper_calibration", {}) if isinstance(dry, dict) else {}
    page_size = physical.get("sqlite_page_size", 0)
    if (
        not isinstance(initialization, dict)
        or not isinstance(evidence, dict)
        or evidence.get("schema")
        != "phase4-g5-1-prearm-wrapper-initialization-evidence-v17"
        or any(evidence.get(name) != initialization.get(name) for name in evidence_fields)
        or artifact_bytes != (compact(initialization) + "\n").encode()
        or hashlib.sha256(evidence_bytes).hexdigest()
        != initialization.get("evidence_sha256")
        or initialization.get("status") != "PASS"
        or initialization.get("classification")
        != "OneTimeRunnerSQLiteInitializationNotOperationObservation"
        or initialization.get("chronology") != "AfterLockAndFrozenCustodyBeforeOrdinal1"
        or initialization.get("within_dry_initialization_bound") is not True
        or type(initialization.get("total_ns")) is not int
        or initialization.get("elapsed_ns") != initialization.get("total_ns")
        or not 0 < initialization.get("query_ns", 0) <= initialization["total_ns"]
        or initialization["total_ns"]
        > initialization.get("dry_initialization_bound_ns", 0)
        or initialization.get("dry_initialization_bound_ns")
        != dry_wrapper.get("initialization_bound_ns")
        or initialization.get("plan_sha256") != dry_wrapper.get("plan_sha256")
        or initialization.get("input_manifest_sha256") != input_manifest_sha256
        or initialization.get("input_manifest_sha256")
        != dry.get("input_manifest_sha256")
        or database not in manifest
        or initialization.get("database_manifest") != manifest.get(database)
        or initialization.get("product_children_started") != 0
        or initialization.get("product_rows") != 0
        or initialization.get("stores_opened") != 0
        or initialization.get("lock_owned") is not True
        or initialization.get("action_counts") != PREARM_INITIALIZATION_ACTIONS
        or initialization.get("terminal_artifact_write_classification")
        != "OutsideInitializationBoundInsideCompleteWallFinalization"
        or initialization.get("terminal_artifact_file_fsync_calls") != 1
        or initialization.get("terminal_artifact_directory_fsync_calls") != 1
        or set(state) != ROOTED_STATE_FIELDS
        or state.get("schema") != ROOTED_STATE_SCHEMA
        or state.get("semantics") != "CalibrationConstantRowShapeNotProductAuthority"
        or state.get("query_only") is not True
        or state.get("autocommit") is not True
        or state.get("head_receipt_bytes") != 216
        or state.get("head_receipt_semantics")
        != "CalibrationOpaqueHeadReceiptHashNotClosureOrFreshness"
        or state.get("closure_provenance") != "CalibrationShapeOnlyNoProductParity"
        or state.get("reachable_published_result_parity")
        != "NotClaimedCalibrationShapeOnly"
        or state.get("all_object_table_catalog_parity")
        != "NotClaimedSeparateFutureAllRowCasAudit"
        or state.get("rollback_freshness") != "NotProtected"
        or set(physical) != PHYSICAL_FIELDS
        or physical.get("schema") != PHYSICAL_SCHEMA
        or physical.get("classification") != "NotLogicalParity"
        or not 512 <= page_size <= 65_536
        or page_size & (page_size - 1) != 0
        or physical.get("sqlite_allocated_bytes")
        != page_size * physical.get("sqlite_page_count", -1)
        or physical.get("sqlite_freelist_bytes")
        != page_size * physical.get("sqlite_freelist_count", -1)
        or not isinstance(rootpages, list)
        or len(rootpages) != 3
    ):
        failures.append("prearm-wrapper-initialization")


def analyze(raw_path, timing_path, schedule_path, expected_path):
    raw = [json.loads(line) for line in pathlib.Path(raw_path).read_text().splitlines() if line]
    operations = [row for row in raw if row.get("schema") == OP_SCHEMA]
    terminals = [row for row in raw if row.get("schema") == TERMINAL_SCHEMA]
    semantic_rows = [row for row in raw if row.get("schema") == "phase4-g5-trusted-semantic-v10"]
    semantic_terminals = [
        row for row in raw if row.get("schema") == "phase4-g5-trusted-semantic-terminal-v10"
    ]
    sentinel_rows = [row for row in raw if row.get("schema") == "phase4-g5-1-protected-sentinel-v17"]
    lifecycle_rows = [row for row in raw if row.get("schema") == LIFECYCLE_SCHEMA]
    work_root_rows = [row for row in raw if row.get("schema") == WORK_ROOT_SCHEMA]
    initialization_rows = [
        row
        for row in raw
        if row.get("schema") == "phase4-g5-1-prearm-wrapper-initialization-v17"
    ]
    timings = read_tsv(timing_path)
    schedule = read_tsv(schedule_path)
    expected_ids = {row["expectation_id"] for row in read_tsv(expected_path)}
    input_manifest = read_tsv(INPUT_MANIFEST)
    input_manifest_sha256 = hashlib.sha256(INPUT_MANIFEST.read_bytes()).hexdigest()
    failures = []

    initialization = initialization_rows[0] if len(initialization_rows) == 1 else None
    initialization_artifact = pathlib.Path(raw_path).parent / (
        "PREARM-WRAPPER-INITIALIZATION-v17.json"
    )
    try:
        initialization_artifact_bytes = initialization_artifact.read_bytes()
    except OSError:
        initialization_artifact_bytes = b""
    evidence_artifact = pathlib.Path(raw_path).parent / (
        "PREARM-WRAPPER-INITIALIZATION-EVIDENCE-v17.json"
    )
    try:
        evidence_artifact_bytes = evidence_artifact.read_bytes()
        dry = json.loads(DRY_RUN.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        evidence_artifact_bytes = b""
        dry = {}
    validate_prearm_initialization(
        initialization,
        initialization_artifact_bytes,
        evidence_artifact_bytes,
        dry,
        input_manifest,
        input_manifest_sha256,
        failures,
    )

    if not operations:
        failures.append("no-operation-rows")
    wrappers = [row.get("wrapper", {}) for row in operations]
    campaign = wrappers[0].get("campaign") if wrappers else None
    expected_count = 200 if campaign == "gate" else None
    if expected_count is not None and len(operations) != expected_count:
        failures.append(f"gate-row-count:{len(operations)}")
    if len(timings) != len(operations):
        failures.append("timing-row-count")
    if [int(row.get("ordinal", -1)) for row in wrappers] != list(range(1, len(operations) + 1)):
        failures.append("operation-order")
    pairs_by_sequence = {row["sequence_id"]: int(row["pairs"]) for row in schedule}
    observed_shape = [
        (
            meta.get("ordinal"), meta.get("sequence_id"), meta.get("comparison"),
            meta.get("operation"), meta.get("pair"), meta.get("role"), meta.get("mode"),
            meta.get("size_bytes"), meta.get("expectation_id"),
        )
        for meta in wrappers
    ]
    if campaign == "gate":
        if observed_shape != expected_gate_shape(schedule):
            failures.append("gate-exact-schedule-shape")
    elif campaign == "screen" and observed_shape != expected_screen_shape(schedule):
        failures.append("screen-exact-schedule-shape")
    if campaign == "gate":
        for comparison, control_role, candidate_role in (
            ("g4-verified-vs-g5-verified", "g4_verified", "g5_verified"),
            ("g5-verified-vs-g5-trusted", "g5_verified", "g5_trusted"),
        ):
            secondary = [
                row for row in operations
                if row.get("wrapper", {}).get("comparison") == comparison
                and row.get("wrapper", {}).get("operation") != "first-edit-after-reopen"
            ]
            first_roles = []
            for row in secondary:
                meta = row.get("wrapper", {})
                key = (meta.get("sequence_id"), meta.get("pair"))
                if not first_roles or first_roles[-1][0] != key:
                    first_roles.append((key, meta.get("role"), meta.get("operation"), meta.get("pair")))
            exact_order = all(
                role
                == (
                    candidate_role
                    if ((pair % 2 == 0) != (operation in SECONDARY_BA_OPERATIONS))
                    else control_role
                )
                for _, role, operation, pair in first_roles
            )
            role_counts = {role: sum(item[1] == role for item in first_roles) for role in (control_role, candidate_role)}
            if len(first_roles) != 30 or role_counts != {control_role: 15, candidate_role: 15} or not exact_order:
                failures.append(f"{comparison}:secondary-order-balance")

    timing_by_ordinal = {int(row["ordinal"]): row for row in timings}
    maximum_rss = 0
    checkpoint_rows = 0
    for row in operations:
        wrapper = row.get("wrapper", {})
        label = f"{wrapper.get('sequence_id')}:{wrapper.get('pair')}:{wrapper.get('role')}"
        if row.get("status") != "PASS":
            failures.append(f"{label}:status")
        timers = row.get("timers_ns", {})
        if any(not isinstance(timers.get(name), int) or timers[name] < 0 for name in TIMER_FIELDS):
            failures.append(f"{label}:timers")
        elif sum(timers[name] for name in TIMER_FIELDS) != row.get("total_ns"):
            failures.append(f"{label}:timer-equation")
        validate_comparison_interval(row, wrapper, timers, failures, label)
        timing = timing_by_ordinal.get(int(wrapper.get("ordinal", -1)))
        if (
            timing is None
            or int(timing.get("total_ns", -1)) != row.get("total_ns")
            or int(timing.get("decision_ns", -1)) != row.get("decision_ns")
            or int(timing.get("comparison_interval_ns", -1)) != row.get("comparison_interval_ns")
            or timing.get("comparison_interval_classification")
            != row.get("comparison_interval_classification")
        ):
            failures.append(f"{label}:timing-sidecar")
        product = row.get("product", {})
        q_current = required(product, "q_current", failures, label)
        transactions = required(product, "transactions", failures, label)
        commits = required(product, "commits", failures, label)
        root = required(product, "root_id", failures, label)
        transition = required(product, "transition_id", failures, label)
        authority = wrapper.get("post_authority_sha256")
        expectations = wrapper.get("post_expectations_sha256")
        work = wrapper.get("mutation_work_sha256")
        if q_current not in (None, 0):
            failures.append(f"{label}:terminal-q")
        resource_fields = (
            "q_high_water", "q_report_output_bytes", "max_single_buffer_bytes",
            "buffer_evidence_complete", "full_file_buffer_bytes", *COMMON_PARITY_FIELDS,
        )
        if any(name not in product for name in resource_fields):
            failures.append(f"{label}:resource-fields")
        elif (
            type(product["q_high_water"]) is not int or product["q_high_water"] <= 0
            or type(product["q_report_output_bytes"]) is not int or product["q_report_output_bytes"] <= 0
            or type(product["max_single_buffer_bytes"]) is not int
            or not 0 <= product["max_single_buffer_bytes"] <= 1_048_576
            or product["buffer_evidence_complete"] is not True
            or product["full_file_buffer_bytes"] != 0
        ):
            failures.append(f"{label}:q-buffer-evidence")
        if transactions not in (None, 1) or commits not in (None, 1):
            failures.append(f"{label}:transaction-commit")
        if (
            root is None or transition is None or not is_sha256(authority)
            or not is_sha256(expectations) or work is None
        ):
            failures.append(f"{label}:post-state-custody")
        validate_clone(wrapper, input_manifest_sha256, input_manifest, failures, label)
        validate_rooted_state(wrapper, product, failures, label)
        pre_dispatch = wrapper.get("pre_dispatch_custody", {})
        if (
            authority != pre_dispatch.get("authority_sha256")
            or expectations != pre_dispatch.get("expectations_sha256")
        ):
            failures.append(f"{label}:sidecar-custody")
        final_pair = pairs_by_sequence.get(wrapper.get("sequence_id"))
        expected_checkpoint = campaign == "screen" or (
            campaign == "gate" and wrapper.get("pair") in (1, final_pair)
        )
        expected_scope = "CompleteRoundTrip" if expected_checkpoint else "CaptureOnly"
        if (
            wrapper.get("fixed_checkpoint") is not expected_checkpoint
            or wrapper.get("validation_scope") != expected_scope
        ):
            failures.append(f"{label}:checkpoint-selection")
        if expected_checkpoint:
            checkpoint_rows += 1
            if (
                type(product.get("fresh_reopen_head_wall_ns")) is not int
                or product["fresh_reopen_head_wall_ns"] <= 0
                or type(product.get("fresh_full_scrub_wall_ns")) is not int
                or product["fresh_full_scrub_wall_ns"] <= 0
                or type(product.get("reconstruction_wall_ns")) is not int
                or product["reconstruction_wall_ns"] <= 0
                or type(product.get("complete_lifecycle_total_wall_ns")) is not int
                or product["complete_lifecycle_total_wall_ns"] <= 0
                or not lifecycle_equation_valid(wrapper.get("role"), product)
            ):
                failures.append(f"{label}:complete-roundtrip-evidence")
        elif (
            product.get("fresh_full_scrub_wall_ns") != 0
            or product.get("reconstruction_wall_ns") != 0
        ):
            failures.append(f"{label}:capture-only-evidence")
        allowed = wrapper.get("allowed_inventory")
        inventory = wrapper.get("post_inventory")
        if not isinstance(allowed, list) or not isinstance(inventory, list):
            failures.append(f"{label}:inventory-missing")
        else:
            allowed_types = {(entry.get("path"), entry.get("kind")) for entry in allowed}
            actual_types = {(entry.get("path"), entry.get("kind")) for entry in inventory}
            allowed_sizes = {entry.get("path"): entry.get("bytes") for entry in allowed}
            immutable_sizes = all(
                entry.get("kind") != "file"
                or str(entry.get("path", "")).endswith(".sqlite")
                or entry.get("bytes") == allowed_sizes.get(entry.get("path"))
                for entry in inventory
            )
            if allowed_types != actual_types or not immutable_sizes or wrapper.get("inventory_residue") != []:
                failures.append(f"{label}:inventory-residue")
        mode = wrapper.get("mode")
        g4 = wrapper.get("role") == "g4_verified"
        if g4:
            fixed = wrapper.get("operation") in ("same-middle", "plus1-early", "plus1-middle")
            expected_environment = {
                "LAYERFS_FIXED_RADIX_ACCEPTANCE" if fixed else "LAYERFS_FAST_LANE": "1",
                "WP4M_EXECUTABLE_SHA256": S07_G4_SHA256,
                "WP4M_BASE_COPY_METHOD": (
                    "fixed-radix-acceptance-master-copy" if fixed else "fast-lane-isolated-prepared-row"
                ),
                "WP4M_BASE_DATABASE_SHA256": pre_dispatch.get("database_sha256"),
                "WP4M_BASE_AUTHORITY_SHA256": pre_dispatch.get("authority_sha256"),
                "WP4M_BASE_EXPECTATIONS_SHA256": pre_dispatch.get("expectations_sha256"),
            }
            if (
                row.get("command_environment") != expected_environment
                or product.get("executable_sha256") != S07_G4_SHA256
                or product.get("pre_edit_database_sha256") != pre_dispatch.get("database_sha256")
                or product.get("pre_edit_authority_sha256") != pre_dispatch.get("authority_sha256")
                or product.get("pre_edit_expectations_sha256") != pre_dispatch.get("expectations_sha256")
            ):
                failures.append(f"{label}:g4-pre-dispatch-environment")
        scrub_calls = None if g4 else required(product, "edit_base_complete_scrub_calls", failures, label)
        scrub_bytes = None if g4 else required(product, "edit_base_complete_scrub_canonical_bytes", failures, label)
        carry = None if g4 else required(product, "verified_carry_forward", failures, label)
        if mode == "trusted-local-dev":
            if scrub_calls not in (None, 0) or scrub_bytes not in (None, 0) or carry not in (None, False):
                failures.append(f"{label}:trusted-authority")
            if product.get("edit_base_provenance") != "trusted-local-unverified-closure":
                failures.append(f"{label}:trusted-provenance")
            if product.get("canonical_bytes_authenticated", 0) <= 0 or product.get("objects_authenticated", 0) <= 0:
                failures.append(f"{label}:trusted-touched-authentication")
            trusted_counters = (
                product.get("trusted_assumed_equal_edges"),
                product.get("trusted_assumed_prior_references"),
                product.get("trusted_assumed_prior_raw_bytes"),
            )
            if (
                product.get("covered_equal_edges") != 0
                or any(type(value) is not int or value < 0 for value in trusted_counters)
                or sum(trusted_counters) <= 0
            ):
                failures.append(f"{label}:trusted-authority-laundering")
        elif not g4 and (scrub_calls is not None and scrub_calls <= 0 or scrub_bytes is not None and scrub_bytes <= 0):
            failures.append(f"{label}:verified-scrub")
        if not g4 and mode != "trusted-local-dev" and any(
            product.get(name) != 0
            for name in (
                "trusted_assumed_equal_edges", "trusted_assumed_prior_references",
                "trusted_assumed_prior_raw_bytes",
            )
        ):
            failures.append(f"{label}:verified-trusted-assumptions")
        external = row.get("external_time")
        if isinstance(external, dict):
            maximum_rss = max(maximum_rss, int(external.get("maximum_resident_set_size", 0)))

    if campaign == "gate" and checkpoint_rows != 56:
        failures.append(f"checkpoint-cardinality:{checkpoint_rows}")
    if campaign == "screen" and checkpoint_rows != 8:
        failures.append(f"screen-checkpoint-cardinality:{checkpoint_rows}")

    for terminal in terminals:
        if terminal.get("status") != "PASS" or terminal.get("q_current") != 0:
            failures.append(f"terminal:{terminal.get('role')}:q-status")
        if (
            type(terminal.get("rows")) is not int
            or terminal.get("rows") <= 0
            or terminal.get("rows") != terminal.get("expected_rows")
        ):
            failures.append(f"terminal:{terminal.get('role')}:cardinality")
        for name in ("argument_owners", "request_owners", "schedule_owners", "timing_owners", "report_owners"):
            if terminal.get(name) != 0:
                failures.append(f"terminal:{terminal.get('role')}:{name}")
        external = terminal.get("external_time", {})
        maximum_rss = max(maximum_rss, int(external.get("maximum_resident_set_size", 0)))
        if (
            terminal.get("rss_classification") != RSS_CLASSIFICATION
            or terminal.get("rss_limit_bytes_per_product_child") != RSS_LIMIT
            or terminal.get("aggregate_product_children_rss_claim") != "NotClaimed"
        ):
            failures.append(f"terminal:{terminal.get('role')}:rss-classification")
    validate_child_lifecycle(
        lifecycle_rows,
        operations,
        terminals,
        semantic_terminals,
        sentinel_rows,
        campaign,
        pathlib.Path(raw_path).parent / "PRODUCT-CHILD-LIFECYCLE-v17.json",
        failures,
    )
    validate_work_root_lifecycle(
        work_root_rows,
        operations,
        pathlib.Path(raw_path).parent / "WORK-ROOT-LIFECYCLE-v17.json",
        failures,
    )
    if campaign == "screen":
        counts = {}
        for row in semantic_rows:
            counts[row.get("case")] = counts.get(row.get("case"), 0) + 1
            if (
                row.get("status") != "PASS" or row.get("cleanup_ok") is not True
                or row.get("residue") is not False or row.get("q_current") != 0
                or type(row.get("q_high_water")) is not int or row.get("q_high_water") <= 0
            ):
                failures.append(f"semantic:{row.get('case')}:{row.get('integrity_mode')}:hard")
        if counts != {
            "touched-error-matrix": 8,
            "unrelated-corruption": 2,
            "trusted-verified-reopen": 1,
            "reconciliation": 5,
        }:
            failures.append(f"semantic-cardinality:{counts}")
        for row in semantic_rows:
            case, mode = row.get("case"), row.get("integrity_mode")
            matrix_fields = ("fault_case", "error_class", "failure_boundary")
            if any(name not in row for name in matrix_fields):
                failures.append(f"semantic:{case}:{mode}:matrix-fields")
            if case != "touched-error-matrix" and any(row.get(name) is not None for name in matrix_fields):
                failures.append(f"semantic:{case}:{mode}:unexpected-matrix-fields")
            if case == "unrelated-corruption":
                if mode == "verified" and ("IdentityMismatch" not in str(row.get("error")) or row.get("commits") != 0):
                    failures.append("semantic:unrelated:verified")
                if mode == "trusted-local-dev" and (row.get("error") is not None or row.get("commits") != 1 or "IdentityMismatch" not in str(row.get("later_snapshot_error"))):
                    failures.append("semantic:unrelated:trusted")
            if case == "trusted-verified-reopen" and (
                row.get("commits") != 1
                or row.get("verified_carry_forward") is not False
                or row.get("verified_reopen_complete_scrub_calls", 0) <= 0
                or row.get("verified_reopen_complete_scrub_canonical_bytes", 0) <= 0
            ):
                failures.append("semantic:trusted-verified-reopen")
        matrix_rows = [row for row in semantic_rows if row.get("case") == "touched-error-matrix"]
        matrix = {(row.get("integrity_mode"), row.get("fault_case")): row for row in matrix_rows}
        expected_matrix = {
            (mode, fault_case)
            for mode in ("verified", "trusted-local-dev")
            for fault_case in TOUCHED_ERRORS
        }
        if (
            len(matrix_rows) != 8
            or set(matrix) != expected_matrix
            or any(
                row.get("error_class") != TOUCHED_ERRORS[fault_case]
                or row.get("failure_boundary") != "PreCommit"
                or not semantic_error_matches(fault_case, row.get("error"))
                or row.get("transactions") != 1
                or row.get("commits") != 0
                or row.get("publication_status") is not None
                or row.get("reconciliation") != "NotAttempted"
                or row.get("verified_carry_forward") is not False
                or row.get("head_unchanged") is not True
                or row.get("cleanup_ok") is not True
                or row.get("residue") is not False
                or row.get("q_current") != 0
                for (mode, fault_case), row in matrix.items()
            )
        ):
            failures.append("semantic:touched-error-matrix")
        expected_reconciliation = {
            "rollback": "NotAttempted", "prior": "PriorVisible",
            "requested": "RequestedVisible", "different": "DifferentHead",
            "ambiguous": "Ambiguous",
        }
        reconciliation_rows = [row for row in semantic_rows if row.get("case") == "reconciliation"]
        reconciliation_by_label = {row.get("integrity_mode"): row for row in reconciliation_rows}
        if (
            len(reconciliation_rows) != 5
            or set(reconciliation_by_label) != set(expected_reconciliation)
            or any(
                reconciliation_by_label[label].get("reconciliation") != outcome
                or reconciliation_by_label[label].get("verified_carry_forward") is not False
                for label, outcome in expected_reconciliation.items()
            )
        ):
            failures.append("semantic:reconciliation-set")
        if len(semantic_terminals) != 4 or any(row.get("status") != "PASS" or row.get("q_current") != 0 for row in semantic_terminals):
            failures.append("semantic-terminal")
        for terminal in semantic_terminals:
            maximum_rss = max(maximum_rss, int(terminal.get("external_time", {}).get("maximum_resident_set_size", 0)))
        if {row.get("route") for row in sentinel_rows} != {"full-create", "range"} or len(sentinel_rows) != 2:
            failures.append("S07:cardinality")
        for row in sentinel_rows:
            route, product = row.get("route"), row.get("product", {})
            expected = S07_FULL if route == "full-create" else S07_RANGE if route == "range" else {}
            custody = row.get("prepared_custody", {})
            expected_env = {
                "LAYERFS_FAST_LANE": "1",
                "WP4M_EXECUTABLE_SHA256": S07_G4_SHA256,
                "WP4M_BASE_COPY_METHOD": "fast-lane-isolated-prepared-row",
                "WP4M_BASE_DATABASE_SHA256": custody.get("database_sha256"),
                "WP4M_BASE_AUTHORITY_SHA256": custody.get("authority_sha256"),
                "WP4M_BASE_EXPECTATIONS_SHA256": custody.get("expectations_sha256"),
            }
            expected_work = {name: expected.get(name) for name in S07_WORK_FIELDS}
            expected_work.update(root_id=expected.get("root_id"), transition_id=expected.get("transition_id"))
            fixture, prepare, command = (
                row.get("fixture_command"), row.get("prepare_command"), row.get("row_command")
            )
            commands_exact = (
                isinstance(fixture, list) and len(fixture) == 4
                and fixture[1] == "--fast-fixture" and fixture[2].endswith("/s07-fixture-probe")
                and fixture[3] == "1048576"
                and isinstance(prepare, list) and len(prepare) == 6 and prepare[0] == fixture[0]
                and prepare[1] == "--fast-prepare" and prepare[3] == "1048576"
                and prepare[4:] == (["write", "0"] if route == "full-create" else ["read-range-1m", "0"])
                and isinstance(command, list) and len(command) == 8 and command[0] == fixture[0]
                and command[1] == "--fast-row" and command[2] == prepare[2]
                and command[3] == "1048576"
                and command[4:] == (["write", "0", "false", "complete-roundtrip"] if route == "full-create" else ["read-range-1m", "0", "false", "complete-roundtrip"])
            )
            product_exact = bool(expected) and all(product.get(key) == value for key, value in expected.items())
            ranges = product.get("range_measurements")
            range_exact = (
                isinstance(ranges, list)
                and len(ranges) == 1
                and all(ranges[0].get(key) == wanted for key, wanted in S07_RANGE_MEASUREMENT.items())
                and type(ranges[0].get("wall_ns")) is int
                and ranges[0]["wall_ns"] >= 0
                and type(ranges[0].get("throughput_mib_s")) in (int, float)
                and ranges[0]["throughput_mib_s"] > 0
                and row.get("deterministic_range") == S07_RANGE_MEASUREMENT
                if route == "range" else (
                    row.get("deterministic_range") is None
                    and isinstance(ranges, list)
                    and len(ranges) == len(S07_FULL_RANGE_SHAPES)
                    and all(
                        all(observed.get(key) == wanted for key, wanted in expected_shape.items())
                        and type(observed.get("wall_ns")) is int
                        and observed["wall_ns"] >= 0
                        for observed, expected_shape in zip(ranges, S07_FULL_RANGE_SHAPES)
                    )
                )
            )
            work_hash = hashlib.sha256(compact(expected_work).encode()).hexdigest()
            hashes_bound = (
                product.get("executable_sha256") == S07_G4_SHA256
                and product.get("base_copy_method") == S07_PRODUCT_BASE_COPY_METHOD
                and product.get("pre_edit_database_sha256") == custody.get("database_sha256")
                and product.get("pre_edit_authority_sha256") == custody.get("authority_sha256")
                and product.get("pre_edit_expectations_sha256") == custody.get("expectations_sha256")
                and row.get("post_authority_sha256") == custody.get("authority_sha256")
                and row.get("post_expectations_sha256") == custody.get("expectations_sha256")
                and "post_database_sha256" not in row
            )
            command_times = row.get("command_external_times", {})
            command_rss = [
                value.get("maximum_resident_set_size")
                for value in command_times.values()
                if isinstance(value, dict)
            ]
            s07_inventory = row.get("allowed_inventory")
            s07_post_inventory = row.get("post_inventory")
            s07_allowed_sizes = {
                item.get("path"): item.get("bytes") for item in s07_inventory
            } if isinstance(s07_inventory, list) else {}
            inventory_exact = (
                isinstance(s07_inventory, list)
                and isinstance(s07_post_inventory, list)
                and {(item.get("path"), item.get("kind")) for item in s07_inventory}
                == {(item.get("path"), item.get("kind")) for item in s07_post_inventory}
                and all(
                    item.get("kind") != "file"
                    or str(item.get("path", "")).endswith(".sqlite")
                    or item.get("bytes") == s07_allowed_sizes.get(item.get("path"))
                    for item in s07_post_inventory
                )
                and row.get("inventory_residue") == []
            )
            resource_fields = (
                "q_high_water", "q_report_output_bytes", "max_single_buffer_bytes",
                "buffer_evidence_complete", "full_file_buffer_bytes", *COMMON_PARITY_FIELDS,
            )
            resource_exact = all(name in product for name in resource_fields) and all((
                type(product.get("q_high_water")) is int and product["q_high_water"] > 0,
                type(product.get("q_report_output_bytes")) is int and product["q_report_output_bytes"] > 0,
                type(product.get("max_single_buffer_bytes")) is int
                and 0 <= product["max_single_buffer_bytes"] <= 1_048_576,
                product.get("buffer_evidence_complete") is True,
                product.get("full_file_buffer_bytes") == 0,
            ))
            validate_rooted_state(row, product, failures, f"S07:{route}")
            if not all((
                row.get("status") == "PASS", row.get("sequence_id") == "S07",
                row.get("executable_sha256") == S07_G4_SHA256,
                row.get("frozen_fixture_sha256") == S07_FIXTURE_SHA256,
                row.get("probe_fixture_sha256") == S07_FIXTURE_SHA256,
                row.get("post_database_hash_semantics") == ROOTED_STATE_SEMANTICS,
                set(custody) == {"database_sha256", "authority_sha256", "expectations_sha256"},
                all(isinstance(value, str) and len(value) == 64 for value in custody.values()),
                row.get("row_environment") == expected_env, commands_exact,
                row.get("pre_cleanup_residue") == [], bool(row.get("base_custody")),
                set(command_times) == {"fixture", "prepare", "row"},
                len(command_rss) == 3,
                all(type(value) is int and 0 < value <= RSS_LIMIT for value in command_rss),
                inventory_exact,
                resource_exact,
                product.get("status") == "PASS", product.get("error") is None,
                product_exact, row.get("deterministic_tuple") == expected, range_exact, hashes_bound,
                row.get("mutation_work") == expected_work,
                row.get("mutation_work_sha256") == work_hash,
            )):
                failures.append(f"S07:{route}:hard")
            maximum_rss = max(maximum_rss, *command_rss, 0)
    if maximum_rss > RSS_LIMIT:
        failures.append(f"rss:{maximum_rss}")

    grouped = {}
    for row in operations:
        wrapper = row["wrapper"]
        key = (wrapper["comparison"], wrapper["operation"])
        grouped.setdefault(key, {}).setdefault(wrapper["role"], []).append(row)
    comparison_results = {}
    for (comparison, operation), roles in sorted(grouped.items()):
        if comparison not in ("g4-verified-vs-g5-verified", "g5-verified-vs-g5-trusted"):
            continue
        control_role, candidate_role = (
            ("g4_verified", "g5_verified")
            if comparison == "g4-verified-vs-g5-verified"
            else ("g5_verified", "g5_trusted")
        )
        control = roles.get(control_role, [])
        candidate = roles.get(candidate_role, [])
        if len(control) != len(candidate):
            failures.append(f"{comparison}:{operation}:unbalanced")
            continue
        control_by_pair = {row["wrapper"]["pair"]: row for row in control}
        candidate_by_pair = {row["wrapper"]["pair"]: row for row in candidate}
        if set(control_by_pair) != set(candidate_by_pair):
            failures.append(f"{comparison}:{operation}:pair-set")
            continue
        control_ns = [
            control_by_pair[pair]["comparison_interval_ns"] for pair in sorted(control_by_pair)
        ]
        candidate_ns = [
            candidate_by_pair[pair]["comparison_interval_ns"] for pair in sorted(candidate_by_pair)
        ]
        for pair in sorted(control_by_pair):
            left = control_by_pair[pair]["product"]
            right = candidate_by_pair[pair]["product"]
            for name in ("root_id", "transition_id"):
                if left.get(name) != right.get(name):
                    failures.append(f"{comparison}:{operation}:pair-{pair}:{name}")
            for name in (
                "post_authority_sha256", "post_expectations_sha256", "mutation_work_sha256",
            ):
                if control_by_pair[pair]["wrapper"].get(name) != candidate_by_pair[pair]["wrapper"].get(name):
                    failures.append(f"{comparison}:{operation}:pair-{pair}:{name}")
            if not paired_rooted_state_equal(
                control_by_pair[pair]["wrapper"], candidate_by_pair[pair]["wrapper"]
            ):
                failures.append(f"{comparison}:{operation}:pair-{pair}:rooted-state")
            parity_fields = comparison_parity_fields(comparison)
            for name in parity_fields:
                if left.get(name) != right.get(name):
                    failures.append(f"{comparison}:{operation}:pair-{pair}:{name}")
        result = {
            "pairs": len(control_ns),
            "comparison_interval_classification": control[0].get(
                "comparison_interval_classification"
            ) if control else None,
            "control_ns": control_ns,
            "candidate_ns": candidate_ns,
            "control_sum_ns": sum(control_ns),
            "candidate_sum_ns": sum(candidate_ns),
            "control_p50_ns": int(statistics.median(control_ns)),
            "candidate_p50_ns": int(statistics.median(candidate_ns)),
            "candidate_p95_ns": percentile(candidate_ns, 95, 100),
        }
        if comparison == "g4-verified-vs-g5-verified":
            result["material_regression"] = (
                sum(candidate_ns) * 100 > sum(control_ns) * 105
                and sum(candidate_ns) - sum(control_ns) >= len(control_ns) * 1_000_000
            )
            if result["material_regression"]:
                failures.append(f"{comparison}:{operation}:material-regression")
        else:
            improvements = [
                ((control_ns[index] - candidate_ns[index]) * 10_000) // control_ns[index]
                for index in range(len(control_ns))
            ]
            result["paired_improvement_basis_points"] = improvements
            result["paired_median_improvement_basis_points"] = int(statistics.median(improvements))
            if operation == "first-edit-after-reopen":
                if result["paired_median_improvement_basis_points"] < 5_000:
                    failures.append("same-g5:first-edit-after-reopen:improvement")
                if result["candidate_p50_ns"] > 15_000_000 or result["candidate_p95_ns"] > 25_000_000:
                    failures.append("same-g5:first-edit-after-reopen:latency")
            if operation in ("plus1-early", "plus1-middle") and result["candidate_p50_ns"] > 15_000_000:
                failures.append(f"same-g5:{operation}:latency")
        comparison_results.setdefault(comparison, {})[operation] = result

    scheduled_ids = {row["expectation_id"] for row in schedule}
    if not scheduled_ids.issubset(expected_ids):
        failures.append("schedule-expectation-custody")
    normalized = {
        "campaign": campaign,
        "operation_rows": len(operations),
        "terminal_rows": len(terminals) + len(semantic_terminals),
        "prearm_wrapper_initialization": initialization,
        "product_child_lifecycle": lifecycle_rows[0] if len(lifecycle_rows) == 1 else None,
        "work_root_lifecycle": work_root_rows[0] if len(work_root_rows) == 1 else None,
        "gate_custody_results": sorted(
            (
                {
                    "ordinal": row.get("wrapper", {}).get("ordinal"),
                    "sequence_id": row.get("wrapper", {}).get("sequence_id"),
                    "pair": row.get("wrapper", {}).get("pair"),
                    "role": row.get("wrapper", {}).get("role"),
                    "validation_scope": row.get("wrapper", {}).get("validation_scope"),
                    "fixed_checkpoint": row.get("wrapper", {}).get("fixed_checkpoint"),
                    "arm_cleanup_receipt": row.get("wrapper", {}).get(
                        "arm_cleanup_receipt"
                    ),
                    "comparison_interval_ns": row.get("comparison_interval_ns"),
                    "comparison_interval_classification": row.get(
                        "comparison_interval_classification"
                    ),
                    "comparison_interval_components": row.get(
                        "comparison_interval_components"
                    ),
                    "comparison_intervals_ns": row.get("comparison_intervals_ns"),
                    "comparison_interval_classifications": row.get(
                        "comparison_interval_classifications"
                    ),
                    "clone_receipt": row.get("wrapper", {}).get("clone_receipt"),
                    "pre_dispatch_custody": row.get("wrapper", {}).get("pre_dispatch_custody"),
                    "rooted_logical_state": row.get("wrapper", {}).get("rooted_logical_state"),
                    "physical_allocation_observation": row.get("wrapper", {}).get(
                        "physical_allocation_observation"
                    ),
                    "post_authority_sha256": row.get("wrapper", {}).get("post_authority_sha256"),
                    "post_expectations_sha256": row.get("wrapper", {}).get("post_expectations_sha256"),
                    "mutation_work_sha256": row.get("wrapper", {}).get("mutation_work_sha256"),
                    "inventory_residue": row.get("wrapper", {}).get("inventory_residue"),
                }
                for row in operations
                if row.get("wrapper", {}).get("campaign") == "gate"
            ),
            key=compact,
        ),
        "semantic_results": sorted(
            (
                {
                    key: row.get(key)
                    for key in (
                        "case", "integrity_mode", "fault_case", "error_class",
                        "failure_boundary", "error", "later_snapshot_error",
                        "publication_status", "reconciliation", "head_unchanged", "transactions",
                        "commits", "edit_base_complete_scrub_calls",
                        "edit_base_complete_scrub_canonical_bytes",
                        "verified_reopen_complete_scrub_calls",
                        "verified_reopen_complete_scrub_canonical_bytes",
                        "verified_carry_forward", "cleanup_ok", "residue", "q_high_water", "q_current",
                    )
                }
                for row in semantic_rows
            ),
            key=compact,
        ),
        "protected_sentinel_results": sorted(
            (
                {
                    "route": row.get("route"),
                    "frozen_fixture_sha256": row.get("frozen_fixture_sha256"),
                    "probe_fixture_sha256": row.get("probe_fixture_sha256"),
                    "prepared_custody": row.get("prepared_custody"),
                    "row_environment": row.get("row_environment"),
                    "fixture_command": row.get("fixture_command"),
                    "prepare_command": row.get("prepare_command"),
                    "row_command": row.get("row_command"),
                    "command_external_times": row.get("command_external_times"),
                    "pre_cleanup_residue": row.get("pre_cleanup_residue"),
                    "allowed_inventory": row.get("allowed_inventory"),
                    "post_inventory": row.get("post_inventory"),
                    "inventory_residue": row.get("inventory_residue"),
                    "deterministic_tuple": row.get("deterministic_tuple"),
                    "deterministic_range": row.get("deterministic_range"),
                    "product_tuple": {
                        key: row.get("product", {}).get(key)
                        for key in (S07_FULL if row.get("route") == "full-create" else S07_RANGE)
                    },
                    "range_measurements": row.get("product", {}).get("range_measurements"),
                    "product_pre_edit_database_sha256": row.get("product", {}).get("pre_edit_database_sha256"),
                    "product_pre_edit_authority_sha256": row.get("product", {}).get("pre_edit_authority_sha256"),
                    "product_pre_edit_expectations_sha256": row.get("product", {}).get("pre_edit_expectations_sha256"),
                    "post_database_hash_semantics": row.get("post_database_hash_semantics"),
                    "rooted_logical_state": row.get("rooted_logical_state"),
                    "physical_allocation_observation": row.get(
                        "physical_allocation_observation"
                    ),
                    "post_authority_sha256": row.get("post_authority_sha256"),
                    "post_expectations_sha256": row.get("post_expectations_sha256"),
                    "mutation_work": row.get("mutation_work"),
                    "mutation_work_sha256": row.get("mutation_work_sha256"),
                }
                for row in sentinel_rows
            ),
            key=compact,
        ),
        "maximum_rss_bytes": maximum_rss,
        "rss_limit_bytes": RSS_LIMIT,
        "comparisons": comparison_results,
        "hard_failures": sorted(set(failures)),
    }
    return {"schema": "phase4-g5-1-primary-analysis-v17", "status": "PASS" if not failures else "REVISE", "normalized": normalized}


def self_check():
    assert lifecycle_equation_valid("g4_verified", {"lifecycle_phase_sum_matches": False})
    assert not lifecycle_equation_valid("g5_verified", {"lifecycle_phase_sum_matches": False})
    assert S07_PRODUCT_BASE_COPY_METHOD == "regenerated-isolated-database"
    digest = "a" * 64
    product = {
        "root_id": digest,
        "transition_id": digest,
        "ordered_closure_digest": digest,
    }
    state = {
        "schema": ROOTED_STATE_SCHEMA,
        "semantics": ROOTED_STATE_SEMANTICS,
        "query_only": True,
        "autocommit": True,
        "head_generation": 1,
        "head_root_id": digest,
        "head_transition_id": digest,
        "head_receipt_bytes": 216,
        "head_receipt_sha256": digest,
        "head_receipt_semantics": "ProductAuthenticatedHeadTupleOpaqueHashNotClosureOrFreshness",
        "ordered_closure_digest": digest,
        "closure_provenance": "PreparedGoldenBoundByExactRootTransitionAndProductQualification",
        "reachable_published_result_parity": "ClaimedHardGated",
        "all_object_table_catalog_parity": "NotClaimedSeparateFutureAllRowCasAudit",
        "rollback_freshness": "NotProtected",
    }
    physical = {
        "schema": PHYSICAL_SCHEMA,
        "classification": "NotLogicalParity",
        "database_file_bytes": 4096,
        "sqlite_page_size": 4096,
        "sqlite_page_count": 1,
        "sqlite_freelist_count": 0,
        "sqlite_allocated_bytes": 4096,
        "sqlite_freelist_bytes": 0,
        "sqlite_schema_rootpages": [
            {"type": "table", "name": "t", "table_name": "t", "rootpage": 1}
        ],
    }
    base = {
        "validation_scope": "CaptureOnly",
        "rooted_logical_state": state,
        "physical_allocation_observation": physical,
        "post_database_hash_semantics": ROOTED_STATE_SEMANTICS,
    }
    failures = []
    validate_rooted_state(base, product, failures, "valid")
    assert failures == []
    mutations = {
        "all-row-claim": {**state, "all_object_table_catalog_parity": "Claimed"},
        "receipt-laundering": {**state, "head_receipt_semantics": "ClosureProof"},
        "closure-mismatch": {**state, "ordered_closure_digest": "b" * 64},
        "provenance-mismatch": {**state, "closure_provenance": "ConstructionProof"},
    }
    for name, mutated in mutations.items():
        failures = []
        validate_rooted_state(
            {**base, "rooted_logical_state": mutated}, product, failures, name
        )
        assert failures
    failures = []
    validate_rooted_state(
        {**base, "rooted_logical_state": None, "logical_catalog": {}},
        product,
        failures,
        "legacy-catalog",
    )
    assert failures
    assert not paired_rooted_state_equal(
        base,
        {
            **base,
            "rooted_logical_state": {**state, "head_receipt_sha256": "b" * 64},
        },
    )
    assert "canonical_bytes_authenticated" in comparison_parity_fields(
        "g4-verified-vs-g5-verified"
    )
    assert "canonical_bytes_authenticated" not in comparison_parity_fields(
        "g5-verified-vs-g5-trusted"
    )
    assert "canonical_new_write_bytes" in comparison_parity_fields(
        "g5-verified-vs-g5-trusted"
    )
    calibration_state = {
        **state,
        "semantics": "CalibrationConstantRowShapeNotProductAuthority",
        "head_receipt_semantics": "CalibrationOpaqueHeadReceiptHashNotClosureOrFreshness",
        "closure_provenance": "CalibrationShapeOnlyNoProductParity",
        "reachable_published_result_parity": "NotClaimedCalibrationShapeOnly",
    }
    calibration_physical = {
        **physical,
        "sqlite_schema_rootpages": [
            {"type": "table", "name": name, "table_name": name, "rootpage": index}
            for index, name in enumerate(("a", "b", "c"), 1)
        ],
    }
    database = "bases/calibration/db.sqlite"
    manifest_sha = "d" * 64
    plan_sha = "e" * 64
    manifest_rows = [{"input_relative_path": database, "bytes": "4096", "sha256": digest}]
    dry = {
        "input_manifest_sha256": manifest_sha,
        "wrapper_calibration": {"initialization_bound_ns": 1_000, "plan_sha256": plan_sha},
    }
    evidence = {
        "schema": "phase4-g5-1-prearm-wrapper-initialization-evidence-v17",
        "classification": "OneTimeRunnerSQLiteInitializationNotOperationObservation",
        "master": "calibration",
        "database": database,
        "database_manifest": {"bytes": 4096, "sha256": digest},
        "input_manifest_sha256": manifest_sha,
        "plan_sha256": plan_sha,
        "action_counts": PREARM_INITIALIZATION_ACTIONS,
        "rooted_state": calibration_state,
        "physical_allocation_observation": calibration_physical,
    }

    def prearm_problems(changes=None, evidence_changes=None):
        changed_evidence = {**evidence, **(evidence_changes or {})}
        evidence_bytes = (compact(changed_evidence) + "\n").encode()
        initialization = {
            **changed_evidence,
            "schema": "phase4-g5-1-prearm-wrapper-initialization-v17",
            "status": "PASS",
            "chronology": "AfterLockAndFrozenCustodyBeforeOrdinal1",
            "query_ns": 10,
            "total_ns": 100,
            "elapsed_ns": 100,
            "evidence_sha256": hashlib.sha256(evidence_bytes).hexdigest(),
            "dry_initialization_bound_ns": 1_000,
            "within_dry_initialization_bound": True,
            "product_children_started": 0,
            "product_rows": 0,
            "stores_opened": 0,
            "lock_owned": True,
            "terminal_artifact_write_classification": "OutsideInitializationBoundInsideCompleteWallFinalization",
            "terminal_artifact_file_fsync_calls": 1,
            "terminal_artifact_directory_fsync_calls": 1,
            **(changes or {}),
        }
        found = []
        validate_prearm_initialization(
            initialization,
            (compact(initialization) + "\n").encode(),
            evidence_bytes,
            dry,
            manifest_rows,
            manifest_sha,
            found,
        )
        return found

    assert prearm_problems() == []
    prearm_mutations = {
        "inflated-bound": ({"dry_initialization_bound_ns": 2_000}, None),
        "wrong-plan": ({"plan_sha256": "f" * 64}, {"plan_sha256": "f" * 64}),
        "wrong-input": (
            {"input_manifest_sha256": "f" * 64},
            {"input_manifest_sha256": "f" * 64},
        ),
        "wrong-database-row": (
            {"database_manifest": {"bytes": 4096, "sha256": "f" * 64}},
            {"database_manifest": {"bytes": 4096, "sha256": "f" * 64}},
        ),
    }
    for changes, evidence_changes in prearm_mutations.values():
        assert prearm_problems(changes, evidence_changes)
    print(compact({"status": "PASS", "checks": 14, "mutations_rejected": sorted(mutations) + sorted(prearm_mutations) + ["legacy-catalog", "paired-receipt-hash"]}))
    return 0


def main():
    if sys.argv[1:] == ["--self-check"]:
        return self_check()
    if len(sys.argv) != 6:
        raise SystemExit("usage: primary.py --self-check|RAW TIMINGS SCHEDULE EXPECTED OUTPUT")
    result = analyze(*map(pathlib.Path, sys.argv[1:5]))
    pathlib.Path(sys.argv[5]).write_text(compact(result) + "\n", encoding="utf-8")
    return 0 if result["status"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
