#!/usr/bin/env python3
"""One-shot G2 decomposition campaign; dry-run is the default safe path."""

import argparse
import csv
import hashlib
import json
import os
import re
import shutil
import sqlite3
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[3]
TARGET = REPO / "target/phase4-g2-materialization-decomposition-20260822-v1"
RESULTS = TARGET / "results-v1"
LOCK = REPO / "target/phase4-g2-materialization-decomposition-20260822-v1.lock"
PREREG = HERE / "PROSPECTIVE-G2-MATERIALIZATION-DECOMPOSITION-v1.md"
MANIFEST = HERE / "METHODOLOGY-MANIFEST-v1.tsv"
DRY_RUN = HERE / "DRY-RUN-v1.json"
ANALYZER = HERE / "analyze_g2.py"
RECOMPUTE = HERE / "recompute_g2.py"
SOURCE = REPO / "crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs"
CDC = REPO / "crates/layerfs-core/src/cdc/mod.rs"
CONTROL = Path("/tmp/layerfs-g2-control.Zo7jOW/phase4_create_edit_benchmark-d79f0e0")
CANDIDATE = Path("/tmp/layerfs-g2-candidate.GAzawZ/phase4_create_edit_benchmark-g2")
FIXTURE = REPO / "target/phase4-g1-writer-memory-cache-spill-20260821-v1/results-v1/input-v1/S1-100.source"
BASE = REPO / "target/phase4-g1-writer-memory-cache-spill-20260821-v1/results-v1/rows-v1/work-v1/04-measured-p1-AB-pos2-B/db-K64-F64-104857600-full-981004.sqlite"
BASE_FILES = {
    "database": BASE,
    "authority": Path(str(BASE) + ".authority"),
    "expectations": Path(str(BASE) + ".expectations"),
}
HASHES = {
    "preregistration": "0d4007b6493fefc3c8fdd5f6db5a8d31362fb13e747931aea9dfffa5f88504af",
    "control": "42e3ddeb15df298c978b14639690e366fbb26ee55851524d42c6c3e9c0e8bd55",
    "candidate": "5d72b46d29a5b77494781f343cc6841a71879b5de426751afe744f27a033e8f5",
    "fixture": "63b3695b8c117b5bc39885e0df0dcd0af1d49e575482bab16577d84b4f40eff4",
    "source": "e5ff84e32547de7116585f03138bb76e898fb337527ab97b14c6794a45ff8c7c",
    "source_diff": "a905d044a2cb0440e20d4bd53995196ebaac86724a5932de366b509c02279ec9",
    "cdc": "bc0346eec113914943d046a4ab4742420acfff570d6b00115082c40bdf8e58b6",
    "base_database": "7db8d50de42b994546789cb67fc7a9b650e2e551dab118e15003e02106b19890",
    "base_authority": "7855ea6096359925f639b91c8d6b9708cfe0bc0df4a3ffd97a280a8e9a9ded48",
    "base_expectations": "a7489b01445e53aa8a0c5824059b8a6b04f92e15a3b6cf953fbb4c83d6b5e18a",
}
SIZES = {
    "control": 1_372_784,
    "candidate": 1_390_512,
    "fixture": 104_857_600,
    "base_database": 109_199_360,
    "base_authority": 32,
    "base_expectations": 1_096,
}
PRIMARY_CEILING_NS = 20_000_000_000
CAMPAIGN_CEILING_NS = 120_000_000_000


def sha256(path):
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def bytes_sha256(data):
    return hashlib.sha256(data).hexdigest()


def mode(path):
    return f"{path.stat().st_mode & 0o7777:04o}"


def verify(path, digest, size=None):
    if not path.is_file() or sha256(path) != digest or (size is not None and path.stat().st_size != size):
        raise RuntimeError(f"custody mismatch: {path}")


def git(*args):
    return subprocess.run(["git", *args], cwd=REPO, capture_output=True, check=True).stdout


def methodology_hash():
    return sha256(MANIFEST)


