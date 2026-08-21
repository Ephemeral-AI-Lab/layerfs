#!/usr/bin/env python3
"""Static and one-shot timed Canonical-v2 complete validation."""

import argparse
import csv
import hashlib
import importlib.util
import json
import os
import shutil
import signal
import stat
import subprocess
import sys
import time
from pathlib import Path

sys.dont_write_bytecode = True

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[3]
CORE_PATH = HERE / "run-compact-closure.py"
ANALYZER = HERE / "analyze-complete-validation.py"
ANALYZER_CORE = HERE / "analyze-compact-closure.py"
PREREG = HERE / "PROSPECTIVE-CANONICAL-V2-COMPLETE-VALIDATION-v1.md"
METHODOLOGY = HERE / "PROSPECTIVE-METHODOLOGY-CUSTODY-COMPLETE-VALIDATION-v1.tsv"
MANIFEST_TOOL = HERE / "manifest-bundle.py"
ROOT = REPO / "target/phase4-canonical-v2-complete-validation-20260821-v1/results-v1"
STATIC_ROOT = REPO / "target/phase4-canonical-v2-complete-validation-20260821-v1/static-validation-v2"
FROZEN_CANDIDATE = REPO / "target/phase4-canonical-v2-complete-validation-20260821-v1/frozen-operands-v1/phase4_create_edit_benchmark-canonical-v2"
LOCK = Path("/tmp/layerfs-CANONICAL_V2_COMPLETE_VALIDATION.lock")
CONTROL_SHA = "9cda87ee7fd92784281a6ec7ee3045eb661681d8b7b930dd36546119ae4749d7"
CANDIDATE_SHA = "f3dd4c9420cc7bb7e7390960db9bf6e4a4a44de3d15dc0573002d3172b570280"
SOURCE_SHA = "16e9beedd2fe49d6da65f89f53f488cffbfdcfc71f10477e854cd2d37d00e120"


def load(path, name):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


core = load(CORE_PATH, "canonical_v2_compact_runner")


def sha(path):
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def write(path, contents):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(contents)


def verify_manifest(manifest):
    rows = list(csv.DictReader(manifest.open(), delimiter="\t"))
    for row in rows:
        path = REPO / row["path"]
        if (not path.is_file() or path.stat().st_size != int(row["size_bytes"])
                or sha(path) != row["sha256"]):
            raise RuntimeError(f"manifest mismatch: {row['path']}")
    return len(rows)


def verify_methodology():
    expected = os.environ.get("CANONICAL_V2_COMPLETE_VALIDATION_METHODOLOGY_SHA256")
    if not expected or sha(METHODOLOGY) != expected:
        raise RuntimeError("methodology custody anchor mismatch")
    rows = list(csv.DictReader(METHODOLOGY.open(), delimiter="\t"))
    required = {"runner", "runner-core", "analyzer", "analyzer-core", "preregistration",
                "manifest-tool", "control", "control-source", "candidate", "oracle",
                "publication-repair-v3-manifest"}
    if {row["label"] for row in rows} != required:
        raise RuntimeError("methodology label set mismatch")
    for row in rows:
        path = REPO / row["path"]
        if (not path.is_file() or path.stat().st_size != int(row["size_bytes"])
                or sha(path) != row["sha256"]):
            raise RuntimeError(f"methodology mismatch: {row['label']}")


