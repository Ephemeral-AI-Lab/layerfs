#!/usr/bin/env python3
"""Fail-closed analyzer for publication-repair v2 and sealed-v1 composition."""

import csv
import hashlib
import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[3]
V1 = REPO / "target/phase4-canonical-v2-publication-repair-20260821-v1/results-v1"
V1_MANIFEST = V1 / "TERMINAL-MANIFEST-v1.tsv"
V1_VERIFICATION = V1 / "TERMINAL-MANIFEST-VERIFICATION-v1.txt"
V1_RAW = V1 / "RAW-v1.jsonl"
V1_CUSTODY = V1 / "INPUT-CUSTODY-v1.tsv"
V1_SOURCE_CUSTODY = V1 / "SOURCE-BUILD-CUSTODY-v1.tsv"
V1_ANALYSIS = V1 / "ANALYSIS-v1.json"
V1_DISPOSITION = V1 / "DISPOSITION-v1.txt"
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
V1_VERIFICATION_SHA = "f38dced6d98ffd30336e6b40694b1744bb90889bec657df6983ed134a5f5f1df"
V1_RAW_SHA = "777ec722f95578c1717e86cd5100c01c497a876d0ffea557bcf2864f285eb532"
V1_CUSTODY_SHA = "d1b7f50897c59996672f761579f0904bb5453d469e09e8f977d72400f153635a"
V1_SOURCE_CUSTODY_SHA = "e78e83ff45add569ae6cf4674f796ac3a857c501cba37d035ab6c14b101630a0"
V1_ANALYSIS_SHA = "57337c844dc85b33cc3e1d2f9bc9baae03ca058ab634e1f3cfe14dbc921797d9"
V1_DISPOSITION_SHA = "c96ad84e436a513e3793e48beb75a50b2a5d59c81ca876c3ff88bb3094eb50f6"
MASTER_SHA = {
    "database": "962b491e70551db76d3712d966c25259a96b23df453a4342b92c97adcc06a996",
    "authority": "abac9762e55b20e4a7db6b42bfaa435fb9af8e3a0a79d061f4dd05ee63ef6f12",
    "expectations": "a9bf6f2ae2592c755e584672bc55b371468beb00721c69fd06403d2b5d6d2b7d",
}
PROFILE = "94a03ba7b6c97b5ff37c0ec62ef1d801b9896494b45456bd3df23e2cb278d13b"
CONTROL_PROFILE = "b0ebb845409ef995a5fa454bb23d10a80c6ecf44deb7832ca2ce1213eb0f4ba1"
FINGERPRINT = "bb883eecf4ea85d80432953791dcc352243da94175e7503e2c476afe9bd0bab7"
SEQUENCE = "4060424f80635c79ea7fba81c8daf7777e9261a3abf4df24104368de5e6b9745"
CLOSURE = "b71da56600ce3c2011cdca037771c9050fbf5f16df2a2297b19e4af11173878e"
ROOT_ID = "ae63b984c0ea1fd0ba7f8fe39c6acaa434f839ff3da2acf63cb2c91880d4a5e0"
TRANSITION_ID = "db53b6664ddbc43c29e43c7fdb106f168dc203266b39383e188a9719fa7da24b"
FULL_CLOSURE = {
    "A": "d6aac6e40cc851dd6295dbeec6488f1c5ebefa7520f86b0cd12bdcdce1f0d54a",
    "B": "29233d6018b031f6035c8e2c8175f1ab86fb721808f21a8858bc965567e9c0c1",
}
GUARD_CLOSURE = {
    "same-middle": "d7614133f35f1a254d0d2222815cdbcbdcd69915baf30c3a801831e6497b1683",
    "one-byte-middle": CLOSURE,
    "plus1-middle": "4cdcd09b47447c6673d391bdbece5eb239bd26bb9320061223f44d22e56d104c",
}


def sha(path):
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def rows(path):
    return list(csv.DictReader(path.open(), delimiter="\t"))


def phase(row, name):
    matches = [item for item in row.get("phase_counters", []) if item.get("phase") == name]
    if len(matches) != 1:
        raise ValueError(f"expected one {name} phase, got {len(matches)}")
    return matches[0]


