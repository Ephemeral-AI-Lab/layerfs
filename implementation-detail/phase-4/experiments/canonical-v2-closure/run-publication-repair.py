#!/usr/bin/env python3
"""One 119-second canonical-v2 publication-repair screen."""

import argparse
import csv
import hashlib
import importlib.util
import json
import os
import re
import shutil
import signal
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[3]
CORE = HERE / "run-compact-closure.py"
ANALYZER = HERE / "analyze-publication-repair.py"
PREREG = HERE / "PROSPECTIVE-CANONICAL-V2-PUBLICATION-REPAIR-v1.md"
METHODOLOGY = HERE / "PROSPECTIVE-METHODOLOGY-CUSTODY-PUBLICATION-REPAIR-v1.tsv"
ROOT = REPO / "target/phase4-canonical-v2-publication-repair-20260821-v1/results-v1"
LOCK = Path("/tmp/layerfs-CANONICAL_V2_PUBLICATION_REPAIR.lock")
HISTORY = REPO / "target/phase4-canonical-v2-closure-20260821-v3/compact-results-v1/TERMINAL-MANIFEST-v1.tsv"
CONTROL_SHA = "9cda87ee7fd92784281a6ec7ee3045eb661681d8b7b930dd36546119ae4749d7"
CONTROL_SOURCE_SHA = "3284c3bdfe20426df78b4cb8ef310248e1e4f644b8422d79c4689653d870652a"
FIXTURE_SHA = "63b3695b8c117b5bc39885e0df0dcd0af1d49e575482bab16577d84b4f40eff4"

spec = importlib.util.spec_from_file_location("canonical_v2_compact_core", CORE)
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)
runner.ROOT = ROOT
runner.LOCK = LOCK
runner.ANALYZER = ANALYZER
runner.PREREG = PREREG
runner.METHODOLOGY = METHODOLOGY


def sha(path):
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def schedule():
    rows = []

    def add(label, kind, operation, order, pair="-", comparable=True):
        for arm in list(order) if comparable else ["B"]:
            rows.append({
                "sequence": len(rows) + 1,
                "label": f"{label}-{arm}",
                "kind": kind,
                "size": 104_857_600,
                "operation": operation,
                "arm": arm,
                "pair": str(pair),
                "order": order,
                "comparable": comparable,
            })

    add("warm-full-100", "warmup", "full", "AB")
    add("primary-full-100-p0", "primary", "full", "AB", 0)
    add("primary-full-100-p1", "primary", "full", "BA", 1)
    add("guard-same-middle", "candidate-only", "same-middle", "B", comparable=False)
    add("guard-one-byte-middle", "candidate-only", "one-byte-middle", "B", comparable=False)
    add("guard-plus1-middle", "candidate-only", "plus1-middle", "B", comparable=False)
    return rows


def assert_schedule():
    actual = [(row["kind"], row["operation"], row["arm"], row["order"]) for row in schedule()]
    expected = [
        ("warmup", "full", "A", "AB"),
        ("warmup", "full", "B", "AB"),
        ("primary", "full", "A", "AB"),
        ("primary", "full", "B", "AB"),
        ("primary", "full", "B", "BA"),
        ("primary", "full", "A", "BA"),
        ("candidate-only", "same-middle", "B", "B"),
        ("candidate-only", "one-byte-middle", "B", "B"),
        ("candidate-only", "plus1-middle", "B", "B"),
    ]
    if actual != expected:
        raise RuntimeError(f"schedule mismatch: {actual!r}")
    return actual


def verify_methodology(require_anchor):
    expected = os.environ.get("CANONICAL_V2_PUBLICATION_REPAIR_METHODOLOGY_SHA256")
    if require_anchor and (not expected or sha(METHODOLOGY) != expected):
        raise RuntimeError("methodology custody anchor mismatch")
    rows = list(csv.DictReader(METHODOLOGY.open(), delimiter="\t"))
    required = {"runner", "runner-core", "analyzer", "preregistration", "manifest-tool", "control", "control-source", "oracle", "historical-v3-manifest"}
    if {row["label"] for row in rows} != required:
        raise RuntimeError("methodology label set mismatch")
    for row in rows:
        path = REPO / row["path"]
        if not path.is_file() or sha(path) != row["sha256"] or path.stat().st_size != int(row["size_bytes"]):
            raise RuntimeError(f"methodology mismatch: {row['label']}")


