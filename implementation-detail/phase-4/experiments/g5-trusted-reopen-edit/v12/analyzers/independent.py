#!/usr/bin/env python3
import csv
import hashlib
import json
import pathlib
import sys


RSS_CAP = 20_971_520
PHASES = [
    "store_preflight_ns", "sqlite_open_and_profile_ns", "visible_head_and_transition_ns",
    "edit_base_scope_ns", "mapping_and_construction_ns", "proof_ns",
    "publication_commit_ns", "reconciliation_ns",
]
S07_G4_SHA256 = "e72988fc25e96f608d0d405e157ea8e837029595ace916f066932082a736db33"
S07_FIXTURE_SHA256 = "4a3acf60f044bbae8ed0d0a8aa8fabd8b4cee74216dbccc36255b9c6fbe50a2a"
S07_COMMON = dict(
    source_fingerprint="f79de600cf44b20c4443e06d2e2b9e8819e956ba5a7bcc9cab4ffd8a08059cf8",
    expected_cdc_references=53, actual_cdc_references=53,
    expected_cdc_sequence_fingerprint="6a1d02f70694a50859c88c0080f0e2cc046c8b0d9e21f474c58dab66a895f1c1",
    root_id="84abbaa054ec67a8411674f5125b5969d0a3b12869b0ac08a1f65f39008b4026",
    transition_id="e923b65ef4041952bb0c92b1b375bf29d7619f7e673454f0711cd7b5a138b90c",
    ordered_closure_digest="f9c0e593b97e0430ec81e9ef763fa005715b465ca99001835f2acba0794a7ee2",
    q_current=0,
)
S07_FULL = dict(
    S07_COMMON, operation="full", canonical_bytes_written=1_053_105,
    canonical_new_write_bytes=1_053_105, canonical_bytes_authenticated=1_053_105,
    objects_created=57, objects_authenticated=57, objects_reused=0,
    mapping_bytes_rewritten=3_840, source_bytes_read=1_048_576,
    raw_bytes_hashed=1_048_576, payload_io_bytes=1_048_576, d_bytes=0,
    sqlite_pre_logical_database_bytes=20_480,
    sqlite_post_logical_database_bytes=1_105_920, transactions=1, commits=1,
    commit_dispatches=1, commit_returns=1, commit_return_successes=1,
    commit_return_errors=0, commit_reconciliation_calls=0, publication_status="Committed",
)
S07_RANGE_MEASUREMENT = dict(
    label="sequential-1m", start=0, end=1_048_576, returned_bytes=1_048_576,
    canonical_bytes_authenticated=1_052_986, objects_authenticated=55,
)
S07_RANGE = dict(
    S07_COMMON, operation="read-range-1m", canonical_bytes_authenticated=1_053_129,
    objects_authenticated=57, canonical_bytes_written=0, canonical_new_write_bytes=0,
    objects_created=0, objects_reused=0, mapping_bytes_rewritten=0,
    payload_io_bytes=1_048_576, d_bytes=1_048_576,
    sqlite_pre_logical_database_bytes=1_105_920,
    sqlite_post_logical_database_bytes=1_105_920, transactions=0, commits=0,
    commit_dispatches=0, commit_returns=0, commit_return_successes=0,
    commit_return_errors=0, commit_reconciliation_calls=0, publication_status="Unavailable",
)
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
    if comparison == "g4-verified-vs-g5-verified":
        return COMMON_PARITY_FIELDS
    return MUTATION_PARITY_FIELDS


def paired_rooted_state_equal(left, right):
    return left.get("rooted_logical_state") == right.get("rooted_logical_state")
SECONDARY_BA_OPERATIONS = {"one-byte-early", "one-byte-late", "plus1-middle"}
COMMON_SECONDARY_PHASES = (
    "edit_base_scope_ns", "mapping_and_construction_ns", "proof_ns",
    "publication_commit_ns", "reconciliation_ns",
)
FULL_INTERVAL = "full-first-edit-equation"
COMMON_INTERVAL = "common-edit-through-reconciliation"
LIFECYCLE_SCHEMA = "phase4-g5-1-product-child-lifecycle-v12"
RSS_CLASSIFICATION = "PerProductChildRetainedPeak"
TOUCHED_CLASSES = {
    "missing-object": "MissingObject",
    "identity-mismatch": "IdentityMismatch",
    "wrong-logical-role": "WrongLogicalRole",
    "malformed-logical-record": "UnexpectedEof",
}
CLONE_SCHEMA = "g5-v12-native-clone-receipt-v1"
CLONE_TOP = frozenset((
    "schema", "method", "copy_content", "sealed_input_manifest_sha256",
    "inventory_equal", "dispatch_modes_exact", "entries",
))
CLONE_ENTRY = frozenset((
    "path", "bytes", "master_manifest_sha256", "clonefile_success",
    "source_device", "source_inode", "source_mode", "destination_device",
    "destination_inode", "clone_destination_mode", "dispatch_mode",
    "mode_transition", "same_device", "distinct_inode", "size_equal",
    "source_unchanged",
))
ROOTED_STATE_SCHEMA = "g5-v12-rooted-logical-state-v1"
ROOTED_STATE_SEMANTICS = (
    "product-authenticated-root-transition-ordered-closure-not-all-object-table"
)
ROOTED_STATE_KEYS = frozenset((
    "schema", "semantics", "query_only", "autocommit", "head_generation",
    "head_root_id", "head_transition_id", "head_receipt_bytes",
    "head_receipt_sha256", "head_receipt_semantics", "ordered_closure_digest",
    "closure_provenance", "reachable_published_result_parity",
    "all_object_table_catalog_parity", "rollback_freshness",
))
PHYSICAL_SCHEMA = "g5-v12-physical-allocation-observation-v1"
PHYSICAL_KEYS = frozenset((
    "schema", "classification", "database_file_bytes", "sqlite_page_size",
    "sqlite_page_count", "sqlite_freelist_count", "sqlite_allocated_bytes",
    "sqlite_freelist_bytes", "sqlite_schema_rootpages",
))
LIFECYCLE_KEYS = frozenset((
    "schema", "status", "lifecycle_scope", "started_product_children",
    "reaped_product_children", "max_simultaneous_product_children",
    "active_product_children_terminal", "construction_failures", "request_failures",
    "close_failures", "rss_classification", "rss_limit_bytes_per_product_child",
    "aggregate_product_children_rss_claim", "rss_observations_complete",
    "per_product_child_rss", "pair_scopes",
))
PAIR_KEYS = frozenset((
    "sequence_id", "pair", "expected_roles", "required_g5_children", "observed_roles",
    "max_simultaneous_product_children", "row_q_zero", "pair_status",
    "active_product_children_after_pair", "sequence_terminal_records_present",
    "sequence_terminal_status_pass", "sequence_terminal_q_zero",
    "sequence_terminal_owners_zero",
    "failure_cleanup_complete", "active_product_children_after_sequence",
))
RSS_KEYS = frozenset((
    "label", "kind", "classification", "maximum_resident_set_size",
    "limit_bytes_per_product_child", "within_per_product_child_limit",
    "aggregate_rss_claim",
))
ARM_CLEANUP_SCHEMA = "phase4-g5-1-arm-cleanup-receipt-v12"
ARM_CLEANUP_KEYS = frozenset((
    "schema", "status", "classification", "ordinal", "sequence_id", "pair", "role",
    "row_root_name", "inventory_entries_removed", "inventory_sha256",
    "row_root_absent", "parent_directory_fsynced", "active_row_roots_after_cleanup",
))
WORK_ROOT_SCHEMA = "phase4-g5-1-work-root-lifecycle-v12"
WORK_ROOT_KEYS = frozenset((
    "schema", "status", "lifecycle_scope", "started_row_roots", "cleaned_row_roots",
    "max_active_row_roots", "active_row_roots_terminal", "cleanup_failures", "receipts",
))
PREARM_INITIALIZATION_ACTIONS = dict(
    database_discovery_enumerations=1,
    published_visible_state_invocations=1,
    published_visible_head_rows=1,
    published_visible_head_receipt_bytes=216,
    query_only_pragma_queries=2,
    physical_pragma_queries=3,
    sqlite_schema_rootpage_queries=1,
    sqlite_schema_rootpage_rows=3,
    ordered_object_all_row_scans=0,
    initialization_evidence_write_calls=1,
    initialization_evidence_file_fsync_calls=1,
    initialization_evidence_directory_fsync_calls=1,
    store_opens=0,
    product_children_started=0,
    product_rows=0,
    locks_acquired=0,
)
INPUT_MANIFEST = pathlib.Path(__file__).resolve().parents[1] / "method/INPUT-MANIFEST-v12.tsv"
DRY_RUN = pathlib.Path(__file__).resolve().parents[1] / "DRY-RUN-v12.json"


def packed(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":"))


def table(path):
    with pathlib.Path(path).open(encoding="utf-8", newline="") as stream:
        return [dict(row) for row in csv.DictReader(stream, delimiter="\t")]


def middle(numbers):
    values = sorted(numbers)
    half = len(values) // 2
    return values[half] if len(values) % 2 else (values[half - 1] + values[half]) // 2