def verify_methodology():
    expected = os.environ.get("G2_METHODOLOGY_SHA256")
    if not expected or methodology_hash() != expected:
        raise RuntimeError("methodology manifest custody mismatch")
    for row in csv.DictReader(MANIFEST.open(), delimiter="\t"):
        verify(HERE / row["path"], row["sha256"], int(row["size_bytes"]))


def verify_inputs():
    if Path.cwd().resolve() != REPO or git("branch", "--show-current").decode().strip() != "codex/empty-worktree" or git("rev-parse", "HEAD").decode().strip() != "d79f0e0e2582d1bc491410224fec2b6cef7482e9":
        raise RuntimeError("repository custody drift")
    verify(PREREG, HASHES["preregistration"])
    verify(CONTROL, HASHES["control"], SIZES["control"])
    verify(CANDIDATE, HASHES["candidate"], SIZES["candidate"])
    verify(FIXTURE, HASHES["fixture"], SIZES["fixture"])
    verify(SOURCE, HASHES["source"])
    verify(CDC, HASHES["cdc"])
    for name, path in BASE_FILES.items():
        verify(path, HASHES[f"base_{name}"], SIZES[f"base_{name}"])
    diff = git("diff", "--binary", "--", "crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs")
    if bytes_sha256(diff) != HASHES["source_diff"]:
        raise RuntimeError("candidate source diff custody mismatch")
    verify_methodology()
    connection = sqlite3.connect(f"file:{BASE}?mode=ro", uri=True)
    runtime = {name: connection.execute(f"PRAGMA {name}").fetchone()[0] for name in ("page_size", "journal_mode", "synchronous", "mmap_size")}
    meta = connection.execute("SELECT lower(hex(profile_id)),schema_version,journal_mode,synchronous,temp_store,mmap_size FROM wp4m_meta WHERE id=1").fetchone()
    head = connection.execute("SELECT lower(hex(child)),lower(hex(transition)) FROM wp4m_visible_head WHERE id=1").fetchone()
    connection.close()
    if runtime != {"page_size": 4096, "journal_mode": "delete", "synchronous": 2, "mmap_size": 0} or meta != ("94a03ba7b6c97b5ff37c0ec62ef1d801b9896494b45456bd3df23e2cb278d13b", 5, "delete", 2, 1, 0) or head != ("93d1b461b5cbf88e8d122ad4e90a15a6e3029c408f0fe02ef51c561e5a94c6d1", "2de8d2ce6b614373ba8fb8b29d3a3eccd535abfccc2654e7922514e5ca90fd89"):
        raise RuntimeError("base runtime/profile/head mismatch")
    return {"runtime": runtime, "meta": list(meta), "head": list(head)}


def primary_schedule():
    rows = []
    for position, arm in enumerate("AB", 1):
        rows.append({"kind": "warmup", "pair": 0, "order": "AB", "position": position, "arm": arm})
    for pair, order in enumerate(("AB", "BA", "AB", "BA"), 1):
        for position, arm in enumerate(order, 1):
            rows.append({"kind": "measured", "pair": pair, "order": order, "position": position, "arm": arm})
    for sequence, row in enumerate(rows, 1):
        row.update({"sequence": sequence, "label": f"{sequence:02d}-{row['kind']}-primary-p{row['pair']}-{row['order']}-pos{row['position']}-{row['arm']}", "iteration": 982_000 + sequence, "workload": "primary", "operation": "materialize-warm", "cli_operation": "materialize-warm", "validation": "complete-roundtrip"})
    return rows


def guard_schedule():
    rows = []
    operations = (
        ("materialize-fresh", "materialize-fresh", "complete-roundtrip"),
        ("read-range-1m", "read-range-1m", "complete-roundtrip"),
        ("reopen", "reopen", "complete-roundtrip"),
        ("same-middle", "edit-same", "capture-only"),
    )
    sequence = 10
    for guard, (operation, cli_operation, validation) in enumerate(operations, 1):
        for position, arm in enumerate("AB", 1):
            sequence += 1
            rows.append({"kind": "guard", "pair": guard, "order": "AB", "position": position, "arm": arm, "sequence": sequence, "label": f"{sequence:02d}-guard-{operation}-pos{position}-{arm}", "iteration": 982_000 + sequence, "workload": "guard", "operation": operation, "cli_operation": cli_operation, "validation": validation})
    return rows


