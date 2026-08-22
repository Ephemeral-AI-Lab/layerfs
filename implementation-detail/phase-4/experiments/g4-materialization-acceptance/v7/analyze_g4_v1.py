#!/usr/bin/env python3
import hashlib
import json
import runpy
import sys
from pathlib import Path

SOURCE = Path("/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty/implementation-detail/phase-4/experiments/g4-materialization-acceptance/v5/analyze_g4_v1.py")
EXPECTED = "ab2ba1f7d62ca9b31437f87bdaf2c29b821a7b2ce40887fcf75eb93c421ae25c"
if hashlib.sha256(SOURCE.read_bytes()).hexdigest() != EXPECTED:
    raise SystemExit("frozen v5 primary analyzer custody mismatch")
try:
    runpy.run_path(str(SOURCE), run_name="__main__")
except SystemExit:
    pass
results = Path(sys.argv[1])
arms = [json.loads(line) for line in (results / "ARM-RAW-v1.jsonl").read_text().splitlines() if line]
arm = lambda sequence, role: next(row["payload"] for row in arms if row["sequence"] == sequence and row["role"] == role)
m0 = arm(14, "m0-candidate")
seed = arm(7, "s1-candidate")
path = results / "PRIMARY-ANALYSIS-v1.json"
report = json.loads(path.read_text())
issues = list(report["issues"])
expected_durability = {
    "data_sync_calls": 1,
    "metadata_operations": 1,
    "metadata_sync_calls": 1,
    "rename_calls": 1,
    "directory_sync_calls": 2,
    "reconciliation_calls": 0,
    "reconciliation_outcome": "not-needed",
    "publication_status": "committed",
    "publication_diagnostic": None,
    "temp_files_created": 1,
    "temp_files_removed": 1,
}
if any(m0.get(key) != value for key, value in expected_durability.items()):
    issues.append("m0-direct-durability-counters")
if seed.get("cache_class") != "same-open-protected-seed-warm-or-unknown":
    issues.append("seed-cache-class")
ledger = report["normalized_ledger"]
ledger["m0_durability"] = {key: m0.get(key) for key in expected_durability}
ledger["seed_cache_class"] = seed.get("cache_class")
ledger["issues"] = sorted(set(issues))
digest = hashlib.sha256(json.dumps(ledger, sort_keys=True, separators=(",", ":")).encode()).hexdigest()
report.update({"schema": "phase4-g4-primary-analysis-v7", "status": "PASS" if not issues else "REVISE", "issues": sorted(set(issues)), "normalized_ledger": ledger, "normalized_ledger_sha256": digest})
path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
print(json.dumps({"status": report["status"], "ledger_sha256": digest}, sort_keys=True))
raise SystemExit(0 if not issues else 2)
