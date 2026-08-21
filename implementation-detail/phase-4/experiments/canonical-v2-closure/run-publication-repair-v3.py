#!/usr/bin/env python3
"""Run publication-repair v3: audited v2 plus target-authority mode 0600."""

import argparse
import csv
import importlib.util
import json
import os
import signal
import stat
import sys
from pathlib import Path

sys.dont_write_bytecode = True

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[3]
V2_RUNNER = HERE / "run-publication-repair-v2.py"
V2_METHOD = HERE / "PROSPECTIVE-METHODOLOGY-CUSTODY-PUBLICATION-REPAIR-v2.tsv"
ANALYZER = HERE / "analyze-publication-repair-v3.py"
PREREG = HERE / "PROSPECTIVE-CANONICAL-V2-PUBLICATION-REPAIR-v3.md"
METHODOLOGY = HERE / "PROSPECTIVE-METHODOLOGY-CUSTODY-PUBLICATION-REPAIR-v3.tsv"
ROOT = REPO / "target/phase4-canonical-v2-publication-repair-20260821-v3/results-v1"
LOCK = Path("/tmp/layerfs-CANONICAL_V2_PUBLICATION_REPAIR_V3.lock")
V2 = REPO / "target/phase4-canonical-v2-publication-repair-20260821-v2/results-v1"
V2_MANIFEST = V2 / "TERMINAL-MANIFEST-v1.tsv"
V1_AUTHORITY = REPO / (
    "target/phase4-canonical-v2-publication-repair-20260821-v1/results-v1/"
    "work-v1/masters/one-byte-middle-100-B/"
    "db-K64-F64-104857600-one-byte-middle-970021.sqlite.authority")
TARGET_AUTHORITY = ROOT / (
    "work-v1/rows/01-fresh-one-byte-middle-B/"
    "db-K64-F64-104857600-one-byte-middle-990001.sqlite.authority")

V2_MANIFEST_SHA = "1e94b51bbc46524ad164aa3db836026d4a79e200f8bf3bd1cb7ba5c176b35131"
V2_VERIFICATION_SHA = "02a7d0f17c8e80658dbacbc5d52d17a97aee542dccd189a3617132dfc47ef5e0"
V2_ANALYSIS_SHA = "576d53c6ae3d1b4104fbf28763065e53f23c6ad65a916d465f04d22f4d3264bf"
V2_DISPOSITION_SHA = "035b2f46db17b748cb4cb2284940b7607bbad4efec60ad5d86b53ba95d60dc02"
V2_STDERR_SHA = "f665ab00c6a188b15810f6c01f152a5941021e698ed13e11cad7c62416d56679"
AUTHORITY_SHA = "abac9762e55b20e4a7db6b42bfaa435fb9af8e3a0a79d061f4dd05ee63ef6f12"


def load(path, name):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


runner = load(V2_RUNNER, "canonical_v2_publication_repair_v2_core")
audit = load(ANALYZER, "canonical_v2_publication_repair_v3_analyzer")
sha = runner.sha
write = runner.write
v2_verify_sealed_inputs = runner.verify_sealed_inputs
v2_copy_bytes = runner.copy_bytes


def verify_methodology_v3(require_anchor):
    expected = os.environ.get("CANONICAL_V2_PUBLICATION_REPAIR_V3_METHODOLOGY_SHA256")
    if require_anchor and (not expected or sha(METHODOLOGY) != expected):
        raise RuntimeError("v3 methodology custody anchor mismatch")
    rows = list(csv.DictReader(METHODOLOGY.open(), delimiter="\t"))
    required = {
        "runner-v3", "analyzer-v3", "preregistration-v3", "runner-v2",
        "analyzer-v2", "preregistration-v2", "methodology-v2", "runner-core",
        "manifest-tool", "candidate-v1", "candidate-source", "control-reference",
        "fixture-v1", "master-database-v1", "master-authority-v1",
        "master-expectations-v1", "v1-terminal-manifest",
        "v1-manifest-verification", "v1-raw", "v1-input-custody",
        "v1-source-build-custody", "v1-analysis", "v1-disposition",
        "v2-terminal-manifest", "v2-manifest-verification", "v2-analysis",
        "v2-disposition", "v2-stderr", "v2-stdout", "v2-raw",
        "v2-actual-invocations", "v2-input-custody", "v2-run-status",
    }
    if {row["label"] for row in rows} != required or len(rows) != len(required):
        raise RuntimeError("v3 methodology label set mismatch")
    for row in rows:
        path = REPO / row["path"]
        if (not path.is_file() or sha(path) != row["sha256"]
                or path.stat().st_size != int(row["size_bytes"])):
            raise RuntimeError(f"v3 methodology mismatch: {row['label']}")


