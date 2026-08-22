#!/usr/bin/env python3
"""One-shot G2-v2 closure runner; dry-run is the only unauthorised mode."""

import argparse
import csv
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[4]
SEALED = REPO / "target/phase4-g2-materialization-decomposition-20260822-v1/results-v1"
TARGET = REPO / "target/phase4-g2-materialization-decomposition-20260822-v2"
RESULTS = TARGET / "results-v2"
LOCK = REPO / "target/phase4-g2-materialization-decomposition-20260822-v2.lock"
MANIFEST = HERE / "METHODOLOGY-MANIFEST-v2.tsv"
DRY_RUN = HERE / "DRY-RUN-v2.json"
ANALYZER = HERE / "analyze_g2_v2.py"
RECOMPUTE = HERE / "recompute_g2_v2.py"
SOURCE = REPO / "crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs"
CDC = REPO / "crates/layerfs-core/src/cdc/mod.rs"
CONTROL = Path("/tmp/layerfs-g2-control.Zo7jOW/phase4_create_edit_benchmark-d79f0e0")
CANDIDATE = Path("/tmp/layerfs-g2-candidate.GAzawZ/phase4_create_edit_benchmark-g2")
FIXTURE = SEALED / "input-v1/S1-100.source"
BASE = SEALED / "input-v1/base.sqlite"
BASE_FILES = {
    "database": BASE,
    "authority": Path(str(BASE) + ".authority"),
    "expectations": Path(str(BASE) + ".expectations"),
}
HASHES = {
    "source": "157699e0cd4cb1e3b5ec631cefb7c967ff7433bdeeb10ee1336e70961b402ad2",
    "cdc": "bc0346eec113914943d046a4ab4742420acfff570d6b00115082c40bdf8e58b6",
    "control": "42e3ddeb15df298c978b14639690e366fbb26ee55851524d42c6c3e9c0e8bd55",
    "candidate": "5d72b46d29a5b77494781f343cc6841a71879b5de426751afe744f27a033e8f5",
    "fixture": "63b3695b8c117b5bc39885e0df0dcd0af1d49e575482bab16577d84b4f40eff4",
    "base_database": "7db8d50de42b994546789cb67fc7a9b650e2e551dab118e15003e02106b19890",
    "base_authority": "7855ea6096359925f639b91c8d6b9708cfe0bc0df4a3ffd97a280a8e9a9ded48",
    "base_expectations": "a7489b01445e53aa8a0c5824059b8a6b04f92e15a3b6cf953fbb4c83d6b5e18a",
    "prepared_expectations": "b3afda400d8cfa55a6145879aff0075e97884edd71c0b4d23d47b5d8c5bffc14",
    "v1_raw": "6f7124cc8d4fdd248b89770da5576f2546f105304e3d486ddb2f9c7ce5352af2",
    "v1_primary": "0840dcf353eff15a53eaa07f748678bfcab5b02b732ec9c592c12d0f38127282",
    "v1_observer": "bfe2e85b7a1fd61d84699cab4f1f3727731e955965a1370e0cfad8d8a406e717",
    "v1_terminal": "b859de6dce9aef9caba43dbf43fd5eb2b7ea24630f7f18ff206749d431e6f2a1",
    "v1_payload_manifest": "28c1b86a3fd3715785617da84195e5ed2cbd5a880dcc883f57f8e51d5edd2d13",
}
SIZES = {"control": 1372784, "candidate": 1390512, "fixture": 104857600, "base_database": 109199360, "base_authority": 32, "base_expectations": 1096}
AUTHORIZATION = "parent-authorized-exactly-one-fresh-same-middle-ba"
CHILD_CEILING_SECONDS = 15
CAMPAIGN_CEILING_NS = 59000000000


def sha256(file_path):
    digest = hashlib.sha256()
    with file_path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def verify(file_path, digest, size=None):
    if not file_path.is_file() or sha256(file_path) != digest or (size is not None and file_path.stat().st_size != size):
        raise RuntimeError(f"custody mismatch: {file_path}")