def schedule():
    return primary_schedule() + guard_schedule()


def ensure_fresh():
    if TARGET.exists() or LOCK.exists():
        raise RuntimeError("G2 result root or lock already exists")


def write_dry_run(preflight):
    ensure_fresh()
    rows = schedule()
    measured = [row for row in rows if row["kind"] == "measured"]
    centers = {arm: sum(row["sequence"] for row in measured if row["arm"] == arm) / 4 for arm in "AB"}
    if centers != {"A": 6.5, "B": 6.5}:
        raise RuntimeError("primary temporal centers differ")
    record = {"schema": "phase4-g2-materialization-decomposition-dry-run-v1", "status": "PASS", "planned_invocations": len(rows), "planned_primary_rows": 10, "planned_primary_measured_rows": 8, "planned_guard_rows": 8, "actual_rows": 0, "benchmark_children_invoked": 0, "database_copies_created": 0, "observer_probes_invoked": 0, "primary_temporal_centers": centers, "primary_ceiling_ns": PRIMARY_CEILING_NS, "campaign_ceiling_ns": CAMPAIGN_CEILING_NS, "hashes": HASHES, "preflight": preflight, "schedule": rows}
    DRY_RUN.write_text(json.dumps(record, indent=2, sort_keys=True) + "\n")
    print(json.dumps({"status": "PASS", "rows": 0, "temporal_centers": centers}, sort_keys=True))


def parse_time(stderr):
    timing = re.search(r"([0-9.]+) real\s+([0-9.]+) user\s+([0-9.]+) sys", stderr)
    rss = re.search(r"(\d+)\s+maximum resident set size", stderr)
    footprint = re.search(r"(\d+)\s+peak memory footprint", stderr)
    if not timing or not rss or not footprint:
        raise RuntimeError("incomplete /usr/bin/time -l output")
    return {"external_real_seconds": float(timing.group(1)), "user_seconds": float(timing.group(2)), "system_seconds": float(timing.group(3)), "maximum_resident_set_bytes": int(rss.group(1)), "peak_memory_footprint_bytes": int(footprint.group(1))}


def run(command, label, output, env=None, timeout=30, allow_nonzero=False):
    completed = subprocess.run([str(item) for item in command], cwd=REPO, env=env, capture_output=True, text=True, timeout=timeout)
    (output / f"{label}.stdout").write_text(completed.stdout)
    (output / f"{label}.stderr").write_text(completed.stderr)
    if completed.returncode and not allow_nonzero:
        raise RuntimeError(f"child failed: {label}")
    return completed


def copy_inputs():
    input_root = RESULTS / "input-v1"
    input_root.mkdir(parents=True)
    fixture = input_root / "S1-100.source"
    shutil.copyfile(FIXTURE, fixture)
    fixture.chmod(0o444)
    base = input_root / "base.sqlite"
    for name, source in BASE_FILES.items():
        destination = base if name == "database" else Path(str(base) + f".{name}")
        if name == "authority":
            destination = Path(str(base) + ".authority")
        elif name == "expectations":
            destination = Path(str(base) + ".expectations")
        shutil.copyfile(source, destination)
        destination.chmod(0o600 if name != "expectations" else 0o400)
        verify(destination, HASHES[f"base_{name}"], SIZES[f"base_{name}"])
    return fixture, base


