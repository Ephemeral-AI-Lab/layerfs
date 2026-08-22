#!/usr/bin/env python3
"""One-shot G2-v4 closure runner; dry-run is the only unauthorised mode."""

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
V3 = REPO / "target/phase4-g2-materialization-decomposition-20260822-v3/results-v3"
TARGET = REPO / "target/phase4-g2-materialization-decomposition-20260822-v4"
RESULTS = TARGET / "results-v4"
LOCK = REPO / "target/phase4-g2-materialization-decomposition-20260822-v4.lock"
MANIFEST = HERE / "METHODOLOGY-MANIFEST-v4.tsv"
DRY_RUN = HERE / "DRY-RUN-v4.json"
ANALYZER = HERE / "analyze_g2_v4.py"
RECOMPUTE = HERE / "recompute_g2_v4.py"
SOURCE = REPO / "crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs"
CDC = REPO / "crates/layerfs-core/src/cdc/mod.rs"
CONTROL = V3 / "operands-v3/phase4_create_edit_benchmark-control"
CANDIDATE = V3 / "operands-v3/phase4_create_edit_benchmark-instrumented"
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
    "v3_payload_manifest": "59e0bbb6d44da9ba02f8c9536a1b55fedfc48ed342a6068087bbd6aaf509a4c3",
    "v3_terminal": "8befdf04037868e0bd2934dccb9e7d3be69b4dad38ba1059d41ea4a375e25f2a",
    "v3_terminal_verification": "85554b79ae15b5f72ccc2d11a84222e7d5aa34a2ce41d2088cc30034535809b3",
    "v3_status": "b8e1ddb9b3eaacea7c4f040f802a4b6bb5224d9535856a941dfc29a5226ce882",
    "v3_raw": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
    "v3_prepare_stderr": "f665ab00c6a188b15810f6c01f152a5941021e698ed13e11cad7c62416d56679",
}
SIZES = {"control": 1372784, "candidate": 1390512, "fixture": 104857600, "base_database": 109199360, "base_authority": 32, "base_expectations": 1096}
AUTHORIZATION = "parent-authorized-exactly-one-fresh-same-middle-v4-ba"
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
        {"sequence": 1, "label": "01-measured-same-middle-pos1-B", "arm": "B", "position": 1, "order": "BA", "operation": "same-middle", "cli_operation": "edit-same", "iteration": 983001, "kind": "measured", "workload": "v4-guard", "warmup": False, "validation": "capture-only"},
        {"sequence": 2, "label": "02-measured-same-middle-pos2-A", "arm": "A", "position": 2, "order": "BA", "operation": "same-middle", "cli_operation": "edit-same", "iteration": 983002, "kind": "measured", "workload": "v4-guard", "warmup": False, "validation": "capture-only"},
    ]


def child_plan():
    operands = RESULTS / "operands-v4"
    candidate = operands / "phase4_create_edit_benchmark-instrumented"
    control = operands / "phase4_create_edit_benchmark-control"
    work = RESULTS / "rows-v4/work-v4"
    planned = []
    for spec in schedule():
        row_root = work / spec["label"]
        planned.append({"kind": "prepare", "label": f"prepare-{spec['label']}", "command": [str(candidate), "--fast-prepare", str(row_root), "104857600", "edit-same", str(spec["iteration"])]})
    for spec in schedule():
        row_root = work / spec["label"]
        binary = candidate if spec["arm"] == "B" else control
        planned.append({"kind": "row", "label": spec["label"], "command": ["/usr/bin/time", "-l", str(binary), "--fast-row", str(row_root), "104857600", "edit-same", str(spec["iteration"]), "false", "capture-only"]})
    planned.extend((
        {"kind": "analyzer", "label": "primary-analysis", "command": [sys.executable, str(ANALYZER), str(RESULTS)]},
        {"kind": "analyzer", "label": "independent-recomputation", "command": [sys.executable, str(RECOMPUTE), str(RESULTS)]},
    ))
    return planned


def ensure_fresh():
    if TARGET.exists() or LOCK.exists():
        raise RuntimeError("G2-v4 result root or lock already exists")


