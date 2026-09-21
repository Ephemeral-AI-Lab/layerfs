#!/usr/bin/env python3
"""S0 custody inventory: every init_namespace receipt row, read from raw perf.jsonl.

Read-only. Emits one TSV row per sample record plus a machine-readable JSON dump.
"""
import json, os, sys, glob

ROOT = sys.argv[1]
CASES = {"namespace-100", "namespace-1000", "namespace-10000", "namespace-100000",
         "namespace-10000-text-v1", "namespace-100000-text-v1"}

REC_FIELDS = ["layerstack_init_ns", "setup_ns", "teardown_ns", "regular_files",
              "tiny_files", "small_files", "medium_files", "empty_files",
              "anchor_files", "anchor_bytes", "logical_bytes", "scanned_files",
              "scanned_bytes", "candidate_objects", "candidate_bytes",
              "inserted_objects", "inserted_bytes", "store_database_bytes",
              "store_canonical_bytes", "init_bytes_per_second",
              "init_files_per_second", "initialization_user_cpu_ns",
              "initialization_system_cpu_ns", "initialization_disk_read_bytes",
              "initialization_disk_write_bytes", "initialization_context_switches",
              "process_t1_peak_rss_bytes", "fixture_cache_profile",
              "measurement_mode", "initialize_admission_transactions",
              "initialize_batch_inserted_objects"]

rows, raw = [], []
for path in sorted(glob.glob(os.path.join(ROOT, "**", "perf.jsonl"), recursive=True)):
    try:
        lines = open(path, encoding="utf-8", errors="replace").read().splitlines()
    except OSError:
        continue
    header, samples, summaries = None, [], []
    for line in lines:
        line = line.strip()
        if not line:
            continue
        try:
            d = json.loads(line)
        except json.JSONDecodeError:
            continue
        k = d.get("kind")
        if k == "header":
            header = d
        elif k == "sample":
            samples.append(d)
        elif k == "summary":
            summaries.append(d)
    for s in samples:
        ids = s.get("identities") or {}
        case = ids.get("case") or ids.get("scenario_id")
        if case not in CASES:
            continue
        recs = s.get("records") or [{}]
        rec = recs[0] if recs else {}
        # summaries in the same file that name this case's timer
        summary = next((x for x in summaries
                        if x.get("timer") == ids.get("timer")), None)
        row = {
            "path": path,
            "case": case,
            "arm": ids.get("source_arm"),
            "route": ids.get("route"),
            "timer": ids.get("timer"),
            "median_ns": (summary or {}).get("median_ns"),
            "attempted": (summary or {}).get("attempted"),
            "status": s.get("status"),
            "summary_status": (summary or {}).get("status"),
            "admission_eligible": (summary or {}).get("admission_eligible"),
            "completion_status": s.get("completion_status"),
            "cache_contract": (header or {}).get("cache_contract"),
            "verification_status": (header or {}).get("verification_status"),
            "collection_mode": (header or {}).get("collection_mode"),
            "product_identity": ids.get("product_identity"),
            "image": ids.get("image"),
            "source_identity": ids.get("source_identity"),
            "harness_identity": ids.get("harness_identity"),
            "seed": ids.get("seed"),
            "pseudorandom": ids.get("pseudorandom"),
            "topology": (ids.get("environment") or {}).get("topology"),
            "wall_ns": s.get("wall_ns"),
            "command_wall_ns": s.get("command_wall_ns"),
            "cleanup_status": (s.get("cleanup") or {}).get("status"),
            "reused_proof_identities": s.get("reused_proof_identities"),
            "omissions": s.get("omissions"),
            "setup_policy": ids.get("setup_policy"),
        }
        for f in REC_FIELDS:
            row[f] = rec.get(f)
        rows.append(row)
        raw.append({"row": row, "sample": s, "header": header, "summary": summary})

FIELDS = list(rows[0].keys()) if rows else []
if len(sys.argv) > 2 and sys.argv[2] == "--json":
    json.dump(raw, open(os.path.join(os.path.dirname(os.path.abspath(__file__)),
                                     "inventory-raw.json"), "w"), indent=1)
print("\t".join(FIELDS))
for r in sorted(rows, key=lambda x: (x["case"], str(x["arm"]), str(x["median_ns"]))):
    print("\t".join("" if r[f] is None else str(r[f]) for f in FIELDS))
print(f"# rows={len(rows)} files_scanned={len(glob.glob(os.path.join(ROOT,'**','perf.jsonl'),recursive=True))}", file=sys.stderr)
