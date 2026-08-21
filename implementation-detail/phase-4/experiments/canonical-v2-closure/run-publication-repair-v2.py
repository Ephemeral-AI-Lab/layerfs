#!/usr/bin/env python3
"""Run the one-row, 59-second canonical-v2 publication-repair v2 screen."""

import argparse
import csv
import importlib.util
import json
import os
import shutil
import signal
import subprocess
import sys
import time
from pathlib import Path

sys.dont_write_bytecode = True

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[3]
CORE = HERE / "run-compact-closure.py"
ANALYZER = HERE / "analyze-publication-repair-v2.py"
MANIFEST_TOOL = HERE / "manifest-bundle.py"
PREREG = HERE / "PROSPECTIVE-CANONICAL-V2-PUBLICATION-REPAIR-v2.md"
METHODOLOGY = HERE / "PROSPECTIVE-METHODOLOGY-CUSTODY-PUBLICATION-REPAIR-v2.tsv"
ROOT = REPO / "target/phase4-canonical-v2-publication-repair-20260821-v2/results-v1"
LOCK = Path("/tmp/layerfs-CANONICAL_V2_PUBLICATION_REPAIR_V2.lock")
V1 = REPO / "target/phase4-canonical-v2-publication-repair-20260821-v1/results-v1"
V1_MANIFEST = V1 / "TERMINAL-MANIFEST-v1.tsv"
V1_RAW = V1 / "RAW-v1.jsonl"
V1_CUSTODY = V1 / "INPUT-CUSTODY-v1.tsv"
V1_SOURCE_CUSTODY = V1 / "SOURCE-BUILD-CUSTODY-v1.tsv"
V1_MANIFEST_VERIFICATION = V1 / "TERMINAL-MANIFEST-VERIFICATION-v1.txt"
V1_CANDIDATE = V1 / "operands-v1/phase4_create_edit_benchmark-canonical-v2-publication-repair"
V1_FIXTURE = V1 / "work-v1/fixtures/S1-100.source"
V1_MASTER = V1 / "work-v1/masters/one-byte-middle-100-B/db-K64-F64-104857600-one-byte-middle-970021.sqlite"
CONTROL = REPO / "target/phase4-canonical-v2-exploration-20260821-v1/control/phase4_create_edit_benchmark-cp0009"
SOURCE = REPO / "crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs"

CANDIDATE_SHA = "75ce43857799f3de035b989fa0dcba49e6eec4b4279b9256cfbd214cbc1aa187"
CONTROL_SHA = "9cda87ee7fd92784281a6ec7ee3045eb661681d8b7b930dd36546119ae4749d7"
SOURCE_SHA = "a22db63db4179606ad0f5dce3a7cbb25d68e4a843f40f98207f9407f21e46f87"
FIXTURE_SHA = "63b3695b8c117b5bc39885e0df0dcd0af1d49e575482bab16577d84b4f40eff4"
V1_MANIFEST_SHA = "91b009a262ec30dc9503fcaa909f9f54103bc5004a47f98efa95606a39a93aef"
V1_RAW_SHA = "777ec722f95578c1717e86cd5100c01c497a876d0ffea557bcf2864f285eb532"
V1_CUSTODY_SHA = "d1b7f50897c59996672f761579f0904bb5453d469e09e8f977d72400f153635a"
V1_SOURCE_CUSTODY_SHA = "e78e83ff45add569ae6cf4674f796ac3a857c501cba37d035ab6c14b101630a0"
V1_MANIFEST_VERIFICATION_SHA = "f38dced6d98ffd30336e6b40694b1744bb90889bec657df6983ed134a5f5f1df"
MASTER_SHA = {
    "database": "962b491e70551db76d3712d966c25259a96b23df453a4342b92c97adcc06a996",
    "authority": "abac9762e55b20e4a7db6b42bfaa435fb9af8e3a0a79d061f4dd05ee63ef6f12",
    "expectations": "a9bf6f2ae2592c755e584672bc55b371468beb00721c69fd06403d2b5d6d2b7d",
}


def load(path, name):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