def exact(item, wanted, reason, reasons):
    if any(item.get(key) != value for key, value in wanted.items()):
        reasons.append(reason)


def timer_equations_match(row):
    keys = (
        "durable_capture_total_wall_ns", "canonical_cas_mapping_stage_wall_ns",
        "precommit_closure_validation_wall_ns", "sqlite_commit_durability_wall_ns",
        "commit_dispatch_to_return_wall_ns", "commit_pre_and_post_dispatch_wall_ns",
        "commit_observation_sum_wall_ns", "commit_publish_call_wall_ns",
        "commit_caller_wrapper_wall_ns", "complete_lifecycle_total_wall_ns",
        "durable_phase_sum_ns", "lifecycle_phase_sum_ns",
    )
    if any(not isinstance(row.get(key), int) or row.get(key) < 0 for key in keys):
        return False
    durable = row["durable_capture_total_wall_ns"]
    return (
        row["commit_dispatch_to_return_wall_ns"] > 0
        and durable == row["canonical_cas_mapping_stage_wall_ns"]
            + row["precommit_closure_validation_wall_ns"]
            + row["sqlite_commit_durability_wall_ns"]
        and row["commit_publish_call_wall_ns"]
            == row["commit_dispatch_to_return_wall_ns"]
            + row["commit_pre_and_post_dispatch_wall_ns"]
        and row["commit_observation_sum_wall_ns"]
            == row["commit_publish_call_wall_ns"] + row["commit_caller_wrapper_wall_ns"]
        and row["sqlite_commit_durability_wall_ns"] == row["commit_observation_sum_wall_ns"]
        and row["complete_lifecycle_total_wall_ns"] == durable
        and row["durable_phase_sum_ns"] == durable
        and row["lifecycle_phase_sum_ns"] == durable
        and row.get("durable_phase_sum_matches") is True
        and row.get("lifecycle_phase_sum_matches") is True
        and row.get("commit_timer_equation_matches") is True
    )