def mode(file_path):
    return f"{file_path.stat().st_mode & 0o7777:04o}"


def schedule():
    return [
        {"sequence": 1, "label": "01-measured-same-middle-pos1-B", "arm": "B", "position": 1, "order": "BA", "operation": "same-middle", "cli_operation": "edit-same", "iteration": 983001, "kind": "measured", "workload": "v2-guard", "warmup": False, "validation": "capture-only"},
        {"sequence": 2, "label": "02-measured-same-middle-pos2-A", "arm": "A", "position": 2, "order": "BA", "operation": "same-middle", "cli_operation": "edit-same", "iteration": 983002, "kind": "measured", "workload": "v2-guard", "warmup": False, "validation": "capture-only"},
    ]


def ensure_fresh():
    if TARGET.exists() or LOCK.exists():
        raise RuntimeError("G2-v2 result root or lock already exists")


def verify_methodology():
    expected = os.environ.get("G2_V2_METHODOLOGY_SHA256")
    if not expected or not MANIFEST.is_file() or sha256(MANIFEST) != expected:
        raise RuntimeError("G2-v2 methodology manifest custody mismatch")
    with MANIFEST.open() as handle:
        for row in csv.DictReader(handle, delimiter="\t"):
            verify(HERE / row["path"], row["sha256"], int(row["size_bytes"]))


def verify_v1_payload():
    payload = SEALED / "PAYLOAD-MANIFEST-v1.tsv"
    rows = list(csv.DictReader(payload.open(), delimiter="\t"))
    mismatches = []
    sealed_root = SEALED.resolve()
    if len(rows) != 178:
        mismatches.append("entry-count")
    for index, row in enumerate(rows, 1):
        artifact = (SEALED / row["path"]).resolve()
        try:
            artifact.relative_to(sealed_root)
        except ValueError:
            mismatches.append(f"{index}:path")
            continue
        if not artifact.is_file() or artifact.stat().st_size != int(row["size_bytes"]) or sha256(artifact) != row["sha256"]:
            mismatches.append(f"{index}:custody")
    if mismatches:
        raise RuntimeError(f"sealed v1 payload mismatch: {mismatches[:3]}")
    return {"entries": len(rows), "mismatches": 0, "manifest_sha256": sha256(payload)}


def preflight():
    if Path.cwd().resolve() != REPO:
        raise RuntimeError("run from the repository root")
    branch = subprocess.run(["git", "branch", "--show-current"], cwd=REPO, capture_output=True, text=True, check=True).stdout.strip()
    head = subprocess.run(["git", "rev-parse", "HEAD"], cwd=REPO, capture_output=True, text=True, check=True).stdout.strip()
    if branch != "codex/empty-worktree" or head != "d79f0e0e2582d1bc491410224fec2b6cef7482e9":
        raise RuntimeError("repository custody drift")
    verify_methodology()
    verify(SOURCE, HASHES["source"])
    verify(CDC, HASHES["cdc"])
    verify(CONTROL, HASHES["control"], SIZES["control"])
    verify(CANDIDATE, HASHES["candidate"], SIZES["candidate"])
    verify(FIXTURE, HASHES["fixture"], SIZES["fixture"])
    for name, file_path in BASE_FILES.items():
        verify(file_path, HASHES[f"base_{name}"], SIZES[f"base_{name}"])
    sealed_artifacts = {
        "v1_raw": SEALED / "rows-v1/G2-RAW-v1.jsonl",
        "v1_primary": SEALED / "G2-PRIMARY-ANALYSIS-v1.json",
        "v1_observer": SEALED / "OBSERVER-PROBES-v1.json",
        "v1_terminal": SEALED / "TERMINAL-v1.json",
        "v1_payload_manifest": SEALED / "PAYLOAD-MANIFEST-v1.tsv",
    }
    for name, file_path in sealed_artifacts.items():
        verify(file_path, HASHES[name])
    v1_root = SEALED.parent
    v1_lock = REPO / "target/phase4-g2-materialization-decomposition-20260822-v1.lock"
    if not v1_root.is_dir() or v1_root.stat().st_mode & 0o222 or SEALED.stat().st_mode & 0o222 or v1_lock.exists():
        raise RuntimeError("sealed v1 root is writable or its lock exists")
    payload = verify_v1_payload()
    return {"branch": branch, "head": head, "methodology_manifest_sha256": sha256(MANIFEST), "sealed_v1_raw_sha256": sha256(sealed_artifacts["v1_raw"]), "retained_g1_source_sha256": sha256(SOURCE), "fastcdc_source_sha256": sha256(CDC), "v1_root_read_only": True, "v1_lock_absent": True, "v1_instrumented_source_bytes": "not-retained-not-verified", "v1_source_diff_bytes": "not-retained-not-verified", "sealed_v1_payload": payload}


