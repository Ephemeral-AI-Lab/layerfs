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
SECONDARY_BA_OPERATIONS = {"one-byte-early", "one-byte-late", "plus1-middle"}


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


def recompute(raw_file, timing_file, schedule_file, expected_file):
    documents = [json.loads(value) for value in pathlib.Path(raw_file).read_text().splitlines() if value]
    rows = [value for value in documents if value.get("schema") == "phase4-g5-1-operation-v8"]
    ends = [value for value in documents if value.get("schema") == "phase4-g5-trusted-child-terminal-v8"]
    semantic = [value for value in documents if value.get("schema") == "phase4-g5-trusted-semantic-v8"]
    semantic_ends = [value for value in documents if value.get("schema") == "phase4-g5-trusted-semantic-terminal-v8"]
    sentinel = [value for value in documents if value.get("schema") == "phase4-g5-1-protected-sentinel-v8"]
    timing_rows = table(timing_file)
    scheduled = table(schedule_file)
    known_expectations = {value["expectation_id"] for value in table(expected_file)}
    problems = set()
    campaign = rows[0].get("wrapper", {}).get("campaign") if rows else None

    if not rows:
        problems.add("no-operation-rows")
    if campaign == "gate" and len(rows) != 200:
        problems.add(f"gate-row-count:{len(rows)}")
    if len(rows) != len(timing_rows):
        problems.add("timing-row-count")
    if [value.get("wrapper", {}).get("ordinal") for value in rows] != list(range(1, len(rows) + 1)):
        problems.add("operation-order")
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
        sidecar = timings.get(meta.get("ordinal"))
        if sidecar is None or int(sidecar["total_ns"]) != value.get("total_ns"):
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
        for field in ("post_database_sha256", "post_authority_sha256", "mutation_work_sha256"):
            if not meta.get(field):
                problems.add(f"{tag}:missing-{field}")
        if meta.get("post_database_hash_semantics") != "physical-byte-parity-only-not-logical-digest":
            problems.add(f"{tag}:database-hash-label")
        pre_dispatch = meta.get("pre_dispatch_custody", {})
        clone_custody = meta.get("base_custody")
        derived_pre = {}
        for item in clone_custody if type(clone_custody) is list else ():
            path = str(item.get("path", ""))
            suffix = (
                "database_sha256" if path.endswith(".sqlite")
                else "authority_sha256" if path.endswith(".sqlite.authority")
                else "expectations_sha256" if path.endswith(".sqlite.expectations")
                else None
            )
            if suffix:
                derived_pre[suffix] = item.get("pre_dispatch_sha256")
        if (
            set(pre_dispatch) != {"database_sha256", "authority_sha256", "expectations_sha256"}
            or type(clone_custody) is not list
            or any(item.get("pre_dispatch_sha256") != item.get("manifest_sha256") for item in clone_custody)
            or derived_pre != pre_dispatch
        ):
            problems.add(f"{tag}:pre-dispatch-custody")
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
        if counts != {"touched-corruption": 2, "unrelated-corruption": 2, "trusted-verified-reopen": 1, "reconciliation": 5}:
            problems.add(f"semantic-cardinality:{counts}")
        for value in semantic:
            case, mode = value.get("case"), value.get("integrity_mode")
            if case == "touched-corruption" and ("IdentityMismatch" not in str(value.get("error")) or value.get("commits") != 0 or value.get("head_unchanged") is not True):
                problems.add(f"semantic:touched:{mode}")
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
                (value.get("post_database_sha256") == custody.get("database_sha256")) == (route == "range"),
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
            exact_facts = [
                value.get("status") == "PASS", value.get("sequence_id") == "S07",
                value.get("executable_sha256") == S07_G4_SHA256,
                value.get("frozen_fixture_sha256") == S07_FIXTURE_SHA256,
                value.get("probe_fixture_sha256") == S07_FIXTURE_SHA256,
                value.get("post_database_hash_semantics") == "physical-byte-parity-only-not-logical-digest",
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
                control.append(left[pair]["decision_ns"])
                candidate.append(right[pair]["decision_ns"])
                for field in ("root_id", "transition_id"):
                    if left[pair]["product"].get(field) != right[pair]["product"].get(field):
                        problems.add(f"{comparison}:{operation}:pair-{pair}:{field}")
                for field in ("post_database_sha256", "post_authority_sha256", "mutation_work_sha256"):
                    if left[pair]["wrapper"].get(field) != right[pair]["wrapper"].get(field):
                        problems.add(f"{comparison}:{operation}:pair-{pair}:{field}")
                parity_fields = (
                    COMMON_PARITY_FIELDS
                    if comparison == "g4-verified-vs-g5-verified"
                    else MUTATION_PARITY_FIELDS
                )
                for field in parity_fields:
                    if left[pair]["product"].get(field) != right[pair]["product"].get(field):
                        problems.add(f"{comparison}:{operation}:pair-{pair}:{field}")
            cell = {
                "pairs": len(control),
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
        "semantic_results": sorted(
            ({key: value.get(key) for key in (
                "case", "integrity_mode", "error", "later_snapshot_error",
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
                "post_database_sha256": value.get("post_database_sha256"),
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
        "schema": "phase4-g5-1-independent-recomputation-v8",
        "status": "PASS" if not problems else "REVISE",
        "normalized": normalized,
    }


def main():
    if len(sys.argv) != 6:
        raise SystemExit("usage: independent.py RAW TIMINGS SCHEDULE EXPECTED OUTPUT")
    result = recompute(*map(pathlib.Path, sys.argv[1:5]))
    pathlib.Path(sys.argv[5]).write_text(packed(result) + "\n", encoding="utf-8")
    return 0 if result["status"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