def static_validation():
    if STATIC_ROOT.exists():
        raise RuntimeError(f"static namespace already exists: {STATIC_ROOT}")
    verify_methodology()
    if sha(FROZEN_CANDIDATE) != CANDIDATE_SHA:
        raise RuntimeError("candidate custody mismatch")
    STATIC_ROOT.mkdir(parents=True)
    commands = [
        ("workspace-tests", ["cargo", "test", "--workspace", "--offline", "--all-targets"]),
        ("clippy", ["cargo", "clippy", "--workspace", "--offline", "--all-targets", "--", "-D", "warnings"]),
        ("fmt", ["cargo", "fmt", "--all", "--", "--check"]),
        ("diff-check", ["git", "diff", "--check"]),
    ]
    ledger = ["label\tcommand\texit\twall_seconds\n"]
    for label, command in commands:
        started = time.monotonic()
        completed = subprocess.run(command, cwd=REPO, capture_output=True, text=True, timeout=59)
        wall = time.monotonic() - started
        write(STATIC_ROOT / f"{label}.stdout", completed.stdout)
        write(STATIC_ROOT / f"{label}.stderr", completed.stderr)
        ledger.append(f"{label}\t{' '.join(command)}\t{completed.returncode}\t{wall:.6f}\n")
        if completed.returncode:
            write(STATIC_ROOT / "STATUS-v1.txt", f"status=REVISE\nfailed={label}\n")
            raise RuntimeError(f"static command failed: {label}")
    write(STATIC_ROOT / "COMMANDS-v1.tsv", "".join(ledger))
    write(STATIC_ROOT / "CUSTODY-v1.tsv",
          "label\tsha256\n"
          f"candidate\t{sha(FROZEN_CANDIDATE)}\n"
          f"benchmark-source\t{sha(REPO / 'crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs')}\n"
          f"evaluator-source\t{sha(REPO / 'tools/layerfs-eval/src/main.rs')}\n")
    write(STATIC_ROOT / "STATUS-v1.txt", "status=PASS\ncommands=4\n")
    manifest = STATIC_ROOT / "TERMINAL-MANIFEST-v1.tsv"
    verification = STATIC_ROOT / "TERMINAL-MANIFEST-VERIFICATION-v1.txt"
    subprocess.run([sys.executable, MANIFEST_TOOL, "write", REPO, STATIC_ROOT, manifest,
                    verification], cwd=REPO, check=True)
    for path in STATIC_ROOT.rglob("*"):
        if path.is_file():
            path.chmod(0o444)
    for path in sorted((path for path in STATIC_ROOT.rglob("*") if path.is_dir()), reverse=True):
        path.chmod(0o555)
    STATIC_ROOT.chmod(0o555)
    print(f"PASS static_manifest_sha256={sha(manifest)}")
    return 0


def verify_static():
    expected = os.environ.get("CANONICAL_V2_COMPLETE_VALIDATION_STATIC_MANIFEST_SHA256")
    manifest = STATIC_ROOT / "TERMINAL-MANIFEST-v1.tsv"
    verification = STATIC_ROOT / "TERMINAL-MANIFEST-VERIFICATION-v1.txt"
    if not expected or not manifest.is_file() or sha(manifest) != expected:
        raise RuntimeError("static manifest custody mismatch")
    entries = verify_manifest(manifest)
    if (STATIC_ROOT / "STATUS-v1.txt").read_text() != "status=PASS\ncommands=4\n":
        raise RuntimeError("static status mismatch")
    actual = {path.resolve() for path in STATIC_ROOT.rglob("*") if path.is_file()}
    recorded = {(REPO / row["path"]).resolve()
                for row in csv.DictReader(manifest.open(), delimiter="\t")}
    if actual != recorded | {manifest.resolve(), verification.resolve()}:
        raise RuntimeError("static root closure mismatch")
    return entries