def dry_run(preflight_record):
    ensure_fresh()
    if DRY_RUN.exists():
        raise RuntimeError("G2-v2 dry-run already exists")
    rows = schedule()
    record = {
        "schema": "phase4-g2-protocol-closure-dry-run-v2",
        "status": "PASS",
        "preflight": preflight_record,
        "schedule": rows,
        "planned_invocations": 2,
        "planned_measured_rows": 2,
        "actual_rows": 0,
        "database_copies_created": 0,
        "benchmark_children_invoked": 0,
        "full_v1_rows_rerun": 0,
        "product_source_changes": 0,
        "planned_order": "BA",
        "planned_executable_snapshots": 2,
        "planned_v1_payload_reverification": {"entries": 178, "mismatches": 0},
        "retained_evidence_ceiling_bytes": 10 * 1024 * 1024,
        "transient_paths_deleted_before_seal": ["results-v2/input-v2", "results-v2/rows-v2/work-v2"],
        "child_ceiling_seconds": CHILD_CEILING_SECONDS,
        "campaign_ceiling_ns": CAMPAIGN_CEILING_NS,
        "execute_authorization_required": AUTHORIZATION,
    }
    DRY_RUN.write_text(json.dumps(record, indent=2, sort_keys=True) + "\n")
    print(json.dumps({"status": "PASS", "actual_rows": 0, "planned_rows": 2}, sort_keys=True))


def run_child(command, label, output_dir, env=None, timeout=CHILD_CEILING_SECONDS, allow_nonzero=False, started_ns=None):
    if started_ns is not None:
        remaining = (CAMPAIGN_CEILING_NS - (time.monotonic_ns() - started_ns)) / 1_000_000_000
        if remaining <= 0:
            raise TimeoutError("G2-v2 global 59-second ceiling exhausted")
        timeout = min(timeout, remaining)
    completed = subprocess.run([str(item) for item in command], cwd=REPO, env=env, capture_output=True, text=True, timeout=timeout)
    output_dir.mkdir(parents=True, exist_ok=True)
    (output_dir / f"{label}.stdout").write_text(completed.stdout)
    (output_dir / f"{label}.stderr").write_text(completed.stderr)
    if completed.returncode and not allow_nonzero:
        raise RuntimeError(f"child failed: {label}")
    return completed


def parse_time(stderr):
    timing = re.search(r"([0-9.]+) real\s+([0-9.]+) user\s+([0-9.]+) sys", stderr)
    rss = re.search(r"(\d+)\s+maximum resident set size", stderr)
    footprint = re.search(r"(\d+)\s+peak memory footprint", stderr)
    if not timing or not rss or not footprint:
        raise RuntimeError("incomplete /usr/bin/time -l output")
    return {"external_real_seconds": float(timing.group(1)), "user_seconds": float(timing.group(2)), "system_seconds": float(timing.group(3)), "maximum_resident_set_bytes": int(rss.group(1)), "peak_memory_footprint_bytes": int(footprint.group(1))}


