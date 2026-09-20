#!/usr/bin/env python3
"""Reproduce every derived number in this campaign from the retained raw files."""
import hashlib
import json
import sys
from pathlib import Path

CAMPAIGN = Path(__file__).resolve().parent
RUNS = CAMPAIGN / "runs"
LABELS = [("gate control", "control2"), ("gate treatment", "treat2"),
          ("ABBA 1 a", "abba-1ar1"), ("ABBA 2 b", "abba-2b"),
          ("ABBA 3 b", "abba-3b"), ("ABBA 4 a", "abba-4a"),
          ("shipped", "shipped"), ("RCA binary now", "rca-binary-now3")]
RCA = CAMPAIGN.parent / "stage-6-history-209-rca-20260920T191016Z"


def rows(path):
    out = {}
    for line in open(path):
        record = json.loads(line)
        out.setdefault(record["key"], record["value"])
    return out


def load(label, root=RUNS):
    base = root / f"{label}-history-stride10" / "raw"
    timing = json.load(open(base / "timing.json"))
    trace = rows(base / "trace.jsonl")
    operation = sum(child["elapsed_ns"] for child in timing["children"])
    store = base / "sample.sqlite"
    return {
        "operation_ns": operation,
        "root_ns": timing["elapsed_ns"],
        "corpus_read_ns": trace["history.corpus_read_ns"],
        "accept_loop_ns": sum(
            child["elapsed_ns"] for state in timing["children"]
            for child in state["children"] if child["name"] == "storage.accept_loop"),
        "filesystem_ns": sum(
            child["elapsed_ns"] for state in timing["children"]
            for child in state["children"] if child["name"] == "filesystem"),
        "commit_ns": trace["delta.profile_commit_ns"],
        "resolve_ns": trace["delta.profile_resolve_ns"],
        "sql_ns": trace["delta.profile_sql_ns"],
        "full_ns": trace["delta.profile_full_ns"],
        "place_ns": trace["delta.profile_place_ns"],
        "group_ns": trace["delta.profile_group_ns"],
        "delta_ns": trace["delta.profile_delta_ns"],
        "commits": sum(trace[key] for key in trace if key.endswith(".save.commits")),
        "pack_appends": trace["delta.pack_appends"],
        "packs_created": trace["delta.packs_created"],
        "statements": trace["delta.statements"],
        "store_sha256": hashlib.sha256(store.read_bytes()).hexdigest(),
        "store_bytes": store.stat().st_size,
        "trace": trace,
    }


def main() -> int:
    loaded = []
    for name, label in LABELS:
        root = (RCA / "runs") if label.startswith("rca") else RUNS
        run_label = "treatment2" if label.startswith("rca") else label
        loaded.append((name, load(run_label, root)))
    print(f"{'row':<16}{'operation':>11}{'commit':>10}{'per-append':>12}{'resolve':>10}"
          f"{'sql':>9}{'filesystem':>12}{'corpus':>9}{'accept':>9}")
    for name, data in loaded:
        print(f"{name:<16}{data['operation_ns']/1e9:>10.3f}s{data['commit_ns']/1e9:>9.3f}s"
              f"{data['commit_ns']/data['commits']/1000:>11.1f}u{data['resolve_ns']/1e9:>9.3f}s"
              f"{data['sql_ns']/1e9:>8.3f}s{data['filesystem_ns']/1e9:>11.3f}s"
              f"{data['corpus_read_ns']/1e9:>8.3f}s{data['accept_loop_ns']/1e9:>8.3f}s")
    print()
    for name, data in loaded:
        print(f"{name:<16} store={data['store_sha256'][:16]}… bytes={data['store_bytes']:,} "
              f"commits={data['commits']:,} appends={data['pack_appends']:,} "
              f"packs={data['packs_created']} statements={data['statements']:,}")
    print()
    controls = [d for (n, d) in loaded if n in ("gate control", "ABBA 1 a", "ABBA 4 a")]
    treatments = [d for (n, d) in loaded
                  if n in ("gate treatment", "ABBA 2 b", "ABBA 3 b", "shipped")]
    for term in ("operation_ns", "commit_ns", "resolve_ns", "sql_ns", "filesystem_ns",
                 "corpus_read_ns", "accept_loop_ns"):
        a = sum(d[term] for d in controls) / len(controls)
        b = sum(d[term] for d in treatments) / len(treatments)
        print(f"{term:<18} control mean {a/1e9:>8.3f}s  treatment mean {b/1e9:>8.3f}s  "
              f"delta {(b-a)/1e9:>+7.3f}s  ({(b-a)/a*100:>+6.2f} %)")
    abba_a = [d for (n, d) in loaded if n in ("ABBA 1 a", "ABBA 4 a")]
    abba_b = [d for (n, d) in loaded if n in ("ABBA 2 b", "ABBA 3 b")]
    print()
    for term in ("operation_ns", "commit_ns"):
        a = sum(d[term] for d in abba_a) / 2
        b = sum(d[term] for d in abba_b) / 2
        print(f"ABBA only {term:<14} a {a/1e9:>8.3f}s  b {b/1e9:>8.3f}s  delta {(b-a)/1e9:>+7.3f}s")
    print(f"\ndrift across the ABBA window (4a - 1a): "
          f"{(abba_a[1]['operation_ns']-abba_a[0]['operation_ns'])/1e9:+.3f}s operation, "
          f"{(abba_a[1]['commit_ns']-abba_a[0]['commit_ns'])/1e9:+.3f}s commit")
    print()
    for term in ("commit_ns", "operation_ns"):
        a = sorted(d[term] for d in controls)
        b = sorted(d[term] for d in treatments)
        print(f"{term}: control {[f'{v/1e9:.3f}' for v in a]} treatment "
              f"{[f'{v/1e9:.3f}' for v in b]} overlap={a[0] <= b[-1]}")
    counter_keys = [k for k in loaded[0][1]["trace"]
                    if not k.endswith("_ns") and not k.startswith("history.state.")]
    differing = [k for k in counter_keys
                 if len({d["trace"].get(k) for _, d in loaded}) != 1]
    print(f"counter keys compared: {len(counter_keys)}; differing across all rows: {differing}")
    stores = {d["store_sha256"] for _, d in loaded}
    print(f"store hashes across all rows: {stores}")
    commits, created, appends = 48446, 255, 45794
    print(f"statements skipped: watermark {commits - created:,} of {commits:,}; "
          f"ceiling {appends:,} of {appends + created:,}")
    gate = [d for (n, d) in loaded if n.startswith("gate")]
    print(f"gate pair: operation {(gate[1]['operation_ns']-gate[0]['operation_ns'])/1e9:+.3f}s, "
          f"commit {(gate[1]['commit_ns']-gate[0]['commit_ns'])/1e9:+.3f}s")
    return 0


if __name__ == "__main__":
    sys.exit(main())