def nearest_rank(numbers, percent):
    values = sorted(numbers)
    position = max(1, (len(values) * percent + 99) // 100)
    return values[position - 1]


def digest(value):
    return (
        type(value) is str
        and len(value) == 64
        and set(value) <= set("0123456789abcdef")
    )


def touched_detail(fault, error):
    if fault == "missing-object":
        start = "MissingObject(ObjectId("
        return (
            type(error) is str
            and error[:len(start)] == start
            and error[-2:] == "))"
            and digest(error[len(start):-2])
        )
    return error == "Core(" + str(TOUCHED_CLASSES.get(fault)) + ")"


def check_clone(meta, input_manifest_sha256, input_manifest, problems, tag):
    receipt = meta.get("clone_receipt")
    pre = meta.get("pre_dispatch_custody")
    if type(receipt) is not dict or frozenset(receipt) != CLONE_TOP:
        problems.add(f"{tag}:clone-receipt-shape")
        return
    entries = receipt.get("entries")
    hard = all((
        receipt.get("schema") == CLONE_SCHEMA,
        receipt.get("method") == "darwin-clonefile",
        receipt.get("copy_content") == "NotRehashedPerFastLaw",
        receipt.get("sealed_input_manifest_sha256") == input_manifest_sha256,
        receipt.get("inventory_equal") is True,
        receipt.get("dispatch_modes_exact") is True,
        type(entries) is list and bool(entries),
    ))
    if not hard:
        problems.add(f"{tag}:clone-receipt-hard")
        return
    path_rows = [(entry.get("path"), entry) for entry in entries if type(entry) is dict]
    if len(path_rows) != len(entries) or [item[0] for item in path_rows] != sorted({item[0] for item in path_rows}):
        problems.add(f"{tag}:clone-receipt-order")
    entry_bad = False
    for path, entry in path_rows:
        numeric = ("source_device", "source_inode", "destination_device", "destination_inode")
        entry_bad |= not all((
            frozenset(entry) == CLONE_ENTRY,
            type(path) is str and bool(path),
            type(entry.get("bytes")) is int and entry.get("bytes") >= 0,
            digest(entry.get("master_manifest_sha256")),
            all(entry.get(field) is True for field in (
                "clonefile_success", "same_device", "distinct_inode", "size_equal", "source_unchanged",
            )),
            all(type(entry.get(field)) is int and entry.get(field) >= 0 for field in numeric),
            entry.get("source_mode") == "-r--r--r--",
            entry.get("clone_destination_mode") == "-r--r--r--",
            entry.get("dispatch_mode") == "-rw-------",
            entry.get("mode_transition") == "sealed-0444-to-private-0600",
            entry.get("source_device") == entry.get("destination_device"),
            entry.get("source_inode") != entry.get("destination_inode"),
            not any("copy" in field and "hash" in field for field in entry),
        ))
    if entry_bad:
        problems.add(f"{tag}:clone-entry-hard")
    prefix = f"bases/{meta.get('operation')}-{meta.get('size_bytes')}/"
    manifest_rows = sorted(
        (
            row["input_relative_path"].removeprefix(prefix),
            int(row["bytes"]),
            row["sha256"],
        )
        for row in input_manifest
        if row["input_relative_path"].startswith(prefix)
    )
    receipt_rows = sorted(
        (entry.get("path"), entry.get("bytes"), entry.get("master_manifest_sha256"))
        for _, entry in path_rows
    )
    if not manifest_rows or receipt_rows != manifest_rows:
        problems.add(f"{tag}:clone-manifest-binding")
    suffixes = {
        ".sqlite": "database_sha256",
        ".sqlite.authority": "authority_sha256",
        ".sqlite.expectations": "expectations_sha256",
    }
    manifest = {}
    for path, entry in path_rows:
        for suffix, name in suffixes.items():
            if path.endswith(suffix):
                manifest[name] = entry.get("master_manifest_sha256")
                break
    if not all((
        type(pre) is dict,
        set(pre) == {"database_sha256", "authority_sha256", "expectations_sha256", "proof"},
        pre.get("proof") == "preverified-sealed-master-plus-native-clone-receipt",
        set(manifest) == {"database_sha256", "authority_sha256", "expectations_sha256"},
        all(pre.get(name) == value for name, value in manifest.items()),
    )):
        problems.add(f"{tag}:manifest-derived-custody")


def check_rooted_state(meta, product, problems, tag):
    state = meta.get("rooted_logical_state")
    if type(state) is not dict or frozenset(state) != ROOTED_STATE_KEYS:
        problems.add(f"{tag}:rooted-state-shape")
        return
    expected_provenance = (
        "ObservedVerifiedCompleteRoundTrip"
        if meta.get("validation_scope", "CompleteRoundTrip") == "CompleteRoundTrip"
        else "PreparedGoldenBoundByExactRootTransitionAndProductQualification"
    )
    facts = (
        state.get("schema") == ROOTED_STATE_SCHEMA,
        state.get("semantics") == ROOTED_STATE_SEMANTICS,
        state.get("query_only") is True,
        state.get("autocommit") is True,
        type(state.get("head_generation")) is int and state["head_generation"] >= 1,
        state.get("head_receipt_bytes") == 216,
        state.get("head_root_id") == product.get("root_id"),
        state.get("head_transition_id") == product.get("transition_id"),
        state.get("ordered_closure_digest") == product.get("ordered_closure_digest"),
        all(digest(state.get(field)) for field in (
            "head_root_id", "head_transition_id", "head_receipt_sha256",
            "ordered_closure_digest",
        )),
        state.get("head_receipt_semantics")
        == "ProductAuthenticatedHeadTupleOpaqueHashNotClosureOrFreshness",
        state.get("closure_provenance") == expected_provenance,
        state.get("reachable_published_result_parity") == "ClaimedHardGated",
        state.get("all_object_table_catalog_parity")
        == "NotClaimedSeparateFutureAllRowCasAudit",
        state.get("rollback_freshness") == "NotProtected",
    )
    if not all(facts):
        problems.add(f"{tag}:rooted-state-hard")
    if "post_database_sha256" in meta:
        problems.add(f"{tag}:physical-database-claim")
    if (
        meta.get("post_database_hash_semantics") != ROOTED_STATE_SEMANTICS
    ):
        problems.add(f"{tag}:database-hash-semantics")
    physical = meta.get("physical_allocation_observation")
    if type(physical) is not dict or frozenset(physical) != PHYSICAL_KEYS:
        problems.add(f"{tag}:physical-allocation-shape")
        return
    page = physical.get("sqlite_page_size", 0)
    rootpages = physical.get("sqlite_schema_rootpages")
    rootpage_order = [
        (item.get("type"), item.get("name"))
        for item in rootpages if type(item) is dict
    ] if type(rootpages) is list else []
    allocation_facts = (
        physical.get("schema") == PHYSICAL_SCHEMA,
        physical.get("classification") == "NotLogicalParity",
        all(
            type(physical.get(field)) is int and physical.get(field) >= 0
            for field in PHYSICAL_KEYS - {"schema", "classification", "sqlite_schema_rootpages"}
        ),
        physical.get("database_file_bytes", 0) > 0,
        512 <= page <= 65_536 and page & (page - 1) == 0,
        physical.get("sqlite_page_count", 0) > 0,
        physical.get("sqlite_freelist_count", 1) <= physical.get("sqlite_page_count", 0),
        physical.get("sqlite_allocated_bytes") == page * physical.get("sqlite_page_count", 0),
        physical.get("sqlite_freelist_bytes") == page * physical.get("sqlite_freelist_count", 0),
        type(rootpages) is list and bool(rootpages),
        type(rootpages) is list and len(rootpage_order) == len(rootpages),
        rootpage_order == sorted(rootpage_order) and len(rootpage_order) == len(set(rootpage_order)),
        type(rootpages) is list and all(all((
            frozenset(item) == {"type", "name", "table_name", "rootpage"},
            all(type(item.get(field)) is str and bool(item.get(field)) for field in (
                "type", "name", "table_name",
            )),
            type(item.get("rootpage")) is int and item.get("rootpage") >= 0,
        )) for item in rootpages),
    )
    if not all(allocation_facts):
        problems.add(f"{tag}:physical-allocation-hard")


def check_interval(value, meta, phases, problems, tag):
    use_common = (
        meta.get("comparison") == "g4-verified-vs-g5-verified"
        and meta.get("operation") != "first-edit-after-reopen"
    )
    components = COMMON_SECONDARY_PHASES if use_common else tuple(PHASES)
    classification = COMMON_INTERVAL if use_common else FULL_INTERVAL
    observed = value.get("comparison_interval_ns")
    if meta.get("comparison") == "g4-g5-triple":
        interval_map = {}
        class_map = {}
        role = meta.get("role")
        if role in ("g4_verified", "g5_verified"):
            g4_components = (
                COMMON_SECONDARY_PHASES
                if meta.get("operation") != "first-edit-after-reopen"
                else tuple(PHASES)
            )
            interval_map["g4_verified_vs_g5_verified"] = sum(
                phases.get(field, -1) for field in g4_components
            )
            class_map["g4_verified_vs_g5_verified"] = (
                COMMON_INTERVAL if g4_components == COMMON_SECONDARY_PHASES else FULL_INTERVAL
            )
        if role in ("g5_verified", "g5_trusted"):
            interval_map["g5_verified_vs_g5_trusted"] = value.get("decision_ns")
            class_map["g5_verified_vs_g5_trusted"] = FULL_INTERVAL
    else:
        key = str(meta.get("comparison")).replace("-", "_")
        interval_map, class_map = {key: observed}, {key: classification}
    valid = all((
        value.get("comparison_interval_classification") == classification,
        value.get("comparison_interval_components") == list(components),
        type(observed) is int and observed >= 0,
        type(observed) is int and observed == sum(phases.get(field, -1) for field in components),
        use_common or observed == value.get("decision_ns"),
        value.get("comparison_intervals_ns") == interval_map,
        value.get("comparison_interval_classifications") == class_map,
        meta.get("comparison_intervals_ns") == interval_map,
        meta.get("comparison_interval_classifications") == class_map,
    ))
    if not valid:
        problems.add(f"{tag}:comparison-interval")


def audit_lifecycle(
    records, rows, terminals, semantic_terminals, sentinels,
    campaign, artifact_file, problems,
):
    if len(records) != 1 or type(records[0]) is not dict:
        problems.add(f"child-lifecycle-cardinality:{len(records)}")
        return
    value = records[0]
    try:
        artifact_bytes = pathlib.Path(artifact_file).read_bytes()
        artifact = json.loads(artifact_bytes)
    except (OSError, json.JSONDecodeError):
        artifact = None
        artifact_bytes = b""
        problems.add("child-lifecycle-artifact")
    if artifact != value or artifact_bytes != (packed(value) + "\n").encode():
        problems.add("child-lifecycle-artifact")
    grouped = {}
    for row in rows:
        meta = row.get("wrapper", {})
        grouped.setdefault((meta.get("sequence_id"), meta.get("pair")), []).append(row)
    live = {}
    expected_scopes = []
    rss_kind = {}
    expected_terminals = set()
    for (sequence, pair), arm_rows in grouped.items():
        retained = live.setdefault(sequence, set())
        roles = [arm.get("wrapper", {}).get("role") for arm in arm_rows]
        required = [f"{sequence}-{role}" for role in roles if str(role).startswith("g5_")]
        peak = len(retained)
        for arm, role in zip(arm_rows, roles):
            meta = arm.get("wrapper", {})
            if role == "g4_verified":
                label = f"{int(meta['ordinal']):03d}-{sequence}-{role}"
                rss_kind[label] = "synchronous-one-shot"
                peak = max(peak, len(retained) + 1)
            else:
                label = f"{sequence}-{role}"
                retained.add(label)
                expected_terminals.add(label)
                rss_kind[label] = "persistent-g5-row-transport"
                peak = max(peak, len(retained))
        expected_scopes.append({
            "sequence_id": sequence, "pair": pair, "expected_roles": roles,
            "required_g5_children": required, "observed_roles": roles,
            "max_simultaneous_product_children": peak, "row_q_zero": True,
            "pair_status": "PASS", "active_product_children_after_pair": len(retained),
            "sequence_terminal_records_present": True, "sequence_terminal_q_zero": True,
            "sequence_terminal_status_pass": True,
            "sequence_terminal_owners_zero": True, "failure_cleanup_complete": True,
            "active_product_children_after_sequence": 0,
        })
    if campaign == "screen":
        semantic_labels = (
            "touched-error-matrix", "unrelated-corruption",
            "trusted-verified-reopen", "reconciliation",
        )
        rss_kind.update({f"semantic-{case}": "semantic-one-shot" for case in semantic_labels})
        rss_kind.update({label: "s07-one-shot" for label in (
            "s07-01-fixture", "s07-02-full-create-prepare", "s07-03-full-create-row",
            "s07-04-range-prepare", "s07-05-range-row",
        )})
    rss_bytes = {}
    for row in rows:
        meta = row.get("wrapper", {})
        if meta.get("role") == "g4_verified":
            rss_bytes[f"{int(meta['ordinal']):03d}-{meta['sequence_id']}-g4_verified"] = (
                row.get("external_time", {}).get("maximum_resident_set_size")
            )
    for terminal in terminals:
        rss_bytes[terminal.get("product_child_label")] = terminal.get(
            "external_time", {}
        ).get("maximum_resident_set_size")
    if campaign == "screen":
        for terminal in semantic_terminals:
            rss_bytes[f"semantic-{terminal.get('case')}"] = terminal.get(
                "external_time", {}
            ).get("maximum_resident_set_size")
        for sentinel in sentinels:
            times = sentinel.get("command_external_times", {})
            route = sentinel.get("route")
            rss_bytes["s07-01-fixture"] = times.get("fixture", {}).get(
                "maximum_resident_set_size"
            )
            prepare = "s07-02-full-create-prepare" if route == "full-create" else "s07-04-range-prepare"
            row_label = "s07-03-full-create-row" if route == "full-create" else "s07-05-range-row"
            rss_bytes[prepare] = times.get("prepare", {}).get("maximum_resident_set_size")
            rss_bytes[row_label] = times.get("row", {}).get("maximum_resident_set_size")
    rss_rows = value.get("per_product_child_rss")
    rss_map = {
        item.get("label"): item for item in rss_rows if type(item) is dict
    } if type(rss_rows) is list else {}
    rss_valid = all((
        type(rss_rows) is list,
        len(rss_map) == len(rss_rows) if type(rss_rows) is list else False,
        set(rss_map) == set(rss_kind),
        all(all((
            frozenset(item) == RSS_KEYS,
            item.get("kind") == rss_kind[label],
            item.get("maximum_resident_set_size") == rss_bytes.get(label),
            item.get("classification") == RSS_CLASSIFICATION,
            type(item.get("maximum_resident_set_size")) is int,
            0 < item.get("maximum_resident_set_size", 0) <= RSS_CAP,
            item.get("limit_bytes_per_product_child") == RSS_CAP,
            item.get("within_per_product_child_limit") is True,
            item.get("aggregate_rss_claim") == "NotClaimed",
        )) for label, item in rss_map.items()),
    ))
    terminal_labels = {terminal.get("product_child_label") for terminal in terminals}
    expected_peak = max(
        (scope["max_simultaneous_product_children"] for scope in expected_scopes),
        default=1 if rss_kind else 0,
    )
    lifecycle_facts = (
        frozenset(value) == LIFECYCLE_KEYS,
        value.get("schema") == LIFECYCLE_SCHEMA,
        value.get("status") == "PASS",
        value.get("lifecycle_scope") == "SequenceScopedMatchedPairs",
        value.get("started_product_children") == len(rss_kind),
        value.get("reaped_product_children") == len(rss_kind),
        value.get("max_simultaneous_product_children") == expected_peak,
        expected_peak <= 2,
        value.get("active_product_children_terminal") == 0,
        all(value.get(field) == 0 for field in (
            "construction_failures", "request_failures", "close_failures",
        )),
        value.get("rss_classification") == RSS_CLASSIFICATION,
        value.get("rss_limit_bytes_per_product_child") == RSS_CAP,
        value.get("aggregate_product_children_rss_claim") == "NotClaimed",
        value.get("rss_observations_complete") is True,
        value.get("pair_scopes") == expected_scopes,
        all(type(scope) is dict and frozenset(scope) == PAIR_KEYS for scope in value.get("pair_scopes", [])),
        terminal_labels == expected_terminals,
        rss_valid,
    )
    if not all(lifecycle_facts):
        problems.add("child-lifecycle-hard")


def audit_work_roots(records, rows, artifact_file, problems):
    ordered_receipts = []
    for row in rows:
        meta = row.get("wrapper", {})
        receipt = meta.get("arm_cleanup_receipt")
        inventory = meta.get("post_inventory")
        valid = all((
            type(receipt) is dict,
            type(receipt) is dict and frozenset(receipt) == ARM_CLEANUP_KEYS,
            type(receipt) is dict and receipt.get("schema") == ARM_CLEANUP_SCHEMA,
            type(receipt) is dict and receipt.get("status") == "PASS",
            type(receipt) is dict and receipt.get("classification") == "ImmediatePostEvidence",
            type(receipt) is dict and receipt.get("ordinal") == meta.get("ordinal"),
            type(receipt) is dict and receipt.get("sequence_id") == meta.get("sequence_id"),
            type(receipt) is dict and receipt.get("pair") == meta.get("pair"),
            type(receipt) is dict and receipt.get("role") == meta.get("role"),
            type(receipt) is dict and receipt.get("row_root_name")
            == f"{int(meta.get('ordinal', -1)):03d}-{meta.get('sequence_id')}-{meta.get('role')}",
            type(inventory) is list,
            type(receipt) is dict and receipt.get("inventory_entries_removed")
            == (len(inventory) if type(inventory) is list else -1),
            type(receipt) is dict and receipt.get("inventory_sha256")
            == hashlib.sha256(packed(inventory).encode()).hexdigest(),
            type(receipt) is dict and receipt.get("row_root_absent") is True,
            type(receipt) is dict and receipt.get("parent_directory_fsynced") is True,
            type(receipt) is dict and receipt.get("active_row_roots_after_cleanup") == 0,
        ))
        if not valid:
            problems.add(
                f"{meta.get('sequence_id')}:{meta.get('pair')}:{meta.get('role')}:arm-cleanup-receipt"
            )
        ordered_receipts.append(receipt)
    if len(records) != 1 or type(records[0]) is not dict:
        problems.add(f"work-root-lifecycle-cardinality:{len(records)}")
        return
    value = records[0]
    try:
        artifact_bytes = pathlib.Path(artifact_file).read_bytes()
        artifact = json.loads(artifact_bytes)
    except (OSError, json.JSONDecodeError):
        artifact = None
        artifact_bytes = b""
        problems.add("work-root-lifecycle-artifact")
    if artifact != value or artifact_bytes != (packed(value) + "\n").encode():
        problems.add("work-root-lifecycle-artifact")
    facts = (
        frozenset(value) == WORK_ROOT_KEYS,
        value.get("schema") == WORK_ROOT_SCHEMA,
        value.get("status") == "PASS",
        value.get("lifecycle_scope") == "ImmediatePerArmPostEvidenceCleanup",
        value.get("started_row_roots") == len(rows),
        value.get("cleaned_row_roots") == len(rows),
        value.get("max_active_row_roots") == 1,
        value.get("active_row_roots_terminal") == 0,
        value.get("cleanup_failures") == 0,
        value.get("receipts") == ordered_receipts,
    )
    if not all(facts):
        problems.add("work-root-lifecycle-hard")


def audit_prearm(
    value, artifact_bytes, evidence_bytes, dry, manifest_rows, manifest_sha, problems
):
    manifest = {
        row["input_relative_path"]: {
            "bytes": int(row["bytes"]),
            "sha256": row["sha256"],
        }
        for row in manifest_rows
    }
    try:
        evidence = json.loads(evidence_bytes)
    except (UnicodeDecodeError, json.JSONDecodeError):
        evidence = None
    fields = (
        "classification", "master", "database", "database_manifest",
        "input_manifest_sha256", "plan_sha256", "action_counts",
        "rooted_state", "physical_allocation_observation",
    )
    state = value.get("rooted_state", {}) if type(value) is dict else {}
    physical = value.get("physical_allocation_observation", {}) if type(value) is dict else {}
    rootpages = physical.get("sqlite_schema_rootpages")
    page = physical.get("sqlite_page_size", 0)
    database = value.get("database") if type(value) is dict else None
    dry_wrapper = dry.get("wrapper_calibration", {}) if type(dry) is dict else {}
    facts = (
        type(value) is dict,
        type(evidence) is dict,
        type(evidence) is dict
        and evidence.get("schema") == "phase4-g5-1-prearm-wrapper-initialization-evidence-v12",
        type(evidence) is dict
        and type(value) is dict
        and all(evidence.get(field) == value.get(field) for field in fields),
        type(value) is dict and artifact_bytes == (packed(value) + "\n").encode(),
        type(value) is dict
        and hashlib.sha256(evidence_bytes).hexdigest() == value.get("evidence_sha256"),
        type(value) is dict and value.get("status") == "PASS",
        type(value) is dict
        and value.get("classification")
        == "OneTimeRunnerSQLiteInitializationNotOperationObservation",
        type(value) is dict and value.get("chronology") == "AfterLockAndFrozenCustodyBeforeOrdinal1",
        type(value) is dict and value.get("within_dry_initialization_bound") is True,
        type(value) is dict and type(value.get("total_ns")) is int,
        type(value) is dict and value.get("elapsed_ns") == value.get("total_ns"),
        type(value) is dict and 0 < value.get("query_ns", 0) <= value.get("total_ns", 0),
        type(value) is dict
        and value.get("total_ns", 1) <= value.get("dry_initialization_bound_ns", 0),
        type(value) is dict
        and value.get("dry_initialization_bound_ns") == dry_wrapper.get("initialization_bound_ns"),
        type(value) is dict and value.get("plan_sha256") == dry_wrapper.get("plan_sha256"),
        type(value) is dict and value.get("input_manifest_sha256") == manifest_sha,
        type(value) is dict and value.get("input_manifest_sha256") == dry.get("input_manifest_sha256"),
        database in manifest,
        type(value) is dict and value.get("database_manifest") == manifest.get(database),
        type(value) is dict and value.get("action_counts") == PREARM_INITIALIZATION_ACTIONS,
        type(value) is dict and value.get("product_children_started") == 0,
        type(value) is dict and value.get("product_rows") == 0,
        type(value) is dict and value.get("stores_opened") == 0,
        type(value) is dict and value.get("lock_owned") is True,
        type(value) is dict
        and value.get("terminal_artifact_write_classification")
        == "OutsideInitializationBoundInsideCompleteWallFinalization",
        type(value) is dict and value.get("terminal_artifact_file_fsync_calls") == 1,
        type(value) is dict and value.get("terminal_artifact_directory_fsync_calls") == 1,
        frozenset(state) == ROOTED_STATE_KEYS,
        state.get("schema") == ROOTED_STATE_SCHEMA,
        state.get("semantics") == "CalibrationConstantRowShapeNotProductAuthority",
        state.get("query_only") is True,
        state.get("autocommit") is True,
        state.get("head_receipt_bytes") == 216,
        state.get("head_receipt_semantics") == "CalibrationOpaqueHeadReceiptHashNotClosureOrFreshness",
        state.get("closure_provenance") == "CalibrationShapeOnlyNoProductParity",
        state.get("reachable_published_result_parity") == "NotClaimedCalibrationShapeOnly",
        state.get("all_object_table_catalog_parity") == "NotClaimedSeparateFutureAllRowCasAudit",
        state.get("rollback_freshness") == "NotProtected",
        frozenset(physical) == PHYSICAL_KEYS,
        physical.get("schema") == PHYSICAL_SCHEMA,
        physical.get("classification") == "NotLogicalParity",
        512 <= page <= 65_536 and page & (page - 1) == 0,
        physical.get("sqlite_allocated_bytes") == page * physical.get("sqlite_page_count", -1),
        physical.get("sqlite_freelist_bytes") == page * physical.get("sqlite_freelist_count", -1),
        type(rootpages) is list and len(rootpages) == 3,
    )
    if not all(facts):
        problems.add("prearm-wrapper-initialization")


def recompute(raw_file, timing_file, schedule_file, expected_file):
    documents = [json.loads(value) for value in pathlib.Path(raw_file).read_text().splitlines() if value]
    rows = [value for value in documents if value.get("schema") == "phase4-g5-1-operation-v12"]
    ends = [value for value in documents if value.get("schema") == "phase4-g5-trusted-child-terminal-v10"]
    semantic = [value for value in documents if value.get("schema") == "phase4-g5-trusted-semantic-v10"]
    semantic_ends = [value for value in documents if value.get("schema") == "phase4-g5-trusted-semantic-terminal-v10"]
    sentinel = [value for value in documents if value.get("schema") == "phase4-g5-1-protected-sentinel-v12"]
    lifecycle_rows = [value for value in documents if value.get("schema") == LIFECYCLE_SCHEMA]
    work_root_rows = [value for value in documents if value.get("schema") == WORK_ROOT_SCHEMA]
    initialization_rows = [
        value
        for value in documents
        if value.get("schema") == "phase4-g5-1-prearm-wrapper-initialization-v12"
    ]
    timing_rows = table(timing_file)
    scheduled = table(schedule_file)
    known_expectations = {value["expectation_id"] for value in table(expected_file)}
    input_manifest = table(INPUT_MANIFEST)
    input_manifest_sha256 = hashlib.sha256(INPUT_MANIFEST.read_bytes()).hexdigest()
    problems = set()
    campaign = rows[0].get("wrapper", {}).get("campaign") if rows else None
    initialization = initialization_rows[0] if len(initialization_rows) == 1 else None
    artifact = pathlib.Path(raw_file).parent / "PREARM-WRAPPER-INITIALIZATION-v12.json"
    try:
        artifact_bytes = artifact.read_bytes()
    except OSError:
        artifact_bytes = b""
    evidence_path = pathlib.Path(raw_file).parent / (
        "PREARM-WRAPPER-INITIALIZATION-EVIDENCE-v12.json"
    )
    try:
        evidence_bytes = evidence_path.read_bytes()
        dry = json.loads(DRY_RUN.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        evidence_bytes = b""
        dry = {}
    audit_prearm(
        initialization,
        artifact_bytes,
        evidence_bytes,
        dry,
        input_manifest,
        input_manifest_sha256,
        problems,
    )

    if not rows:
        problems.add("no-operation-rows")
    if campaign == "gate" and len(rows) != 200:
        problems.add(f"gate-row-count:{len(rows)}")
    if len(rows) != len(timing_rows):
        problems.add("timing-row-count")
    if [value.get("wrapper", {}).get("ordinal") for value in rows] != list(range(1, len(rows) + 1)):
        problems.add("operation-order")
    pairs_by_sequence = {value["sequence_id"]: int(value["pairs"]) for value in scheduled}
    if campaign == "gate":
        expected_shape = []
        next_ordinal = 0
        for sequence in (value for value in scheduled if value.get("campaign") == "gate"):
            control, candidate = (
                ("g4_verified", "g5_verified")
                if sequence["comparison"] == "g4-verified-vs-g5-verified"
                else ("g5_verified", "g5_trusted")
            )
            for pair in range(1, int(sequence["pairs"]) + 1):
                role_order = [control, candidate]
                if (pair % 2 == 0) != (
                    int(sequence["pairs"]) == 5 and sequence["operation"] in SECONDARY_BA_OPERATIONS
                ):
                    role_order = role_order[::-1]
                for role in role_order:
                    next_ordinal += 1
                    expected_shape.append((
                        next_ordinal, sequence["sequence_id"], sequence["comparison"],
                        sequence["operation"], pair, role,
                        "trusted-local-dev" if role == "g5_trusted" else "verified",
                        int(sequence["size_bytes"]), sequence["expectation_id"],
                    ))
        actual_shape = [
            (
                meta.get("ordinal"), meta.get("sequence_id"), meta.get("comparison"),
                meta.get("operation"), meta.get("pair"), meta.get("role"), meta.get("mode"),
                meta.get("size_bytes"), meta.get("expectation_id"),
            )
            for meta in (row.get("wrapper", {}) for row in rows)
        ]
        if actual_shape != expected_shape:
            problems.add("gate-exact-schedule-shape")
    elif campaign == "screen":
        expected_shape = []
        next_ordinal = 0
        roles_by_comparison = {
            "same-g5": ("g5_verified", "g5_trusted"),
            "g4-g5-triple": ("g4_verified", "g5_verified", "g5_trusted"),
        }
        for sequence in (
            value for value in scheduled
            if value.get("sequence_id") in {"S01", "S05", "S06"}
        ):
            for role in roles_by_comparison[sequence["comparison"]]:
                next_ordinal += 1
                expected_shape.append((
                    next_ordinal, sequence["sequence_id"], sequence["comparison"],
                    sequence["operation"], 1, role,
                    "trusted-local-dev" if role == "g5_trusted" else "verified",
                    int(sequence["size_bytes"]), sequence["expectation_id"],
                ))
        actual_shape = [
            (
                meta.get("ordinal"), meta.get("sequence_id"), meta.get("comparison"),
                meta.get("operation"), meta.get("pair"), meta.get("role"), meta.get("mode"),
                meta.get("size_bytes"), meta.get("expectation_id"),
            )
            for meta in (row.get("wrapper", {}) for row in rows)
        ]
        if actual_shape != expected_shape:
            problems.add("screen-exact-schedule-shape")
    if campaign == "gate":
        for comparison, roles in (
            ("g4-verified-vs-g5-verified", ("g4_verified", "g5_verified")),
            ("g5-verified-vs-g5-trusted", ("g5_verified", "g5_trusted")),
        ):
            first_by_pair = {}
            for value in rows:
                meta = value.get("wrapper", {})
                if meta.get("comparison") == comparison and meta.get("operation") != "first-edit-after-reopen":
                    first_by_pair.setdefault(
                        (meta.get("sequence_id"), meta.get("pair")),
                        (meta.get("role"), meta.get("operation"), meta.get("pair")),
                    )
            counts = {role: sum(item[0] == role for item in first_by_pair.values()) for role in roles}
            exact = all(
                role == (roles[1] if ((pair % 2 == 0) != (operation in SECONDARY_BA_OPERATIONS)) else roles[0])
                for role, operation, pair in first_by_pair.values()
            )
            if len(first_by_pair) != 30 or counts != {roles[0]: 15, roles[1]: 15} or not exact:
                problems.add(f"{comparison}:secondary-order-balance")
    timings = {int(value["ordinal"]): value for value in timing_rows}

    rss_peak = 0
    checkpoint_rows = 0
    for value in rows:
        meta = value.get("wrapper", {})
        tag = f"{meta.get('sequence_id')}:{meta.get('pair')}:{meta.get('role')}"
        if value.get("status") != "PASS":
            problems.add(f"{tag}:status")
        phases = value.get("timers_ns", {})
        valid_phases = all(type(phases.get(name)) is int and phases[name] >= 0 for name in PHASES)
        if not valid_phases:
            problems.add(f"{tag}:timers")
        elif sum(phases[name] for name in PHASES) != value.get("total_ns"):
            problems.add(f"{tag}:timer-equation")
        check_interval(value, meta, phases, problems, tag)
        sidecar = timings.get(meta.get("ordinal"))
        if (
            sidecar is None
            or int(sidecar.get("total_ns", -1)) != value.get("total_ns")
            or int(sidecar.get("decision_ns", -1)) != value.get("decision_ns")
            or int(sidecar.get("comparison_interval_ns", -1))
            != value.get("comparison_interval_ns")
            or sidecar.get("comparison_interval_classification")
            != value.get("comparison_interval_classification")
        ):
            problems.add(f"{tag}:timing-sidecar")

        product = value.get("product", {})
        fields = ["q_current", "transactions", "commits", "root_id", "transition_id"]
        if meta.get("role") != "g4_verified":
            fields.extend(("edit_base_complete_scrub_calls", "edit_base_complete_scrub_canonical_bytes", "verified_carry_forward"))
        for field in fields:
            if field not in product:
                problems.add(f"{tag}:missing-{field}")
        if product.get("q_current") not in (None, 0):
            problems.add(f"{tag}:terminal-q")
        resources = (
            "q_high_water", "q_report_output_bytes", "max_single_buffer_bytes",
            "buffer_evidence_complete", "full_file_buffer_bytes", *COMMON_PARITY_FIELDS,
        )
        if any(field not in product for field in resources):
            problems.add(f"{tag}:resource-fields")
        elif not all((
            type(product["q_high_water"]) is int and product["q_high_water"] > 0,
            type(product["q_report_output_bytes"]) is int and product["q_report_output_bytes"] > 0,
            type(product["max_single_buffer_bytes"]) is int
            and 0 <= product["max_single_buffer_bytes"] <= 1_048_576,
            product["buffer_evidence_complete"] is True,
            product["full_file_buffer_bytes"] == 0,
        )):
            problems.add(f"{tag}:q-buffer-evidence")
        if product.get("transactions") not in (None, 1) or product.get("commits") not in (None, 1):
            problems.add(f"{tag}:transaction-commit")
        if meta.get("mode") == "trusted-local-dev":
            if (
                product.get("edit_base_complete_scrub_calls") not in (None, 0)
                or product.get("edit_base_complete_scrub_canonical_bytes") not in (None, 0)
                or product.get("verified_carry_forward") not in (None, False)
            ):
                problems.add(f"{tag}:trusted-authority")
            if product.get("edit_base_provenance") != "trusted-local-unverified-closure":
                problems.add(f"{tag}:trusted-provenance")
            if product.get("canonical_bytes_authenticated", 0) <= 0 or product.get("objects_authenticated", 0) <= 0:
                problems.add(f"{tag}:trusted-touched-authentication")
            trusted = [
                product.get("trusted_assumed_equal_edges"),
                product.get("trusted_assumed_prior_references"),
                product.get("trusted_assumed_prior_raw_bytes"),
            ]
            if (
                product.get("covered_equal_edges") != 0
                or any(type(counter) is not int or counter < 0 for counter in trusted)
                or sum(trusted) <= 0
            ):
                problems.add(f"{tag}:trusted-authority-laundering")
        elif meta.get("role") != "g4_verified":
            scrub_calls = product.get("edit_base_complete_scrub_calls")
            scrub = product.get("edit_base_complete_scrub_canonical_bytes")
            if (scrub_calls is not None and scrub_calls <= 0) or (scrub is not None and scrub <= 0):
                problems.add(f"{tag}:verified-scrub")
        if meta.get("role") != "g4_verified" and meta.get("mode") != "trusted-local-dev" and any(
            product.get(field) != 0
            for field in (
                "trusted_assumed_equal_edges", "trusted_assumed_prior_references",
                "trusted_assumed_prior_raw_bytes",
            )
        ):
            problems.add(f"{tag}:verified-trusted-assumptions")
        if (
            product.get("root_id") is None or product.get("transition_id") is None
            or not digest(meta.get("post_authority_sha256"))
            or not digest(meta.get("post_expectations_sha256"))
            or meta.get("mutation_work_sha256") is None
        ):
            problems.add(f"{tag}:post-state-custody")
        check_clone(meta, input_manifest_sha256, input_manifest, problems, tag)
        check_rooted_state(meta, product, problems, tag)
        pre_dispatch = meta.get("pre_dispatch_custody", {})
        if (
            meta.get("post_authority_sha256") != pre_dispatch.get("authority_sha256")
            or meta.get("post_expectations_sha256") != pre_dispatch.get("expectations_sha256")
        ):
            problems.add(f"{tag}:sidecar-custody")
        last_pair = pairs_by_sequence.get(meta.get("sequence_id"))
        checkpoint = campaign == "screen" or (
            campaign == "gate" and meta.get("pair") in (1, last_pair)
        )
        scope = "CompleteRoundTrip" if checkpoint else "CaptureOnly"
        if meta.get("fixed_checkpoint") is not checkpoint or meta.get("validation_scope") != scope:
            problems.add(f"{tag}:checkpoint-selection")
        if checkpoint:
            checkpoint_rows += 1
            roundtrip_numbers = (
                product.get("fresh_reopen_head_wall_ns"), product.get("fresh_full_scrub_wall_ns"),
                product.get("reconstruction_wall_ns"), product.get("complete_lifecycle_total_wall_ns"),
            )
            if any(type(number) is not int or number <= 0 for number in roundtrip_numbers) or product.get("lifecycle_phase_sum_matches") is not True:
                problems.add(f"{tag}:complete-roundtrip-evidence")
        elif product.get("fresh_full_scrub_wall_ns") != 0 or product.get("reconstruction_wall_ns") != 0:
            problems.add(f"{tag}:capture-only-evidence")
        if meta.get("role") == "g4_verified":
            fixed = meta.get("operation") in ("same-middle", "plus1-early", "plus1-middle")
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
                value.get("command_environment") != expected_environment
                or product.get("executable_sha256") != S07_G4_SHA256
                or product.get("pre_edit_database_sha256") != pre_dispatch.get("database_sha256")
                or product.get("pre_edit_authority_sha256") != pre_dispatch.get("authority_sha256")
                or product.get("pre_edit_expectations_sha256") != pre_dispatch.get("expectations_sha256")
            ):
                problems.add(f"{tag}:g4-pre-dispatch-environment")
        allowed = meta.get("allowed_inventory")
        inventory = meta.get("post_inventory")
        if type(allowed) is not list or type(inventory) is not list:
            problems.add(f"{tag}:inventory-missing")
        else:
            allowed_by_path = {item.get("path"): item for item in allowed}
            same_shape = {
                (item.get("path"), item.get("kind")) for item in allowed
            } == {(item.get("path"), item.get("kind")) for item in inventory}
            same_immutable_sizes = all(
                item.get("kind") != "file"
                or str(item.get("path", "")).endswith(".sqlite")
                or item.get("bytes") == allowed_by_path.get(item.get("path"), {}).get("bytes")
                for item in inventory
            )
            if not same_shape or not same_immutable_sizes or meta.get("inventory_residue") != []:
                problems.add(f"{tag}:inventory-residue")
        external = value.get("external_time", {})
        if isinstance(external, dict):
            rss_peak = max(rss_peak, int(external.get("maximum_resident_set_size", 0)))

    if campaign == "gate" and checkpoint_rows != 56:
        problems.add(f"checkpoint-cardinality:{checkpoint_rows}")
    if campaign == "screen" and checkpoint_rows != 8:
        problems.add(f"screen-checkpoint-cardinality:{checkpoint_rows}")

    for end in ends:
        identity = end.get("role")
        if end.get("status") != "PASS" or end.get("q_current") != 0:
            problems.add(f"terminal:{identity}:q-status")
        if type(end.get("rows")) is not int or end.get("rows") <= 0 or end.get("rows") != end.get("expected_rows"):
            problems.add(f"terminal:{identity}:cardinality")
        for owner in ("argument_owners", "request_owners", "schedule_owners", "timing_owners", "report_owners"):
            if end.get(owner) != 0:
                problems.add(f"terminal:{identity}:{owner}")
        rss_peak = max(rss_peak, int(end.get("external_time", {}).get("maximum_resident_set_size", 0)))
        if not all((
            end.get("rss_classification") == RSS_CLASSIFICATION,
            end.get("rss_limit_bytes_per_product_child") == RSS_CAP,
            end.get("aggregate_product_children_rss_claim") == "NotClaimed",
        )):
            problems.add(f"terminal:{identity}:rss-classification")
    audit_lifecycle(
        lifecycle_rows,
        rows,
        ends,
        semantic_ends,
        sentinel,
        campaign,
        pathlib.Path(raw_file).parent / "PRODUCT-CHILD-LIFECYCLE-v12.json",
        problems,
    )
    audit_work_roots(
        work_root_rows,
        rows,
        pathlib.Path(raw_file).parent / "WORK-ROOT-LIFECYCLE-v12.json",
        problems,
    )
    if campaign == "screen":
        counts = {}
        for value in semantic:
            counts[value.get("case")] = counts.get(value.get("case"), 0) + 1
            if (
                value.get("status") != "PASS" or value.get("cleanup_ok") is not True
                or value.get("residue") is not False or value.get("q_current") != 0
                or type(value.get("q_high_water")) is not int or value.get("q_high_water") <= 0
            ):
                problems.add(f"semantic:{value.get('case')}:{value.get('integrity_mode')}:hard")
        if counts != {"touched-error-matrix": 8, "unrelated-corruption": 2, "trusted-verified-reopen": 1, "reconciliation": 5}:
            problems.add(f"semantic-cardinality:{counts}")
        for value in semantic:
            case, mode = value.get("case"), value.get("integrity_mode")
            special = ("fault_case", "error_class", "failure_boundary")
            if not all(field in value for field in special):
                problems.add(f"semantic:{case}:{mode}:matrix-fields")
            if case != "touched-error-matrix" and any(value.get(field) is not None for field in special):
                problems.add(f"semantic:{case}:{mode}:unexpected-matrix-fields")
            if case == "unrelated-corruption" and mode == "verified" and ("IdentityMismatch" not in str(value.get("error")) or value.get("commits") != 0):
                problems.add("semantic:unrelated:verified")
            if case == "unrelated-corruption" and mode == "trusted-local-dev" and (value.get("error") is not None or value.get("commits") != 1 or "IdentityMismatch" not in str(value.get("later_snapshot_error"))):
                problems.add("semantic:unrelated:trusted")
            if case == "trusted-verified-reopen" and (
                value.get("commits") != 1
                or value.get("verified_carry_forward") is not False
                or value.get("verified_reopen_complete_scrub_calls", 0) <= 0
                or value.get("verified_reopen_complete_scrub_canonical_bytes", 0) <= 0
            ):
                problems.add("semantic:trusted-verified-reopen")
        touched_rows = [value for value in semantic if value.get("case") == "touched-error-matrix"]
        keyed_touched = {
            (value.get("integrity_mode"), value.get("fault_case")): value
            for value in touched_rows
        }
        expected_touched = {
            (mode, fault)
            for mode in ("verified", "trusted-local-dev")
            for fault in TOUCHED_CLASSES
        }
        touched_bad = (
            len(touched_rows) != 8
            or set(keyed_touched) != expected_touched
            or any(not all((
                value.get("error_class") == TOUCHED_CLASSES[fault],
                value.get("failure_boundary") == "PreCommit",
                touched_detail(fault, value.get("error")),
                value.get("transactions") == 1,
                value.get("commits") == 0,
                value.get("publication_status") is None,
                value.get("reconciliation") == "NotAttempted",
                value.get("verified_carry_forward") is False,
                value.get("head_unchanged") is True,
                value.get("cleanup_ok") is True,
                value.get("residue") is False,
                value.get("q_current") == 0,
            )) for (_, fault), value in keyed_touched.items())
        )
        if touched_bad:
            problems.add("semantic:touched-error-matrix")
        expected_reconciliation = {
            "rollback": "NotAttempted", "prior": "PriorVisible",
            "requested": "RequestedVisible", "different": "DifferentHead",
            "ambiguous": "Ambiguous",
        }
        reconciliation_rows = [value for value in semantic if value.get("case") == "reconciliation"]
        by_label = {value.get("integrity_mode"): value for value in reconciliation_rows}
        if (
            len(reconciliation_rows) != 5
            or set(by_label) != set(expected_reconciliation)
            or any(
                by_label[label].get("reconciliation") != outcome
                or by_label[label].get("verified_carry_forward") is not False
                for label, outcome in expected_reconciliation.items()
            )
        ):
            problems.add("semantic:reconciliation-set")
        if len(semantic_ends) != 4 or any(value.get("status") != "PASS" or value.get("q_current") != 0 for value in semantic_ends):
            problems.add("semantic-terminal")
        for value in semantic_ends:
            rss_peak = max(rss_peak, int(value.get("external_time", {}).get("maximum_resident_set_size", 0)))
        if len(sentinel) != 2 or {value.get("route") for value in sentinel} != {"full-create", "range"}:
            problems.add("S07:cardinality")
        for value in sentinel:
            route, product = value.get("route"), value.get("product", {})
            expected = {"full-create": S07_FULL, "range": S07_RANGE}.get(route, {})
            custody = value.get("prepared_custody", {})
            environment = value.get("row_environment", {})
            required_environment = {
                "LAYERFS_FAST_LANE": "1", "WP4M_EXECUTABLE_SHA256": S07_G4_SHA256,
                "WP4M_BASE_COPY_METHOD": "fast-lane-isolated-prepared-row",
                "WP4M_BASE_DATABASE_SHA256": custody.get("database_sha256"),
                "WP4M_BASE_AUTHORITY_SHA256": custody.get("authority_sha256"),
                "WP4M_BASE_EXPECTATIONS_SHA256": custody.get("expectations_sha256"),
            }
            expected_work = {field: expected.get(field) for field in S07_WORK_FIELDS}
            expected_work |= {"root_id": expected.get("root_id"), "transition_id": expected.get("transition_id")}
            fixture = value.get("fixture_command")
            prepare = value.get("prepare_command")
            row_command = value.get("row_command")
            command_facts = [
                type(fixture) is list and len(fixture) == 4,
                type(prepare) is list and len(prepare) == 6,
                type(row_command) is list and len(row_command) == 8,
            ]
            if all(command_facts):
                operation = "write" if route == "full-create" else "read-range-1m"
                command_facts += [
                    fixture[0] == prepare[0] == row_command[0],
                    fixture[1:] == ["--fast-fixture", fixture[2], "1048576"],
                    fixture[2].endswith("/s07-fixture-probe"),
                    prepare[1:] == ["--fast-prepare", prepare[2], "1048576", operation, "0"],
                    row_command[1:] == ["--fast-row", prepare[2], "1048576", operation, "0", "false", "complete-roundtrip"],
                ]
            custody_facts = [
                set(custody) == {"database_sha256", "authority_sha256", "expectations_sha256"},
                all(type(item) is str and len(item) == 64 for item in custody.values()),
                environment == required_environment,
                product.get("pre_edit_database_sha256") == custody.get("database_sha256"),
                product.get("pre_edit_authority_sha256") == custody.get("authority_sha256"),
                product.get("pre_edit_expectations_sha256") == custody.get("expectations_sha256"),
                value.get("post_authority_sha256") == custody.get("authority_sha256"),
                value.get("post_expectations_sha256") == custody.get("expectations_sha256"),
                "post_database_sha256" not in value,
            ]
            ranges = product.get("range_measurements")
            range_facts = (
                [
                    type(ranges) is list and len(ranges) == 1,
                    type(ranges) is list and len(ranges) == 1
                    and all(ranges[0].get(field) == wanted for field, wanted in S07_RANGE_MEASUREMENT.items()),
                    type(ranges) is list and len(ranges) == 1
                    and type(ranges[0].get("wall_ns")) is int and ranges[0]["wall_ns"] >= 0,
                    type(ranges) is list and len(ranges) == 1
                    and type(ranges[0].get("throughput_mib_s")) in (int, float)
                    and ranges[0]["throughput_mib_s"] > 0,
                    value.get("deterministic_range") == S07_RANGE_MEASUREMENT,
                ]
                if route == "range" else [
                    value.get("deterministic_range") is None,
                    product.get("range_measurements") == [],
                ]
            )
            command_times = value.get("command_external_times", {})
            command_rss = [
                item.get("maximum_resident_set_size")
                for item in command_times.values()
                if type(item) is dict
            ]
            allowed_inventory = value.get("allowed_inventory")
            post_inventory = value.get("post_inventory")
            allowed_sizes = {
                item.get("path"): item.get("bytes") for item in allowed_inventory
            } if type(allowed_inventory) is list else {}
            inventory_facts = [
                type(allowed_inventory) is list,
                type(post_inventory) is list,
                type(allowed_inventory) is list and type(post_inventory) is list
                and {(item.get("path"), item.get("kind")) for item in allowed_inventory}
                == {(item.get("path"), item.get("kind")) for item in post_inventory},
                type(post_inventory) is list and all(
                    item.get("kind") != "file"
                    or str(item.get("path", "")).endswith(".sqlite")
                    or item.get("bytes") == allowed_sizes.get(item.get("path"))
                    for item in post_inventory
                ),
                value.get("inventory_residue") == [],
            ]
            resource_fields = (
                "q_high_water", "q_report_output_bytes", "max_single_buffer_bytes",
                "buffer_evidence_complete", "full_file_buffer_bytes", *COMMON_PARITY_FIELDS,
            )
            resource_facts = [
                all(field in product for field in resource_fields),
                type(product.get("q_high_water")) is int and product.get("q_high_water") > 0,
                type(product.get("q_report_output_bytes")) is int and product.get("q_report_output_bytes") > 0,
                type(product.get("max_single_buffer_bytes")) is int
                and 0 <= product.get("max_single_buffer_bytes") <= 1_048_576,
                product.get("buffer_evidence_complete") is True,
                product.get("full_file_buffer_bytes") == 0,
            ]
            work_digest = hashlib.sha256(packed(expected_work).encode()).hexdigest()
            check_rooted_state(value, product, problems, f"S07:{route}")
            exact_facts = [
                value.get("status") == "PASS", value.get("sequence_id") == "S07",
                value.get("executable_sha256") == S07_G4_SHA256,
                value.get("frozen_fixture_sha256") == S07_FIXTURE_SHA256,
                value.get("probe_fixture_sha256") == S07_FIXTURE_SHA256,
                value.get("post_database_hash_semantics") == ROOTED_STATE_SEMANTICS,
                value.get("pre_cleanup_residue") == [], bool(value.get("base_custody")), bool(expected),
                set(command_times) == {"fixture", "prepare", "row"}, len(command_rss) == 3,
                all(type(item) is int and 0 < item <= RSS_CAP for item in command_rss),
                product.get("status") == "PASS", product.get("error") is None,
                product.get("executable_sha256") == S07_G4_SHA256,
                product.get("base_copy_method") == "fast-lane-isolated-prepared-row",
                all(product.get(field) == wanted for field, wanted in expected.items()),
                value.get("deterministic_tuple") == expected,
                value.get("mutation_work") == expected_work,
                value.get("mutation_work_sha256") == work_digest,
            ]
            if not all(
                exact_facts + custody_facts + command_facts + range_facts
                + inventory_facts + resource_facts
            ):
                problems.add(f"S07:{route}:hard")
            rss_peak = max(rss_peak, *command_rss, 0)
    if rss_peak > RSS_CAP:
        problems.add(f"rss:{rss_peak}")

    cells = {}
    for value in rows:
        meta = value["wrapper"]
        identity = (meta["comparison"], meta["operation"], meta["role"])
        cells.setdefault(identity, {})[meta["pair"]] = value

    comparisons = {}
    for comparison in ("g4-verified-vs-g5-verified", "g5-verified-vs-g5-trusted"):
        roles = (
            ("g4_verified", "g5_verified")
            if comparison == "g4-verified-vs-g5-verified"
            else ("g5_verified", "g5_trusted")
        )
        operations = sorted({key[1] for key in cells if key[0] == comparison})
        for operation in operations:
            left = cells.get((comparison, operation, roles[0]), {})
            right = cells.get((comparison, operation, roles[1]), {})
            if set(left) != set(right):
                problems.add(f"{comparison}:{operation}:pair-set")
                continue
            control, candidate = [], []
            for pair in sorted(left):
                control.append(left[pair]["comparison_interval_ns"])
                candidate.append(right[pair]["comparison_interval_ns"])
                for field in ("root_id", "transition_id"):
                    if left[pair]["product"].get(field) != right[pair]["product"].get(field):
                        problems.add(f"{comparison}:{operation}:pair-{pair}:{field}")
                for field in (
                    "post_authority_sha256", "post_expectations_sha256", "mutation_work_sha256",
                ):
                    if left[pair]["wrapper"].get(field) != right[pair]["wrapper"].get(field):
                        problems.add(f"{comparison}:{operation}:pair-{pair}:{field}")
                if not paired_rooted_state_equal(
                    left[pair]["wrapper"], right[pair]["wrapper"]
                ):
                    problems.add(f"{comparison}:{operation}:pair-{pair}:rooted-state")
                parity_fields = comparison_parity_fields(comparison)
                for field in parity_fields:
                    if left[pair]["product"].get(field) != right[pair]["product"].get(field):
                        problems.add(f"{comparison}:{operation}:pair-{pair}:{field}")
            cell = {
                "pairs": len(control),
                "comparison_interval_classification": left[min(left)].get(
                    "comparison_interval_classification"
                ) if left else None,
                "control_ns": control,
                "candidate_ns": candidate,
                "control_sum_ns": sum(control),
                "candidate_sum_ns": sum(candidate),
                "control_p50_ns": middle(control),
                "candidate_p50_ns": middle(candidate),
                "candidate_p95_ns": nearest_rank(candidate, 95),
            }
            if comparison == "g4-verified-vs-g5-verified":
                material = (
                    sum(candidate) * 100 > sum(control) * 105
                    and sum(candidate) - sum(control) >= len(control) * 1_000_000
                )
                cell["material_regression"] = material
                if material:
                    problems.add(f"{comparison}:{operation}:material-regression")
            else:
                gains = [((control[i] - candidate[i]) * 10_000) // control[i] for i in range(len(control))]
                cell["paired_improvement_basis_points"] = gains
                cell["paired_median_improvement_basis_points"] = middle(gains)
                if operation == "first-edit-after-reopen":
                    if middle(gains) < 5_000:
                        problems.add("same-g5:first-edit-after-reopen:improvement")
                    if middle(candidate) > 15_000_000 or nearest_rank(candidate, 95) > 25_000_000:
                        problems.add("same-g5:first-edit-after-reopen:latency")
                if operation in ("plus1-early", "plus1-middle") and middle(candidate) > 15_000_000:
                    problems.add(f"same-g5:{operation}:latency")
            comparisons.setdefault(comparison, {})[operation] = cell

    if not {row["expectation_id"] for row in scheduled}.issubset(known_expectations):
        problems.add("schedule-expectation-custody")
    normalized = {
        "campaign": campaign,
        "operation_rows": len(rows),
        "terminal_rows": len(ends) + len(semantic_ends),
        "prearm_wrapper_initialization": initialization,
        "product_child_lifecycle": lifecycle_rows[0] if len(lifecycle_rows) == 1 else None,
        "work_root_lifecycle": work_root_rows[0] if len(work_root_rows) == 1 else None,
        "gate_custody_results": sorted(
            ({
                "ordinal": value.get("wrapper", {}).get("ordinal"),
                "sequence_id": value.get("wrapper", {}).get("sequence_id"),
                "pair": value.get("wrapper", {}).get("pair"),
                "role": value.get("wrapper", {}).get("role"),
                "validation_scope": value.get("wrapper", {}).get("validation_scope"),
                "fixed_checkpoint": value.get("wrapper", {}).get("fixed_checkpoint"),
                "arm_cleanup_receipt": value.get("wrapper", {}).get(
                    "arm_cleanup_receipt"
                ),
                "comparison_interval_ns": value.get("comparison_interval_ns"),
                "comparison_interval_classification": value.get(
                    "comparison_interval_classification"
                ),
                "comparison_interval_components": value.get(
                    "comparison_interval_components"
                ),
                "comparison_intervals_ns": value.get("comparison_intervals_ns"),
                "comparison_interval_classifications": value.get(
                    "comparison_interval_classifications"
                ),
                "clone_receipt": value.get("wrapper", {}).get("clone_receipt"),
                "pre_dispatch_custody": value.get("wrapper", {}).get("pre_dispatch_custody"),
                "rooted_logical_state": value.get("wrapper", {}).get("rooted_logical_state"),
                "physical_allocation_observation": value.get("wrapper", {}).get(
                    "physical_allocation_observation"
                ),
                "post_authority_sha256": value.get("wrapper", {}).get("post_authority_sha256"),
                "post_expectations_sha256": value.get("wrapper", {}).get("post_expectations_sha256"),
                "mutation_work_sha256": value.get("wrapper", {}).get("mutation_work_sha256"),
                "inventory_residue": value.get("wrapper", {}).get("inventory_residue"),
            } for value in rows if value.get("wrapper", {}).get("campaign") == "gate"),
            key=packed,
        ),
        "semantic_results": sorted(
            ({key: value.get(key) for key in (
                "case", "integrity_mode", "fault_case", "error_class",
                "failure_boundary", "error", "later_snapshot_error",
                "publication_status", "reconciliation", "head_unchanged", "transactions",
                "commits", "edit_base_complete_scrub_calls",
                "edit_base_complete_scrub_canonical_bytes",
                "verified_reopen_complete_scrub_calls",
                "verified_reopen_complete_scrub_canonical_bytes",
                "verified_carry_forward", "cleanup_ok", "residue", "q_high_water", "q_current",
            )} for value in semantic),
            key=packed,
        ),
        "protected_sentinel_results": sorted(
            ({
                "route": value.get("route"),
                "frozen_fixture_sha256": value.get("frozen_fixture_sha256"),
                "probe_fixture_sha256": value.get("probe_fixture_sha256"),
                "prepared_custody": value.get("prepared_custody"),
                "row_environment": value.get("row_environment"),
                "fixture_command": value.get("fixture_command"),
                "prepare_command": value.get("prepare_command"),
                "row_command": value.get("row_command"),
                "command_external_times": value.get("command_external_times"),
                "pre_cleanup_residue": value.get("pre_cleanup_residue"),
                "allowed_inventory": value.get("allowed_inventory"),
                "post_inventory": value.get("post_inventory"),
                "inventory_residue": value.get("inventory_residue"),
                "deterministic_tuple": value.get("deterministic_tuple"),
                "deterministic_range": value.get("deterministic_range"),
                "product_tuple": {
                    field: value.get("product", {}).get(field)
                    for field in (S07_FULL if value.get("route") == "full-create" else S07_RANGE)
                },
                "range_measurements": value.get("product", {}).get("range_measurements"),
                "product_pre_edit_database_sha256": value.get("product", {}).get("pre_edit_database_sha256"),
                "product_pre_edit_authority_sha256": value.get("product", {}).get("pre_edit_authority_sha256"),
                "product_pre_edit_expectations_sha256": value.get("product", {}).get("pre_edit_expectations_sha256"),
                "post_database_hash_semantics": value.get("post_database_hash_semantics"),
                "rooted_logical_state": value.get("rooted_logical_state"),
                "physical_allocation_observation": value.get(
                    "physical_allocation_observation"
                ),
                "post_authority_sha256": value.get("post_authority_sha256"),
                "post_expectations_sha256": value.get("post_expectations_sha256"),
                "mutation_work": value.get("mutation_work"),
                "mutation_work_sha256": value.get("mutation_work_sha256"),
            } for value in sentinel), key=packed),
        "maximum_rss_bytes": rss_peak,
        "rss_limit_bytes": RSS_CAP,
        "comparisons": comparisons,
        "hard_failures": sorted(problems),
    }
    return {
        "schema": "phase4-g5-1-independent-recomputation-v12",
        "status": "PASS" if not problems else "REVISE",
        "normalized": normalized,
    }


def self_check():
    value = "c" * 64
    product = dict(root_id=value, transition_id=value, ordered_closure_digest=value)
    state = dict(
        schema=ROOTED_STATE_SCHEMA,
        semantics=ROOTED_STATE_SEMANTICS,
        query_only=True,
        autocommit=True,
        head_generation=2,
        head_root_id=value,
        head_transition_id=value,
        head_receipt_bytes=216,
        head_receipt_sha256=value,
        head_receipt_semantics="ProductAuthenticatedHeadTupleOpaqueHashNotClosureOrFreshness",
        ordered_closure_digest=value,
        closure_provenance="PreparedGoldenBoundByExactRootTransitionAndProductQualification",
        reachable_published_result_parity="ClaimedHardGated",
        all_object_table_catalog_parity="NotClaimedSeparateFutureAllRowCasAudit",
        rollback_freshness="NotProtected",
    )
    physical = dict(
        schema=PHYSICAL_SCHEMA,
        classification="NotLogicalParity",
        database_file_bytes=4096,
        sqlite_page_size=4096,
        sqlite_page_count=1,
        sqlite_freelist_count=0,
        sqlite_allocated_bytes=4096,
        sqlite_freelist_bytes=0,
        sqlite_schema_rootpages=[dict(type="table", name="x", table_name="x", rootpage=1)],
    )
    wrapper = dict(
        validation_scope="CaptureOnly",
        rooted_logical_state=state,
        physical_allocation_observation=physical,
        post_database_hash_semantics=ROOTED_STATE_SEMANTICS,
    )
    problems = set()
    check_rooted_state(wrapper, product, problems, "valid")
    assert not problems
    mutations = {
        "all-row-claim": dict(state, all_object_table_catalog_parity="Claimed"),
        "receipt-laundering": dict(state, head_receipt_semantics="ClosureProof"),
        "closure-mismatch": dict(state, ordered_closure_digest="d" * 64),
        "provenance-mismatch": dict(state, closure_provenance="ConstructionProof"),
    }
    for name, changed in mutations.items():
        problems = set()
        check_rooted_state(dict(wrapper, rooted_logical_state=changed), product, problems, name)
        assert problems
    problems = set()
    check_rooted_state(
        dict(wrapper, rooted_logical_state=None, logical_catalog={}),
        product,
        problems,
        "legacy-catalog",
    )
    assert problems
    assert not paired_rooted_state_equal(
        wrapper,
        dict(
            wrapper,
            rooted_logical_state=dict(state, head_receipt_sha256="d" * 64),
        ),
    )
    assert "objects_authenticated" in comparison_parity_fields(
        "g4-verified-vs-g5-verified"
    )
    assert "objects_authenticated" not in comparison_parity_fields(
        "g5-verified-vs-g5-trusted"
    )
    assert "objects_created" in comparison_parity_fields(
        "g5-verified-vs-g5-trusted"
    )
    calibration_state = dict(
        state,
        semantics="CalibrationConstantRowShapeNotProductAuthority",
        head_receipt_semantics="CalibrationOpaqueHeadReceiptHashNotClosureOrFreshness",
        closure_provenance="CalibrationShapeOnlyNoProductParity",
        reachable_published_result_parity="NotClaimedCalibrationShapeOnly",
    )
    calibration_physical = dict(
        physical,
        sqlite_schema_rootpages=[
            dict(type="table", name=name, table_name=name, rootpage=index)
            for index, name in enumerate(("a", "b", "c"), 1)
        ],
    )
    database = "bases/calibration/db.sqlite"
    manifest_sha = "d" * 64
    plan_sha = "e" * 64
    manifest_rows = [dict(input_relative_path=database, bytes="4096", sha256=value)]
    dry = dict(
        input_manifest_sha256=manifest_sha,
        wrapper_calibration=dict(initialization_bound_ns=1_000, plan_sha256=plan_sha),
    )
    evidence = dict(
        schema="phase4-g5-1-prearm-wrapper-initialization-evidence-v12",
        classification="OneTimeRunnerSQLiteInitializationNotOperationObservation",
        master="calibration",
        database=database,
        database_manifest=dict(bytes=4096, sha256=value),
        input_manifest_sha256=manifest_sha,
        plan_sha256=plan_sha,
        action_counts=PREARM_INITIALIZATION_ACTIONS,
        rooted_state=calibration_state,
        physical_allocation_observation=calibration_physical,
    )

    def prearm_problems(changes=None, evidence_changes=None):
        changed_evidence = dict(evidence, **(evidence_changes or {}))
        evidence_bytes = (packed(changed_evidence) + "\n").encode()
        initialization = dict(
            changed_evidence,
            schema="phase4-g5-1-prearm-wrapper-initialization-v12",
            status="PASS",
            chronology="AfterLockAndFrozenCustodyBeforeOrdinal1",
            query_ns=10,
            total_ns=100,
            elapsed_ns=100,
            evidence_sha256=hashlib.sha256(evidence_bytes).hexdigest(),
            dry_initialization_bound_ns=1_000,
            within_dry_initialization_bound=True,
            product_children_started=0,
            product_rows=0,
            stores_opened=0,
            lock_owned=True,
            terminal_artifact_write_classification="OutsideInitializationBoundInsideCompleteWallFinalization",
            terminal_artifact_file_fsync_calls=1,
            terminal_artifact_directory_fsync_calls=1,
        )
        initialization.update(changes or {})
        found = set()
        audit_prearm(
            initialization,
            (packed(initialization) + "\n").encode(),
            evidence_bytes,
            dry,
            manifest_rows,
            manifest_sha,
            found,
        )
        return found

    assert not prearm_problems()
    prearm_mutations = {
        "inflated-bound": (dict(dry_initialization_bound_ns=2_000), None),
        "wrong-plan": (dict(plan_sha256="f" * 64), dict(plan_sha256="f" * 64)),
        "wrong-input": (
            dict(input_manifest_sha256="f" * 64),
            dict(input_manifest_sha256="f" * 64),
        ),
        "wrong-database-row": (
            dict(database_manifest=dict(bytes=4096, sha256="f" * 64)),
            dict(database_manifest=dict(bytes=4096, sha256="f" * 64)),
        ),
    }
    for changes, evidence_changes in prearm_mutations.values():
        assert prearm_problems(changes, evidence_changes)
    print(packed({"status": "PASS", "checks": 14, "mutations_rejected": sorted(mutations) + sorted(prearm_mutations) + ["legacy-catalog", "paired-receipt-hash"]}))
    return 0


def main():
    if sys.argv[1:] == ["--self-check"]:
        return self_check()
    if len(sys.argv) != 6:
        raise SystemExit("usage: independent.py --self-check|RAW TIMINGS SCHEDULE EXPECTED OUTPUT")
    result = recompute(*map(pathlib.Path, sys.argv[1:5]))
    pathlib.Path(sys.argv[5]).write_text(packed(result) + "\n", encoding="utf-8")
    return 0 if result["status"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
