#!/usr/bin/env python3
import importlib.util
import pathlib


HERE = pathlib.Path(__file__).resolve().parent
V1 = HERE.parent / "v1/runner.py"
spec = importlib.util.spec_from_file_location("h11_v1_runner", V1)
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)
runner.RESULT = runner.REPO / "target/phase4-g5-foundation-h11-20260822-v2"
runner.METHOD_MANIFEST = HERE / "method/METHOD-MANIFEST-v2.tsv"
runner.PRIMARY = HERE / "analyzers/primary.py"
runner.INDEPENDENT = HERE / "analyzers/independent.py"
runner.main()