core = load(CORE, "canonical_v2_compact_core_v2")
audit = load(ANALYZER, "canonical_v2_publication_repair_v2_analyzer")
manifest_tool = load(MANIFEST_TOOL, "canonical_v2_manifest_tool_v2")
sha = core.sha
write = core.write

started = 0.0
deadline = 0.0
current_child = None
lock_held = False
root_created = False


def remaining():
    return deadline - time.monotonic()


def schedule():
    return [{
        "sequence": 1,
        "label": "fresh-one-byte-middle-B",
        "kind": "candidate-only",
        "size": 104_857_600,
        "operation": "one-byte-middle",
        "arm": "B",
        "warmup": False,
        "timing_claim": "none",
    }]


def assert_schedule():
    expected = [(1, "fresh-one-byte-middle-B", "candidate-only", 104_857_600,
                 "one-byte-middle", "B", False, "none")]
    actual = [tuple(row[key] for key in (
        "sequence", "label", "kind", "size", "operation", "arm", "warmup",
        "timing_claim")) for row in schedule()]
    if actual != expected:
        raise RuntimeError(f"schedule mismatch: {actual!r}")
    return actual


def verify_methodology(require_anchor):
    expected = os.environ.get("CANONICAL_V2_PUBLICATION_REPAIR_V2_METHODOLOGY_SHA256")
    if require_anchor and (not expected or sha(METHODOLOGY) != expected):
        raise RuntimeError("v2 methodology custody anchor mismatch")
    rows = list(csv.DictReader(METHODOLOGY.open(), delimiter="\t"))
    required = {
        "runner-v2", "runner-core", "analyzer-v2", "preregistration-v2",
        "manifest-tool", "candidate-v1", "candidate-source", "control-reference",
        "fixture-v1", "master-database-v1", "master-authority-v1",
        "master-expectations-v1", "v1-terminal-manifest",
        "v1-manifest-verification", "v1-raw", "v1-input-custody",
        "v1-source-build-custody", "v1-analysis", "v1-disposition",
    }
    if {row["label"] for row in rows} != required:
        raise RuntimeError("v2 methodology label set mismatch")
    for row in rows:
        path = REPO / row["path"]
        if (not path.is_file() or sha(path) != row["sha256"]
                or path.stat().st_size != int(row["size_bytes"])):
            raise RuntimeError(f"v2 methodology mismatch: {row['label']}")


def verify_sealed_inputs(require_anchor):
    assert_schedule()
    verify_methodology(require_anchor)
    exact = {
        V1_CANDIDATE: CANDIDATE_SHA,
        CONTROL: CONTROL_SHA,
        SOURCE: SOURCE_SHA,
        V1_FIXTURE: FIXTURE_SHA,
        V1_MANIFEST: V1_MANIFEST_SHA,
        V1_RAW: V1_RAW_SHA,
        V1_CUSTODY: V1_CUSTODY_SHA,
        V1_SOURCE_CUSTODY: V1_SOURCE_CUSTODY_SHA,
        V1_MANIFEST_VERIFICATION: V1_MANIFEST_VERIFICATION_SHA,
        V1_MASTER: MASTER_SHA["database"],
        Path(str(V1_MASTER) + ".authority"): MASTER_SHA["authority"],
        Path(str(V1_MASTER) + ".expectations"): MASTER_SHA["expectations"],
    }
    for path, expected in exact.items():
        if not path.is_file() or sha(path) != expected:
            raise RuntimeError(f"sealed custody mismatch: {path}")
    rows = list(csv.DictReader(V1_MANIFEST.open(), delimiter="\t"))
    if len(rows) != 126:
        raise RuntimeError(f"sealed v1 manifest entry count: {len(rows)}")
    core.verify_manifest(V1_MANIFEST)
    manifest_paths = {(REPO / row["path"]).resolve() for row in rows}
    actual_paths = {path.resolve() for path in V1.rglob("*") if path.is_file()}
    if actual_paths != manifest_paths | {V1_MANIFEST.resolve(), V1_MANIFEST_VERIFICATION.resolve()} or len(actual_paths) != 128:
        raise RuntimeError("sealed v1 root file-set closure mismatch")
    source_rows = list(csv.DictReader(V1_SOURCE_CUSTODY.open(), delimiter="\t"))
    if len(source_rows) != 36:
        raise RuntimeError(f"sealed source/build custody count: {len(source_rows)}")
    for row in source_rows:
        path = REPO / row["path"]
        if (not path.is_file() or sha(path) != row["sha256"]
                or path.stat().st_size != int(row["size_bytes"])):
            raise RuntimeError(f"sealed source/build custody mismatch: {row['path']}")
    verification = V1_MANIFEST_VERIFICATION.read_text()
    if ("status=PASS\n" not in verification or "entries=126\n" not in verification
            or "mismatches=0\n" not in verification
            or f"manifest_sha256={V1_MANIFEST_SHA}\n" not in verification):
        raise RuntimeError("sealed v1 manifest verification mismatch")