def verify_v2_history():
    fixed = {
        V2_MANIFEST: V2_MANIFEST_SHA,
        V2 / "TERMINAL-MANIFEST-VERIFICATION-v1.txt": V2_VERIFICATION_SHA,
        V2 / "ANALYSIS-v1.json": V2_ANALYSIS_SHA,
        V2 / "DISPOSITION-v1.txt": V2_DISPOSITION_SHA,
        V2 / "logs-v1/row-01-fresh-one-byte-middle-B.stderr": V2_STDERR_SHA,
    }
    for path, expected in fixed.items():
        if not path.is_file() or sha(path) != expected:
            raise RuntimeError(f"sealed v2 anchor mismatch: {path.name}")
    rows = list(csv.DictReader(V2_MANIFEST.open(), delimiter="\t"))
    if len(rows) != 20:
        raise RuntimeError(f"sealed v2 manifest entry count: {len(rows)}")
    runner.core.verify_manifest(V2_MANIFEST)
    recorded = {(REPO / row["path"]).resolve() for row in rows}
    verification = V2 / "TERMINAL-MANIFEST-VERIFICATION-v1.txt"
    actual = {path.resolve() for path in V2.rglob("*") if path.is_file()}
    if actual != recorded | {V2_MANIFEST.resolve(), verification.resolve()} or len(actual) != 22:
        raise RuntimeError("sealed v2 root file-set closure mismatch")
    text = verification.read_text()
    if ("status=PASS\nentries=20\nmismatches=0\n" not in text
            or f"manifest_sha256={V2_MANIFEST_SHA}\n" not in text
            or "wall_ceiling_seconds=59\nchild_ceiling_seconds=15\n" not in text):
        raise RuntimeError("sealed v2 terminal verification mismatch")
    analysis = json.loads((V2 / "ANALYSIS-v1.json").read_text())
    if analysis != {
        "disposition": "CANONICAL-V2 PUBLICATION-REPAIR-v2 REVISE",
        "reasons": ["RuntimeError: row-01-fresh-one-byte-middle-B exited 1"],
        "status": "REVISE",
    }:
        raise RuntimeError("sealed v2 analysis mismatch")
    if (V2 / "RAW-v1.jsonl").stat().st_size != 0:
        raise RuntimeError("sealed v2 unexpectedly contains a JSON row")
    stderr = V2 / "logs-v1/row-01-fresh-one-byte-middle-B.stderr"
    if stderr.read_text() != "Error: ValidationAuthorityUnavailable\n":
        raise RuntimeError("sealed v2 failure text mismatch")
    invocations = list(csv.DictReader(
        (V2 / "ACTUAL-INVOCATIONS-v1.tsv").open(), delimiter="\t"))
    if (len(invocations) != 2
            or [(row["event"], row["exit"]) for row in invocations]
                != [("started", "-"), ("completed", "1")]
            or any(row["label"] != "row-01-fresh-one-byte-middle-B" for row in invocations)):
        raise RuntimeError("sealed v2 invocation mismatch")
    if stat.S_IMODE(V1_AUTHORITY.stat().st_mode) != 0o444 or sha(V1_AUTHORITY) != AUTHORITY_SHA:
        raise RuntimeError("sealed v1 authority source changed")
    residue = [path for path in V2.rglob("*")
               if path.name.endswith(("-journal", "-wal", "-shm"))]
    if residue:
        raise RuntimeError("sealed v2 contains residue")


def verify_sealed_inputs_v3(require_anchor):
    saved = runner.METHODOLOGY
    runner.METHODOLOGY = V2_METHOD
    try:
        v2_verify_sealed_inputs(require_anchor=False)
    finally:
        runner.METHODOLOGY = saved
    verify_methodology_v3(require_anchor)
    verify_v2_history()
    if ROOT.exists():
        write(ROOT / "SCREEN-ATTEMPT-v1.txt",
              "attempt=1\nclassification=authority-mode publication repair v3\n"
              "one_change=target-authority-0600\ntiming_claim=none\n")
        write(ROOT / "HISTORICAL-V2-CUSTODY-v1.tsv",
              "label\tsha256\tentries_or_bytes\n"
              f"terminal-manifest\t{V2_MANIFEST_SHA}\t20\n"
              f"terminal-verification\t{V2_VERIFICATION_SHA}\t22-root-files\n"
              f"analysis\t{V2_ANALYSIS_SHA}\tREVISE\n"
              f"disposition\t{V2_DISPOSITION_SHA}\tREVISE\n"
              f"stderr\t{V2_STDERR_SHA}\tValidationAuthorityUnavailable\n"
              "raw\te3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855\t0\n")


