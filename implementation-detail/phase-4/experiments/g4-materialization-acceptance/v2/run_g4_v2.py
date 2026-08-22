#!/usr/bin/env python3
import hashlib
import importlib.util
import sys
from pathlib import Path

REPO = Path("/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty")
HERE = Path(__file__).resolve().parent
V1 = HERE.parent / "v1"
DEPENDENCIES = {
    V1 / "run_g4_v1.py": "9888aab78edb2bb0d8d4f38ea69062829afe493ec199a85ae22e9cdc3d018624",
    V1 / "METHODOLOGY-MANIFEST-v1.tsv": "d6675db6cd340b12453f1719d55f183d3ef5f2b56cc07b9cac4c4f7b75b3c517",
    REPO / "target/phase4-g3-incremental-materialization-20260822-v13/results-v13/OPERAND-CUSTODY-v13.json": "58b652948950ed27e7ceb57c5b156705932e44e9d89724c63e8687f84b782d58",
}
for path, expected in DEPENDENCIES.items():
    if not path.is_file() or hashlib.sha256(path.read_bytes()).hexdigest() != expected:
        raise SystemExit(f"frozen dependency custody mismatch: {path}")

sys.path.insert(0, str(HERE))
spec = importlib.util.spec_from_file_location("phase4_g4_frozen_v1_runner", V1 / "run_g4_v1.py")
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)
runner.HERE = HERE
runner.MANIFEST = HERE / "METHODOLOGY-MANIFEST-v2.tsv"
runner.TARGET = REPO / "target/phase4-g4-materialization-acceptance-20260822-v2"
runner.RESULTS = runner.TARGET / "results-v2"
runner.WORK = runner.TARGET / "work-v2"
runner.HASHES["g3_control"] = "535bfa178a8a569ea43d9f1d23808775c2349a29f9cdacddae508391a6e5e61e"

if __name__ == "__main__":
    raise SystemExit(runner.main())
