#!/usr/bin/env python3
import csv
import json
import pathlib
import sys


RSS_CAP = 20_971_520
PHASES = [
    "store_preflight_ns", "sqlite_open_and_profile_ns", "visible_head_and_transition_ns",
    "edit_base_scope_ns", "mapping_and_construction_ns", "proof_ns",
    "publication_commit_ns", "reconciliation_ns",
]


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
    rows = [value for value in documents if value.get("schema") == "phase4-g5-1-operation-v3"]
    ends = [value for value in documents if value.get("schema") == "phase4-g5-trusted-child-terminal-v3"]
    semantic = [value for value in documents if value.get("schema") == "phase4-g5-trusted-semantic-v3"]
    semantic_ends = [value for value in documents if value.get("schema") == "phase4-g5-trusted-semantic-terminal-v3"]
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
        elif meta.get("role") != "g4_verified":
            scrub = product.get("edit_base_complete_scrub_canonical_bytes")
            if scrub is not None and scrub <= 0:
                problems.add(f"{tag}:verified-scrub")
        for field in ("post_database_sha256", "post_authority_sha256", "mutation_work_sha256"):
            if not meta.get(field):
                problems.add(f"{tag}:missing-{field}")
        if meta.get("post_database_hash_semantics") != "physical-byte-parity-only-not-logical-digest":
            problems.add(f"{tag}:database-hash-label")
        external = value.get("external_time", {})
        if isinstance(external, dict):
            rss_peak = max(rss_peak, int(external.get("maximum_resident_set_size", 0)))

    for end in ends:
        identity = end.get("role")
        if end.get("status") != "PASS" or end.get("q_current") != 0:
            problems.add(f"terminal:{identity}:q-status")
        for owner in ("argument_owners", "request_owners", "schedule_owners", "timing_owners", "report_owners"):
            if end.get(owner) != 0:
                problems.add(f"terminal:{identity}:{owner}")
        rss_peak = max(rss_peak, int(end.get("external_time", {}).get("maximum_resident_set_size", 0)))
    if campaign == "screen":
        counts = {}
        for value in semantic:
            counts[value.get("case")] = counts.get(value.get("case"), 0) + 1
            if value.get("status") != "PASS" or value.get("cleanup_ok") is not True or value.get("residue") is not False or value.get("q_current") != 0:
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
            if case == "trusted-verified-reopen" and (value.get("commits") != 1 or value.get("verified_carry_forward") is not False):
                problems.add("semantic:trusted-verified-reopen")
        if {value.get("reconciliation") for value in semantic if value.get("case") == "reconciliation"} != {"NotAttempted", "PriorVisible", "RequestedVisible", "DifferentHead", "Ambiguous"}:
            problems.add("semantic:reconciliation-set")
        if len(semantic_ends) != 4 or any(value.get("status") != "PASS" or value.get("q_current") != 0 for value in semantic_ends):
            problems.add("semantic-terminal")
        for value in semantic_ends:
            rss_peak = max(rss_peak, int(value.get("external_time", {}).get("maximum_resident_set_size", 0)))
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
                "edit_base_complete_scrub_canonical_bytes", "verified_carry_forward",
                "cleanup_ok", "residue", "q_current",
            )} for value in semantic),
            key=packed,
        ),
        "maximum_rss_bytes": rss_peak,
        "rss_limit_bytes": RSS_CAP,
        "comparisons": comparisons,
        "hard_failures": sorted(problems),
    }
    return {
        "schema": "phase4-g5-1-independent-recomputation-v3",
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