def prepare_rows(rows, fixture, base):
    prepared = {}
    work = RESULTS / "rows-v1/work-v1"
    work.mkdir(parents=True)
    for spec in rows:
        row_root = work / spec["label"]
        row_root.mkdir()
        os.symlink(os.path.relpath(fixture, row_root), row_root / fixture.name)
        env = os.environ.copy()
        env["LAYERFS_PREPARED_BASE_DATABASE"] = str(base)
        run([CANDIDATE, "--fast-prepare", row_root, "104857600", spec["cli_operation"], spec["iteration"]], f"prepare-{spec['label']}", RESULTS / "preparation-v1", env=env, timeout=30)
        database = row_root / f"db-K64-F64-104857600-{spec['operation']}-{spec['iteration']}.sqlite"
        # The CLI maps edit-same to same-middle; all other labels are identical.
        if not database.exists():
            database = row_root / f"db-K64-F64-104857600-same-middle-{spec['iteration']}.sqlite"
        authority = Path(str(database) + ".authority")
        expectations = Path(str(database) + ".expectations")
        database.chmod(0o600)
        authority.chmod(0o600)
        expectations.chmod(0o400)
        hashes = {"database": sha256(database), "authority": sha256(authority), "expectations": sha256(expectations)}
        if hashes["database"] != HASHES["base_database"] or hashes["authority"] != HASHES["base_authority"]:
            raise RuntimeError(f"prepared base drift: {spec['label']}")
        prepared[spec["label"]] = (row_root, database, authority, expectations, hashes)
    return prepared


def row_environment(spec, hashes):
    env = os.environ.copy()
    env.update({
        "LAYERFS_FAST_LANE": "1",
        "WP4M_EXECUTABLE_SHA256": HASHES["control" if spec["arm"] == "A" else "candidate"],
        "WP4M_BASE_COPY_METHOD": "physical-byte-copy-identical-database-authority-expectations",
        "WP4M_BASE_DATABASE_SHA256": hashes["database"],
        "WP4M_BASE_AUTHORITY_SHA256": hashes["authority"],
        "WP4M_BASE_EXPECTATIONS_SHA256": hashes["expectations"],
    })
    env.pop("LAYERFS_G2_DECOMPOSE", None)
    if spec["arm"] == "B" and spec["operation"].startswith("materialize-"):
        env["LAYERFS_G2_DECOMPOSE"] = "1"
    return env


def acquire_row(spec, prepared, started):
    if time.monotonic_ns() - started >= CAMPAIGN_CEILING_NS:
        raise TimeoutError("campaign ceiling exhausted")
    row_root, database, authority, expectations, hashes = prepared[spec["label"]]
    binary = CONTROL if spec["arm"] == "A" else CANDIDATE
    command = ["/usr/bin/time", "-l", binary, "--fast-row", row_root, "104857600", spec["cli_operation"], spec["iteration"], str(spec["kind"] == "warmup").lower(), spec["validation"]]
    completed = run(command, spec["label"], RESULTS / "rows-v1", env=row_environment(spec, hashes), timeout=20)
    row = json.loads(completed.stdout)
    row.update(spec)
    row.update(parse_time(completed.stderr))
    row["binary_sha256"] = sha256(binary)
    row["residue_files"] = sorted(str(path.relative_to(row_root)) for path in row_root.rglob("*") if path.is_file() and path.name.endswith(("-journal", "-wal", "-shm")))
    row["post_database_sha256"] = sha256(database)
    row["post_authority_sha256"] = sha256(authority)
    row["post_expectations_sha256"] = sha256(expectations)
    row["post_modes"] = {"database": mode(database), "authority": mode(authority), "expectations": mode(expectations)}
    with (RESULTS / "rows-v1/G2-RAW-v1.jsonl").open("a") as handle:
        handle.write(json.dumps(row, separators=(",", ":"), sort_keys=True) + "\n")
    return row


def observer_probes(timer_regions, started):
    probes = []
    for index in range(1, 6):
        if time.monotonic_ns() - started >= PRIMARY_CEILING_NS:
            raise TimeoutError("primary ceiling exhausted before observer probe")
        completed = run([CANDIDATE, "--g2-timer-probe", timer_regions], f"observer-{index}", RESULTS / "observer-v1", timeout=5)
        probes.append(json.loads(completed.stdout))
    (RESULTS / "OBSERVER-PROBES-v1.json").write_text(json.dumps(probes, indent=2, sort_keys=True) + "\n")