def copy_image(source_db, target_db, expectations=None):
    core.copy_image_original(source_db, target_db, expectations)
    source_authority = Path(str(source_db) + ".authority")
    target_authority = Path(str(target_db) + ".authority")
    source_stat = source_authority.lstat()
    target_stat = target_authority.lstat()
    source_sha = sha(source_authority)
    target_sha = sha(target_authority)
    if (source_authority.is_symlink() or target_authority.is_symlink()
            or not stat.S_ISREG(source_stat.st_mode) or not stat.S_ISREG(target_stat.st_mode)
            or (source_stat.st_dev, source_stat.st_ino) == (target_stat.st_dev, target_stat.st_ino)
            or source_sha != target_sha):
        raise RuntimeError("authority copy custody mismatch")
    target_authority.chmod(0o600)
    if stat.S_IMODE(target_authority.lstat().st_mode) != 0o600 or sha(target_authority) != source_sha:
        raise RuntimeError("authority runtime mode/hash mismatch")
    label = target_db.parent.name.split("-", 1)[1]
    with (ROOT / "AUTHORITY-MODE-CUSTODY-v1.tsv").open("a") as handle:
        handle.write(
            f"{label}\t{source_authority.relative_to(REPO)}\t{target_authority.relative_to(REPO)}\t"
            f"{source_stat.st_dev}:{source_stat.st_ino}\t{target_stat.st_dev}:{target_stat.st_ino}\t"
            f"{source_sha}\t{target_sha}\t0600\ttrue\n")


def source_custody(candidate):
    paths = [REPO / "Cargo.lock", REPO / "Cargo.toml"]
    for root in (REPO / "crates/layerfs-core", REPO / "crates/layerfs-engine"):
        paths.extend(sorted(path for path in root.rglob("*")
                            if path.is_file() and (path.suffix == ".rs" or path.name == "Cargo.toml")))
    paths.extend([REPO / "tools/layerfs-eval/src/main.rs", PREREG, ANALYZER,
                  Path(__file__).resolve(), CORE_PATH])
    body = "path\tsha256\tsize_bytes\n"
    body += "".join(f"{path.relative_to(REPO)}\t{sha(path)}\t{path.stat().st_size}\n"
                    for path in paths)
    body += f"{candidate.relative_to(REPO)}\t{sha(candidate)}\t{candidate.stat().st_size}\n"
    write(ROOT / "SOURCE-BUILD-CUSTODY-v1.tsv", body)


def execute():
    if ROOT.exists():
        raise RuntimeError(f"result namespace already exists: {ROOT}")
    core.started = time.monotonic()
    core.deadline = core.started + 119.0
    signal.signal(signal.SIGALRM, core.alarm_handler)
    signal.signal(signal.SIGTERM, core.alarm_handler)
    signal.signal(signal.SIGINT, core.alarm_handler)
    signal.setitimer(signal.ITIMER_REAL, 119.0)
    ROOT.mkdir(parents=True)
    write(ROOT / "SCREEN-ATTEMPT-v1.txt", "attempt=1\nclassification=canonical-v2 complete validation\n")
    write(ROOT / "ACTUAL-INVOCATIONS-v1.tsv", "sequence\tevent\ttime_ns\tlabel\tcommand\texit\n")
    LOCK.mkdir()
    core.lock_held = True
    write(ROOT / "LOCK-v1.txt", f"lock={LOCK}\nacquired_ns={time.time_ns()}\nwall_ceiling_seconds=119\n")
    verify_methodology()
    static_entries = verify_static()
    if sha(core.CONTROL) != CONTROL_SHA or sha(FROZEN_CANDIDATE) != CANDIDATE_SHA:
        raise RuntimeError("operand custody mismatch")
    core.verify_manifest(core.HISTORY_MANIFEST)
    core.verify_manifest(core.HISTORY_CLARIFICATION)
    core.check_quiescence("PREVALIDATION")

    operands = ROOT / "operands-v1"
    operands.mkdir()
    candidate = operands / "phase4_create_edit_benchmark-canonical-v2"
    shutil.copy2(FROZEN_CANDIDATE, candidate)
    shutil.copy2(core.CONTROL, operands / core.CONTROL.name)
    candidate.chmod(0o555)
    (operands / core.CONTROL.name).chmod(0o555)
    write(ROOT / "CONTROL-SHA256-v1.txt", CONTROL_SHA + "\n")
    write(ROOT / "CANDIDATE-SHA256-v1.txt", CANDIDATE_SHA + "\n")
    write(ROOT / "STATIC-CUSTODY-v1.txt",
          f"manifest_sha256={os.environ['CANONICAL_V2_COMPLETE_VALIDATION_STATIC_MANIFEST_SHA256']}\n"
          f"entries={static_entries}\n")
    source_custody(candidate)
    write(ROOT / "ENVIRONMENT-v1.txt",
          f"rustc={subprocess.check_output(['rustc','--version'], text=True).strip()}\n"
          f"cargo={subprocess.check_output(['cargo','--version'], text=True).strip()}\n"
          f"methodology_sha256={sha(METHODOLOGY)}\n"
          "cache_scope=warm developer environment; OS/filesystem cache warm-or-unknown\n"
          "instructions=Unavailable\ncycles=Unavailable\nphysical_io=Unavailable\n")
    write(ROOT / "AUTHORITY-MODE-CUSTODY-v1.tsv",
          "label\tsource_path\ttarget_path\tsource_file_id\ttarget_file_id\t"
          "source_sha256\ttarget_sha256\ttarget_runtime_mode\tdistinct_file\n")
    core.check_quiescence("PREROW")
    fixtures, masters = core.prepare()
    core.acquire_rows(fixtures, masters)
    code, _, _ = core.run_child("analysis", [sys.executable, ANALYZER, ROOT], check=False)
    result = json.loads((ROOT / "ANALYSIS-v1.json").read_text())
    core.release_lock()
    core.seal("PASS" if not code and result.get("status") == "PASS" else "REVISE",
              "none" if not code and result.get("status") == "PASS" else "ANALYSIS")
    signal.setitimer(signal.ITIMER_REAL, 0)
    return 0 if result.get("status") == "PASS" else 1


