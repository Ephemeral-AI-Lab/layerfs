#!/usr/bin/env python3
import csv
import hashlib
import json
import pathlib
import statistics
import sys


OP_SCHEMA = "phase4-g5-1-operation-v6"
TERMINAL_SCHEMA = "phase4-g5-trusted-child-terminal-v6"
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


def analyze(raw_path, timing_path, schedule_path, expected_path):
    raw = [json.loads(line) for line in pathlib.Path(raw_path).read_text().splitlines() if line]
    operations = [row for row in raw if row.get("schema") == OP_SCHEMA]
    terminals = [row for row in raw if row.get("schema") == TERMINAL_SCHEMA]
    semantic_rows = [row for row in raw if row.get("schema") == "phase4-g5-trusted-semantic-v6"]
    semantic_terminals = [
        row for row in raw if row.get("schema") == "phase4-g5-trusted-semantic-terminal-v6"
    ]
    sentinel_rows = [row for row in raw if row.get("schema") == "phase4-g5-1-protected-sentinel-v6"]
    timings = read_tsv(timing_path)
    schedule = read_tsv(schedule_path)
    expected_ids = {row["expectation_id"] for row in read_tsv(expected_path)}
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

    timing_by_ordinal = {int(row["ordinal"]): row for row in timings}
    maximum_rss = 0
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
        database = wrapper.get("post_database_sha256")
        authority = wrapper.get("post_authority_sha256")
        work = wrapper.get("mutation_work_sha256")
        if q_current not in (None, 0):
            failures.append(f"{label}:terminal-q")
        if transactions not in (None, 1) or commits not in (None, 1):
            failures.append(f"{label}:transaction-commit")
        if root is None or transition is None or database is None or authority is None or work is None:
            failures.append(f"{label}:post-state-custody")
        if wrapper.get("post_database_hash_semantics") != "physical-byte-parity-only-not-logical-digest":
            failures.append(f"{label}:database-hash-label")
        mode = wrapper.get("mode")
        g4 = wrapper.get("role") == "g4_verified"
        scrub_calls = None if g4 else required(product, "edit_base_complete_scrub_calls", failures, label)
        scrub_bytes = None if g4 else required(product, "edit_base_complete_scrub_canonical_bytes", failures, label)
        carry = None if g4 else required(product, "verified_carry_forward", failures, label)
        if mode == "trusted-local-dev":
            if scrub_calls not in (None, 0) or scrub_bytes not in (None, 0) or carry not in (None, False):
                failures.append(f"{label}:trusted-authority")
            if product.get("edit_base_provenance") != "trusted-local-unverified-closure":
                failures.append(f"{label}:trusted-provenance")
        elif not g4 and scrub_bytes is not None and scrub_bytes <= 0:
            failures.append(f"{label}:verified-scrub")
        external = row.get("external_time")
        if isinstance(external, dict):
            maximum_rss = max(maximum_rss, int(external.get("maximum_resident_set_size", 0)))

    for terminal in terminals:
        if terminal.get("status") != "PASS" or terminal.get("q_current") != 0:
            failures.append(f"terminal:{terminal.get('role')}:q-status")
        for name in ("argument_owners", "request_owners", "schedule_owners", "timing_owners", "report_owners"):
            if terminal.get(name) != 0:
                failures.append(f"terminal:{terminal.get('role')}:{name}")
        external = terminal.get("external_time", {})
        maximum_rss = max(maximum_rss, int(external.get("maximum_resident_set_size", 0)))
    if campaign == "screen":
        counts = {}
        for row in semantic_rows:
            counts[row.get("case")] = counts.get(row.get("case"), 0) + 1
            if row.get("status") != "PASS" or row.get("cleanup_ok") is not True or row.get("residue") is not False or row.get("q_current") != 0:
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
            if case == "trusted-verified-reopen" and (row.get("commits") != 1 or row.get("verified_carry_forward") is not False):
                failures.append("semantic:trusted-verified-reopen")
        if {row.get("reconciliation") for row in semantic_rows if row.get("case") == "reconciliation"} != {
            "NotAttempted", "PriorVisible", "RequestedVisible", "DifferentHead", "Ambiguous"
        }:
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
            range_exact = (
                product.get("range_measurements") == [S07_RANGE_MEASUREMENT]
                and row.get("deterministic_range") == S07_RANGE_MEASUREMENT
                if route == "range" else row.get("deterministic_range") is None
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
                and (row.get("post_database_sha256") == custody.get("database_sha256")) == (route == "range")
            )
            if not all((
                row.get("status") == "PASS", row.get("sequence_id") == "S07",
                row.get("executable_sha256") == S07_G4_SHA256,
                row.get("frozen_fixture_sha256") == S07_FIXTURE_SHA256,
                row.get("probe_fixture_sha256") == S07_FIXTURE_SHA256,
                row.get("post_database_hash_semantics") == "physical-byte-parity-only-not-logical-digest",
                set(custody) == {"database_sha256", "authority_sha256", "expectations_sha256"},
                all(isinstance(value, str) and len(value) == 64 for value in custody.values()),
                row.get("row_environment") == expected_env, commands_exact,
                row.get("pre_cleanup_residue") == [], bool(row.get("base_custody")),
                product.get("status") == "PASS", product.get("error") is None,
                product_exact, row.get("deterministic_tuple") == expected, range_exact, hashes_bound,
                row.get("mutation_work") == expected_work,
                row.get("mutation_work_sha256") == work_hash,
            )):
                failures.append(f"S07:{route}:hard")
            maximum_rss = max(maximum_rss, int(row.get("external_time", {}).get("maximum_resident_set_size", 0)))
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
            for name in ("post_database_sha256", "post_authority_sha256", "mutation_work_sha256"):
                if control_by_pair[pair]["wrapper"].get(name) != candidate_by_pair[pair]["wrapper"].get(name):
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
        "semantic_results": sorted(
            (
                {
                    key: row.get(key)
                    for key in (
                        "case", "integrity_mode", "error", "later_snapshot_error",
                        "publication_status", "reconciliation", "head_unchanged", "transactions",
                        "commits", "edit_base_complete_scrub_calls",
                        "edit_base_complete_scrub_canonical_bytes", "verified_carry_forward",
                        "cleanup_ok", "residue", "q_current",
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
                    "pre_cleanup_residue": row.get("pre_cleanup_residue"),
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
                    "post_database_sha256": row.get("post_database_sha256"),
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
    return {"schema": "phase4-g5-1-primary-analysis-v6", "status": "PASS" if not failures else "REVISE", "normalized": normalized}


def main():
    if len(sys.argv) != 6:
        raise SystemExit("usage: primary.py RAW TIMINGS SCHEDULE EXPECTED OUTPUT")
    result = analyze(*map(pathlib.Path, sys.argv[1:5]))
    pathlib.Path(sys.argv[5]).write_text(compact(result) + "\n", encoding="utf-8")
    return 0 if result["status"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
