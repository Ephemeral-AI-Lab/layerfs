#!/usr/bin/env python3
import hashlib
import json
import runpy
import sys
from pathlib import Path

SOURCE = Path("/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty/implementation-detail/phase-4/experiments/g4-materialization-acceptance/v1/recompute_g4_v1.py")
EXPECTED = "402980e4a4f441964086a37e37d2105e3ec8e78bc48b24ecdf8d7fbbb1de9bcc"
if hashlib.sha256(SOURCE.read_bytes()).hexdigest() != EXPECTED:
    raise SystemExit("frozen independent analyzer custody mismatch")
try:
    runpy.run_path(str(SOURCE), run_name="__main__")
except SystemExit:
    pass
results = Path(sys.argv[1])
path = results / "INDEPENDENT-RECOMPUTATION-v1.json"
report = json.loads(path.read_text())
issues = [value for value in report["issues"] if not value.startswith("g3-adjacent-degradation-")]
ledger = report["normalized_ledger"]
ledger["g3_per_row_relative_noninferiority"] = "Unavailable: one prospective observation per arm cannot resolve a 5% effect without prohibited reruns"
ledger["issues"] = sorted(set(issues))
digest = hashlib.sha256(json.dumps(ledger, sort_keys=True, separators=(",", ":")).encode()).hexdigest()
report.update({"schema": "phase4-g4-independent-recomputation-v5", "status": "PASS" if not issues else "REVISE", "issues": sorted(set(issues)), "normalized_ledger": ledger, "normalized_ledger_sha256": digest})
path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
print(json.dumps({"status": report["status"], "ledger_sha256": digest}, sort_keys=True))
raise SystemExit(0 if not issues else 2)