def prepare():
    fixture_root = ROOT / "work-v1/fixtures"
    fixture_root.mkdir(parents=True)
    runner.run_child("fixtures", [runner.CONTROL, "--fixed-radix-acceptance-fixtures", fixture_root])
    fixture = fixture_root / "S1-100.source"
    if fixture.stat().st_size != 104_857_600 or sha(fixture) != FIXTURE_SHA:
        raise RuntimeError("100-MiB fixture custody mismatch")
    runner.write(ROOT / "FIXTURE-MANIFEST-v1.tsv", f"size_bytes\tpath\tsha256\n104857600\t{fixture.relative_to(REPO)}\t{sha(fixture)}\n")

    candidate = ROOT / "operands-v1/phase4_create_edit_benchmark-canonical-v2-publication-repair"
    masters = {}
    for index, (arm, executable) in enumerate((("A", runner.CONTROL), ("B", candidate))):
        root = ROOT / f"work-v1/masters/full-104857600-{arm}"
        runner.link_source(root, fixture)
        iteration = 970000 + index
        runner.run_child(f"prepare-full-104857600-{arm}", [executable, "--fast-prepare", root, "104857600", "write", str(iteration)])
        masters[(arm, 104_857_600, "full")] = root / f"db-K64-F64-104857600-full-{iteration}.sqlite"

    base_root = ROOT / "work-v1/masters/published-100-B"
    runner.link_source(base_root, fixture)
    runner.run_child("prepare-published-100-B", [candidate, "--fast-prepare", base_root, "104857600", "materialize-warm", "970010"])
    base = runner.database(base_root, "materialize-warm", 970010)
    for index, operation in enumerate(("same-middle", "one-byte-middle", "plus1-middle")):
        root = ROOT / f"work-v1/masters/{operation}-100-B"
        runner.link_source(root, fixture)
        iteration = 970020 + index
        cli, public = runner.command_for(operation, prepare=True)
        env = os.environ.copy()
        env["LAYERFS_PREPARED_BASE_DATABASE"] = str(base)
        runner.run_child(f"prepare-{operation}-100-B", [candidate, cli, root, "104857600", public, str(iteration)], env=env)
        masters[("B", 104_857_600, operation)] = runner.database(root, operation, iteration)
    return {104_857_600: fixture}, masters


def acquire_rows(fixtures, masters):
    candidate = ROOT / "operands-v1/phase4_create_edit_benchmark-canonical-v2-publication-repair"
    rows = schedule()
    fields = ["sequence", "label", "kind", "size", "operation", "arm", "pair", "order", "comparable"]
    with (ROOT / "SCHEDULE-v1.tsv").open("w", newline="") as handle:
        writer = csv.DictWriter(handle, fields, delimiter="\t", lineterminator="\n")
        writer.writeheader()
        writer.writerows(rows)
    runner.write(ROOT / "RAW-v1.jsonl", "")
    runner.write(ROOT / "ROW-STARTS-v1.tsv", "sequence\tevent\tmonotonic_ns\tlabel\tarm\toperation\n")
    runner.write(ROOT / "INPUT-CUSTODY-v1.tsv", "sequence\tlabel\tarm\texecutable_sha256\tfixture_sha256\tdatabase_path\tdatabase_device\tdatabase_inode\tdatabase_sha256\tauthority_sha256\texpectations_sha256\n")
    runner.write(ROOT / "EXTERNAL-TIME-v1.tsv", "label\treal_seconds\tuser_seconds\tsystem_seconds\tmaximum_resident_set_bytes\tpeak_memory_footprint_bytes\tinstructions\tcycles\n")
    seen_inodes = set()
    for row in rows:
        if runner.remaining() <= 1.0:
            raise TimeoutError("time exhausted before row")
        size, arm, operation = row["size"], row["arm"], row["operation"]
        source = fixtures[size]
        root = ROOT / f"work-v1/rows/{row['sequence']:02d}-{row['label']}"
        runner.link_source(root, source)
        iteration = 980000 + row["sequence"]
        target_db = root / f"db-K64-F64-{size}-{operation}-{iteration}.sqlite"
        master = masters[(arm, size, operation)]
        runner.copy_image(master, target_db)
        stat = target_db.stat()
        inode = (stat.st_dev, stat.st_ino)
        if inode in seen_inodes or target_db.samefile(master):
            raise RuntimeError(f"non-distinct copied database: {row['label']}")
        seen_inodes.add(inode)
        executable = runner.CONTROL if arm == "A" else candidate
        cli, public = runner.command_for(operation)
        authority = Path(str(target_db) + ".authority")
        expectations = Path(str(target_db) + ".expectations")
        env = os.environ.copy()
        env.update({
            "LAYERFS_FAST_LANE": "1",
            "WP4M_EXECUTABLE_SHA256": sha(executable),
            "WP4M_BASE_COPY_METHOD": "physical-byte-copy-identical-database-authority-expectations",
            "WP4M_BASE_DATABASE_SHA256": sha(target_db),
            "WP4M_BASE_AUTHORITY_SHA256": sha(authority),
            "WP4M_BASE_EXPECTATIONS_SHA256": sha(expectations),
        })
        with (ROOT / "INPUT-CUSTODY-v1.tsv").open("a") as handle:
            handle.write(f"{row['sequence']}\t{row['label']}\t{arm}\t{sha(executable)}\t{sha(source)}\t{target_db.relative_to(REPO)}\t{stat.st_dev}\t{stat.st_ino}\t{sha(target_db)}\t{sha(authority)}\t{sha(expectations)}\n")
        with (ROOT / "ROW-STARTS-v1.tsv").open("a") as handle:
            handle.write(f"{row['sequence']}\tstarted\t{time.monotonic_ns()}\t{row['label']}\t{arm}\t{operation}\n")
        warmup = str(row["kind"] == "warmup").lower()
        command = ["/usr/bin/time", "-l", executable, cli, root, str(size), public, str(iteration), warmup, "capture-only"]
        _, stdout, _ = runner.run_child(f"row-{row['sequence']:02d}-{row['label']}", command, env=env, timed_row=True)
        result = json.loads(stdout.read_text())
        if result.get("status") != "PASS":
            raise RuntimeError(f"row failed: {row['label']}")
        with (ROOT / "RAW-v1.jsonl").open("a") as handle:
            handle.write(json.dumps(result, separators=(",", ":")) + "\n")
        with (ROOT / "ROW-STARTS-v1.tsv").open("a") as handle:
            handle.write(f"{row['sequence']}\tcompleted\t{time.monotonic_ns()}\t{row['label']}\t{arm}\t{operation}\n")