def snapshot_binaries():
    operands = RESULTS / "operands-v2"
    operands.mkdir()
    control = operands / "phase4_create_edit_benchmark-control"
    candidate = operands / "phase4_create_edit_benchmark-instrumented"
    for source, destination, name in ((CONTROL, control, "control"), (CANDIDATE, candidate, "candidate")):
        shutil.copyfile(source, destination)
        destination.chmod(0o500)
        verify(destination, HASHES[name], SIZES[name])
    custody = []
    for source, copied, name in ((CONTROL, control, "control"), (CANDIDATE, candidate, "candidate")):
        source_stat, copied_stat = source.stat(), copied.stat()
        if (source_stat.st_dev, source_stat.st_ino) == (copied_stat.st_dev, copied_stat.st_ino):
            raise RuntimeError(f"operand snapshot is not distinct: {name}")
        custody.append({"name": name, "source_path": str(source), "copy_path": str(copied), "sha256": sha256(copied), "size_bytes": copied_stat.st_size, "source_mode": mode(source), "copy_mode": mode(copied), "source_device": source_stat.st_dev, "source_inode": source_stat.st_ino, "copy_device": copied_stat.st_dev, "copy_inode": copied_stat.st_ino, "distinct_device_inode": True, "execution_path": "snapshot-only"})
    (RESULTS / "OPERAND-CUSTODY-v2.json").write_text(json.dumps(custody, indent=2, sort_keys=True) + "\n")
    return control, candidate


def copy_methodology():
    destination = RESULTS / "methodology-v2"
    destination.mkdir()
    with MANIFEST.open() as handle:
        for row in csv.DictReader(handle, delimiter="\t"):
            shutil.copyfile(HERE / row["path"], destination / row["path"])
    shutil.copyfile(MANIFEST, destination / MANIFEST.name)
    shutil.copyfile(DRY_RUN, destination / DRY_RUN.name)


def chronology(event, started_ns, **fields):
    record = {"event": event, "monotonic_elapsed_ns": time.monotonic_ns() - started_ns, "wall_time_ns": time.time_ns(), **fields}
    with (RESULTS / "CHRONOLOGY-v2.jsonl").open("a") as handle:
        handle.write(json.dumps(record, separators=(",", ":"), sort_keys=True) + "\n")


def global_gate(started_ns):
    if time.monotonic_ns() - started_ns >= CAMPAIGN_CEILING_NS:
        raise TimeoutError("G2-v2 global 59-second ceiling exhausted")


def copy_inputs():
    input_root = RESULTS / "input-v2"
    input_root.mkdir(parents=True)
    fixture = input_root / "S1-100.source"
    shutil.copyfile(FIXTURE, fixture)
    fixture.chmod(0o444)
    base = input_root / "base.sqlite"
    for name, source in BASE_FILES.items():
        destination = base if name == "database" else Path(str(base) + f".{name}")
        shutil.copyfile(source, destination)
        destination.chmod(0o400)
        verify(destination, HASHES[f"base_{name}"], SIZES[f"base_{name}"])
    return fixture, base


def prepare_rows(rows, fixture, base, candidate_binary, started_ns):
    prepared = {}
    for spec in rows:
        row_root = RESULTS / "rows-v2/work-v2" / spec["label"]
        row_root.mkdir(parents=True)
        os.symlink(os.path.relpath(fixture, row_root), row_root / fixture.name)
        env = os.environ.copy()
        env["LAYERFS_PREPARED_BASE_DATABASE"] = str(base)
        global_gate(started_ns)
        run_child([candidate_binary, "--fast-prepare", row_root, "104857600", spec["cli_operation"], spec["iteration"]], f"prepare-{spec['label']}", RESULTS / "preparation-v2", env=env, started_ns=started_ns)
        database = row_root / f"db-K64-F64-104857600-same-middle-{spec['iteration']}.sqlite"
        authority = Path(str(database) + ".authority")
        expectations = Path(str(database) + ".expectations")
        database.chmod(0o600)
        authority.chmod(0o600)
        expectations.chmod(0o400)
        hashes = {"database": sha256(database), "authority": sha256(authority), "expectations": sha256(expectations)}
        if hashes != {"database": HASHES["base_database"], "authority": HASHES["base_authority"], "expectations": HASHES["prepared_expectations"]}:
            raise RuntimeError(f"prepared row custody drift: {spec['label']}")
        prepared[spec["label"]] = (row_root, database, authority, expectations, hashes)
    return prepared


