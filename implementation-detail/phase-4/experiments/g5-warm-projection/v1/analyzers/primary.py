#!/usr/bin/env python3
import hashlib, json, pathlib, sys

POPS = {"self-check": (1, 0), "screen-count": (1, 1), "screen": (2, 2), "gate": (64, 100)}
PLANS = {"screen": ["self-check", "screen-count", "screen"], "gate": ["self-check", "screen-count", "gate"]}
TERMINAL = ("terminal_in_flight", "terminal_pending", "terminal_workers", "terminal_active_descriptors", "terminal_successor_descriptors", "terminal_temp_residue", "q_terminal")

def compact(v): return json.dumps(v, sort_keys=True, separators=(",", ":"))
def pct(v, p):
    v = sorted(v)
    return v[max(0, (len(v) * p + 99) // 100 - 1)]

def inspect(record, failures):
    mode, p = record.get("mode"), record.get("product", {})
    def need(ok, name):
        if not ok: failures.append(f"{mode}:{name}")
    need(p.get("schema") == "phase4-g5-projection-suite-v1" and p.get("status") == "PASS" and p.get("mode") == mode, "schema/status/mode")
    need((p.get("exact_every_root_population"), p.get("latest_following_population")) == POPS.get(mode), "populations")
    exact, latest = POPS.get(mode, (-1, -1))
    expected_started = exact + min(latest, 2) + 4
    expected_submitted = exact + latest + 5
    expected_coalesced = latest - min(latest, 2) + 1
    need(p.get("worker_count") == 1 and p.get("max_in_flight", 2) <= 1 and p.get("max_pending", 2) <= 1, "worker/slots")
    started = p.get("started", -1)
    need((p.get("submitted"), started, p.get("published"), p.get("coalesced")) == (expected_submitted, expected_started, expected_started, expected_coalesced) and all(p.get(k) == 0 for k in ("cancelled", "failed", "stale")), "exact-vs-latest-conservation")
    need(p.get("submitted") == p.get("coalesced", 0) + started, "request-conservation")
    need(started == sum(p.get(k, -1) for k in ("published", "cancelled", "failed", "stale")), "build-conservation")
    need(p.get("seed_rotations") == p.get("published"), "seed-rotation")
    need(p.get("projected_equals_last_requested") is True and p.get("projected_root") == p.get("last_requested_root"), "terminal-root")
    need(all(p.get(k) == 0 for k in ("sqlite_write_calls", "sqlite_transactions", "sqlite_commits", "sqlite_busy_errors", "sqlite_locked_errors")), "sqlite-read-only")
    need(p.get("foreground_transactions") == 1 and p.get("foreground_commits") == 1 and p.get("contention_intervals_overlap") is True, "foreground-contention")
    need(p.get("contention_worker_start_ns", 1) < p.get("contention_worker_end_ns", 0) and p.get("contention_foreground_start_ns", 1) < p.get("contention_foreground_end_ns", 0) and p.get("contention_foreground_start_ns", 1) < p.get("contention_worker_end_ns", 0) and p.get("contention_worker_start_ns", 1) < p.get("contention_foreground_end_ns", 0), "contention-equation")
    need([p.get("reader_barrier_autocommit"), p.get("reader_barrier_scope_live"), p.get("reader_commit_autocommit"), p.get("reader_commit_scope_live"), p.get("foreground_commit_primary_code"), p.get("foreground_commit_extended_code")] == [1, 0, 1, 0, 0, 0], "contention-sqlite-state")
    need(1 <= p.get("full_fallbacks", -1) <= started and 1 <= p.get("range_fetches", -1) <= 256 * started and 0 < p.get("fetched_bytes", -1) <= 8_388_608 * started, "bounded-routes")
    need(1 <= p.get("clone_successes", -1) <= p.get("clone_calls", -1) <= started, "clone-counters")
    need(p.get("max_buffer_bytes", 1_048_577) <= 1_048_576 and all(p.get(k) == 0 for k in TERMINAL), "terminal/buffer")
    need(p.get("shutdown") == "drained" and p.get("checkpoint_outside_service_timer") is True, "shutdown/checkpoint")
    for label, p50max, p95max in (("exact", 5_000_000, 8_000_000), ("sparse", 6_000_000, 10_000_000)):
        values = p.get(f"{label}_build_ns")
        valid = isinstance(values, list) and bool(values) and all(type(x) is int and x >= 0 for x in values)
        need(valid, label + "-timers")
        if valid: need((pct(values, 50), pct(values, 95)) == (p.get(f"{label}_p50_ns"), p.get(f"{label}_p95_ns")) and p[f"{label}_p50_ns"] <= p50max and p[f"{label}_p95_ns"] <= p95max, label + "-latency")
    evidence = p.get("build_evidence")
    classified = {"exact": [], "sparse": [], "full_fallback": [], "contention_fallback": []}
    policies = {"ExactEveryRoot": 0, "LatestFollowing": 0}
    valid_evidence = isinstance(evidence, list) and len(evidence) == started
    if valid_evidence:
        for value in evidence:
            valid_evidence = isinstance(value, dict) and type(value.get("contention")) is bool and value.get("plan") in ("Ranges", "FullFallback") and value.get("policy") in policies and all(type(value.get(k)) is int and value[k] >= 0 for k in ("parent_length", "target_length", "range_count", "wall_ns", "ordinal"))
            if not valid_evidence: break
            policies[value["policy"]] += 1
            if value["plan"] == "FullFallback" or value["parent_length"] != value["target_length"]: route = "contention_fallback" if value["contention"] else "full_fallback"
            elif value["range_count"] == 0: route = "exact"
            else: route = "sparse"
            if route in ("exact", "sparse") and value["contention"]: valid_evidence = False; break
            classified[route].append(value["wall_ns"])
    need(valid_evidence, "build-evidence")
    if valid_evidence:
        need(classified["exact"] == p.get("exact_build_ns") and classified["sparse"] == p.get("sparse_build_ns") and classified["full_fallback"] == p.get("full_fallback_build_ns") and classified["contention_fallback"] == p.get("contention_full_fallback_build_ns"), "route-classification")
        need(policies == {"ExactEveryRoot": exact + 2, "LatestFollowing": min(latest, 2) + 2}, "semantic-policy-execution")
        need(len(classified["exact"]) == 1 and len(classified["sparse"]) == exact + min(latest, 2) + 1, "execution-route-classification")
        fallback = classified["full_fallback"]
        contention_fallback = classified["contention_fallback"]
        need(bool(fallback) and len(fallback) + len(contention_fallback) == p.get("full_fallbacks") and (pct(fallback, 50), pct(fallback, 95)) == (p.get("full_fallback_p50_ns"), p.get("full_fallback_p95_ns")), "fallback-timers")
        need(p.get("full_fallback_g3_bound_ns") == 329_237_000 and p.get("full_fallback_within_g3_bound") is True and max(fallback) <= 329_237_000, "fallback-g3-bound")
        need(len(fallback) == 1 and len(contention_fallback) == 1 and (pct(contention_fallback, 50), pct(contention_fallback, 95)) == (p.get("contention_full_fallback_p50_ns"), p.get("contention_full_fallback_p95_ns")) and p.get("contention_full_fallback_latency_claim") == "NotClaimedDifferentConcurrentExecutionShape", "contention-fallback")
    need(type(p.get("reader_initialization_ns")) is int and p["reader_initialization_ns"] > 0 and p.get("reader_initialization_classification") == "OneTimeReadOnlyProcessInitializationInsideCompleteWallOutsideServiceSamples", "reader-initialization")
    need(p.get("reader_initialization_calls") == 1 and p.get("reader_initialization_bytes_requested") == 1 and p.get("reader_initialization_sql_queries", 0) > 0 and p.get("reader_initialization_authenticated_objects", 0) > 0 and p.get("reader_initialization_authenticated_bytes", 0) > 0 and p.get("reader_initialization_q_high_water", 0) > 0, "reader-initialization-work")
    need([p.get("reader_initialization_read_only"), p.get("reader_initialization_query_only"), p.get("reader_initialization_inside_complete_wall"), p.get("reader_initialization_excluded_from_service_samples")] == [True, True, True, True] and len(p.get("build_evidence", [])) == started, "reader-initialization-boundary")
    need(record.get("maximum_resident_set_size", 33_554_433) <= 33_554_432, "rss")
    need(record.get("clone", {}).get("method") == "APFSCloneCpC" and record.get("clone", {}).get("inventory_equal") is True, "clone")

def analyze(row):
    failures, phase, records = [], row.get("phase"), row.get("products")
    plan = PLANS.get(phase)
    if row.get("schema") != "phase4-g5-2-harness-row-v1" or row.get("status") != "PASS": failures.append("row-schema/status")
    if not isinstance(records, list) or [r.get("mode") for r in records] != plan or row.get("product_processes") != 3: failures.append("product-processes/modes")
    else:
        for record in records: inspect(record, failures)
    if records and row.get("maximum_resident_set_size") != max(r.get("maximum_resident_set_size", -1) for r in records): failures.append("aggregate-rss-max")
    stage, terminal = row.get("analysis_stage"), row.get("terminal")
    if stage == "preliminary":
        if terminal is not None: failures.append("preliminary-terminal-present")
        complete_wall = None
    elif stage == "final":
        valid_terminal = isinstance(terminal, dict) and terminal.get("schema") == "phase4-g5-2-terminal-v1" and terminal.get("status") == "PASS" and terminal.get("complete_wall_ns", -1) <= terminal.get("limit_ns", -2) and terminal.get("lock_released") is True and terminal.get("terminal_fixture_roots") == 0
        if not valid_terminal: failures.append("terminal-complete-wall")
        if isinstance(terminal, dict) and row.get("terminal_sha256") != hashlib.sha256((compact(terminal) + "\n").encode()).hexdigest(): failures.append("terminal-binding")
        complete_wall = terminal.get("complete_wall_ns") if isinstance(terminal, dict) else None
    else:
        failures.append("analysis-stage")
        complete_wall = None
    if row.get("cache_state") != "WarmUnknownPreparedFixtureAPFSClone" or row.get("cold_reopen_claim") is not False: failures.append("cache-claim")
    p = records[-1].get("product", {}) if records else {}
    normalized = {"status": "PASS" if not failures else "REVISE", "analysis_stage": stage, "phase": phase, "hard_failures": sorted(failures), "product_processes": row.get("product_processes"), "sentinel_modes": plan[:-1] if plan else None, "primary_mode": plan[-1] if plan else None, "exact_population": p.get("exact_every_root_population"), "latest_population": p.get("latest_following_population"), "projected_root": p.get("projected_root"), "rss_bytes": row.get("maximum_resident_set_size"), "complete_wall_ns": complete_wall, "exact_p50_ns": p.get("exact_p50_ns"), "exact_p95_ns": p.get("exact_p95_ns"), "sparse_p50_ns": p.get("sparse_p50_ns"), "sparse_p95_ns": p.get("sparse_p95_ns"), "fallback_p50_ns": p.get("full_fallback_p50_ns"), "fallback_p95_ns": p.get("full_fallback_p95_ns")}
    return {"schema": "phase4-g5-2-primary-v1", "normalized": normalized, "normalized_sha256": hashlib.sha256(compact(normalized).encode()).hexdigest()}

def main():
    rows = [json.loads(x) for x in pathlib.Path(sys.argv[1]).read_text().splitlines() if x.strip()]
    report = analyze(rows[0]) if len(rows) == 1 else {"schema": "phase4-g5-2-primary-v1", "normalized": {"status": "REVISE", "hard_failures": ["row-count"]}}
    pathlib.Path(sys.argv[2]).write_text(compact(report) + "\n"); print(compact(report))
    return report["normalized"]["status"] != "PASS"
if __name__ == "__main__": raise SystemExit(main())
