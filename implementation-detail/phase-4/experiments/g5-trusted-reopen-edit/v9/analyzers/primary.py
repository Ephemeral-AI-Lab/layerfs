#!/usr/bin/env python3
import csv
import hashlib
import json
import pathlib
import statistics
import sys


OP_SCHEMA = "phase4-g5-1-operation-v9"
TERMINAL_SCHEMA = "phase4-g5-trusted-child-terminal-v9"
RSS_LIMIT = 20_971_520
TIMER_FIELDS = (
    "store_preflight_ns", "sqlite_open_and_profile_ns", "visible_head_and_transition_ns",
    "edit_base_scope_ns", "mapping_and_construction_ns", "proof_ns",
    "publication_commit_ns", "reconciliation_ns",
)
S07_G4_SHA256 = "e72988fc25e96f608d0d405e157ea8e837029595ace916f066932082a736db33"
S07_FIXTURE_SHA256 = "4a3acf60f044bbae8ed0d0a8aa8fabd8b4cee74216dbccc36255b9c6fbe50a2a"
S07_COMMON = {
    "source_fingerprint": "f79de600cf44b20c4443e06d2e2b9e8819e956ba5a7bcc9cab4ffd8a08059cf8",
    "expected_cdc_references": 53,
    "actual_cdc_references": 53,
    "expected_cdc_sequence_fingerprint": "6a1d02f70694a50859c88c0080f0e2cc046c8b0d9e21f474c58dab66a895f1c1",
    "root_id": "84abbaa054ec67a8411674f5125b5969d0a3b12869b0ac08a1f65f39008b4026",
    "transition_id": "e923b65ef4041952bb0c92b1b375bf29d7619f7e673454f0711cd7b5a138b90c",
    "ordered_closure_digest": "f9c0e593b97e0430ec81e9ef763fa005715b465ca99001835f2acba0794a7ee2",
    "q_current": 0,
}
S07_FULL = {
    **S07_COMMON, "operation": "full", "canonical_bytes_written": 1_053_105,
    "canonical_new_write_bytes": 1_053_105, "canonical_bytes_authenticated": 1_053_105,
    "objects_created": 57, "objects_authenticated": 57, "objects_reused": 0,
    "mapping_bytes_rewritten": 3_840, "source_bytes_read": 1_048_576,
    "raw_bytes_hashed": 1_048_576, "payload_io_bytes": 1_048_576, "d_bytes": 0,
    "sqlite_pre_logical_database_bytes": 20_480,
    "sqlite_post_logical_database_bytes": 1_105_920, "transactions": 1, "commits": 1,
    "commit_dispatches": 1, "commit_returns": 1, "commit_return_successes": 1,
    "commit_return_errors": 0, "commit_reconciliation_calls": 0,
    "publication_status": "Committed",
}
S07_RANGE_MEASUREMENT = {
    "label": "sequential-1m", "start": 0, "end": 1_048_576,
    "returned_bytes": 1_048_576, "canonical_bytes_authenticated": 1_052_986,
    "objects_authenticated": 55,
}
S07_RANGE = {
    **S07_COMMON, "operation": "read-range-1m", "canonical_bytes_authenticated": 1_053_129,
    "objects_authenticated": 57, "canonical_bytes_written": 0,
    "canonical_new_write_bytes": 0, "objects_created": 0, "objects_reused": 0,
    "mapping_bytes_rewritten": 0, "payload_io_bytes": 1_048_576, "d_bytes": 1_048_576,
    "sqlite_pre_logical_database_bytes": 1_105_920,
    "sqlite_post_logical_database_bytes": 1_105_920, "transactions": 0, "commits": 0,
    "commit_dispatches": 0, "commit_returns": 0, "commit_return_successes": 0,
    "commit_return_errors": 0, "commit_reconciliation_calls": 0,
    "publication_status": "Unavailable",
}
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
SECONDARY_BA_OPERATIONS = {"one-byte-early", "one-byte-late", "plus1-middle"}
CLONE_SCHEMA = "g5-v9-native-clone-receipt-v1"
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
CATALOG_SCHEMA = "g5-v9-ordered-logical-catalog-v1"
CATALOG_FIELDS = {
    "schema", "hash_semantics", "query_only", "autocommit", "sqlite_page_size",
    "sqlite_page_count", "sqlite_freelist_count", "sqlite_logical_database_bytes",
    "sqlite_schema_sha256", "meta_row_count", "meta_sha256", "object_count",
    "canonical_length_sum", "blob_length_sum", "object_catalog_sha256",
    "head_row_count", "head_generation", "head_root_id", "head_transition_id",
    "head_receipt_bytes", "head_receipt_sha256", "logical_catalog_sha256",
}
CATALOG_HASH_FIELDS = (
    "sqlite_schema_sha256", "meta_sha256", "object_catalog_sha256",
    "head_receipt_sha256", "logical_catalog_sha256",
)
INPUT_MANIFEST = pathlib.Path(__file__).resolve().parents[1] / "method/INPUT-MANIFEST-v9.tsv"


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