def acquire(spec, prepared, started_ns, control_binary, candidate_binary):
    global_gate(started_ns)
    row_root, database, authority, expectations, hashes = prepared[spec["label"]]
    binary = control_binary if spec["arm"] == "A" else candidate_binary
    env = os.environ.copy()
    env.pop("LAYERFS_G2_DECOMPOSE", None)
    env.update({
        "LAYERFS_FAST_LANE": "1",
        "WP4M_EXECUTABLE_SHA256": HASHES["control" if spec["arm"] == "A" else "candidate"],
        "WP4M_BASE_COPY_METHOD": "physical-byte-copy-identical-database-authority-expectations",
        "WP4M_BASE_DATABASE_SHA256": hashes["database"],
        "WP4M_BASE_AUTHORITY_SHA256": hashes["authority"],
        "WP4M_BASE_EXPECTATIONS_SHA256": hashes["expectations"],
    })
    command = ["/usr/bin/time", "-l", binary, "--fast-row", row_root, "104857600", spec["cli_operation"], spec["iteration"], "false", spec["validation"]]
    command_text = [str(item) for item in command]
    chronology("row-start", started_ns, sequence=spec["sequence"], label=spec["label"], arm=spec["arm"], order="BA", command=command_text)
    completed = run_child(command, spec["label"], RESULTS / "rows-v2", env=env, started_ns=started_ns)
    row = json.loads(completed.stdout)
    row.update(spec)
    row.update(parse_time(completed.stderr))
    row["binary_sha256"] = sha256(binary)
    row["residue_files"] = sorted(str(item.relative_to(row_root)) for item in row_root.rglob("*") if item.is_file() and item.name.endswith(("-journal", "-wal", "-shm")))
    row["post_database_sha256"] = sha256(database)
    row["post_authority_sha256"] = sha256(authority)
    row["post_expectations_sha256"] = sha256(expectations)
    row["post_modes"] = {"database": mode(database), "authority": mode(authority), "expectations": mode(expectations)}
    with (RESULTS / "rows-v2/G2-V2-RAW.jsonl").open("a") as handle:
        handle.write(json.dumps(row, separators=(",", ":"), sort_keys=True) + "\n")
    chronology("row-complete", started_ns, sequence=spec["sequence"], label=spec["label"], arm=spec["arm"], order="BA", command=command_text, exit_code=completed.returncode, raw_rows=spec["sequence"])


def transient_report(prepared, complete):
    records = []
    for label, (_, database, authority, expectations, _) in prepared.items():
        if all(item.is_file() for item in (database, authority, expectations)):
            records.append({"label": label, "database_sha256": sha256(database), "database_size": database.stat().st_size, "authority_sha256": sha256(authority), "expectations_sha256": sha256(expectations), "residue": sorted(item.name for item in database.parent.iterdir() if item.name.endswith(("-journal", "-wal", "-shm")))})
    exact = all(row["database_sha256"] == "b69861ee81c4a01906cf2fb70fe4ef49c4de534cab9ab9b000006efe6802fe31" and row["database_size"] == 109314048 and row["authority_sha256"] == HASHES["base_authority"] and row["expectations_sha256"] == HASHES["prepared_expectations"] and not row["residue"] for row in records)
    report = {"schema": "phase4-g2-v2-transient-verification-v1", "status": "PASS" if complete and len(records) == 2 and exact else "INCOMPLETE", "records": records, "declared_deletions": ["input-v2", "rows-v2/work-v2"], "deletion_complete": False}
    path = RESULTS / "TRANSIENT-VERIFICATION-v2.json"
    path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    return path, report


