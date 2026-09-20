#!/usr/bin/env python3
"""Reproduce every derived number of the #209 instrument round from the raw files.

Two diagnostic rows, one binary (sha256 in binary-archive/instrumented.sha256), the
probe enabled by LAYERFS_STORAGE_RCA_PROBE=1 in both. Neither row is a sample: the
probe observes the save's own connection, so its rows are diagnostics and every
budget class is NOT_RUN. Run:  python3 analyze.py
"""
import hashlib
import json
import re
from pathlib import Path

D = Path(__file__).resolve().parent
ROWS = ["rca-instrument", "rca-packcache"]
BUCKETS = ["begin", "commit", "rollback", "next_pack", "insert_pack", "append_pack",
           "insert_objects", "ceiling", "advance_pack", "publish_policy", "publish_save",
           "ordinal_select", "ordinal_update", "value_group", "pool_latest", "locator",
           "pack_bytes", "candidates_read", "candidates_write", "scope", "other"]


def load(label):
    raw = D / "runs" / f"{label}-history-stride10" / "raw"
    trace = {}
    for line in open(raw / "trace.jsonl"):
        record = json.loads(line)
        trace.setdefault(record["key"], record["value"])
    timing = json.load(open(raw / "timing.json"))
    span = {}
    for state in timing["children"]:
        for child in state["children"]:
            span[child["name"]] = span.get(child["name"], 0) + child["elapsed_ns"]
    declaration = json.load(open(D / "runs" / f"{label}-history-stride10"
                                 / "perf-declaration.json"))
    return {
        "operation_ns": sum(child["elapsed_ns"] for child in timing["children"]),
        "accept_ns": span.get("storage.accept_loop", 0),
        "filesystem_ns": span.get("filesystem", 0),
        "trace": trace,
        "store": hashlib.sha256((raw / "sample.sqlite").read_bytes()).hexdigest(),
        "binary": declaration["binary_sha256"],
        "extra_environment": declaration["extra_environment"],
        "seal": declaration["harness_source_seal"],
    }


def main():
    for label in ROWS:
        row = load(label)
        t = row["trace"]
        appends = t["delta.pack_appends"]
        commits = t["delta.rca.commit.calls"]
        print(f"== {label} ==")
        print(f"  binary {row['binary'][:16]}  store {row['store'][:16]}  "
              f"harness seal {row['seal'][:12]}  probe {row['extra_environment']}")
        print(f"  operation {row['operation_ns']/1e9:.3f} s   accept_loop "
              f"{row['accept_ns']/1e9:.3f} s   filesystem {row['filesystem_ns']/1e9:.3f} s")
        charged = sum(t[k] for k in ("delta.profile_commit_ns", "delta.profile_sql_ns",
                                     "delta.profile_resolve_ns", "delta.profile_full_ns",
                                     "delta.profile_delta_ns", "delta.profile_group_ns",
                                     "delta.profile_place_ns"))
        engine = sum(v for k, v in t.items()
                     if k.startswith("delta.rca.") and k.endswith(".engine_ns"))
        print(f"  charged {charged/1e9:.3f} s   engine-measured {engine/1e9:.3f} s   "
              f"uncharged {row['accept_ns']-charged:.0f} ns")
        print(f"  commit_ns {t['delta.profile_commit_ns']/1e9:.3f} s "
              f"({t['delta.profile_commit_ns']/appends/1000:.2f} us/append)   "
              f"sql_ns {t['delta.profile_sql_ns']/1e9:.3f} s   "
              f"resolve_ns {t['delta.profile_resolve_ns']/1e9:.3f} s")
        print(f"  Q1: pages/commit {t['delta.rca.pages_at_commit']/commits:.3f}   "
              f"body pages/append {t['delta.rca.pack_payload_pages']/appends:.3f}   "
              f"excess {t['delta.rca.pages_at_commit']-t['delta.rca.pack_payload_pages']} pages "
              f"({(t['delta.rca.pages_at_commit']-t['delta.rca.pack_payload_pages'])/commits:.2f}/commit)")
        print(f"      cache_write {t['delta.rca.cache_write']}  spill {t['delta.rca.cache_spill']}  "
              f"hit {t['delta.rca.cache_hit']}  miss {t['delta.rca.cache_miss']}  "
              f"cache_used {t['delta.rca.cache_used_end']} B  stmt_used {t['delta.rca.stmt_used_end']} B")
        print(f"      page_count {t['delta.rca.page_count_start']} -> {t['delta.rca.page_count_end']}  "
              f"freelist {t['delta.rca.freelist_start']} -> {t['delta.rca.freelist_end']}")
        print(f"  Q4: begin {t['delta.rca.begin_ns']/t['delta.rca.begin.calls']/1000:.3f} us wall / "
              f"{t['delta.rca.begin.engine_ns']/t['delta.rca.begin.calls']/1000:.3f} us engine, "
              f"{t['delta.rca.begin.calls']} calls = {t['delta.rca.begin_ns']/1e9:.3f} s")
        print(f"      next_pack {t['delta.rca.next_pack_ns']/t['delta.rca.next_pack.calls']/1000:.3f} us wall / "
              f"{t['delta.rca.next_pack.engine_ns']/t['delta.rca.next_pack.calls']/1000:.3f} us engine")
        print(f"      commit stmt {t['delta.rca.commit_statement_ns']/commits/1000:.3f} us wall / "
              f"{t['delta.rca.commit.engine_ns']/commits/1000:.3f} us engine")
        print("  Q2: per-statement engine counters (ms-resolution straddle estimator)")
        print(f"      {'bucket':<16}{'calls':>8}{'us/call':>10}{'vm/call':>9}"
              f"{'scan':>6}{'sort':>6}{'auto':>6}{'repr':>6}")
        for b in BUCKETS:
            calls = t[f"delta.rca.{b}.calls"]
            if calls == 0:
                continue
            print(f"      {b:<16}{calls:>8}"
                  f"{t[f'delta.rca.{b}.engine_ns']/calls/1000:>10.3f}"
                  f"{t[f'delta.rca.{b}.vm_steps']/calls:>9.1f}"
                  f"{t[f'delta.rca.{b}.fullscan_steps']:>6}{t[f'delta.rca.{b}.sorts']:>6}"
                  f"{t[f'delta.rca.{b}.autoindexes']:>6}{t[f'delta.rca.{b}.reprepares']:>6}")
        # The pack-cache counters were added to the probe after the first row was
        # taken, so the first row does not carry them; that is recorded, not hidden.
        if "delta.rca.pack_cache_hits" in t:
            hits = t["delta.rca.pack_cache_hits"]
            misses = t["delta.rca.pack_cache_misses"]
            print(f"      pack cache: hits {hits} misses {misses} "
                  f"rate {hits/(hits+misses):.4f} distinct {t['delta.rca.pack_cache_distinct']} "
                  f"clears {t['delta.rca.pack_cache_clears']}")
        else:
            print("      pack cache: not instrumented in this row (the counters were "
                  "added after it was taken)")
        print()
    print("== micro-probe arms (scratch/step-cost-arms.txt, one process, round-robin) ==")
    for line in open(D / "scratch" / "step-cost-arms.txt"):
        if "us per call" in line or line.startswith("step arm"):
            print("  " + line.rstrip())
        elif re.match(r"^[a-z].*\s+[0-9.]+$", line):
            print("  " + line.rstrip())


if __name__ == "__main__":
    main()