def verify_v1(reasons):
    anchors = {
        V1_MANIFEST: V1_MANIFEST_SHA,
        V1_VERIFICATION: V1_VERIFICATION_SHA,
        V1_RAW: V1_RAW_SHA,
        V1_CUSTODY: V1_CUSTODY_SHA,
        V1_SOURCE_CUSTODY: V1_SOURCE_CUSTODY_SHA,
        V1_ANALYSIS: V1_ANALYSIS_SHA,
        V1_DISPOSITION: V1_DISPOSITION_SHA,
        V1_CANDIDATE: CANDIDATE_SHA,
        V1_FIXTURE: FIXTURE_SHA,
        CONTROL: CONTROL_SHA,
        SOURCE: SOURCE_SHA,
        V1_MASTER: MASTER_SHA["database"],
        Path(str(V1_MASTER) + ".authority"): MASTER_SHA["authority"],
        Path(str(V1_MASTER) + ".expectations"): MASTER_SHA["expectations"],
    }
    for path, expected in anchors.items():
        if not path.is_file() or sha(path) != expected:
            reasons.append(f"v1-anchor:{path.name}")

    manifest_rows = rows(V1_MANIFEST) if V1_MANIFEST.is_file() else []
    mismatches = []
    for item in manifest_rows:
        path = REPO / item["path"]
        if (not path.is_file() or sha(path) != item["sha256"]
                or path.stat().st_size != int(item["size_bytes"])):
            mismatches.append(item["path"])
    recorded = {(REPO / item["path"]).resolve() for item in manifest_rows}
    actual = {path.resolve() for path in V1.rglob("*") if path.is_file()}
    if (len(manifest_rows) != 126 or mismatches
            or actual != recorded | {V1_MANIFEST.resolve(), V1_VERIFICATION.resolve()}
            or len(actual) != 128):
        reasons.append("v1-manifest-closure")
    verification = V1_VERIFICATION.read_text() if V1_VERIFICATION.is_file() else ""
    if verification != (
            "status=PASS\nentries=126\nmismatches=0\n"
            f"manifest_sha256={V1_MANIFEST_SHA}\n"):
        reasons.append("v1-manifest-verification")

    source_rows = rows(V1_SOURCE_CUSTODY) if V1_SOURCE_CUSTODY.is_file() else []
    if len(source_rows) != 36:
        reasons.append("v1-source-custody-count")
    for item in source_rows:
        path = REPO / item["path"]
        if (not path.is_file() or sha(path) != item["sha256"]
                or path.stat().st_size != int(item["size_bytes"])):
            reasons.append(f"v1-source-custody:{item['path']}")

    historical = json.loads(V1_ANALYSIS.read_text()) if V1_ANALYSIS.is_file() else {}
    if (historical.get("status") != "REVISE"
            or historical.get("reasons") != ["guard-one-byte-middle-B:changed-spine-work"]
            or not V1_DISPOSITION.read_text().startswith("CANONICAL-V2 PUBLICATION-REPAIR REVISE\n")):
        reasons.append("v1-historical-disposition")

    expected_labels = [
        "warm-full-100-A", "warm-full-100-B", "primary-full-100-p0-A",
        "primary-full-100-p0-B", "primary-full-100-p1-B",
        "primary-full-100-p1-A", "guard-same-middle-B",
        "guard-one-byte-middle-B", "guard-plus1-middle-B",
    ]
    schedule = rows(V1 / "SCHEDULE-v1.tsv")
    raw = [json.loads(line) for line in V1_RAW.read_text().splitlines() if line]
    if len(schedule) != 9 or len(raw) != 9 or [item.get("label") for item in schedule] != expected_labels:
        reasons.append("v1-raw-schedule")
        return []
    by_label = dict(zip(expected_labels, raw))
    for spec, row in zip(schedule, raw):
        arm, operation, label = spec.get("arm"), spec.get("operation"), spec.get("label")
        references = 5_285 if operation == "plus1-middle" else 5_284
        closure = FULL_CLOSURE.get(arm) if operation == "full" else GUARD_CLOSURE.get(operation)
        expected_profile = CONTROL_PROFILE if arm == "A" else PROFILE
        expected_executable = CONTROL_SHA if arm == "A" else CANDIDATE_SHA
        if (row.get("status") != "PASS" or row.get("error") is not None
                or row.get("operation") != operation or row.get("size_bytes") != 104_857_600
                or row.get("profile_id") != expected_profile
                or row.get("executable_sha256") != expected_executable
                or row.get("source_fingerprint") != FINGERPRINT
                or (row.get("expected_cdc_references"), row.get("actual_cdc_references"))
                    != (references, references)
                or row.get("ordered_closure_digest") != closure):
            reasons.append(f"v1-{label}-identity")
        if ((row.get("transactions"), row.get("commits"), row.get("commit_dispatches"),
                row.get("commit_returns"), row.get("commit_return_successes"),
                row.get("commit_return_errors")) != (1, 1, 1, 1, 1, 0)
                or row.get("publication_status") != "Committed"
                or row.get("sqlite_runtime_journal_mode") != "delete"
                or row.get("sqlite_runtime_synchronous") != 2
                or row.get("sqlite_runtime_temp_store") != 1
                or row.get("sqlite_runtime_mmap_size") != 0):
            reasons.append(f"v1-{label}-durability")
        q_ceiling = 131_072 if operation == "full" else 4_194_304
        if (row.get("q_current") != 0 or not isinstance(row.get("q_high_water"), int)
                or row.get("q_high_water", q_ceiling + 1) > q_ceiling
                or row.get("q_fixed_envelope_removed") is not True
                or row.get("physical_journal_apparent_bytes") != 0
                or row.get("physical_journal_allocated_bytes") != 0
                or row.get("base_preparation_in_measured_interval") is not False):
            reasons.append(f"v1-{label}-q-cleanup")
        if not timer_equations_match(row):
            reasons.append(f"v1-{label}-timer-equation")

    for label in ("primary-full-100-p0-B", "primary-full-100-p1-B"):
        row = by_label[label]
        commit = phase(row, "sqlite_commit")
        graph_zero = (
            "identity_bytes_hashed", "canonical_bytes_authenticated",
            "canonical_authenticated_nonnew_bytes", "canonical_authentication_hash_bytes",
            "canonical_authentication_hashes", "objects_authenticated",
            "statement_cache_acquisitions", "borrowed_row_blob_reads",
            "borrowed_row_blob_bytes",
        )
        if (any(commit.get(key) != 0 for key in graph_zero)
                or (commit.get("sql_query_calls"), commit.get("sql_execute_calls"),
                    commit.get("sql_rows_returned"), commit.get("row_blob_reads"),
                    commit.get("row_blob_writes"), commit.get("commits"))
                    != (1, 2, 0, 0, 4, 1)
                or (row.get("sql_query_calls"), row.get("row_blob_reads")) != (4, 4)):
            reasons.append(f"v1-{label}-publication-boundary")

    v1_one_byte = phase(by_label["guard-one-byte-middle-B"], "precommit_closure")
    compact_tuple = (
        v1_one_byte.get("incremental_qualification_calls"),
        v1_one_byte.get("sql_query_calls"), v1_one_byte.get("row_blob_reads"),
        v1_one_byte.get("borrowed_row_blob_reads"), v1_one_byte.get("objects_authenticated"),
    )
    same_phase = phase(by_label["guard-same-middle-B"], "precommit_closure")
    stale_tuple = (
        same_phase.get("incremental_qualification_calls"), same_phase.get("sql_query_calls"),
        same_phase.get("row_blob_reads"), same_phase.get("borrowed_row_blob_reads"),
        same_phase.get("objects_authenticated"),
    )
    if (compact_tuple != (1, 22, 25, 2, 21) or stale_tuple != (1, 25, 28, 5, 24)
            or compact_tuple == stale_tuple):
        reasons.append("v1-one-byte-reason-recomputation")
    residue = [str(path.relative_to(V1)) for path in V1.rglob("*")
               if path.name.endswith(("-journal", "-wal", "-shm"))]
    if residue:
        reasons.append("v1-residue")

    pairs = []
    for pair, order in ((0, "AB"), (1, "BA")):
        control = by_label[f"primary-full-100-p{pair}-A"]
        candidate = by_label[f"primary-full-100-p{pair}-B"]
        if (control.get("status") != "PASS" or candidate.get("status") != "PASS"
                or control.get("operation") != "full" or candidate.get("operation") != "full"
                or control.get("executable_sha256") != CONTROL_SHA
                or candidate.get("executable_sha256") != CANDIDATE_SHA):
            reasons.append(f"v1-pair-{pair}-identity")
            continue
        control_ns = control.get("durable_capture_total_wall_ns")
        candidate_ns = candidate.get("durable_capture_total_wall_ns")
        if not all(isinstance(value, int) and value > 0 for value in (control_ns, candidate_ns)):
            reasons.append(f"v1-pair-{pair}-timer")
            continue
        improvement = (control_ns - candidate_ns) * 100.0 / control_ns
        expected = 26.721 if pair == 0 else 27.622
        if candidate_ns >= control_ns or round(improvement, 3) != expected:
            reasons.append(f"v1-pair-{pair}-composition")
        pairs.append({
            "pair": pair, "order": order, "control_ns": control_ns,
            "candidate_ns": candidate_ns, "improvement_percent": improvement,
        })
    return pairs