def discard_transients(report_path, report):
    results_root = RESULTS.resolve()
    deleted = []
    for transient in (RESULTS / "input-v2", RESULTS / "rows-v2/work-v2"):
        resolved = transient.resolve()
        try:
            resolved.relative_to(results_root)
        except ValueError as error:
            raise RuntimeError(f"unsafe transient path: {transient}") from error
        if transient.exists():
            shutil.rmtree(transient)
            deleted.append(str(transient.relative_to(RESULTS)))
    report.update({"deletion_complete": True, "deleted": deleted})
    report_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")


def payload_manifest():
    manifest = RESULTS / "PAYLOAD-MANIFEST-v2.tsv"
    excluded = {manifest, RESULTS / "TERMINAL-v2.json", RESULTS / "TERMINAL-VERIFICATION-v2.txt"}
    files = sorted(item for item in RESULTS.rglob("*") if item.is_file() and not item.is_symlink() and item not in excluded)
    with manifest.open("w") as handle:
        handle.write("path\tsha256\tsize_bytes\n")
        for item in files:
            handle.write(f"{item.relative_to(RESULTS)}\t{sha256(item)}\t{item.stat().st_size}\n")
    return manifest, files


def verify_payload(manifest):
    mismatches = []
    rows = list(csv.DictReader(manifest.open(), delimiter="\t"))
    for row in rows:
        artifact = RESULTS / row["path"]
        if not artifact.is_file() or artifact.stat().st_size != int(row["size_bytes"]) or sha256(artifact) != row["sha256"]:
            mismatches.append(row["path"])
    return rows, mismatches


def chronology_failures():
    records = [json.loads(line) for line in (RESULTS / "CHRONOLOGY-v2.jsonl").read_text().splitlines() if line]
    starts = [(row.get("arm"), row.get("sequence"), row.get("label")) for row in records if row.get("event") == "row-start"]
    completions = [(row.get("arm"), row.get("sequence"), row.get("label")) for row in records if row.get("event") == "row-complete"]
    expected = [("B", 1, "01-measured-same-middle-pos1-B"), ("A", 2, "02-measured-same-middle-pos2-A")]
    command_records = [row for row in records if row.get("event") in ("row-start", "row-complete")]
    commands_exact = len(command_records) == 4 and all(row.get("command", [None])[0:4:3] == ["/usr/bin/time", "--fast-row"] for row in command_records)
    exits_exact = all(row.get("exit_code") == 0 for row in records if row.get("event") == "row-complete")
    return [] if starts == expected and completions == expected and commands_exact and exits_exact else ["row-chronology"]


