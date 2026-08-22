#!/usr/bin/env python3
import hashlib
import runpy
from pathlib import Path

SOURCE = Path("/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty/implementation-detail/phase-4/experiments/g4-materialization-acceptance/v5/recompute_g4_v1.py")
EXPECTED = "4a2c7c9f242e51151f8a50962a375eb44d69fa47bc4667dcb94d1ae63155349d"
if hashlib.sha256(SOURCE.read_bytes()).hexdigest() != EXPECTED:
    raise SystemExit("frozen v5 independent analyzer custody mismatch")
runpy.run_path(str(SOURCE), run_name="__main__")