def verify_chronology(root, reasons):
    schedule = rows(root / "SCHEDULE-v1.tsv")
    expected_schedule = [{
        "sequence": "1", "label": "fresh-one-byte-middle-B",
        "kind": "candidate-only", "size": "104857600",
        "operation": "one-byte-middle", "arm": "B", "warmup": "False",
        "timing_claim": "none",
    }]
    if schedule != expected_schedule:
        reasons.append("v2-schedule")
    starts = rows(root / "ROW-STARTS-v1.tsv")
    expected_projection = [
        ("1", "started", "fresh-one-byte-middle-B", "B", "one-byte-middle"),
        ("1", "completed", "fresh-one-byte-middle-B", "B", "one-byte-middle"),
    ]
    projection = [tuple(item.get(key) for key in
                        ("sequence", "event", "label", "arm", "operation"))
                  for item in starts]
    if projection != expected_projection:
        reasons.append("v2-row-chronology")
    try:
        if len(starts) != 2 or int(starts[1]["monotonic_ns"]) <= int(starts[0]["monotonic_ns"]):
            reasons.append("v2-row-chronology-time")
    except (KeyError, TypeError, ValueError):
        reasons.append("v2-row-chronology-time")

    invocations = rows(root / "ACTUAL-INVOCATIONS-v1.tsv")
    expected_invocations = [
        ("1", "started", "row-01-fresh-one-byte-middle-B", "-"),
        ("1", "completed", "row-01-fresh-one-byte-middle-B", "0"),
    ]
    projected = [tuple(item.get(key) for key in ("sequence", "event", "label", "exit"))
                 for item in invocations]
    if projected != expected_invocations:
        reasons.append("v2-row-invocation")
    expected_command = " ".join(map(str, [
        root / "operands-v1/phase4_create_edit_benchmark-canonical-v2-publication-repair",
        "--fast-row", root / "work-v1/rows/01-fresh-one-byte-middle-B",
        "104857600", "edit-one-byte-middle", "990001", "false", "capture-only",
    ]))
    if len(invocations) != 2 or any(item.get("command") != expected_command for item in invocations):
        reasons.append("v2-row-command")
    try:
        if len(invocations) != 2 or int(invocations[1]["time_ns"]) <= int(invocations[0]["time_ns"]):
            reasons.append("v2-row-invocation-time")
    except (KeyError, TypeError, ValueError):
        reasons.append("v2-row-invocation-time")


