#!/usr/bin/env python3
"""Fresh wrapper for the one-line compile repair; core runner remains frozen."""

import csv
import hashlib
import importlib.util
import os
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[3]
CORE = HERE / "run-compact-closure.py"
METHOD = HERE / "PROSPECTIVE-METHODOLOGY-CUSTODY-COMPACT-v2.tsv"
REPAIR = HERE / "PROSPECTIVE-COMPACT-CLOSURE-REPAIR-v2.md"
FAILED = REPO / "target/phase4-canonical-v2-closure-20260821-v2/compact-results-v1/TERMINAL-MANIFEST-v1.tsv"


def sha(path):
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


spec = importlib.util.spec_from_file_location("compact_runner_v1", CORE)
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)
runner.ROOT = REPO / "target/phase4-canonical-v2-closure-20260821-v3/compact-results-v1"
runner.METHODOLOGY = METHOD
runner.PREREG = REPAIR


def verify_methodology_v2():
    expected = os.environ.get("CANONICAL_V2_COMPACT_METHODOLOGY_SHA256")
    if not expected or sha(METHOD) != expected:
        raise RuntimeError("methodology custody anchor mismatch")
    rows = list(csv.DictReader(METHOD.open(), delimiter="\t"))
    labels = {row["label"] for row in rows}
    required = {
        "runner", "runner-core", "analyzer", "repair", "original-preregistration",
        "manifest-tool", "control", "control-source", "oracle",
        "historical-revise-manifest", "historical-clarification-manifest",
        "failed-attempt-manifest",
    }
    if labels != required:
        raise RuntimeError("methodology label set mismatch")
    for row in rows:
        path = REPO / row["path"]
        if not path.is_file() or sha(path) != row["sha256"] or path.stat().st_size != int(row["size_bytes"]):
            raise RuntimeError(f"methodology mismatch: {row['label']}")
    runner.verify_manifest(FAILED)


runner.verify_methodology = verify_methodology_v2

if __name__ == "__main__":
    raise SystemExit(runner.main())