def finalize(status, disposition, reason, started_ns):
    for _ in range(2):
        status_record = {"status": status, "disposition": disposition, "reason": reason, "fresh_rows": sum(1 for line in (RESULTS / "rows-v2/G2-V2-RAW.jsonl").read_text().splitlines() if line) if (RESULTS / "rows-v2/G2-V2-RAW.jsonl").is_file() else 0, "sealed_v1_rows_rerun": 0}
        (RESULTS / "STATUS-v2.json").write_text(json.dumps(status_record, indent=2, sort_keys=True) + "\n")
        manifest, files = payload_manifest()
        rows, mismatches = verify_payload(manifest)
        issues = mismatches + chronology_failures()
        retained_bytes = sum(item.stat().st_size for item in files)
        if retained_bytes > 10 * 1024 * 1024:
            issues.append("retained-evidence-ceiling")
        if time.monotonic_ns() - started_ns >= CAMPAIGN_CEILING_NS:
            issues.append("global-59-second-ceiling")
        if status == "PASS" and issues:
            status, disposition, reason = "REVISE", "G2 REVISE", ",".join(issues)
            continue
        primary_path = RESULTS / "G2-V2-ANALYSIS.json"
        independent_path = RESULTS / "G2-V2-INDEPENDENT-RECOMPUTATION.json"
        normalized = None
        if primary_path.is_file() and independent_path.is_file():
            primary_ledger = json.loads(primary_path.read_text()).get("normalized_ledger")
            independent_ledger = json.loads(independent_path.read_text()).get("normalized_ledger")
            if primary_ledger == independent_ledger:
                normalized = hashlib.sha256(json.dumps(primary_ledger, sort_keys=True, separators=(",", ":")).encode()).hexdigest()
        bound_paths = {"fresh_raw_sha256": RESULTS / "rows-v2/G2-V2-RAW.jsonl", "primary_analysis_sha256": primary_path, "independent_analysis_sha256": independent_path, "chronology_sha256": RESULTS / "CHRONOLOGY-v2.jsonl", "cleanup_sha256": RESULTS / "TRANSIENT-VERIFICATION-v2.json"}
        terminal = {"status": status, "disposition": disposition, "reason": reason, "payload_manifest_sha256": sha256(manifest), "payload_manifest_entries": len(rows), "payload_mismatches": len(mismatches), "retained_evidence_bytes": retained_bytes, "status_sha256": sha256(RESULTS / "STATUS-v2.json"), "methodology_manifest_sha256": sha256(MANIFEST), "dry_run_sha256": sha256(DRY_RUN), "normalized_ledger_sha256": normalized, "v1_terminal_sha256": sha256(SEALED / "TERMINAL-v1.json"), "v1_terminal_verification_sha256": sha256(SEALED / "TERMINAL-VERIFICATION-v1.txt"), **{name: sha256(path) if path.is_file() else None for name, path in bound_paths.items()}}
        (RESULTS / "TERMINAL-v2.json").write_text(json.dumps(terminal, indent=2, sort_keys=True) + "\n")
        verification = {"status": "PASS" if not mismatches else "FAIL", "disposition": disposition, "payload_manifest_sha256": sha256(manifest), "payload_manifest_entries": len(rows), "payload_mismatches": len(mismatches), "terminal_sha256": sha256(RESULTS / "TERMINAL-v2.json"), "status_sha256": sha256(RESULTS / "STATUS-v2.json"), "campaign_elapsed_ns": time.monotonic_ns() - started_ns}
        (RESULTS / "TERMINAL-VERIFICATION-v2.txt").write_text("\n".join(f"{key}={value}" for key, value in verification.items()) + "\n")
        if status == "PASS" and time.monotonic_ns() - started_ns >= CAMPAIGN_CEILING_NS:
            status, disposition, reason = "REVISE", "G2 REVISE", "global-59-second-ceiling"
            continue
        return status, disposition
    raise RuntimeError("unable to finalize G2-v2 terminal state")


def seal():
    for item in sorted((entry for entry in TARGET.rglob("*") if entry.is_file() and not entry.is_symlink()), key=lambda entry: len(entry.parts), reverse=True):
        item.chmod(0o444)
    for item in sorted((entry for entry in TARGET.rglob("*") if entry.is_dir()), key=lambda entry: len(entry.parts), reverse=True):
        item.chmod(0o555)
    TARGET.chmod(0o555)