def verify_custody(root, row, reasons):
    custody = rows(root / "INPUT-CUSTODY-v1.tsv")
    if len(custody) != 1:
        reasons.append("v2-input-custody-count")
        return
    item = custody[0]
    exact(item, {
        "sequence": "1", "label": "fresh-one-byte-middle-B",
        "executable_sha256": CANDIDATE_SHA, "fixture_sha256": FIXTURE_SHA,
        "database_sha256": MASTER_SHA["database"],
        "authority_sha256": MASTER_SHA["authority"],
        "expectations_sha256": MASTER_SHA["expectations"],
    }, "v2-input-custody-anchor", reasons)
    candidate = root / "operands-v1/phase4_create_edit_benchmark-canonical-v2-publication-repair"
    fixture = root / "work-v1/rows/01-fresh-one-byte-middle-B/S1-100.source"
    row_root = root / "work-v1/rows/01-fresh-one-byte-middle-B"
    expected_target = row_root / "db-K64-F64-104857600-one-byte-middle-990001.sqlite"
    if (not candidate.is_file() or candidate.is_symlink() or sha(candidate) != CANDIDATE_SHA
            or (candidate.stat().st_dev, candidate.stat().st_ino)
                == (V1_CANDIDATE.stat().st_dev, V1_CANDIDATE.stat().st_ino)):
        reasons.append("v2-candidate-copy")
    if (not fixture.is_file() or fixture.is_symlink() or fixture.stat().st_size != 104_857_600
            or sha(fixture) != FIXTURE_SHA):
        reasons.append("v2-fixture-copy")
    try:
        source_stat, target_stat = V1_FIXTURE.stat(), fixture.stat()
        if (item.get("fixture_source_path") != str(V1_FIXTURE.relative_to(REPO))
                or item.get("fixture_target_path") != str(fixture.relative_to(REPO))
                or int(item["fixture_source_device"]) != source_stat.st_dev
                or int(item["fixture_source_inode"]) != source_stat.st_ino
                or int(item["fixture_source_size"]) != source_stat.st_size
                or int(item["fixture_target_device"]) != target_stat.st_dev
                or int(item["fixture_target_inode"]) != target_stat.st_ino
                or int(item["fixture_target_size"]) != target_stat.st_size
                or (source_stat.st_dev, source_stat.st_ino) == (target_stat.st_dev, target_stat.st_ino)):
            reasons.append("v2-fixture-copy-distinctness")
    except (KeyError, TypeError, ValueError, OSError):
        reasons.append("v2-fixture-copy-distinctness")
    for kind in ("database", "authority", "expectations"):
        master = REPO / item.get(f"master_{kind}_path", "")
        target = REPO / item.get(f"target_{kind}_path", "")
        expected_master = V1_MASTER if kind == "database" else Path(str(V1_MASTER) + f".{kind}")
        expected_path = expected_target if kind == "database" else Path(str(expected_target) + f".{kind}")
        try:
            master_stat, target_stat = master.stat(), target.stat()
            if (master.resolve() != expected_master.resolve() or target.resolve() != expected_path.resolve()
                    or target.is_symlink()
                    or int(item[f"master_{kind}_device"]) != master.stat().st_dev
                    or int(item[f"master_{kind}_inode"]) != master.stat().st_ino
                    or int(item[f"master_{kind}_size"]) != master.stat().st_size
                    or int(item[f"target_{kind}_device"]) != target.stat().st_dev
                    or int(item[f"target_{kind}_inode"]) != target.stat().st_ino
                    or int(item[f"target_{kind}_size_before"]) != master.stat().st_size
                    or (master_stat.st_dev, master_stat.st_ino) == (target_stat.st_dev, target_stat.st_ino)):
                reasons.append(f"v2-{kind}-copy-distinctness")
        except (KeyError, TypeError, ValueError, OSError):
            reasons.append(f"v2-{kind}-copy-distinctness")
        if kind != "database" and (not target.is_file() or sha(target) != MASTER_SHA[kind]):
            reasons.append(f"v2-{kind}-post-row-custody")
    if (not expected_target.is_file()
            or expected_target.stat().st_size != row.get("sqlite_post_logical_database_bytes")):
        reasons.append("v2-database-post-row-custody")
    if (row.get("pre_edit_database_sha256"), row.get("pre_edit_authority_sha256"),
            row.get("pre_edit_expectations_sha256")) != (
                MASTER_SHA["database"], MASTER_SHA["authority"], MASTER_SHA["expectations"]):
        reasons.append("v2-row-pre-edit-custody")