def stop_child():
    global current_child
    if current_child is not None and current_child.poll() is None:
        try:
            os.killpg(current_child.pid, signal.SIGTERM)
            current_child.wait(timeout=0.35)
        except Exception:
            try:
                os.killpg(current_child.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass


def alarm_handler(_signum, _frame):
    signal.setitimer(signal.ITIMER_REAL, 0)
    stop_child()
    raise TimeoutError("global 59-second supervisor expired")


def release_lock():
    global lock_held
    if not lock_held:
        return
    LOCK.rmdir()
    lock_held = False
    if ROOT.exists():
        with (ROOT / "LOCK-v1.txt").open("a") as handle:
            handle.write(f"released_ns={time.time_ns()}\n")


def copy_bytes(source, destination):
    destination.parent.mkdir(parents=True, exist_ok=True)
    with source.open("rb") as source_handle, destination.open("xb") as target_handle:
        shutil.copyfileobj(source_handle, target_handle, 1024 * 1024)
    source_stat, destination_stat = source.stat(), destination.stat()
    if source.samefile(destination) or ((source_stat.st_dev, source_stat.st_ino)
                                        == (destination_stat.st_dev, destination_stat.st_ino)):
        raise RuntimeError(f"copy is not a distinct file: {destination}")


def run_row(label, command, env):
    global current_child
    if remaining() <= 2.5:
        raise TimeoutError("insufficient budget before row")
    timeout = min(15.0, remaining() - 2.0)
    stdout_path = ROOT / f"logs-v1/{label}.stdout"
    stderr_path = ROOT / f"logs-v1/{label}.stderr"
    stdout_path.parent.mkdir()
    with (ROOT / "ACTUAL-INVOCATIONS-v1.tsv").open("a") as ledger:
        ledger.write(f"1\tstarted\t{time.time_ns()}\t{label}\t{' '.join(map(str, command))}\t-\n")
    with stdout_path.open("wb") as stdout, stderr_path.open("wb") as stderr:
        current_child = subprocess.Popen(
            command, cwd=REPO, env=env, stdout=stdout, stderr=stderr,
            start_new_session=True)
        try:
            code = current_child.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            stop_child()
            raise TimeoutError(f"child timeout after {timeout:.3f}s: {label}")
        finally:
            current_child = None
    with (ROOT / "ACTUAL-INVOCATIONS-v1.tsv").open("a") as ledger:
        ledger.write(f"1\tcompleted\t{time.time_ns()}\t{label}\t{' '.join(map(str, command))}\t{code}\n")
    if code:
        raise RuntimeError(f"{label} exited {code}")
    return stdout_path


def write_schedule():
    fields = ("sequence", "label", "kind", "size", "operation", "arm", "warmup", "timing_claim")
    with (ROOT / "SCHEDULE-v1.tsv").open("w", newline="") as handle:
        writer = csv.DictWriter(handle, fields, delimiter="\t", lineterminator="\n")
        writer.writeheader()
        writer.writerows(schedule())


def acquire_one_row():
    row = schedule()[0]
    operands = ROOT / "operands-v1"
    candidate = operands / "phase4_create_edit_benchmark-canonical-v2-publication-repair"
    copy_bytes(V1_CANDIDATE, candidate)
    candidate.chmod(0o555)
    if sha(candidate) != CANDIDATE_SHA:
        raise RuntimeError("copied candidate custody mismatch")

    row_root = ROOT / "work-v1/rows/01-fresh-one-byte-middle-B"
    row_root.mkdir(parents=True)
    fixture = row_root / V1_FIXTURE.name
    copy_bytes(V1_FIXTURE, fixture)
    if sha(fixture) != FIXTURE_SHA or fixture.stat().st_size != 104_857_600:
        raise RuntimeError("copied fixture custody mismatch")
    iteration = 990001
    target = row_root / f"db-K64-F64-104857600-one-byte-middle-{iteration}.sqlite"
    copies = (
        (V1_MASTER, target, "database"),
        (Path(str(V1_MASTER) + ".authority"), Path(str(target) + ".authority"), "authority"),
        (Path(str(V1_MASTER) + ".expectations"), Path(str(target) + ".expectations"), "expectations"),
    )
    for source, destination, kind in copies:
        copy_bytes(source, destination)
        if sha(destination) != MASTER_SHA[kind]:
            raise RuntimeError(f"copied {kind} custody mismatch")

    with (ROOT / "INPUT-CUSTODY-v1.tsv").open("a") as handle:
        fixture_source_stat, fixture_target_stat = V1_FIXTURE.stat(), fixture.stat()
        values = [
            "1", row["label"], CANDIDATE_SHA, FIXTURE_SHA,
            str(V1_FIXTURE.relative_to(REPO)), str(fixture_source_stat.st_dev),
            str(fixture_source_stat.st_ino), str(fixture_source_stat.st_size),
            str(fixture.relative_to(REPO)), str(fixture_target_stat.st_dev),
            str(fixture_target_stat.st_ino), str(fixture_target_stat.st_size),
        ]
        for source, destination, kind in copies:
            source_stat, target_stat = source.stat(), destination.stat()
            values.extend((str(source.relative_to(REPO)), str(source_stat.st_dev),
                           str(source_stat.st_ino), str(source_stat.st_size),
                           str(destination.relative_to(REPO)),
                           str(target_stat.st_dev), str(target_stat.st_ino),
                           str(target_stat.st_size), MASTER_SHA[kind]))
        handle.write("\t".join(values) + "\n")

    env = os.environ.copy()
    env.update({
        "LAYERFS_FAST_LANE": "1",
        "WP4M_EXECUTABLE_SHA256": CANDIDATE_SHA,
        # This is the executable's historical label for a distinct-inode,
        # byte-identical filesystem copy; it is not a physical-I/O claim.
        "WP4M_BASE_COPY_METHOD": "physical-byte-copy-identical-database-authority-expectations",
        "WP4M_BASE_DATABASE_SHA256": MASTER_SHA["database"],
        "WP4M_BASE_AUTHORITY_SHA256": MASTER_SHA["authority"],
        "WP4M_BASE_EXPECTATIONS_SHA256": MASTER_SHA["expectations"],
    })
    with (ROOT / "ROW-STARTS-v1.tsv").open("a") as starts_file:
        starts_file.write(f"1\tstarted\t{time.monotonic_ns()}\t{row['label']}\tB\tone-byte-middle\n")
    command = [candidate, "--fast-row", row_root, "104857600",
               "edit-one-byte-middle", str(iteration), "false", "capture-only"]
    write(ROOT / "ROW-BUDGET-v1.txt",
          f"remaining_seconds_before_row={remaining():.6f}\nchild_ceiling_seconds=15\n")
    stdout = run_row("row-01-fresh-one-byte-middle-B", command, env)
    result = json.loads(stdout.read_text())
    if result.get("status") != "PASS":
        raise RuntimeError("fresh one-byte-middle row did not PASS")
    write(ROOT / "RAW-v1.jsonl", json.dumps(result, separators=(",", ":")) + "\n")
    with (ROOT / "ROW-STARTS-v1.tsv").open("a") as starts_file:
        starts_file.write(f"1\tcompleted\t{time.monotonic_ns()}\t{row['label']}\tB\tone-byte-middle\n")


def seal(status, reason):
    write(ROOT / "RUN-STATUS-v1.txt",
          f"status={status}\nreason={reason}\ntimeout={'true' if 'TIME-BUDGET' in reason else 'false'}\n"
          f"attempt=1\nwall_seconds_at_status={time.monotonic()-started:.6f}\nwall_ceiling_seconds=59\n")
    manifest = ROOT / "TERMINAL-MANIFEST-v1.tsv"
    verification = ROOT / "TERMINAL-MANIFEST-VERIFICATION-v1.txt"
    if remaining() <= 0.5:
        raise TimeoutError("insufficient budget for terminal manifest")
    rows, mismatches = manifest_tool.write(REPO, ROOT, manifest, verification)
    verification.write_text(
        f"status={'PASS' if not mismatches else 'FAIL'}\nentries={len(rows)}\n"
        f"mismatches={len(mismatches)}\nmanifest_sha256={sha(manifest)}\n")
    checked, mismatches = manifest_tool.verify(REPO, manifest)
    excluded = {manifest.resolve(), verification.resolve()}
    actual = {path.resolve() for path in ROOT.rglob("*") if path.is_file() and path.resolve() not in excluded}
    recorded = {(REPO / row["path"]).resolve() for row in checked}
    if mismatches or recorded != actual or len(checked) != len(rows):
        raise RuntimeError("terminal manifest closure mismatch")
    final_wall = time.monotonic() - started
    if final_wall >= 59.0:
        raise TimeoutError(f"terminal verification completed at {final_wall:.6f}s")
    with verification.open("a") as handle:
        handle.write(
            f"wall_seconds_at_verification={final_wall:.6f}\n"
            "wall_ceiling_seconds=59\nchild_ceiling_seconds=15\n")
    for path in ROOT.rglob("*"):
        if path.is_file() and not path.is_symlink():
            path.chmod(0o444)
    for path in sorted((path for path in ROOT.rglob("*") if path.is_dir()), reverse=True):
        path.chmod(0o555)
    ROOT.chmod(0o555)


def execute():
    global started, deadline, lock_held, root_created
    if ROOT.exists():
        raise RuntimeError(f"fresh result namespace already exists: {ROOT}")
    started = time.monotonic()
    deadline = started + 59.0
    signal.signal(signal.SIGALRM, alarm_handler)
    signal.signal(signal.SIGTERM, alarm_handler)
    signal.signal(signal.SIGINT, alarm_handler)
    signal.setitimer(signal.ITIMER_REAL, 59.0)
    LOCK.mkdir()
    lock_held = True
    ROOT.mkdir(parents=True)
    root_created = True
    write(ROOT / "SCREEN-ATTEMPT-v1.txt", "attempt=1\nclassification=deterministic publication repair v2\ntiming_claim=none\n")
    write(ROOT / "LOCK-v1.txt",
          f"lock={LOCK}\npid={os.getpid()}\nacquired_wall_ns={time.time_ns()}\n"
          f"acquired_monotonic_ns={time.monotonic_ns()}\n"
          f"deadline_monotonic_seconds={deadline:.9f}\n"
          "wall_ceiling_seconds=59\nchild_ceiling_seconds=15\n")
    write(ROOT / "ACTUAL-INVOCATIONS-v1.tsv", "sequence\tevent\ttime_ns\tlabel\tcommand\texit\n")
    write(ROOT / "ROW-STARTS-v1.tsv", "sequence\tevent\tmonotonic_ns\tlabel\tarm\toperation\n")
    write(ROOT / "RAW-v1.jsonl", "")
    write(ROOT / "INPUT-CUSTODY-v1.tsv",
          "sequence\tlabel\texecutable_sha256\tfixture_sha256\t"
          "fixture_source_path\tfixture_source_device\tfixture_source_inode\tfixture_source_size\tfixture_target_path\tfixture_target_device\tfixture_target_inode\tfixture_target_size\t"
          "master_database_path\tmaster_database_device\tmaster_database_inode\tmaster_database_size\ttarget_database_path\ttarget_database_device\ttarget_database_inode\ttarget_database_size_before\tdatabase_sha256\t"
          "master_authority_path\tmaster_authority_device\tmaster_authority_inode\tmaster_authority_size\ttarget_authority_path\ttarget_authority_device\ttarget_authority_inode\ttarget_authority_size_before\tauthority_sha256\t"
          "master_expectations_path\tmaster_expectations_device\tmaster_expectations_inode\tmaster_expectations_size\ttarget_expectations_path\ttarget_expectations_device\ttarget_expectations_inode\ttarget_expectations_size_before\texpectations_sha256\n")
    write_schedule()
    verify_sealed_inputs(require_anchor=True)
    write(ROOT / "PREFLIGHT-CUSTODY-v1.tsv",
          "label\tsha256\n"
          f"candidate-v1\t{CANDIDATE_SHA}\ncontrol-reference\t{CONTROL_SHA}\n"
          f"candidate-source\t{SOURCE_SHA}\nfixture-v1\t{FIXTURE_SHA}\n"
          f"v1-terminal-manifest\t{V1_MANIFEST_SHA}\nv1-raw\t{V1_RAW_SHA}\n"
          f"v1-input-custody\t{V1_CUSTODY_SHA}\nmethodology-v2\t{sha(METHODOLOGY)}\n"
          f"v1-source-build-custody\t{V1_SOURCE_CUSTODY_SHA}\n"
          f"v1-manifest-verification\t{V1_MANIFEST_VERIFICATION_SHA}\n"
          "v1-manifest-entries\t126\nv1-root-files\t128\nv1-manifest-mismatches\t0\n")
    write(ROOT / "ENVIRONMENT-v1.txt",
          f"python={sys.version.split()[0]}\nmethodology_sha256={sha(METHODOLOGY)}\n"
          "screen=deterministic semantic closure; no timing claim\n"
          "control=reference-only; not invoked\nbuild=NotApplicable; sealed candidate reused\n")
    acquire_one_row()
    result = audit.analyze(ROOT)
    audit.write_outputs(ROOT, result)
    release_lock()
    status = "PASS" if result.get("status") == "PASS" else "REVISE"
    seal(status, "none" if status == "PASS" else "ANALYSIS")
    signal.setitimer(signal.ITIMER_REAL, 0)
    return 0 if status == "PASS" else 1


def fail(error):
    if root_created and ROOT.exists():
        try:
            write(ROOT / "DISPOSITION-v1.txt",
                  f"CANONICAL-V2 PUBLICATION-REPAIR-v2 REVISE\nBlocker: {type(error).__name__}: {error}\n"
                  "Sealed v1 remains historical REVISE; CP-0009 remains accepted.\n")
            write(ROOT / "ANALYSIS-v1.json", json.dumps({
                "status": "REVISE",
                "disposition": "CANONICAL-V2 PUBLICATION-REPAIR-v2 REVISE",
                "reasons": [f"{type(error).__name__}: {error}"],
            }, indent=2, sort_keys=True) + "\n")
            release_lock()
            seal("REVISE", "TIME-BUDGET" if isinstance(error, TimeoutError) else "ORCHESTRATION-OR-VALIDATION")
        except Exception:
            pass
    return 124 if isinstance(error, TimeoutError) else 1


def dry_run():
    if ROOT.exists():
        raise RuntimeError(f"fresh result namespace already exists: {ROOT}")
    verify_sealed_inputs(require_anchor=False)
    print(json.dumps({
        "status": "PASS",
        "mode": "dry-run",
        "result_namespace_created": False,
        "copies_created": 0,
        "children_started": 0,
        "rows_written": 0,
        "global_ceiling_seconds": 59,
        "child_ceiling_seconds": 15,
        "schedule": [row["label"] for row in schedule()],
        "candidate_sha256": CANDIDATE_SHA,
        "source_sha256": SOURCE_SHA,
        "v1_manifest_sha256": V1_MANIFEST_SHA,
        "v1_manifest_entries": 126,
        "methodology_sha256": sha(METHODOLOGY),
    }, indent=2, sort_keys=True))
    return 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--dry-run", action="store_true",
                      help="read-only schedule/custody validation; no root, copy, child, or row")
    mode.add_argument("--execute", action="store_true",
                      help="run the single authorized 59-second screen")
    args = parser.parse_args()
    if args.dry_run:
        return dry_run()
    try:
        return execute()
    except Exception as error:
        print(f"REVISE: {type(error).__name__}: {error}", file=sys.stderr)
        return fail(error)
    finally:
        stop_child()
        if lock_held:
            try:
                release_lock()
            except Exception:
                pass


if __name__ == "__main__":
    raise SystemExit(main())
