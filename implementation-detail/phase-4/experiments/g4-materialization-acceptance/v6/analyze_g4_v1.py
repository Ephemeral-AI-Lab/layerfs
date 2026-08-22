#!/usr/bin/env python3
import hashlib
import runpy
from pathlib import Path

SOURCE = Path("/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty/implementation-detail/phase-4/experiments/g4-materialization-acceptance/v5/analyze_g4_v1.py")
EXPECTED = "ab2ba1f7d62ca9b31437f87bdaf2c29b821a7b2ce40887fcf75eb93c421ae25c"
if hashlib.sha256(SOURCE.read_bytes()).hexdigest() != EXPECTED:
    raise SystemExit("frozen v5 primary analyzer custody mismatch")
runpy.run_path(str(SOURCE), run_name="__main__")