def verify_fresh_row(root, row, reasons):
    exact(row, {
        "status": "PASS", "error": None, "operation": "one-byte-middle",
        "size_bytes": 104_857_600, "input_size_bytes": 104_857_600,
        "iteration": 990001, "throughput_measurement_admissible": False,
        "executable_sha256": CANDIDATE_SHA, "profile_id": PROFILE,
        "source_fingerprint": FINGERPRINT,
        "expected_cdc_sequence_fingerprint": SEQUENCE,
        "ordered_closure_digest": CLOSURE, "root_id": ROOT_ID,
        "transition_id": TRANSITION_ID, "expected_cdc_references": 5_284,
        "actual_cdc_references": 5_284, "edit_reference_count_before": 5_284,
        "edit_reference_count_after": 5_284, "edit_count_classification": "same-count",
        "edit_offset": 52_480_416, "edit_removed_hex": "f1", "edit_inserted_hex": "ab",
        "qualification_mode": "C1-changed-spine", "warmup": False,
        "base_preparation_in_measured_interval": False,
        "base_copy_method": "physical-byte-copy-identical-database-authority-expectations",
    }, "v2-row-identity", reasons)

    exact(row, {
        "transactions": 1, "commits": 1, "commit_dispatches": 1,
        "commit_returns": 1, "commit_return_successes": 1,
        "commit_return_errors": 0, "commit_return_status": "ok",
        "publication_status": "Committed", "sqlite_runtime_journal_mode": "delete",
        "sqlite_runtime_synchronous": 2, "sqlite_runtime_temp_store": 1,
        "sqlite_runtime_mmap_size": 0, "physical_journal_apparent_bytes": 0,
        "physical_journal_allocated_bytes": 0, "commit_return_journal_apparent_bytes": 0,
        "commit_return_journal_allocated_bytes": 0,
    }, "v2-row-durability", reasons)

    q_parts = (row.get("q_cdc_scan_input_bytes"), row.get("q_cdc_old_window_bytes"),
               row.get("q_cdc_base_live_bytes"), row.get("q_cdc_old_chunk_slots_bytes"))
    if (q_parts != (1_066_637, 1_066_637, 1_257, 12_672)
            or sum(q_parts) != 2_147_203 or row.get("q_high_water") != 2_147_203
            or row.get("q_cdc_overlap_current") != 2_147_203
            or row.get("q_current") != 0 or row.get("q_equation") != "Q1"
            or row.get("q_fixed_envelope_removed") is not True):
        reasons.append("v2-row-q")

    precommit = phase(row, "precommit_closure")
    exact(precommit, {
        "incremental_qualification_calls": 1, "sql_query_calls": 22,
        "sql_rows_returned": 22, "row_blob_reads": 25,
        "borrowed_row_blob_reads": 2, "borrowed_row_blob_bytes": 36_940,
        "objects_authenticated": 21, "statement_cache_acquisitions": 21,
        "canonical_authentication_hashes": 21,
        "canonical_bytes_authenticated": 48_164,
        "canonical_authenticated_nonnew_bytes": 48_164,
        "canonical_authentication_hash_bytes": 48_164,
        "identity_bytes_hashed": 48_164,
        "incremental_prior_spine_objects_authenticated": 4,
        "incremental_prior_spine_bytes_authenticated": 5_104,
        "incremental_replacement_spine_objects_authenticated": 4,
        "incremental_replacement_spine_bytes_authenticated": 5_104,
        "incremental_new_subtree_objects_authenticated": 2,
        "incremental_new_subtree_bytes_authenticated": 36_940,
        "incremental_receipt_covered_edges": 126,
        "incremental_new_or_different_edges": 5,
    }, "v2-precommit-tuple", reasons)
    if (precommit.get("incremental_receipt_covered_edges", -1)
            + precommit.get("incremental_new_or_different_edges", -1) != 131):
        reasons.append("v2-precommit-edge-equation")

    commit = phase(row, "sqlite_commit")
    zero = {
        key: 0 for key in (
            "identity_bytes_hashed", "canonical_bytes_authenticated",
            "canonical_authenticated_nonnew_bytes", "canonical_authentication_hash_bytes",
            "canonical_authentication_hashes", "objects_authenticated",
            "statement_cache_acquisitions", "borrowed_row_blob_reads",
            "borrowed_row_blob_bytes", "incremental_qualification_calls",
            "incremental_prior_spine_objects_authenticated",
            "incremental_prior_spine_bytes_authenticated",
            "incremental_replacement_spine_objects_authenticated",
            "incremental_replacement_spine_bytes_authenticated",
            "incremental_new_subtree_objects_authenticated",
            "incremental_new_subtree_bytes_authenticated",
            "incremental_receipt_covered_edges", "incremental_new_or_different_edges",
        )
    }
    exact(commit, zero, "v2-publication-nonzero", reasons)
    exact(commit, {
        "sql_query_calls": 1, "sql_execute_calls": 2, "sql_rows_returned": 1,
        "row_blob_reads": 4, "row_blob_writes": 4, "commits": 1,
    }, "v2-publication-sql-tuple", reasons)

    if not timer_equations_match(row):
        reasons.append("v2-timer-equation")

    residue = [str(path.relative_to(root)) for path in root.rglob("*")
               if path.name.endswith(("-journal", "-wal", "-shm"))]
    if residue:
        reasons.append("v2-residue:" + ",".join(residue))


