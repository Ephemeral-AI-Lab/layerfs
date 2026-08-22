#!/usr/bin/env python3
import hashlib
import json
import runpy
import sys
from pathlib import Path

SOURCE = Path("/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty/implementation-detail/phase-4/experiments/g4-materialization-acceptance/v7/recompute_g4_v1.py")
EXPECTED = "0c0f897ea8a1f318afb6554829e6c74907565f6e5a46ed0de39ca26f182d010b"
if hashlib.sha256(SOURCE.read_bytes()).hexdigest() != EXPECTED:
    raise SystemExit("frozen v7 independent analyzer custody mismatch")
try:
    runpy.run_path(str(SOURCE), run_name="__main__")
except SystemExit:
    pass
results = Path(sys.argv[1])
arms = [json.loads(line) for line in (results / "ARM-RAW-v1.jsonl").read_text().splitlines() if line]
index = {(row["sequence"], row["role"]): row["payload"] for row in arms}
observed = {
    "r0_control": index[(3, "r0-control")].get("sqlite_cache_size_pages"),
    "r1_attribution": index[(3, "r1-attribution-control")].get("sqlite_cache_size_pages"),
    "r1_candidate": index[(3, "r1-candidate")].get("sqlite_cache_size_pages"),
    "r1_fresh": index[(4, "r1-candidate")].get("sqlite_cache_size_pages"),
    "m0_control": index[(11, "m0-control")].get("sqlite_cache_size_pages"),
    "m0_candidate": index[(14, "m0-candidate")].get("sqlite_cache_size_pages"),
}
expected_values = (2000, 1500, 1500, 1500, 2000, 1500)
path = results / "INDEPENDENT-RECOMPUTATION-v1.json"
report = json.loads(path.read_text())
issues = list(report["issues"])
if tuple(observed.values()) != expected_values:
    issues.append("g4-read-cache-profile")
ledger = report["normalized_ledger"]
ledger["g4_read_cache_pages"] = observed
ledger["issues"] = sorted(set(issues))
digest = hashlib.sha256(json.dumps(ledger, sort_keys=True, separators=(",", ":")).encode()).hexdigest()
report.update({"schema": "phase4-g4-independent-recomputation-v8", "status": "PASS" if not issues else "REVISE", "issues": sorted(set(issues)), "normalized_ledger": ledger, "normalized_ledger_sha256": digest})
path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
print(json.dumps({"status": report["status"], "ledger_sha256": digest}, sort_keys=True))
raise SystemExit(0 if not issues else 2)