def execute(preflight_record, started_ns):
    ensure_fresh()
    if os.environ.get("G2_V2_EXECUTE_AUTHORIZATION") != AUTHORIZATION:
        raise RuntimeError("parent execute authorization is absent")
    expected_dry = os.environ.get("G2_V2_DRY_RUN_SHA256")
    if not expected_dry or not DRY_RUN.is_file() or sha256(DRY_RUN) != expected_dry:
        raise RuntimeError("G2-v2 dry-run custody mismatch")
    LOCK.mkdir()
    failure = None
    prepared = {}
    status, disposition, reason = "REVISE", "G2 REVISE", "campaign did not reach analyzer agreement"
    try:
        RESULTS.mkdir(parents=True)
        (RESULTS / "rows-v2").mkdir()
        (RESULTS / "CHRONOLOGY-v2.jsonl").write_text("")
        chronology("campaign-start", started_ns, planned_rows=2, order="BA")
        copy_methodology()
        control_binary, candidate_binary = snapshot_binaries()
        (RESULTS / "G2-V2-SCHEDULE.json").write_text(json.dumps(schedule(), indent=2, sort_keys=True) + "\n")
        (RESULTS / "INPUT-BINDINGS-v2.json").write_text(json.dumps({"preflight": preflight_record, "hashes": HASHES, "methodology_manifest_sha256": sha256(MANIFEST), "dry_run_sha256": sha256(DRY_RUN)}, indent=2, sort_keys=True) + "\n")
        (RESULTS / "rows-v2/G2-V2-RAW.jsonl").write_text("")
        fixture, base = copy_inputs()
        prepared = prepare_rows(schedule(), fixture, base, candidate_binary, started_ns)
        for spec in schedule():
            acquire(spec, prepared, started_ns, control_binary, candidate_binary)
        global_gate(started_ns)
        primary = run_child([sys.executable, ANALYZER, RESULTS], "primary-analysis", RESULTS, allow_nonzero=True, started_ns=started_ns)
        global_gate(started_ns)
        independent = run_child([sys.executable, RECOMPUTE, RESULTS], "independent-recomputation", RESULTS, allow_nonzero=True, started_ns=started_ns)
        primary_result = json.loads((RESULTS / "G2-V2-ANALYSIS.json").read_text())
        independent_result = json.loads((RESULTS / "G2-V2-INDEPENDENT-RECOMPUTATION.json").read_text())
        if primary.returncode or independent.returncode or (primary_result["status"], primary_result["disposition"], primary_result["normalized_ledger"]) != (independent_result["status"], independent_result["disposition"], independent_result["normalized_ledger"]):
            raise RuntimeError("G2-v2 analyzers failed or disagreed")
        status, disposition, reason = primary_result["status"], primary_result["disposition"], "primary and independent normalized ledgers agree"
        chronology("analyzers-complete", started_ns, status=status, normalized_ledger_sha256=hashlib.sha256(json.dumps(primary_result["normalized_ledger"], sort_keys=True).encode()).hexdigest())
    except BaseException as error:
        failure = error
        status, disposition, reason = "REVISE", "G2 REVISE", f"{type(error).__name__}: {error}"
    finally:
        try:
            if RESULTS.exists():
                try:
                    report_path, report = transient_report(prepared, failure is None)
                    discard_transients(report_path, report)
                except BaseException as error:
                    status, disposition, reason = "REVISE", "G2 REVISE", f"cleanup {type(error).__name__}: {error}"
                    failure = failure or error
                try:
                    status, disposition = finalize(status, disposition, reason, started_ns)
                except BaseException as error:
                    failure = failure or error
                try:
                    seal()
                except BaseException as error:
                    failure = failure or error
        finally:
            if LOCK.exists():
                LOCK.rmdir()
    if failure:
        raise failure
    if status != "PASS":
        raise RuntimeError(f"{disposition}: {reason}")
    print(json.dumps({"status": status, "disposition": disposition}, sort_keys=True))


def main():
    parser = argparse.ArgumentParser()
    modes = parser.add_mutually_exclusive_group(required=True)
    modes.add_argument("--dry-run", action="store_true")
    modes.add_argument("--execute", action="store_true")
    args = parser.parse_args()
    started_ns = time.monotonic_ns()
    preflight_record = preflight()
    if args.dry_run:
        dry_run(preflight_record)
        return 0
    execute(preflight_record, started_ns)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as error:
        print(f"G2-v2 failure: {type(error).__name__}: {error}", file=sys.stderr)
        raise SystemExit(1)