def analyze(root):
    reasons = []
    historical_pairs = verify_v1(reasons)
    verify_chronology(root, reasons)
    raw = [json.loads(line) for line in (root / "RAW-v1.jsonl").read_text().splitlines() if line]
    if len(raw) != 1:
        reasons.append("v2-row-count")
        row = {}
    else:
        row = raw[0]
    verify_custody(root, row, reasons)
    try:
        verify_fresh_row(root, row, reasons)
    except Exception as error:
        reasons.append(f"v2-row-analysis:{type(error).__name__}:{error}")
    status = "PASS" if not reasons else "REVISE"
    return {
        "status": status,
        "disposition": ("CANONICAL-V2 PUBLICATION-REPAIR-v2 PASS / SCREEN CLOSED"
                        if status == "PASS" else "CANONICAL-V2 PUBLICATION-REPAIR-v2 REVISE"),
        "reasons": reasons,
        "fresh_row_count": len(raw),
        "fresh_row": "fresh-one-byte-middle-B",
        "fresh_row_timing_claim": "none",
        "sealed_v1": {
            "disposition": "CANONICAL-V2 PUBLICATION-REPAIR REVISE",
            "reasons": ["guard-one-byte-middle-B:changed-spine-work"],
            "manifest_sha256": V1_MANIFEST_SHA,
            "manifest_entries": 126,
            "root_files": 128,
            "manifest_mismatches": 0,
            "full_create_pairs": historical_pairs,
            "composed_context_only": True,
            "relabelled": False,
        },
        "screen_closed": status == "PASS",
        "eligible_for_complete_canonical_v2_validation": status == "PASS",
        "promotion_authorized": False,
        "limitations": [
            "The fresh row is a deterministic semantic closure; it makes no timing claim.",
            "The two full-create improvements are composed unchanged from sealed v1 raw rows.",
            "PASS closes only this compact screen and does not promote or integrate canonical-v2.",
        ],
    }