def validate_catalog(wrapper, product, failures, label):
    catalog = wrapper.get("logical_catalog")
    if not isinstance(catalog, dict) or set(catalog) != CATALOG_FIELDS:
        failures.append(f"{label}:logical-catalog-shape")
        return
    integers = (
        "sqlite_page_size", "sqlite_page_count", "sqlite_freelist_count",
        "sqlite_logical_database_bytes", "meta_row_count", "object_count",
        "canonical_length_sum", "blob_length_sum", "head_row_count",
        "head_generation", "head_receipt_bytes",
    )
    if (
        catalog.get("schema") != CATALOG_SCHEMA
        or catalog.get("hash_semantics")
        != "ordered-logical-catalog-content-address-digest-not-physical-sqlite-bytes"
        or catalog.get("query_only") is not True
        or catalog.get("autocommit") is not True
        or any(type(catalog.get(name)) is not int or catalog[name] < 0 for name in integers)
        or catalog.get("sqlite_page_size", 0) < 512
        or catalog.get("sqlite_page_size", 0) > 65_536
        or catalog.get("sqlite_page_size", 0) & (catalog.get("sqlite_page_size", 0) - 1) != 0
        or catalog.get("sqlite_page_count", 0) <= 0
        or catalog.get("sqlite_freelist_count", 0) > catalog.get("sqlite_page_count", 0)
        or catalog.get("sqlite_logical_database_bytes")
        != catalog.get("sqlite_page_size") * catalog.get("sqlite_page_count")
        or catalog.get("sqlite_logical_database_bytes") != product.get("sqlite_post_logical_database_bytes")
        or catalog.get("meta_row_count") != 1
        or catalog.get("object_count", 0) <= 0
        or catalog.get("canonical_length_sum") != catalog.get("blob_length_sum")
        or catalog.get("head_row_count") != 1
        or catalog.get("head_receipt_bytes") != 216
        or catalog.get("head_root_id") != product.get("root_id")
        or catalog.get("head_transition_id") != product.get("transition_id")
        or not is_sha256(catalog.get("head_root_id"))
        or not is_sha256(catalog.get("head_transition_id"))
        or any(not is_sha256(catalog.get(name)) for name in CATALOG_HASH_FIELDS)
    ):
        failures.append(f"{label}:logical-catalog-hard")
    if "post_database_sha256" in wrapper:
        failures.append(f"{label}:physical-database-claim")
    if (
        wrapper.get("post_database_hash_semantics")
        != "ordered-logical-catalog-content-address-digest-not-physical-sqlite-bytes"
    ):
        failures.append(f"{label}:database-hash-semantics")


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