def verify_methodology():
    expected = os.environ.get("G2_V4_METHODOLOGY_SHA256")
    if not expected or not MANIFEST.is_file() or sha256(MANIFEST) != expected:
        raise RuntimeError("G2-v4 methodology manifest custody mismatch")
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
    expected_nodes = {row["path"] for row in rows} | {"PAYLOAD-MANIFEST-v1.tsv", "TERMINAL-v1.json", "TERMINAL-VERIFICATION-v1.txt"}
    actual_nodes = {str(item.relative_to(SEALED)) for item in SEALED.rglob("*") if not item.is_dir()}
    if actual_nodes != expected_nodes:
        mismatches.append("complete-file-closure")
    writable = [str(item.relative_to(SEALED.parent)) for item in (SEALED.parent, *SEALED.parent.rglob("*")) if not item.is_symlink() and item.stat().st_mode & 0o222]
    if writable:
        mismatches.append("writable-subtree")
    bad_symlinks = []
    for item in (entry for entry in SEALED.rglob("*") if entry.is_symlink()):
        try:
            item.resolve().relative_to(SEALED.resolve())
        except ValueError:
            bad_symlinks.append(str(item.relative_to(SEALED)))
    if bad_symlinks:
        mismatches.append("external-symlink")
    if mismatches:
        raise RuntimeError(f"sealed v1 payload mismatch: {mismatches[:3]}")
    return {"entries": len(rows), "mismatches": 0, "manifest_sha256": sha256(payload), "expected_nodes": len(expected_nodes), "actual_nodes": len(actual_nodes), "nonwritable_subtree": True, "symlinks_internal": True}


def verify_v3_history():
    artifacts = {
        "payload": (V3 / "PAYLOAD-MANIFEST-v3.tsv", HASHES["v3_payload_manifest"]),
        "terminal": (V3 / "TERMINAL-v3.json", HASHES["v3_terminal"]),
        "verification": (V3 / "TERMINAL-VERIFICATION-v3.txt", HASHES["v3_terminal_verification"]),
        "status": (V3 / "STATUS-v3.json", HASHES["v3_status"]),
        "raw": (V3 / "rows-v3/G2-V3-RAW.jsonl", HASHES["v3_raw"]),
        "prepare_stderr": (V3 / "preparation-v3/prepare-01-measured-same-middle-pos1-B.stderr", HASHES["v3_prepare_stderr"]),
    }
    for path, digest in artifacts.values():
        verify(path, digest)
    payload_rows = list(csv.DictReader(artifacts["payload"][0].open(), delimiter="\t"))
    mismatches = []
    for row in payload_rows:
        path = V3 / row["path"]
        if not path.is_file() or path.stat().st_size != int(row["size_bytes"]) or sha256(path) != row["sha256"]:
            mismatches.append(row["path"])
    expected_nodes = {row["path"] for row in payload_rows} | {"PAYLOAD-MANIFEST-v3.tsv", "TERMINAL-v3.json", "TERMINAL-VERIFICATION-v3.txt"}
    actual_nodes = {str(item.relative_to(V3)) for item in V3.rglob("*") if not item.is_dir()}
    status = json.loads(artifacts["status"][0].read_text())
    terminal = json.loads(artifacts["terminal"][0].read_text())
    exact_failure = artifacts["prepare_stderr"][0].read_text() == "Error: ValidationAuthorityUnavailable\n" and artifacts["raw"][0].stat().st_size == 0 and status.get("status") == "REVISE" and status.get("fresh_rows") == 0 and status.get("reason") == "RuntimeError: child failed: prepare-01-measured-same-middle-pos1-B" and terminal.get("status") == "REVISE"
    v3_lock = REPO / "target/phase4-g2-materialization-decomposition-20260822-v3.lock"
    nonwritable = not any(item.stat().st_mode & 0o222 for item in (V3.parent, *V3.parent.rglob("*")) if not item.is_symlink())
    if mismatches or len(payload_rows) != 19 or actual_nodes != expected_nodes or not exact_failure or v3_lock.exists() or not nonwritable:
        raise RuntimeError("sealed v3 historical failure custody mismatch")
    return {"payload_entries": 19, "payload_mismatches": 0, "expected_nodes": 22, "actual_nodes": 22, "status": "REVISE", "fresh_rows": 0, "raw_sha256": HASHES["v3_raw"], "failure": "ValidationAuthorityUnavailable", "terminal_sha256": HASHES["v3_terminal"], "terminal_verification_sha256": HASHES["v3_terminal_verification"], "payload_manifest_sha256": HASHES["v3_payload_manifest"], "subtree_nonwritable": True, "lock_absent": True}


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
    if mode(CONTROL) != "0444" or mode(CANDIDATE) != "0444" or mode(BASE) != "0444" or mode(BASE_FILES["authority"]) != "0444":
        raise RuntimeError("sealed v3 operands or sealed v1 base modes drifted")
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
    v3_history = verify_v3_history()
    return {"branch": branch, "head": head, "methodology_manifest_sha256": sha256(MANIFEST), "sealed_v1_raw_sha256": sha256(sealed_artifacts["v1_raw"]), "retained_g1_source_sha256": sha256(SOURCE), "fastcdc_source_sha256": sha256(CDC), "v1_root_read_only": True, "v1_lock_absent": True, "v1_instrumented_source_bytes": "not-retained-not-verified", "v1_source_diff_bytes": "not-retained-not-verified", "sealed_v1_payload": payload, "sealed_v3_history": v3_history, "base_proxy_plan_verified": {**base_proxy_plan(), "source_database_exists": BASE.is_file(), "source_authority_exists": BASE_FILES["authority"].is_file(), "expectations_not_required_by_prepare": True}}