def copy_bytes_v3(source, destination):
    v2_copy_bytes(source, destination)
    if destination.resolve() != TARGET_AUTHORITY.resolve():
        return
    if source.resolve() != V1_AUTHORITY.resolve():
        raise RuntimeError("v3 authority source path mismatch")
    source_stat, target_stat = source.lstat(), destination.lstat()
    before_hash = sha(destination)
    source_mode = stat.S_IMODE(source_stat.st_mode)
    copied_mode = stat.S_IMODE(target_stat.st_mode)
    if (source.is_symlink() or destination.is_symlink()
            or not stat.S_ISREG(source_stat.st_mode) or not stat.S_ISREG(target_stat.st_mode)
            or source_mode != 0o444 or copied_mode != 0o644 or before_hash != AUTHORITY_SHA
            or (source_stat.st_dev, source_stat.st_ino)
                == (target_stat.st_dev, target_stat.st_ino)):
        raise RuntimeError("v3 pre-chmod authority custody mismatch")
    destination.chmod(0o600)
    runtime_stat = destination.lstat()
    after_hash = sha(destination)
    if stat.S_IMODE(runtime_stat.st_mode) != 0o600 or after_hash != before_hash:
        raise RuntimeError("v3 target authority mode/hash mismatch")
    write(ROOT / "AUTHORITY-MODE-CUSTODY-v1.tsv",
          "source_path\tsource_device\tsource_inode\tsource_mode_octal\t"
          "target_path\ttarget_device\ttarget_inode\ttarget_mode_copied_octal\t"
          "target_mode_runtime_octal\tsha256_before_chmod\tsha256_after_chmod\tchange_scope\n"
          f"{source.relative_to(REPO)}\t{source_stat.st_dev}\t{source_stat.st_ino}\t0444\t"
          f"{destination.relative_to(REPO)}\t{runtime_stat.st_dev}\t{runtime_stat.st_ino}\t0644\t"
          f"0600\t{before_hash}\t{after_hash}\ttarget-only\n")


def fail_v3(error):
    if runner.root_created and ROOT.exists():
        try:
            write(ROOT / "DISPOSITION-v1.txt",
                  f"CANONICAL-V2 PUBLICATION-REPAIR-v3 REVISE\n"
                  f"Blocker: {type(error).__name__}: {error}\n"
                  "Sealed v1/v2 remain historical REVISE; CP-0009 remains accepted.\n")
            write(ROOT / "ANALYSIS-v1.json", json.dumps({
                "status": "REVISE",
                "disposition": "CANONICAL-V2 PUBLICATION-REPAIR-v3 REVISE",
                "reasons": [f"{type(error).__name__}: {error}"],
            }, indent=2, sort_keys=True) + "\n")
            runner.release_lock()
            runner.seal(
                "REVISE",
                "TIME-BUDGET" if isinstance(error, TimeoutError)
                else "ORCHESTRATION-OR-VALIDATION")
        except Exception:
            pass
    return 124 if isinstance(error, TimeoutError) else 1


runner.ROOT = ROOT
runner.LOCK = LOCK
runner.METHODOLOGY = METHODOLOGY
runner.PREREG = PREREG
runner.audit = audit
runner.verify_sealed_inputs = verify_sealed_inputs_v3
runner.copy_bytes = copy_bytes_v3


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--dry-run", action="store_true",
                      help="read-only v1/v2/schedule/custody validation")
    mode.add_argument("--execute", action="store_true",
                      help="run the single authorized 59-second v3 screen")
    args = parser.parse_args()
    if args.dry_run:
        return runner.dry_run()
    try:
        return runner.execute()
    except Exception as error:
        print(f"REVISE: {type(error).__name__}: {error}", file=sys.stderr)
        return fail_v3(error)
    finally:
        runner.stop_child()
        if runner.lock_held:
            try:
                runner.release_lock()
            except Exception:
                pass


if __name__ == "__main__":
    raise SystemExit(main())