def build_sources():
    paths = sorted((REPO / "crates/layerfs-core/src").rglob("*.rs"))
    paths += sorted((REPO / "crates/layerfs-engine/src").rglob("*.rs"))
    paths += [
        REPO / "Cargo.lock",
        REPO / "Cargo.toml",
        REPO / "crates/layerfs-core/Cargo.toml",
        REPO / "crates/layerfs-engine/Cargo.toml",
    ]
    return paths


def execute():
    assert_schedule()
    if ROOT.exists():
        raise RuntimeError(f"result namespace already exists: {ROOT}")
    runner.started = time.monotonic()
    runner.deadline = runner.started + 119.0
    signal.signal(signal.SIGALRM, runner.alarm_handler)
    signal.signal(signal.SIGTERM, runner.alarm_handler)
    signal.signal(signal.SIGINT, runner.alarm_handler)
    signal.setitimer(signal.ITIMER_REAL, 119.0)
    ROOT.mkdir(parents=True)
    runner.write(ROOT / "SCREEN-ATTEMPT-v1.txt", "attempt=1\nclassification=fresh publication repair\n")
    runner.write(ROOT / "ACTUAL-INVOCATIONS-v1.tsv", "sequence\tevent\ttime_ns\tlabel\tcommand\texit\n")
    LOCK.mkdir()
    runner.lock_held = True
    runner.write(ROOT / "LOCK-v1.txt", f"lock={LOCK}\nacquired_ns={time.time_ns()}\nwall_ceiling_seconds=119\n")
    verify_methodology(require_anchor=True)
    if sha(runner.CONTROL) != CONTROL_SHA or sha(runner.CONTROL_SOURCE) != CONTROL_SOURCE_SHA:
        raise RuntimeError("CP-0009 custody mismatch")
    runner.verify_manifest(HISTORY)
    runner.check_quiescence("PREVALIDATION")

    candidate_sources = build_sources()
    before_build = {path: (sha(path), path.stat().st_size) for path in candidate_sources}
    runner.write(ROOT / "SOURCE-PREBUILD-CUSTODY-v1.tsv", "path\tsha256\tsize_bytes\n" + "".join(f"{path.relative_to(REPO)}\t{digest}\t{size}\n" for path, (digest, size) in before_build.items()))
    commands = [
        ["cargo", "test", "--offline", "--locked", "-p", "layerfs-engine", "--bin", "phase4_create_edit_benchmark", "publication_repair_", "--", "--nocapture"],
        ["cargo", "build", "--offline", "--release", "--locked", "-p", "layerfs-engine", "--bin", "phase4_create_edit_benchmark"],
    ]
    runner.write(ROOT / "BUILD-COMMAND-v1.txt", "\n".join(" ".join(command) for command in commands) + "\n")
    _, test_stdout, _ = runner.run_child("focused-tests", commands[0], env=os.environ.copy())
    if not re.search(r"test result: ok\. 4 passed; 0 failed", test_stdout.read_text(errors="replace")):
        raise RuntimeError("publication_repair_ protected smoke did not select exactly four passing tests")
    runner.run_child("release-build", commands[1], env=os.environ.copy())
    if any((sha(path), path.stat().st_size) != expected for path, expected in before_build.items()):
        raise RuntimeError("candidate source changed during test/build")

    operands = ROOT / "operands-v1"
    operands.mkdir()
    candidate = operands / "phase4_create_edit_benchmark-canonical-v2-publication-repair"
    shutil.copy2(runner.CANDIDATE_BUILD, candidate)
    shutil.copy2(runner.CONTROL, operands / runner.CONTROL.name)
    candidate.chmod(0o555)
    (operands / runner.CONTROL.name).chmod(0o555)
    runner.write(ROOT / "CONTROL-SHA256-v1.txt", CONTROL_SHA + "\n")
    runner.write(ROOT / "CANDIDATE-SHA256-v1.txt", sha(candidate) + "\n")
    source_paths = candidate_sources + [
        PREREG,
        ANALYZER,
        Path(__file__).resolve(),
        CORE,
        METHODOLOGY,
    ]
    runner.write(ROOT / "SOURCE-BUILD-CUSTODY-v1.tsv", "path\tsha256\tsize_bytes\n" + "".join(f"{path.relative_to(REPO)}\t{sha(path)}\t{path.stat().st_size}\n" for path in source_paths) + f"{candidate.relative_to(REPO)}\t{sha(candidate)}\t{candidate.stat().st_size}\n")
    runner.write(ROOT / "SOURCE-STATUS-v1.txt", subprocess.check_output(["git", "status", "--short", "--", "crates/layerfs-core", "crates/layerfs-engine"], cwd=REPO, text=True))
    runner.write(ROOT / "ENVIRONMENT-v1.txt", f"rustc={subprocess.check_output(['rustc','--version'], text=True).strip()}\ncargo={subprocess.check_output(['cargo','--version'], text=True).strip()}\nmethodology_sha256={sha(METHODOLOGY)}\ncache_scope=warm developer environment; OS/filesystem cache warm-or-unknown\ninstructions=Unavailable\ncycles=Unavailable\nphysical_io=Unavailable\n")
    runner.check_quiescence("PREROW")
    fixtures, masters = prepare()
    acquire_rows(fixtures, masters)
    code, _, _ = runner.run_child("analysis", [sys.executable, ANALYZER, ROOT], check=False)
    result = json.loads((ROOT / "ANALYSIS-v1.json").read_text())
    if runner.remaining() <= 2.0:
        raise TimeoutError("TIME-BUDGET before terminal seal")
    runner.release_lock()
    runner.seal("PASS" if not code and result.get("status") == "PASS" else "REVISE", "none" if not code and result.get("status") == "PASS" else "ANALYSIS")
    if not (ROOT / "TERMINAL-MANIFEST-v1.tsv").is_file() or not (ROOT / "TERMINAL-MANIFEST-VERIFICATION-v1.txt").is_file():
        raise RuntimeError("terminal manifest closure missing")
    signal.setitimer(signal.ITIMER_REAL, 0)
    return 0 if result.get("status") == "PASS" else 1


def dry_run():
    plan = assert_schedule()
    verify_methodology(require_anchor=False)
    if sha(runner.CONTROL) != CONTROL_SHA or sha(runner.CONTROL_SOURCE) != CONTROL_SOURCE_SHA:
        raise RuntimeError("CP-0009 custody mismatch")
    runner.verify_manifest(HISTORY)
    result = {
        "status": "PASS",
        "mode": "dry-run",
        "result_namespace_created": False,
        "measured_rows_written": 0,
        "wall_ceiling_seconds": 119,
        "methodology_sha256": sha(METHODOLOGY),
        "row_count": len(plan),
        "arm_order": "AB,AB,BA,B,B,B",
        "schedule": [row["label"] for row in schedule()],
        "commands": ["cargo test --offline ... publication_repair_", "cargo build --offline --release ..."],
    }
    print(json.dumps(result, indent=2))
    return 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--dry-run", action="store_true", help="validate schedule and frozen inputs without creating evidence or running a child")
    mode.add_argument("--execute", action="store_true", help="run the single authorized 119-second campaign")
    args = parser.parse_args()
    if args.dry_run:
        return dry_run()
    runner.execute = execute
    return runner.main()


if __name__ == "__main__":
    raise SystemExit(main())
