#!/usr/bin/env python3
import hashlib
import runpy
from pathlib import Path

SOURCE = Path("/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty/implementation-detail/phase-4/experiments/g4-materialization-acceptance/v1/analyze_g4_v1.py")
EXPECTED = "5dcc5a3b9283b47c18d3565a9dda457290681807491e2cad4003d8096116ab74"
if hashlib.sha256(SOURCE.read_bytes()).hexdigest() != EXPECTED:
    raise SystemExit("frozen primary analyzer custody mismatch")
runpy.run_path(str(SOURCE), run_name="__main__")
