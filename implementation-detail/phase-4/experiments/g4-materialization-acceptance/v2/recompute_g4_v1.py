#!/usr/bin/env python3
import hashlib
import runpy
from pathlib import Path

SOURCE = Path("/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty/implementation-detail/phase-4/experiments/g4-materialization-acceptance/v1/recompute_g4_v1.py")
EXPECTED = "402980e4a4f441964086a37e37d2105e3ec8e78bc48b24ecdf8d7fbbb1de9bcc"
if hashlib.sha256(SOURCE.read_bytes()).hexdigest() != EXPECTED:
    raise SystemExit("frozen independent analyzer custody mismatch")
runpy.run_path(str(SOURCE), run_name="__main__")
