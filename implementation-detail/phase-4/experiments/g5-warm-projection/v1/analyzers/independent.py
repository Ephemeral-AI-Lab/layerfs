#!/usr/bin/env python3
import hashlib, json, pathlib, sys

POPS = {"self-check": [1, 0], "screen-count": [1, 1], "screen": [2, 2], "gate": [64, 100]}
PLANS = {"screen": ["self-check", "screen-count", "screen"], "gate": ["self-check", "screen-count", "gate"]}
def canonical(v): return json.dumps(v, sort_keys=True, separators=(",", ":"))
def rank(v, p):
    v = sorted(v)
    return v[max(1, (len(v) * p + 99) // 100) - 1]

def audit(record):
    mode, p, bad = record.get("mode"), record.get("product", {}), []
    def check(ok, name):
        if not ok: bad.append(f"{mode}:{name}")
    check(p.get("schema") == "phase4-g5-projection-suite-v1" and p.get("status") == "PASS" and p.get("mode") == mode, "schema/status/mode")
    check([p.get("exact_every_root_population"), p.get("latest_following_population")] == POPS.get(mode), "populations")
    exact, latest = POPS.get(mode, [-1, -1])
    expected = (exact + latest + 5, exact + min(latest, 2) + 4, latest - min(latest, 2) + 1)
    check(p.get("worker_count") == 1 and p.get("max_in_flight") in (0, 1) and p.get("max_pending") in (0, 1), "worker/slots")
    s = p.get("started", -1)
    check((p.get("submitted"), s, p.get("published"), p.get("coalesced")) == (expected[0], expected[1], expected[1], expected[2]) and not any(p.get(k) for k in ("cancelled", "failed", "stale")), "exact-vs-latest-conservation")
    check(p.get("submitted") == p.get("coalesced", 0) + s, "request-conservation")
    check(s == p.get("published", -1) + p.get("cancelled", -1) + p.get("failed", -1) + p.get("stale", -1), "build-conservation")
    check(p.get("seed_rotations") == p.get("published"), "seed-rotation")
    check(p.get("projected_equals_last_requested") is True and p.get("projected_root") == p.get("last_requested_root"), "terminal-root")
    check(all(p.get(k) == 0 for k in ("sqlite_write_calls", "sqlite_transactions", "sqlite_commits", "sqlite_busy_errors", "sqlite_locked_errors")), "sqlite-read-only")
    check(p.get("foreground_transactions") == p.get("foreground_commits") == 1 and p.get("contention_intervals_overlap") is True, "foreground-contention")
    check(p.get("contention_worker_start_ns", 1) < p.get("contention_worker_end_ns", 0) and p.get("contention_foreground_start_ns", 1) < p.get("contention_foreground_end_ns", 0) and max(p.get("contention_worker_start_ns", 0), p.get("contention_foreground_start_ns", 0)) < min(p.get("contention_worker_end_ns", 0), p.get("contention_foreground_end_ns", 0)), "contention-equation")
    check(tuple(p.get(k) for k in ("reader_barrier_autocommit", "reader_barrier_scope_live", "reader_commit_autocommit", "reader_commit_scope_live", "foreground_commit_primary_code", "foreground_commit_extended_code")) == (1, 0, 1, 0, 0, 0), "contention-sqlite-state")
    check(1 <= p.get("full_fallbacks", -1) <= s and 1 <= p.get("range_fetches", -1) <= 256 * s and 0 < p.get("fetched_bytes", -1) <= 8_388_608 * s, "bounded-routes")
    check(1 <= p.get("clone_successes", -1) <= p.get("clone_calls", -1) <= s, "clone-counters")
    zero = ("terminal_in_flight", "terminal_pending", "terminal_workers", "terminal_active_descriptors", "terminal_successor_descriptors", "terminal_temp_residue", "q_terminal")
    check(p.get("max_buffer_bytes", 1_048_577) <= 1_048_576 and all(p.get(k) == 0 for k in zero), "terminal/buffer")
    check(p.get("shutdown") == "drained" and p.get("checkpoint_outside_service_timer") is True, "shutdown/checkpoint")
    for name, middle, tail in (("exact", 5_000_000, 8_000_000), ("sparse", 6_000_000, 10_000_000)):
        values = p.get(name + "_build_ns")
        valid = isinstance(values, list) and bool(values) and all(isinstance(x, int) and not isinstance(x, bool) and x >= 0 for x in values)
        check(valid, name + "-timers")
        if valid: check([rank(values, 50), rank(values, 95)] == [p.get(name + "_p50_ns"), p.get(name + "_p95_ns")] and p[name + "_p50_ns"] <= middle and p[name + "_p95_ns"] <= tail, name + "-latency")
    evidence = p.get("build_evidence")
    buckets = [[], [], [], []]
    policy_counts = {"ExactEveryRoot": 0, "LatestFollowing": 0}
    evidence_ok = isinstance(evidence, list) and len(evidence) == s
    if evidence_ok:
        for item in evidence:
            evidence_ok = isinstance(item, dict) and isinstance(item.get("contention"), bool) and item.get("plan") in {"Ranges", "FullFallback"} and item.get("policy") in policy_counts and all(isinstance(item.get(field), int) and not isinstance(item[field], bool) and item[field] >= 0 for field in ("parent_length", "target_length", "range_count", "wall_ns", "ordinal"))
            if not evidence_ok: break
            policy_counts[item["policy"]] += 1
            fallback = item["plan"] == "FullFallback" or item["parent_length"] != item["target_length"]
            index = (3 if item["contention"] else 2) if fallback else (0 if item["range_count"] == 0 else 1)
            if not fallback and item["contention"]: evidence_ok = False; break
            buckets[index].append(item["wall_ns"])
    check(evidence_ok, "build-evidence")
    if evidence_ok:
        check(buckets == [p.get("exact_build_ns"), p.get("sparse_build_ns"), p.get("full_fallback_build_ns"), p.get("contention_full_fallback_build_ns")], "route-classification")
        check(policy_counts == {"ExactEveryRoot": exact + 2, "LatestFollowing": min(latest, 2) + 2}, "semantic-policy-execution")
        check(len(buckets[0]) == 1 and len(buckets[1]) == exact + min(latest, 2) + 1, "execution-route-classification")
        fallbacks = buckets[2]
        contention_fallbacks = buckets[3]
        check(bool(fallbacks) and len(fallbacks) + len(contention_fallbacks) == p.get("full_fallbacks") and [rank(fallbacks, 50), rank(fallbacks, 95)] == [p.get("full_fallback_p50_ns"), p.get("full_fallback_p95_ns")], "fallback-timers")
        check(p.get("full_fallback_g3_bound_ns") == 329_237_000 and p.get("full_fallback_within_g3_bound") is True and max(fallbacks) <= 329_237_000, "fallback-g3-bound")
        check(len(fallbacks) == 1 and len(contention_fallbacks) == 1 and [rank(contention_fallbacks, 50), rank(contention_fallbacks, 95)] == [p.get("contention_full_fallback_p50_ns"), p.get("contention_full_fallback_p95_ns")] and p.get("contention_full_fallback_latency_claim") == "NotClaimedDifferentConcurrentExecutionShape", "contention-fallback")
    check(isinstance(p.get("reader_initialization_ns"), int) and not isinstance(p["reader_initialization_ns"], bool) and p["reader_initialization_ns"] > 0 and p.get("reader_initialization_classification") == "OneTimeReadOnlyProcessInitializationInsideCompleteWallOutsideServiceSamples", "reader-initialization")
    check((p.get("reader_initialization_calls"), p.get("reader_initialization_bytes_requested")) == (1, 1) and all(isinstance(p.get(field), int) and not isinstance(p[field], bool) and p[field] > 0 for field in ("reader_initialization_sql_queries", "reader_initialization_authenticated_objects", "reader_initialization_authenticated_bytes", "reader_initialization_q_high_water")), "reader-initialization-work")
    check(all(p.get(field) is True for field in ("reader_initialization_read_only", "reader_initialization_query_only", "reader_initialization_inside_complete_wall", "reader_initialization_excluded_from_service_samples")) and len(p.get("build_evidence", ())) == s, "reader-initialization-boundary")
    check(record.get("maximum_resident_set_size", 33_554_433) <= 33_554_432, "rss")
    check(record.get("clone", {}).get("method") == "APFSCloneCpC" and record.get("clone", {}).get("inventory_equal") is True, "clone")
    return bad

def recompute(row):
    bad, phase, records = [], row.get("phase"), row.get("products")
    plan = PLANS.get(phase)
    if row.get("schema") != "phase4-g5-2-harness-row-v1" or row.get("status") != "PASS": bad.append("row-schema/status")
    if not isinstance(records, list) or [x.get("mode") for x in records] != plan or row.get("product_processes") != 3: bad.append("product-processes/modes")
    else:
        for record in records: bad.extend(audit(record))
    if records and row.get("maximum_resident_set_size") != max(x.get("maximum_resident_set_size", -1) for x in records): bad.append("aggregate-rss-max")
    stage, terminal = row.get("analysis_stage"), row.get("terminal")
    if stage == "preliminary":
        if terminal is not None: bad.append("preliminary-terminal-present")
        complete_wall = None
    elif stage == "final":
        okay = isinstance(terminal, dict) and terminal.get("schema") == "phase4-g5-2-terminal-v1" and terminal.get("status") == "PASS" and terminal.get("complete_wall_ns", -1) <= terminal.get("limit_ns", -2) and terminal.get("lock_released") is True and terminal.get("terminal_fixture_roots") == 0
        if not okay: bad.append("terminal-complete-wall")
        if isinstance(terminal, dict) and row.get("terminal_sha256") != hashlib.sha256((canonical(terminal) + "\n").encode()).hexdigest(): bad.append("terminal-binding")
        complete_wall = terminal.get("complete_wall_ns") if isinstance(terminal, dict) else None
    else:
        bad.append("analysis-stage")
        complete_wall = None
    if row.get("cache_state") != "WarmUnknownPreparedFixtureAPFSClone" or row.get("cold_reopen_claim") is not False: bad.append("cache-claim")
    p = records[-1].get("product", {}) if records else {}
    normalized = {"status": "PASS" if not bad else "REVISE", "analysis_stage": stage, "phase": phase, "hard_failures": sorted(bad), "product_processes": row.get("product_processes"), "sentinel_modes": plan[:-1] if plan else None, "primary_mode": plan[-1] if plan else None, "exact_population": p.get("exact_every_root_population"), "latest_population": p.get("latest_following_population"), "projected_root": p.get("projected_root"), "rss_bytes": row.get("maximum_resident_set_size"), "complete_wall_ns": complete_wall, "exact_p50_ns": p.get("exact_p50_ns"), "exact_p95_ns": p.get("exact_p95_ns"), "sparse_p50_ns": p.get("sparse_p50_ns"), "sparse_p95_ns": p.get("sparse_p95_ns"), "fallback_p50_ns": p.get("full_fallback_p50_ns"), "fallback_p95_ns": p.get("full_fallback_p95_ns")}
    return {"schema": "phase4-g5-2-independent-v1", "normalized": normalized, "normalized_sha256": hashlib.sha256(canonical(normalized).encode()).hexdigest()}

def main():
    rows = [json.loads(x) for x in pathlib.Path(sys.argv[1]).read_text().splitlines() if x.strip()]
    report = recompute(rows[0]) if len(rows) == 1 else {"schema": "phase4-g5-2-independent-v1", "normalized": {"status": "REVISE", "hard_failures": ["row-count"]}}
    pathlib.Path(sys.argv[2]).write_text(canonical(report) + "\n"); print(canonical(report))
    return report["normalized"]["status"] != "PASS"
if __name__ == "__main__": raise SystemExit(main())