def dry_run(preflight_record):
    ensure_fresh()
    if DRY_RUN.exists():
        raise RuntimeError("G2-v4 dry-run already exists")
    rows = schedule()
    record = {
        "schema": "phase4-g2-protocol-closure-dry-run-v4",
        "status": "PASS",
        "preflight": preflight_record,
        "schedule": rows,
        "planned_invocations": 6,
        "planned_measured_rows": 2,
        "actual_rows": 0,
        "database_copies_created": 0,
        "benchmark_children_invoked": 0,
        "planned_benchmark_children": 4,
        "planned_analyzer_children": 2,
        "invocation_plan": child_plan(),
        "full_v1_rows_rerun": 0,
        "product_source_changes": 0,
        "planned_order": "BA",
        "planned_executable_snapshots": 2,
        "executable_sources": {"control": str(CONTROL), "candidate": str(CANDIDATE), "source_mode": "0444", "copy_mode": "0500"},
        "planned_v1_payload_reverification": {"entries": 178, "mismatches": 0},
        "retained_evidence_ceiling_bytes": 10 * 1024 * 1024,
        "sealed_fixture_and_base_referenced_read_only": True,
        "base_proxy_plan": base_proxy_plan(),
        "planned_private_authority_copies": 1,
        "planned_expectations_copies": 0,
        "planned_distinct_row_copies": 2,
        "transient_peak_ceiling_bytes": 300 * 1024 * 1024,
        "transient_paths_deleted_before_seal": ["results-v4/rows-v4/work-v4"],
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
            raise TimeoutError("G2-v4 global 59-second ceiling exhausted")
        timeout = min(timeout, remaining)
    completed = subprocess.run([str(item) for item in command], cwd=REPO, env=env, capture_output=True, text=True, timeout=timeout)
    output_dir.mkdir(parents=True, exist_ok=True)
    (output_dir / f"{label}.stdout").write_text(completed.stdout)
    (output_dir / f"{label}.stderr").write_text(completed.stderr)
    if completed.returncode and not allow_nonzero:
        raise RuntimeError(f"child failed: {label}")
    return completed


def recorded_child(plan_entry, output_dir, started_ns, env=None, allow_nonzero=False):
    chronology("child-start", started_ns, kind=plan_entry["kind"], label=plan_entry["label"], command=plan_entry["command"])
    completed = run_child(plan_entry["command"], plan_entry["label"], output_dir, env=env, allow_nonzero=allow_nonzero, started_ns=started_ns)
    chronology("child-complete", started_ns, kind=plan_entry["kind"], label=plan_entry["label"], command=plan_entry["command"], exit_code=completed.returncode)
    return completed


def parse_time(stderr):
    timing = re.search(r"([0-9.]+) real\s+([0-9.]+) user\s+([0-9.]+) sys", stderr)
    rss = re.search(r"(\d+)\s+maximum resident set size", stderr)
    footprint = re.search(r"(\d+)\s+peak memory footprint", stderr)
    if not timing or not rss or not footprint:
        raise RuntimeError("incomplete /usr/bin/time -l output")
    return {"external_real_seconds": float(timing.group(1)), "user_seconds": float(timing.group(2)), "system_seconds": float(timing.group(3)), "maximum_resident_set_bytes": int(rss.group(1)), "peak_memory_footprint_bytes": int(footprint.group(1))}


def snapshot_binaries():
    operands = RESULTS / "operands-v4"
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
    (RESULTS / "OPERAND-CUSTODY-v4.json").write_text(json.dumps(custody, indent=2, sort_keys=True) + "\n")
    return control, candidate


def copy_methodology():
    destination = RESULTS / "methodology-v4"
    destination.mkdir()
    with MANIFEST.open() as handle:
        for row in csv.DictReader(handle, delimiter="\t"):
            shutil.copyfile(HERE / row["path"], destination / row["path"])
    shutil.copyfile(MANIFEST, destination / MANIFEST.name)
    shutil.copyfile(DRY_RUN, destination / DRY_RUN.name)


def chronology(event, started_ns, **fields):
    record = {"event": event, "monotonic_elapsed_ns": time.monotonic_ns() - started_ns, "wall_time_ns": time.time_ns(), **fields}
    with (RESULTS / "CHRONOLOGY-v4.jsonl").open("a") as handle:
        handle.write(json.dumps(record, separators=(",", ":"), sort_keys=True) + "\n")


def global_gate(started_ns):
    if time.monotonic_ns() - started_ns >= CAMPAIGN_CEILING_NS:
        raise TimeoutError("G2-v4 global 59-second ceiling exhausted")


def base_proxy_plan():
    proxy = RESULTS / "rows-v4/work-v4/base-proxy/base.sqlite"
    return {"database_proxy_path": str(proxy), "database_symlink_target": str(BASE), "database_sha256": HASHES["base_database"], "database_source_mode": "0444", "authority_proxy_path": str(Path(str(proxy) + ".authority")), "authority_source_path": str(BASE_FILES["authority"]), "authority_sha256": HASHES["base_authority"], "authority_source_mode": "0444", "authority_private_mode": "0600", "expectations_copy": False, "prepare_source_contract": ["database", "authority"], "declared_cleanup": "rows-v4/work-v4"}


def create_base_proxy():
    plan = base_proxy_plan()
    proxy = Path(plan["database_proxy_path"])
    authority = Path(plan["authority_proxy_path"])
    proxy.parent.mkdir(parents=True)
    os.symlink(os.path.relpath(BASE, proxy.parent), proxy)
    shutil.copyfile(BASE_FILES["authority"], authority)
    authority.chmod(0o600)
    source_stat, authority_stat = BASE_FILES["authority"].stat(), authority.stat()
    custody = {**plan, "status": "READY", "database_is_symlink": proxy.is_symlink(), "database_resolved_path": str(proxy.resolve()), "database_resolved_sha256": sha256(proxy.resolve()), "database_resolved_mode": mode(proxy.resolve()), "authority_private_sha256": sha256(authority), "authority_private_mode_actual": mode(authority), "authority_source_device": source_stat.st_dev, "authority_source_inode": source_stat.st_ino, "authority_private_device": authority_stat.st_dev, "authority_private_inode": authority_stat.st_ino, "authority_distinct_device_inode": (source_stat.st_dev, source_stat.st_ino) != (authority_stat.st_dev, authority_stat.st_ino), "work_path_absent_after_cleanup": False}
    exact = custody["database_is_symlink"] and custody["database_resolved_path"] == str(BASE.resolve()) and custody["database_resolved_sha256"] == HASHES["base_database"] and custody["database_resolved_mode"] == "0444" and custody["authority_private_sha256"] == HASHES["base_authority"] and custody["authority_private_mode_actual"] == "0600" and custody["authority_distinct_device_inode"] and not Path(str(proxy) + ".expectations").exists()
    if not exact:
        raise RuntimeError("base proxy custody mismatch")
    (RESULTS / "BASE-PROXY-CUSTODY-v4.json").write_text(json.dumps(custody, indent=2, sort_keys=True) + "\n")
    sample_transient("base-proxy-ready")
    return proxy


def prepare_rows(rows, started_ns, prepared_base):
    prepared = {}
    plans = child_plan()[:2]
    for spec, plan_entry in zip(rows, plans):
        row_root = RESULTS / "rows-v4/work-v4" / spec["label"]
        row_root.mkdir(parents=True)
        os.symlink(os.path.relpath(FIXTURE, row_root), row_root / FIXTURE.name)
        env = os.environ.copy()
        env["LAYERFS_PREPARED_BASE_DATABASE"] = str(prepared_base)
        global_gate(started_ns)
        recorded_child(plan_entry, RESULTS / "preparation-v4", started_ns, env=env)
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
        sample_transient(f"prepared-{spec['label']}")
    return prepared


def acquire(spec, prepared, started_ns):
    global_gate(started_ns)
    row_root, database, authority, expectations, hashes = prepared[spec["label"]]
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
    plan_entry = child_plan()[1 + spec["sequence"]]
    completed = recorded_child(plan_entry, RESULTS / "rows-v4", started_ns, env=env)
    row = json.loads(completed.stdout)
    row.update(spec)
    row.update(parse_time(completed.stderr))
    row["binary_sha256"] = HASHES["control" if spec["arm"] == "A" else "candidate"]
    row["residue_files"] = sorted(str(item.relative_to(row_root)) for item in row_root.rglob("*") if item.is_file() and item.name.endswith(("-journal", "-wal", "-shm")))
    row["post_database_sha256"] = sha256(database)
    row["post_authority_sha256"] = sha256(authority)
    row["post_expectations_sha256"] = sha256(expectations)
    row["post_modes"] = {"database": mode(database), "authority": mode(authority), "expectations": mode(expectations)}
    with (RESULTS / "rows-v4/G2-V4-RAW.jsonl").open("a") as handle:
        handle.write(json.dumps(row, separators=(",", ":"), sort_keys=True) + "\n")
    sample_transient(f"completed-{spec['label']}")


def transient_usage():
    root = RESULTS / "rows-v4/work-v4"
    apparent = allocated = 0
    if root.exists():
        for item in root.rglob("*"):
            if item.is_symlink() or not item.is_file():
                continue
            stat = item.stat()
            apparent += stat.st_size
            allocated += stat.st_blocks * 512
    return {"apparent_bytes": apparent, "allocated_bytes": allocated}


def sample_transient(stage):
    path = RESULTS / "TRANSIENT-USAGE-v4.json"
    record = json.loads(path.read_text()) if path.is_file() else {"samples": [], "peak_apparent_bytes": 0, "peak_allocated_bytes": 0, "ceiling_bytes": 300 * 1024 * 1024}
    usage = {"stage": stage, **transient_usage()}
    record["samples"].append(usage)
    record["peak_apparent_bytes"] = max(record["peak_apparent_bytes"], usage["apparent_bytes"])
    record["peak_allocated_bytes"] = max(record["peak_allocated_bytes"], usage["allocated_bytes"])
    record["within_ceiling"] = max(record["peak_apparent_bytes"], record["peak_allocated_bytes"]) <= record["ceiling_bytes"]
    path.write_text(json.dumps(record, indent=2, sort_keys=True) + "\n")


def transient_report(prepared, complete):
    records = []
    for label, (_, database, authority, expectations, _) in prepared.items():
        if all(item.is_file() for item in (database, authority, expectations)):
            records.append({"label": label, "database_sha256": sha256(database), "database_size": database.stat().st_size, "authority_sha256": sha256(authority), "expectations_sha256": sha256(expectations), "residue": sorted(item.name for item in database.parent.iterdir() if item.name.endswith(("-journal", "-wal", "-shm")))})
    exact = all(row["database_sha256"] == "b69861ee81c4a01906cf2fb70fe4ef49c4de534cab9ab9b000006efe6802fe31" and row["database_size"] == 109314048 and row["authority_sha256"] == HASHES["base_authority"] and row["expectations_sha256"] == HASHES["prepared_expectations"] and not row["residue"] for row in records)
    usage = json.loads((RESULTS / "TRANSIENT-USAGE-v4.json").read_text()) if (RESULTS / "TRANSIENT-USAGE-v4.json").is_file() else {"within_ceiling": False}
    proxy_custody = json.loads((RESULTS / "BASE-PROXY-CUSTODY-v4.json").read_text()) if (RESULTS / "BASE-PROXY-CUSTODY-v4.json").is_file() else {}
    work = RESULTS / "rows-v4/work-v4"
    report = {"schema": "phase4-g2-v4-transient-verification-v1", "status": "PENDING_DELETE", "rows_validated": complete and len(records) == 2 and exact, "base_proxy_ready": proxy_custody.get("status") == "READY" and proxy_custody.get("authority_private_mode_actual") == "0600" and proxy_custody.get("expectations_copy") is False, "records": records, "usage": usage, "declared_deletions": ["rows-v4/work-v4"], "work_path_existed": work.is_dir(), "deletion_complete": False, "work_path_absent": False}
    path = RESULTS / "TRANSIENT-VERIFICATION-v4.json"
    path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    return path, report


def discard_transients(report_path, report):
    results_root = RESULTS.resolve()
    deleted = []
    for transient in (RESULTS / "rows-v4/work-v4",):
        resolved = transient.resolve()
        try:
            resolved.relative_to(results_root)
        except ValueError as error:
            raise RuntimeError(f"unsafe transient path: {transient}") from error
        if not transient.is_dir():
            raise RuntimeError(f"declared transient work path never existed: {transient}")
        shutil.rmtree(transient)
        deleted.append(str(transient.relative_to(RESULTS)))
        if transient.exists():
            raise RuntimeError(f"transient work path survived cleanup: {transient}")
    passed = report["rows_validated"] and report["base_proxy_ready"] and report["usage"].get("within_ceiling") is True and report["work_path_existed"] and deleted == report["declared_deletions"]
    report.update({"status": "PASS" if passed else "REVISE", "deletion_complete": True, "deleted": deleted, "work_path_absent": not (RESULTS / "rows-v4/work-v4").exists()})
    report_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    custody_path = RESULTS / "BASE-PROXY-CUSTODY-v4.json"
    if custody_path.is_file():
        custody = json.loads(custody_path.read_text())
        custody.update({"status": "PASS" if passed else "REVISE", "work_path_absent_after_cleanup": not (RESULTS / "rows-v4/work-v4").exists(), "deleted": deleted})
        custody_path.write_text(json.dumps(custody, indent=2, sort_keys=True) + "\n")


def payload_manifest():
    manifest = RESULTS / "PAYLOAD-MANIFEST-v4.tsv"
    excluded = {manifest, RESULTS / "TERMINAL-v4.json", RESULTS / "TERMINAL-VERIFICATION-v4.txt"}
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
    records = [json.loads(line) for line in (RESULTS / "CHRONOLOGY-v4.jsonl").read_text().splitlines() if line]
    observed = [{key: row.get(key) for key in ("event", "kind", "label", "command", "exit_code") if key in row} for row in records if row.get("event") in ("child-start", "child-complete")]
    expected = []
    for child in child_plan():
        expected.append({"event": "child-start", "kind": child["kind"], "label": child["label"], "command": child["command"]})
        expected.append({"event": "child-complete", "kind": child["kind"], "label": child["label"], "command": child["command"], "exit_code": 0})
    return [] if observed == expected else ["exact-child-chronology"]


def prepare_terminal(status, disposition, reason, started_ns):
    for _ in range(2):
        status_record = {"status": status, "disposition": disposition, "reason": reason, "fresh_rows": sum(1 for line in (RESULTS / "rows-v4/G2-V4-RAW.jsonl").read_text().splitlines() if line) if (RESULTS / "rows-v4/G2-V4-RAW.jsonl").is_file() else 0, "sealed_v1_rows_rerun": 0, "g3_eligible": False, "post_pass_static_closure_required": True}
        (RESULTS / "STATUS-v4.json").write_text(json.dumps(status_record, indent=2, sort_keys=True) + "\n")
        mode_policy = {"schema": "phase4-g2-v4-final-mode-policy-v1", "retained_files_mode": "0444", "retained_directories_mode": "0555", "symlinks": 0, "lock_absent_before_authoritative_terminal": True}
        (RESULTS / "FINAL-MODE-POLICY-v4.json").write_text(json.dumps(mode_policy, indent=2, sort_keys=True) + "\n")
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
        primary_path = RESULTS / "G2-V4-ANALYSIS.json"
        independent_path = RESULTS / "G2-V4-INDEPENDENT-RECOMPUTATION.json"
        normalized = None
        if primary_path.is_file() and independent_path.is_file():
            primary_ledger = json.loads(primary_path.read_text()).get("normalized_ledger")
            independent_ledger = json.loads(independent_path.read_text()).get("normalized_ledger")
            if primary_ledger == independent_ledger:
                normalized = hashlib.sha256(json.dumps(primary_ledger, sort_keys=True, separators=(",", ":")).encode()).hexdigest()
        bound_paths = {"fresh_raw_sha256": RESULTS / "rows-v4/G2-V4-RAW.jsonl", "primary_analysis_sha256": primary_path, "independent_analysis_sha256": independent_path, "chronology_sha256": RESULTS / "CHRONOLOGY-v4.jsonl", "cleanup_sha256": RESULTS / "TRANSIENT-VERIFICATION-v4.json", "base_proxy_custody_sha256": RESULTS / "BASE-PROXY-CUSTODY-v4.json"}
        elapsed = time.monotonic_ns() - started_ns
        terminal = {"status": status, "disposition": disposition, "reason": reason, "payload_manifest_sha256": sha256(manifest), "payload_manifest_entries": len(rows), "payload_mismatches": len(mismatches), "retained_evidence_bytes": retained_bytes, "status_sha256": sha256(RESULTS / "STATUS-v4.json"), "methodology_manifest_sha256": sha256(MANIFEST), "dry_run_sha256": sha256(DRY_RUN), "normalized_ledger_sha256": normalized, "v1_terminal_sha256": sha256(SEALED / "TERMINAL-v1.json"), "v1_terminal_verification_sha256": sha256(SEALED / "TERMINAL-VERIFICATION-v1.txt"), "v3_terminal_sha256": HASHES["v3_terminal"], "v3_terminal_verification_sha256": HASHES["v3_terminal_verification"], "v3_payload_manifest_sha256": HASHES["v3_payload_manifest"], "v3_empty_raw_sha256": HASHES["v3_raw"], "final_mode_policy_sha256": sha256(RESULTS / "FINAL-MODE-POLICY-v4.json"), "lock_absent": not LOCK.exists(), "global_elapsed_ns": elapsed, "global_ceiling_ns": CAMPAIGN_CEILING_NS, "global_within_ceiling": elapsed < CAMPAIGN_CEILING_NS, "g3_eligible": False, **{name: sha256(path) if path.is_file() else None for name, path in bound_paths.items()}}
        verification = {"status": "PASS" if not mismatches else "FAIL", "disposition": disposition, "payload_manifest_sha256": sha256(manifest), "payload_manifest_entries": len(rows), "payload_mismatches": len(mismatches), "status_sha256": sha256(RESULTS / "STATUS-v4.json"), "final_mode_policy_sha256": terminal["final_mode_policy_sha256"], "global_elapsed_ns": elapsed, "global_ceiling_ns": CAMPAIGN_CEILING_NS, "lock_absent": not LOCK.exists(), "final_mode_mismatches": 0}
        if status == "PASS" and elapsed >= CAMPAIGN_CEILING_NS:
            status, disposition, reason = "REVISE", "G2 REVISE", "global-59-second-ceiling"
            continue
        return status, disposition, terminal, verification
    raise RuntimeError("unable to prepare G2-v4 terminal state")


def seal_payload():
    if any(entry.is_symlink() for entry in TARGET.rglob("*")):
        raise RuntimeError("retained symlink before seal")
    for item in sorted((entry for entry in TARGET.rglob("*") if entry.is_file()), key=lambda entry: len(entry.parts), reverse=True):
        item.chmod(0o444)
    for item in sorted((entry for entry in TARGET.rglob("*") if entry.is_dir() and entry != RESULTS), key=lambda entry: len(entry.parts), reverse=True):
        item.chmod(0o555)


def write_authoritative_terminal(terminal, verification, started_ns):
    if LOCK.exists():
        raise RuntimeError("lock exists before authoritative terminal")
    terminal["global_elapsed_ns"] = time.monotonic_ns() - started_ns
    terminal["global_within_ceiling"] = terminal["global_elapsed_ns"] < CAMPAIGN_CEILING_NS
    terminal_path = RESULTS / "TERMINAL-v4.json"
    verification_path = RESULTS / "TERMINAL-VERIFICATION-v4.txt"
    for path in (terminal_path, verification_path):
        if path.exists():
            path.chmod(0o600)
    terminal_path.write_text(json.dumps(terminal, indent=2, sort_keys=True) + "\n")
    verification.update({"terminal_sha256": sha256(terminal_path), "global_elapsed_ns": time.monotonic_ns() - started_ns, "lock_absent": True})
    verification_path.write_text("\n".join(f"{key}={value}" for key, value in verification.items()) + "\n")
    terminal_path.chmod(0o444)
    verification_path.chmod(0o444)
    RESULTS.chmod(0o555)
    TARGET.chmod(0o555)


def verify_final_seal(started_ns):
    issues = []
    if LOCK.exists() or any(entry.is_symlink() for entry in TARGET.rglob("*")):
        issues.append("lock-or-symlink")
    issues.extend(f"file-mode:{item.relative_to(TARGET)}" for item in TARGET.rglob("*") if item.is_file() and mode(item) != "0444")
    issues.extend(f"directory-mode:{item.relative_to(TARGET)}" for item in (TARGET, *TARGET.rglob("*")) if item.is_dir() and mode(item) != "0555")
    manifest = RESULTS / "PAYLOAD-MANIFEST-v4.tsv"
    if manifest.is_file():
        _, mismatches = verify_payload(manifest)
        issues.extend(f"payload:{item}" for item in mismatches)
    else:
        issues.append("payload-manifest-missing")
    if time.monotonic_ns() - started_ns >= CAMPAIGN_CEILING_NS:
        issues.append("global-59-second-ceiling")
    return issues


def unseal_target():
    TARGET.chmod(0o755)
    for item in (entry for entry in TARGET.rglob("*") if entry.is_dir()):
        item.chmod(0o755)
    for item in (entry for entry in TARGET.rglob("*") if entry.is_file()):
        item.chmod(0o600)


def release_lock():
    if LOCK.exists():
        if not LOCK.is_dir():
            raise RuntimeError("G2-v4 lock is not a directory")
        LOCK.rmdir()
    if LOCK.exists():
        raise RuntimeError("G2-v4 lock survived release")


def finalize_and_seal(status, disposition, reason, started_ns):
    try:
        status, disposition, terminal, verification = prepare_terminal(status, disposition, reason, started_ns)
        seal_payload()
        write_authoritative_terminal(terminal, verification, started_ns)
        issues = verify_final_seal(started_ns)
    except BaseException as original:
        unseal_target()
        status, disposition, terminal, verification = prepare_terminal("REVISE", "G2 REVISE", f"finalization {type(original).__name__}: {original}", started_ns)
        seal_payload()
        write_authoritative_terminal(terminal, verification, started_ns)
        remaining = [item for item in verify_final_seal(started_ns) if item != "global-59-second-ceiling"]
        if remaining:
            raise RuntimeError(f"failed to reseal REVISE evidence: {remaining}")
        raise original
    if issues:
        unseal_target()
        status, disposition, terminal, verification = prepare_terminal("REVISE", "G2 REVISE", ",".join(issues), started_ns)
        seal_payload()
        write_authoritative_terminal(terminal, verification, started_ns)
        remaining = [item for item in verify_final_seal(started_ns) if item != "global-59-second-ceiling"]
        if remaining:
            raise RuntimeError(f"failed to reseal REVISE evidence: {remaining}")
    return status, disposition


def execute(preflight_record, started_ns):
    ensure_fresh()
    if os.environ.get("G2_V4_EXECUTE_AUTHORIZATION") != AUTHORIZATION:
        raise RuntimeError("parent execute authorization is absent")
    expected_dry = os.environ.get("G2_V4_DRY_RUN_SHA256")
    if not expected_dry or not DRY_RUN.is_file() or sha256(DRY_RUN) != expected_dry:
        raise RuntimeError("G2-v4 dry-run custody mismatch")
    LOCK.mkdir()
    failure = None
    prepared = {}
    cleanup_done = False
    status, disposition, reason = "REVISE", "G2 REVISE", "campaign did not reach analyzer agreement"
    try:
        RESULTS.mkdir(parents=True)
        (RESULTS / "rows-v4").mkdir()
        (RESULTS / "CHRONOLOGY-v4.jsonl").write_text("")
        chronology("campaign-start", started_ns, planned_rows=2, order="BA")
        copy_methodology()
        control_binary, candidate_binary = snapshot_binaries()
        (RESULTS / "G2-V4-SCHEDULE.json").write_text(json.dumps(schedule(), indent=2, sort_keys=True) + "\n")
        (RESULTS / "CHRONOLOGY-PLAN-v4.json").write_text(json.dumps(child_plan(), indent=2, sort_keys=True) + "\n")
        (RESULTS / "INPUT-BINDINGS-v4.json").write_text(json.dumps({"preflight": preflight_record, "hashes": HASHES, "methodology_manifest_sha256": sha256(MANIFEST), "dry_run_sha256": sha256(DRY_RUN)}, indent=2, sort_keys=True) + "\n")
        (RESULTS / "rows-v4/G2-V4-RAW.jsonl").write_text("")
        prepared_base = create_base_proxy()
        prepared = prepare_rows(schedule(), started_ns, prepared_base)
        for spec in schedule():
            acquire(spec, prepared, started_ns)
        report_path, report = transient_report(prepared, True)
        discard_transients(report_path, report)
        if report.get("status") != "PASS":
            raise RuntimeError("transient cleanup verification failed")
        cleanup_done = True
        global_gate(started_ns)
        primary = recorded_child(child_plan()[4], RESULTS, started_ns, allow_nonzero=True)
        global_gate(started_ns)
        independent = recorded_child(child_plan()[5], RESULTS, started_ns, allow_nonzero=True)
        primary_result = json.loads((RESULTS / "G2-V4-ANALYSIS.json").read_text())
        independent_result = json.loads((RESULTS / "G2-V4-INDEPENDENT-RECOMPUTATION.json").read_text())
        if primary.returncode or independent.returncode or (primary_result["status"], primary_result["disposition"], sorted(primary_result["failures"]), primary_result["normalized_ledger"]) != (independent_result["status"], independent_result["disposition"], sorted(independent_result["failures"]), independent_result["normalized_ledger"]):
            raise RuntimeError("G2-v4 analyzers failed or disagreed")
        status, disposition, reason = primary_result["status"], primary_result["disposition"], "primary and independent normalized ledgers agree"
        chronology("analyzers-complete", started_ns, status=status, normalized_ledger_sha256=hashlib.sha256(json.dumps(primary_result["normalized_ledger"], sort_keys=True).encode()).hexdigest())
    except BaseException as error:
        failure = error
        status, disposition, reason = "REVISE", "G2 REVISE", f"{type(error).__name__}: {error}"
    finally:
        try:
            if RESULTS.exists():
                try:
                    if not cleanup_done and (RESULTS / "rows-v4/work-v4").exists():
                        report_path, report = transient_report(prepared, False)
                        discard_transients(report_path, report)
                except BaseException as error:
                    status, disposition, reason = "REVISE", "G2 REVISE", f"cleanup {type(error).__name__}: {error}"
                    failure = failure or error
                try:
                    release_lock()
                except BaseException as error:
                    status, disposition, reason = "REVISE", "G2 REVISE", f"lock {type(error).__name__}: {error}"
                    failure = failure or error
                try:
                    status, disposition = finalize_and_seal(status, disposition, reason, started_ns)
                except BaseException as error:
                    failure = failure or error
        finally:
            try:
                release_lock()
            except BaseException as error:
                failure = failure or error
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
        print(f"G2-v4 failure: {type(error).__name__}: {error}", file=sys.stderr)
        raise SystemExit(1)