core.ROOT = ROOT
core.LOCK = LOCK
core.ANALYZER = ANALYZER
core.PREREG = PREREG
core.METHODOLOGY = METHODOLOGY
core.copy_image_original = core.copy_image
core.copy_image = copy_image


def dry_run():
    if ROOT.exists() or LOCK.exists():
        raise RuntimeError("result root or lock already exists")
    verify_methodology()
    verify_static()
    if sha(FROZEN_CANDIDATE) != CANDIDATE_SHA or sha(core.CONTROL) != CONTROL_SHA:
        raise RuntimeError("operand custody mismatch")
    schedule = core.schedule()
    expected = ["AB", "AB", "BA", "AB", "BA"]
    actual = ["".join(row["arm"] for row in schedule
                      if row["label"].rsplit("-", 1)[0] == prefix)
              for prefix in ("warm-full-100", "scale-full-1", "scale-full-10",
                             "primary-full-100-p0", "primary-full-100-p1")]
    if len(schedule) != 29 or actual != expected:
        raise RuntimeError("schedule mismatch")
    print(json.dumps({"status": "PASS", "rows": 29, "opening_orders": actual,
                      "writes": 0}, sort_keys=True))
    return 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--static", action="store_true")
    mode.add_argument("--dry-run", action="store_true")
    mode.add_argument("--execute", action="store_true")
    args = parser.parse_args()
    if args.static:
        return static_validation()
    if args.dry_run:
        return dry_run()
    try:
        return execute()
    except Exception as error:
        print(f"REVISE: {type(error).__name__}: {error}", file=sys.stderr)
        if ROOT.exists():
            try:
                write(ROOT / "ANALYSIS-v1.json", json.dumps({"status": "REVISE",
                      "disposition": "CANONICAL-V2 COMPLETE VALIDATION REVISE",
                      "reasons": [f"{type(error).__name__}: {error}"],
                      "baseline_eligible": False}, indent=2, sort_keys=True) + "\n")
                core.release_lock()
                core.seal("REVISE", "TIME-BUDGET" if isinstance(error, TimeoutError)
                          else "ORCHESTRATION-OR-VALIDATION")
            except Exception:
                pass
        return 124 if isinstance(error, TimeoutError) else 1
    finally:
        core.stop_child()
        if core.lock_held:
            try:
                core.release_lock()
            except Exception:
                pass


if __name__ == "__main__":
    raise SystemExit(main())