def analyze(raw_path, timing_path, schedule_path, expected_path):
    raw = [json.loads(line) for line in pathlib.Path(raw_path).read_text().splitlines() if line]
    operations = [row for row in raw if row.get("schema") == OP_SCHEMA]
    terminals = [row for row in raw if row.get("schema") == TERMINAL_SCHEMA]
    semantic_rows = [row for row in raw if row.get("schema") == "phase4-g5-trusted-semantic-v9"]
    semantic_terminals = [
        row for row in raw if row.get("schema") == "phase4-g5-trusted-semantic-terminal-v9"
    ]
    sentinel_rows = [row for row in raw if row.get("schema") == "phase4-g5-1-protected-sentinel-v9"]
    timings = read_tsv(timing_path)
    schedule = read_tsv(schedule_path)
    expected_ids = {row["expectation_id"] for row in read_tsv(expected_path)}
    input_manifest = read_tsv(INPUT_MANIFEST)
    input_manifest_sha256 = hashlib.sha256(INPUT_MANIFEST.read_bytes()).hexdigest()
    failures = []

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
        timing = timing_by_ordinal.get(int(wrapper.get("ordinal", -1)))
        if timing is None or int(timing.get("total_ns", -1)) != row.get("total_ns"):
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
        validate_catalog(wrapper, product, failures, label)
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
                or product.get("lifecycle_phase_sum_matches") is not True
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
            "touched-corruption": 2,
            "unrelated-corruption": 2,
            "trusted-verified-reopen": 1,
            "reconciliation": 5,
        }:
            failures.append(f"semantic-cardinality:{counts}")
        for row in semantic_rows:
            case, mode = row.get("case"), row.get("integrity_mode")
            if case == "touched-corruption" and ("IdentityMismatch" not in str(row.get("error")) or row.get("commits") != 0 or row.get("head_unchanged") is not True):
                failures.append(f"semantic:touched:{mode}")
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
                    and product.get("range_measurements") == []
                )
            )
            work_hash = hashlib.sha256(compact(expected_work).encode()).hexdigest()
            hashes_bound = (
                product.get("executable_sha256") == S07_G4_SHA256
                and product.get("base_copy_method") == "fast-lane-isolated-prepared-row"
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
            validate_catalog(row, product, failures, f"S07:{route}")
            if not all((
                row.get("status") == "PASS", row.get("sequence_id") == "S07",
                row.get("executable_sha256") == S07_G4_SHA256,
                row.get("frozen_fixture_sha256") == S07_FIXTURE_SHA256,
                row.get("probe_fixture_sha256") == S07_FIXTURE_SHA256,
                row.get("post_database_hash_semantics")
                == "ordered-logical-catalog-content-address-digest-not-physical-sqlite-bytes",
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
        control_ns = [control_by_pair[pair]["decision_ns"] for pair in sorted(control_by_pair)]
        candidate_ns = [candidate_by_pair[pair]["decision_ns"] for pair in sorted(candidate_by_pair)]
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
            if control_by_pair[pair]["wrapper"].get("logical_catalog") != candidate_by_pair[pair]["wrapper"].get("logical_catalog"):
                failures.append(f"{comparison}:{operation}:pair-{pair}:logical-catalog")
            parity_fields = (
                COMMON_PARITY_FIELDS
                if comparison == "g4-verified-vs-g5-verified"
                else MUTATION_PARITY_FIELDS
            )
            for name in parity_fields:
                if left.get(name) != right.get(name):
                    failures.append(f"{comparison}:{operation}:pair-{pair}:{name}")
        result = {
            "pairs": len(control_ns),
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
        "gate_custody_results": sorted(
            (
                {
                    "ordinal": row.get("wrapper", {}).get("ordinal"),
                    "sequence_id": row.get("wrapper", {}).get("sequence_id"),
                    "pair": row.get("wrapper", {}).get("pair"),
                    "role": row.get("wrapper", {}).get("role"),
                    "validation_scope": row.get("wrapper", {}).get("validation_scope"),
                    "fixed_checkpoint": row.get("wrapper", {}).get("fixed_checkpoint"),
                    "clone_receipt": row.get("wrapper", {}).get("clone_receipt"),
                    "pre_dispatch_custody": row.get("wrapper", {}).get("pre_dispatch_custody"),
                    "logical_catalog": row.get("wrapper", {}).get("logical_catalog"),
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
                        "case", "integrity_mode", "error", "later_snapshot_error",
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
                    "logical_catalog": row.get("logical_catalog"),
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
    return {"schema": "phase4-g5-1-primary-analysis-v9", "status": "PASS" if not failures else "REVISE", "normalized": normalized}


def main():
    if len(sys.argv) != 6:
        raise SystemExit("usage: primary.py RAW TIMINGS SCHEDULE EXPECTED OUTPUT")
    result = analyze(*map(pathlib.Path, sys.argv[1:5]))
    pathlib.Path(sys.argv[5]).write_text(compact(result) + "\n", encoding="utf-8")
    return 0 if result["status"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