def manifest_payload():
    path = RESULTS / "PAYLOAD-MANIFEST-v1.tsv"
    excluded = {path, RESULTS / "TERMINAL-v1.json", RESULTS / "TERMINAL-VERIFICATION-v1.txt"}
    files = sorted(item for item in RESULTS.rglob("*") if item.is_file() and item not in excluded)
    with path.open("w") as handle:
        handle.write("path\tsha256\tsize_bytes\n")
        for item in files:
            handle.write(f"{item.relative_to(RESULTS)}\t{sha256(item)}\t{item.stat().st_size}\n")
    return path, len(files)


def seal():
    for path in sorted((item for item in TARGET.rglob("*") if item.is_file()), key=lambda item: len(item.parts), reverse=True):
        path.chmod(0o444)
    for path in sorted((item for item in TARGET.rglob("*") if item.is_dir()), key=lambda item: len(item.parts), reverse=True):
        path.chmod(0o555)
    TARGET.chmod(0o555)


def execute(preflight):
    ensure_fresh()
    expected_dry = os.environ.get("G2_DRY_RUN_SHA256")
    if not expected_dry or not DRY_RUN.is_file() or sha256(DRY_RUN) != expected_dry:
        raise RuntimeError("dry-run custody mismatch")
    LOCK.mkdir()
    status, disposition, failure = "REVISE", "G2 REVISE", None
    started = None
    try:
        RESULTS.mkdir(parents=True)
        for directory in (RESULTS / "rows-v1", RESULTS / "preparation-v1", RESULTS / "observer-v1", RESULTS / "methodology-v1"):
            directory.mkdir(parents=True, exist_ok=True)
        for row in csv.DictReader(MANIFEST.open(), delimiter="\t"):
            shutil.copyfile(HERE / row["path"], RESULTS / "methodology-v1" / row["path"])
        shutil.copyfile(MANIFEST, RESULTS / "methodology-v1" / MANIFEST.name)
        shutil.copyfile(DRY_RUN, RESULTS / "DRY-RUN-v1.json")
        fixture, base = copy_inputs()
        rows = schedule()
        prepared = prepare_rows(rows, fixture, base)
        with (RESULTS / "G2-SCHEDULE-v1.tsv").open("w", newline="") as handle:
            writer = csv.DictWriter(handle, fieldnames=list(rows[0]), delimiter="\t", lineterminator="\n")
            writer.writeheader()
            writer.writerows(rows)
        bindings = {"schema": "phase4-g2-materialization-decomposition-input-bindings-v1", "hashes": HASHES, "methodology_manifest_sha256": methodology_hash(), "dry_run_sha256": sha256(DRY_RUN), "preflight": preflight, "control_path": str(CONTROL), "candidate_path": str(CANDIDATE)}
        (RESULTS / "INPUT-BINDINGS-v1.json").write_text(json.dumps(bindings, indent=2, sort_keys=True) + "\n")
        (RESULTS / "rows-v1/G2-RAW-v1.jsonl").write_text("")
        started = time.monotonic_ns()
        primary = primary_schedule()
        warmup_b = None
        for spec in primary[:2]:
            row = acquire_row(spec, prepared, started)
            if spec["arm"] == "B":
                warmup_b = row
        observer_probes(warmup_b["g2_decomposition"]["timer_regions"], started)
        for spec in primary[2:]:
            acquire_row(spec, prepared, started)
        primary_run = run([sys.executable, ANALYZER, RESULTS], "primary-analysis", RESULTS, timeout=5, allow_nonzero=True)
        primary_elapsed = time.monotonic_ns() - started
        if primary_elapsed >= PRIMARY_CEILING_NS or primary_run.returncode:
            raise RuntimeError("primary G2 screen failed or exceeded 20 seconds")
        for spec in guard_schedule():
            acquire_row(spec, prepared, started)
        final_run = run([sys.executable, ANALYZER, RESULTS, "--final"], "final-analysis", RESULTS, timeout=5, allow_nonzero=True)
        independent_run = run([sys.executable, RECOMPUTE, RESULTS], "independent-recomputation", RESULTS, timeout=5, allow_nonzero=True)
        if final_run.returncode or independent_run.returncode:
            raise RuntimeError("final or independent analysis failed")
        final = json.loads((RESULTS / "G2-ANALYSIS-v1.json").read_text())
        independent = json.loads((RESULTS / "INDEPENDENT-RECOMPUTATION-v1.json").read_text())
        if (final["status"], final["disposition"]) != (independent["status"], independent["disposition"]):
            raise RuntimeError("primary/independent disposition mismatch")
        elapsed = time.monotonic_ns() - started
        if elapsed >= CAMPAIGN_CEILING_NS:
            raise TimeoutError("campaign exceeded 120 seconds")
        status, disposition = final["status"], final["disposition"]
        (RESULTS / "STATUS-v1.json").write_text(json.dumps({"status": status, "disposition": disposition, "rows": 18, "measured_rows": 16, "primary_elapsed_ns": primary_elapsed, "campaign_elapsed_before_manifest_ns": elapsed}, indent=2, sort_keys=True) + "\n")
    except BaseException as error:
        failure = error
        if RESULTS.exists():
            (RESULTS / "STATUS-v1.json").write_text(json.dumps({"status": "REVISE", "disposition": "G2 REVISE", "error_type": type(error).__name__, "error": str(error), "elapsed_ns": None if started is None else time.monotonic_ns() - started}, indent=2, sort_keys=True) + "\n")
    finally:
        if RESULTS.exists():
            payload, count = manifest_payload()
            terminal = {"status": status if failure is None else "REVISE", "disposition": disposition if failure is None else "G2 REVISE", "payload_manifest_sha256": sha256(payload), "payload_manifest_entries": count, "status_sha256": sha256(RESULTS / "STATUS-v1.json"), "campaign_elapsed_ns": None if started is None else time.monotonic_ns() - started, "primary_ceiling_ns": PRIMARY_CEILING_NS, "campaign_ceiling_ns": CAMPAIGN_CEILING_NS}
            (RESULTS / "TERMINAL-v1.json").write_text(json.dumps(terminal, indent=2, sort_keys=True) + "\n")
            verification = all(sha256(RESULTS / row["path"]) == row["sha256"] and (RESULTS / row["path"]).stat().st_size == int(row["size_bytes"]) for row in csv.DictReader(payload.open(), delimiter="\t"))
            (RESULTS / "TERMINAL-VERIFICATION-v1.txt").write_text(f"status={'PASS' if verification else 'FAIL'}\ndisposition={terminal['disposition']}\npayload_manifest_sha256={terminal['payload_manifest_sha256']}\npayload_manifest_entries={count}\nterminal_sha256={sha256(RESULTS / 'TERMINAL-v1.json')}\nmanifest_verification_pass={str(verification).lower()}\n")
            seal()
        if LOCK.exists():
            LOCK.rmdir()
    if failure:
        print(f"G2 campaign failure: {type(failure).__name__}: {failure}", file=sys.stderr)
        return 1
    print(json.dumps({"status": status, "disposition": disposition}, sort_keys=True))
    return 0


def main():
    parser = argparse.ArgumentParser()
    modes = parser.add_mutually_exclusive_group(required=True)
    modes.add_argument("--dry-run", action="store_true")
    modes.add_argument("--execute", action="store_true")
    args = parser.parse_args()
    preflight = verify_inputs()
    if args.dry_run:
        write_dry_run(preflight)
        return 0
    return execute(preflight)


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as error:
        print(f"G2 premeasurement failure: {type(error).__name__}: {error}", file=sys.stderr)
        raise SystemExit(1)
