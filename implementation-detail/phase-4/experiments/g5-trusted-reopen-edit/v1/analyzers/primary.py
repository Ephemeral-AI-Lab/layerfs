#!/usr/bin/env python3
import csv
import json
import pathlib
import statistics
import sys


OP_SCHEMA = "phase4-g5-1-operation-v1"
TERMINAL_SCHEMA = "phase4-g5-trusted-child-terminal-v1"
RSS_LIMIT = 20_971_520
TIMER_FIELDS = (
    "store_preflight_ns", "sqlite_open_and_profile_ns", "visible_head_and_transition_ns",
    "edit_base_scope_ns", "mapping_and_construction_ns", "proof_ns",
    "publication_commit_ns", "reconciliation_ns",
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
        database = required(product, "logical_database_digest", failures, label)
        if q_current not in (None, 0):
            failures.append(f"{label}:terminal-q")
        if transactions not in (None, 1) or commits not in (None, 1):
            failures.append(f"{label}:transaction-commit")
        if root is None or transition is None or database is None:
            pass
        mode = wrapper.get("mode")
        scrub_calls = required(product, "edit_base_complete_scrub_calls", failures, label)
        scrub_bytes = required(product, "edit_base_complete_scrub_canonical_bytes", failures, label)
        carry = required(product, "verified_carry_forward", failures, label)
        if mode == "trusted-local-dev":
            if scrub_calls not in (None, 0) or scrub_bytes not in (None, 0) or carry not in (None, False):
                failures.append(f"{label}:trusted-authority")
            if product.get("edit_base_provenance") != "trusted-local-unverified-closure":
                failures.append(f"{label}:trusted-provenance")
        elif wrapper.get("role") != "g4_verified" and scrub_bytes is not None and scrub_bytes <= 0:
            failures.append(f"{label}:verified-scrub")
        external = row.get("external_time")
        if isinstance(external, dict):
            maximum_rss = max(maximum_rss, int(external.get("maximum_resident_set_size", 0)))

    for terminal in terminals:
        if terminal.get("status") != "PASS" or terminal.get("q_current") != 0:
            failures.append(f"terminal:{terminal.get('role')}:q-status")
        for name in ("argument_owners", "schedule_owners", "timing_owners", "report_owners"):
            if terminal.get(name) != 0:
                failures.append(f"terminal:{terminal.get('role')}:{name}")
        external = terminal.get("external_time", {})
        maximum_rss = max(maximum_rss, int(external.get("maximum_resident_set_size", 0)))
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
            for name in ("root_id", "transition_id", "logical_database_digest"):
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
        "terminal_rows": len(terminals),
        "maximum_rss_bytes": maximum_rss,
        "rss_limit_bytes": RSS_LIMIT,
        "comparisons": comparison_results,
        "hard_failures": sorted(set(failures)),
    }
    return {"schema": "phase4-g5-1-primary-analysis-v1", "status": "PASS" if not failures else "REVISE", "normalized": normalized}


def main():
    if len(sys.argv) != 6:
        raise SystemExit("usage: primary.py RAW TIMINGS SCHEDULE EXPECTED OUTPUT")
    result = analyze(*map(pathlib.Path, sys.argv[1:5]))
    pathlib.Path(sys.argv[5]).write_text(compact(result) + "\n", encoding="utf-8")
    return 0 if result["status"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