def write_outputs(root, result):
    (root / "ANALYSIS-v1.json").write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
    report = [
        "# Canonical-v2 publication repair deterministic closure v2\n",
        f"Disposition: **{result['disposition']}**\n",
        "Sealed v1 remains historical **REVISE** and is not relabeled.\n",
        "\nHistorical sealed-v1 full-create comparisons recomputed from raw rows:\n",
    ]
    for pair in result["sealed_v1"]["full_create_pairs"]:
        report.append(
            f"- pair {pair['pair']} {pair['order']}: control {pair['control_ns']/1e6:.3f} ms; "
            f"candidate {pair['candidate_ns']/1e6:.3f} ms; improvement "
            f"{pair['improvement_percent']:.3f}%.\n")
    report.extend([
        "\nFresh evidence: exactly one candidate-only `one-byte-middle` row; direct counters, "
        "identities, Q, timers, transaction/durability, custody, and residue are semantic gates. "
        "Its elapsed values make no performance claim.\n",
        "\nHard-gate reasons: " + (", ".join(result["reasons"]) if result["reasons"] else "none") + ".\n",
        "\nLimitations:\n" + "".join(f"- {item}\n" for item in result["limitations"]),
    ])
    (root / "REPORT-v1.md").write_text("\n".join(report))
    disposition = result["disposition"] + "\n"
    disposition += "Sealed v1 remains historical REVISE; CP-0009 remains accepted.\n"
    disposition += "Reasons: " + (", ".join(result["reasons"]) if result["reasons"] else "none") + "\n"
    disposition += ("Eligible only for complete canonical-v2 validation; no promotion, integration, commit, or later optimization is authorized.\n"
                    if result["status"] == "PASS" else
                    "Not eligible; no promotion, integration, commit, or later optimization is authorized.\n")
    (root / "DISPOSITION-v1.txt").write_text(disposition)


def main():
    if len(sys.argv) != 2:
        raise SystemExit("usage: analyze-publication-repair-v2.py RESULT_ROOT")
    root = Path(sys.argv[1]).resolve()
    try:
        result = analyze(root)
    except Exception as error:
        result = {
            "status": "REVISE",
            "disposition": "CANONICAL-V2 PUBLICATION-REPAIR-v2 REVISE",
            "reasons": [f"analyzer:{type(error).__name__}:{error}"],
            "fresh_row_count": 0,
            "fresh_row": "fresh-one-byte-middle-B",
            "fresh_row_timing_claim": "none",
            "sealed_v1": {"full_create_pairs": []},
            "screen_closed": False,
            "eligible_for_complete_canonical_v2_validation": False,
            "promotion_authorized": False,
            "limitations": [],
        }
    write_outputs(root, result)
    print(json.dumps(result, sort_keys=True))
    return 0 if result["status"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
