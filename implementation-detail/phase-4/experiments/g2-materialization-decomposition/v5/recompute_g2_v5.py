#!/usr/bin/env python3
"""Independent recomputation for the prospective G2-v5 protocol closure."""

import argparse
import hashlib
import json
import statistics
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[4]
SEALED = REPO / "target/phase4-g2-materialization-decomposition-20260822-v1/results-v1"
V3 = REPO / "target/phase4-g2-materialization-decomposition-20260822-v3/results-v3"
V4_METHOD = REPO / "implementation-detail/phase-4/experiments/g2-materialization-decomposition/v4"
RAW_V1 = SEALED / "rows-v1/G2-RAW-v1.jsonl"
PRIMARY_V1 = SEALED / "G2-PRIMARY-ANALYSIS-v1.json"
OBSERVER_V1 = SEALED / "OBSERVER-PROBES-v1.json"
TERMINAL_V1 = SEALED / "TERMINAL-v1.json"
PAYLOAD_V1 = SEALED / "PAYLOAD-MANIFEST-v1.tsv"
FINAL_V1 = SEALED / "G2-ANALYSIS-v1.json"
INDEPENDENT_V1 = SEALED / "INDEPENDENT-RECOMPUTATION-v1.json"
STATUS_V1 = SEALED / "STATUS-v1.json"
TERMINAL_VERIFICATION_V1 = SEALED / "TERMINAL-VERIFICATION-v1.txt"
EXPECTED_HASHES = {
    RAW_V1: "6f7124cc8d4fdd248b89770da5576f2546f105304e3d486ddb2f9c7ce5352af2",
    PRIMARY_V1: "0840dcf353eff15a53eaa07f748678bfcab5b02b732ec9c592c12d0f38127282",
    OBSERVER_V1: "bfe2e85b7a1fd61d84699cab4f1f3727731e955965a1370e0cfad8d8a406e717",
    TERMINAL_V1: "b859de6dce9aef9caba43dbf43fd5eb2b7ea24630f7f18ff206749d431e6f2a1",
    PAYLOAD_V1: "28c1b86a3fd3715785617da84195e5ed2cbd5a880dcc883f57f8e51d5edd2d13",
    FINAL_V1: "e926187cd9da28647b3b4695616efe237192721105270844750ac49e5a35bb21",
    INDEPENDENT_V1: "803c9658a3a3ab4238a15a03fe8b7ec8dcef7a313bfee5af4530533fdd5ee5d7",
    STATUS_V1: "1f4112e0bd48a44000f0096b2c7db5a1d9ac3892672ce2ff7356035ba513e97c",
    TERMINAL_VERIFICATION_V1: "d004339854fded0c39af5a7b05a6fea78e398a703846e5eec43ad180f971b1be",
}
V3_HASHES = {
    V3 / "PAYLOAD-MANIFEST-v3.tsv": "59e0bbb6d44da9ba02f8c9536a1b55fedfc48ed342a6068087bbd6aaf509a4c3",
    V3 / "TERMINAL-v3.json": "8befdf04037868e0bd2934dccb9e7d3be69b4dad38ba1059d41ea4a375e25f2a",
    V3 / "TERMINAL-VERIFICATION-v3.txt": "85554b79ae15b5f72ccc2d11a84222e7d5aa34a2ce41d2088cc30034535809b3",
    V3 / "STATUS-v3.json": "b8e1ddb9b3eaacea7c4f040f802a4b6bb5224d9535856a941dfc29a5226ce882",
    V3 / "rows-v3/G2-V3-RAW.jsonl": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
    V3 / "preparation-v3/prepare-01-measured-same-middle-pos1-B.stderr": "f665ab00c6a188b15810f6c01f152a5941021e698ed13e11cad7c62416d56679",
}
V4_HASHES = {
    V4_METHOD / "PROSPECTIVE-G2-MATERIALIZATION-DECOMPOSITION-v4.md": "31cd21ebc56d0d8ee18c1b1f9ca8813a87dd51e7e0cf59c737a3c91acfd2a3f3",
    V4_METHOD / "run_g2_v4.py": "eb307f775d4aa6d0e1751e79b4838491956afee6a2d88a7fafc6b2dc01698450",
    V4_METHOD / "analyze_g2_v4.py": "7b427aad341c250c4ef649fa1bcbe13889625758525205d9c3a08ff61311a547",
    V4_METHOD / "recompute_g2_v4.py": "02a0da07e81a09a7de929eda531e005924266cf4905b9a9ece140f9cfd435b29",
    V4_METHOD / "METHODOLOGY-MANIFEST-v4.tsv": "2101fa07cf66c17ab09261f02c51a8e187838d08f1deefe4ba9913f6de7238b1",
    V4_METHOD / "DRY-RUN-v4.json": "6f12dfb18f1c68d3b22b79af5eb409b7ba188b103db9cd7861a4a8de513b4635",
}
PASS_DISPOSITION = "G2 PASS / INSUFFICIENT_EVIDENCE FOR A CONSTANT-FACTOR CANDIDATE"
SOURCE = "bb883eecf4ea85d80432953791dcc352243da94175e7503e2c476afe9bd0bab7"
BASE_IDENTITIES = {
    "root_id": "93d1b461b5cbf88e8d122ad4e90a15a6e3029c408f0fe02ef51c561e5a94c6d1",
    "transition_id": "2de8d2ce6b614373ba8fb8b29d3a3eccd535abfccc2654e7922514e5ca90fd89",
    "ordered_closure_digest": "29233d6018b031f6035c8e2c8175f1ab86fb721808f21a8858bc965567e9c0c1",
    "expected_cdc_sequence_fingerprint": "5bf63ef8adad7bb373be4e968759997b385590d076d75a1488e1958b64a3f8e2",
    "actual_cdc_references": 5284,
}
EDIT_EXPECTED = {
    "status": "PASS",
    "source_fingerprint": SOURCE,
    "root_id": "8df9bc09f9ba99351f11f3cb01b039713090120873b6dea8903e7d835a2a9faf",
    "transition_id": "b185f7670f748b5713d4d8538c513bce4b3019e17991840c369575f404fbf2ed",
    "ordered_closure_digest": "d7614133f35f1a254d0d2222815cdbcbdcd69915baf30c3a801831e6497b1683",
    "expected_cdc_sequence_fingerprint": "2f7cd2e85591ad9dbca8005402c4b209624bb5a058c7d7358620b5d2f2575bec",
    "actual_cdc_references": 5284,
    "objects_created": 11,
    "objects_reused": 0,
    "objects_authenticated": 5483,
    "canonical_bytes_authenticated": 106457915,
    "canonical_bytes_written": 108697,
    "mapping_bytes_rewritten": 5334,
    "transactions": 1,
    "commits": 1,
    "q_current": 0,
    "pre_edit_database_sha256": "7db8d50de42b994546789cb67fc7a9b650e2e551dab118e15003e02106b19890",
    "pre_edit_authority_sha256": "7855ea6096359925f639b91c8d6b9708cfe0bc0df4a3ffd97a280a8e9a9ded48",
    "pre_edit_expectations_sha256": "b3afda400d8cfa55a6145879aff0075e97884edd71c0b4d23d47b5d8c5bffc14",
    "post_database_sha256": "b69861ee81c4a01906cf2fb70fe4ef49c4de534cab9ab9b000006efe6802fe31",
    "post_authority_sha256": "7855ea6096359925f639b91c8d6b9708cfe0bc0df4a3ffd97a280a8e9a9ded48",
    "post_expectations_sha256": "b3afda400d8cfa55a6145879aff0075e97884edd71c0b4d23d47b5d8c5bffc14",
    "sqlite_pre_logical_database_bytes": 109199360,
    "sqlite_pre_logical_store_bytes": 109199392,
    "sqlite_pre_apparent_database_bytes": 109199360,
    "sqlite_pre_apparent_store_bytes": 109199392,
    "sqlite_pre_allocated_database_bytes": 109199360,
    "sqlite_pre_allocated_store_bytes": 109203456,
    "sqlite_post_logical_database_bytes": 109314048,
    "sqlite_post_logical_store_bytes": 109314080,
    "sqlite_post_apparent_database_bytes": 109314048,
    "sqlite_post_apparent_store_bytes": 109314080,
    "sqlite_post_allocated_database_bytes": 125976576,
    "sqlite_post_allocated_store_bytes": 125980672,
    "allocated_store_delta_bytes": 16777216,
    "post_modes": {"authority": "0600", "database": "0600", "expectations": "0400"},
    "edit_offset": 52480416,
    "changed_work_bytes": 18854,
    "edit_reference_count_before": 5284,
    "edit_reference_count_after": 5284,
    "edit_count_classification": "same-count",
    "sqlite_main_db_dirty_pages_written": 45,
    "sqlite_main_db_pager_write_bytes": 184320,
    "sqlite_cache_spill_pages": 0,
    "commit_dispatches": 1,
    "commit_returns": 1,
    "commit_return_successes": 1,
    "commit_return_errors": 0,
    "commit_return_status": "ok",
    "publication_status": "Committed",
    "q_high_water": 2222803,
    "sqlite_page_size_bytes": 4096,
    "sqlite_runtime_journal_mode": "delete",
    "sqlite_runtime_synchronous": 2,
    "sqlite_runtime_temp_store": 1,
    "sqlite_runtime_mmap_size": 0,
    "commit_timer_equation_matches": True,
    "durable_phase_sum_matches": True,
    "commit_reconciliation_calls": 0,
    "commit_reconciliation_wall_ns": 0,
    "physical_journal_apparent_bytes": 0,
    "physical_journal_allocated_bytes": 0,
    "q_equation": "Q1",
}
TIMERS = (
    "sqlite_blob_acquisition_wall_ns", "canonical_authentication_wall_ns",
    "mapping_validation_wall_ns", "closure_commitment_wall_ns",
    "occurrence_commitment_wall_ns", "source_fingerprint_wall_ns",
    "secondary_bytes_decode_wall_ns",
)


def file_hash(file_path):
    digest = hashlib.sha256()
    with file_path.open("rb") as source:
        while block := source.read(1024 * 1024):
            digest.update(block)
    return digest.hexdigest()


def mode(file_path):
    return f"{file_path.stat().st_mode & 0o7777:04o}"


def mismatch(row, expected, label):
    return [f"{label}:{field}" for field, value in expected.items() if row.get(field) != value]


def validate_payload():
    problems = []
    entries = PAYLOAD_V1.read_text().splitlines()
    if len(entries) != 179 or entries[0].split("\t") != ["path", "sha256", "size_bytes"]:
        return ["payload:shape"]
    sealed_root = SEALED.resolve()
    for ordinal, record in enumerate(entries[1:], 1):
        relative, expected_digest, size = record.split("\t")
        candidate = (SEALED / relative).resolve()
        try:
            candidate.relative_to(sealed_root)
        except ValueError:
            problems.append(f"payload:{ordinal}:path")
            continue
        if not candidate.is_file() or candidate.stat().st_size != int(size) or file_hash(candidate) != expected_digest:
            problems.append(f"payload:{ordinal}:custody")
    expected_nodes = {record.split("\t")[0] for record in entries[1:]} | {"PAYLOAD-MANIFEST-v1.tsv", "TERMINAL-v1.json", "TERMINAL-VERIFICATION-v1.txt"}
    actual_nodes = {str(item.relative_to(SEALED)) for item in SEALED.rglob("*") if not item.is_dir()}
    if actual_nodes != expected_nodes:
        problems.append("payload:complete-file-closure")
    if any(item.stat().st_mode & 0o222 for item in (SEALED.parent, *SEALED.parent.rglob("*")) if not item.is_symlink()):
        problems.append("payload:writable-subtree")
    if (REPO / "target/phase4-g2-materialization-decomposition-20260822-v1.lock").exists():
        problems.append("payload:v1-lock")
    return problems


def validate_v3_history():
    problems = []
    for path, expected in V3_HASHES.items():
        if not path.is_file() or file_hash(path) != expected:
            problems.append(f"v3:{path.name}:custody")
    if problems:
        return problems
    status = json.loads((V3 / "STATUS-v3.json").read_text())
    terminal = json.loads((V3 / "TERMINAL-v3.json").read_text())
    if status.get("status") != "REVISE" or status.get("fresh_rows") != 0 or status.get("reason") != "RuntimeError: child failed: prepare-01-measured-same-middle-pos1-B":
        problems.append("v3:status")
    if terminal.get("status") != "REVISE" or terminal.get("fresh_raw_sha256") != V3_HASHES[V3 / "rows-v3/G2-V3-RAW.jsonl"] or (V3 / "preparation-v3/prepare-01-measured-same-middle-pos1-B.stderr").read_bytes() != b"Error: ValidationAuthorityUnavailable\n":
        problems.append("v3:failure")
    if (REPO / "target/phase4-g2-materialization-decomposition-20260822-v3.lock").exists() or any(item.stat().st_mode & 0o222 for item in (V3.parent, *V3.parent.rglob("*")) if not item.is_symlink()):
        problems.append("v3:seal-lock")
    return problems


def validate_v4_history():
    problems = [f"v4:{path.name}:custody" for path, expected in V4_HASHES.items() if not path.is_file() or file_hash(path) != expected]
    if problems:
        return problems
    dry = json.loads((V4_METHOD / "DRY-RUN-v4.json").read_text())
    if dry.get("actual_rows") != 0 or dry.get("benchmark_children_invoked") != 0 or dry.get("base_proxy_plan", {}).get("database_symlink_target") != str(SEALED / "input-v1/base.sqlite") or (REPO / "target/phase4-g2-materialization-decomposition-20260822-v4").exists() or (REPO / "target/phase4-g2-materialization-decomposition-20260822-v4.lock").exists():
        problems.append("v4:rejected-preexec-shape")
    return problems


def check_read_row(row):
    label = f"sealed-read:{row.get('label', '?')}"
    failures = mismatch(row, {"status": "PASS", "source_fingerprint": SOURCE, "q_current": 0, "transactions": 0, "commits": 0, **BASE_IDENTITIES}, label)
    if row.get("residue_files"):
        failures.append(f"{label}:residue")
    failures.extend(mismatch(row, {"sqlite_page_size_bytes": "Unavailable", "sqlite_runtime_journal_mode": "delete", "sqlite_runtime_synchronous": 2, "sqlite_runtime_temp_store": 1, "sqlite_runtime_mmap_size": 0, "post_modes": {"authority": "0600", "database": "0600", "expectations": "0400"}}, label))
    if row.get("allocated_store_delta_bytes") != 0:
        failures.append(f"{label}:allocated-delta")
    for measure in ("logical", "apparent", "allocated"):
        for scope in ("database", "store"):
            if row.get(f"sqlite_pre_{measure}_{scope}_bytes") != row.get(f"sqlite_post_{measure}_{scope}_bytes"):
                failures.append(f"{label}:{measure}-{scope}")
    for item in ("database", "authority", "expectations"):
        if row.get(f"pre_edit_{item}_sha256") != row.get(f"post_{item}_sha256"):
            failures.append(f"{label}:{item}-hash")
    return failures


def check_edit_row(row):
    label = f"fresh-edit:{row.get('label', '?')}"
    failures = mismatch(row, EDIT_EXPECTED, label)
    if row.get("operation") != "same-middle" or row.get("warmup") is not False:
        failures.append(f"{label}:shape")
    if row.get("residue_files"):
        failures.append(f"{label}:residue")
    if row.get("maximum_resident_set_bytes", 20 * 1024 * 1024 + 1) > 20 * 1024 * 1024:
        failures.append(f"{label}:rss")
    try:
        removed = bytes.fromhex(row.get("edit_removed_hex", ""))
        inserted = bytes.fromhex(row.get("edit_inserted_hex", ""))
    except ValueError:
        failures.append(f"{label}:hex")
    else:
        if len(removed) != 18854 or hashlib.sha256(removed).hexdigest() != "fdc04dd5bea39e9480dd5559068fd72e0e9c2c3ce5b92fa8c62798ee3425a8fa":
            failures.append(f"{label}:removed")
        if len(inserted) != 18854 or hashlib.sha256(inserted).hexdigest() != "8a4df9c28dcf4e0625ba08fa5f92c4ea2e462274c081072823900ab0e75611d6":
            failures.append(f"{label}:inserted")
    for measure, delta in (("logical", 114688), ("apparent", 114688), ("allocated", 16777216)):
        for scope in ("database", "store"):
            before = row.get(f"sqlite_pre_{measure}_{scope}_bytes")
            after = row.get(f"sqlite_post_{measure}_{scope}_bytes")
            if before is None or after is None or after - before != delta:
                failures.append(f"{label}:{measure}-{scope}-delta")
    return failures


def expected_plan(results):
    operands = results / "operands-v5"
    work = results / "rows-v5/work-v5"
    candidate = operands / "phase4_create_edit_benchmark-instrumented"
    control = operands / "phase4_create_edit_benchmark-control"
    rows = (("01-measured-same-middle-pos1-B", 983001, candidate), ("02-measured-same-middle-pos2-A", 983002, control))
    plan = [{"kind": "prepare", "label": f"prepare-{label}", "command": [str(candidate), "--fast-prepare", str(work / label), "104857600", "edit-same", str(iteration)]} for label, iteration, _ in rows]
    plan.extend({"kind": "row", "label": label, "command": ["/usr/bin/time", "-l", str(binary), "--fast-row", str(work / label), "104857600", "edit-same", str(iteration), "false", "capture-only"]} for label, iteration, binary in rows)
    plan.extend((
        {"kind": "analyzer", "label": "primary-analysis", "command": [sys.executable, str(HERE / "analyze_g2_v5.py"), str(results)]},
        {"kind": "analyzer", "label": "independent-recomputation", "command": [sys.executable, str(HERE / "recompute_g2_v5.py"), str(results)]},
    ))
    return plan


def artifact_failures(results):
    failures = []
    custody_path = results / "OPERAND-CUSTODY-v5.json"
    cleanup_path = results / "TRANSIENT-VERIFICATION-v5.json"
    plan_path = results / "CHRONOLOGY-PLAN-v5.json"
    chronology_path = results / "CHRONOLOGY-v5.jsonl"
    proxy_path = results / "BASE-PROXY-CUSTODY-v5.json"
    if not all(path.is_file() for path in (custody_path, cleanup_path, plan_path, chronology_path, proxy_path)):
        return ["artifact-set:missing"], [], {}, [], {}
    custody = json.loads(custody_path.read_text())
    expected_operands = {"control": ("42e3ddeb15df298c978b14639690e366fbb26ee55851524d42c6c3e9c0e8bd55", 1372784), "candidate": ("5d72b46d29a5b77494781f343cc6841a71879b5de426751afe744f27a033e8f5", 1390512)}
    if len(custody) != 2 or {item.get("name") for item in custody} != set(expected_operands):
        failures.append("operand:shape")
    for item in custody:
        digest, size = expected_operands.get(item.get("name"), (None, None))
        copied, source = Path(item.get("copy_path", "")), Path(item.get("source_path", ""))
        expected_source = V3 / f"operands-v3/phase4_create_edit_benchmark-{'control' if item.get('name') == 'control' else 'instrumented'}"
        if not copied.is_file() or source != expected_source or not source.is_file() or file_hash(copied) != digest or copied.stat().st_size != size or mode(copied) != "0500" or mode(source) != "0444" or item.get("sha256") != digest or item.get("size_bytes") != size or item.get("source_mode") != "0444" or item.get("copy_mode") != "0500" or item.get("execution_path") != "snapshot-only":
            failures.append(f"operand:{item.get('name')}:bytes-mode")
            continue
        actual_identity = (source.stat().st_dev, source.stat().st_ino, copied.stat().st_dev, copied.stat().st_ino)
        recorded_identity = (item.get("source_device"), item.get("source_inode"), item.get("copy_device"), item.get("copy_inode"))
        if actual_identity != recorded_identity or actual_identity[:2] == actual_identity[2:]:
            failures.append(f"operand:{item.get('name')}:inode")
    cleanup = json.loads(cleanup_path.read_text())
    proxy = json.loads(proxy_path.read_text())
    if not (proxy.get("status") == "PASS" and proxy.get("database_is_regular") is True and proxy.get("database_copy_kind") == "private-regular-byte-identical" and proxy.get("database_source_path") == str(SEALED / "input-v1/base.sqlite") and proxy.get("database_private_sha256") == "7db8d50de42b994546789cb67fc7a9b650e2e551dab118e15003e02106b19890" and proxy.get("database_private_mode_actual") == "0600" and proxy.get("database_distinct_device_inode") is True and proxy.get("authority_private_sha256") == "7855ea6096359925f639b91c8d6b9708cfe0bc0df4a3ffd97a280a8e9a9ded48" and proxy.get("authority_private_mode_actual") == "0600" and proxy.get("authority_distinct_device_inode") is True and proxy.get("expectations_copy") is False and proxy.get("transient_ceiling_bytes") == 512 * 1024 * 1024 and proxy.get("work_path_absent_after_cleanup") is True):
        failures.append("base-proxy:contract")
    if not (cleanup.get("status") == "PASS" and cleanup.get("base_proxy_ready") is True and cleanup.get("declared_deletions") == ["rows-v5/work-v5"] and cleanup.get("deleted") == ["rows-v5/work-v5"] and cleanup.get("work_path_existed") is True and cleanup.get("work_path_absent") is True and cleanup.get("rows_validated") is True and cleanup.get("usage", {}).get("ceiling_bytes") == 512 * 1024 * 1024 and cleanup.get("usage", {}).get("within_ceiling") is True and len(cleanup.get("usage", {}).get("samples", [])) == 5 and all(sample.get("prepare_oracle_reserve_bytes") == 109314048 for sample in cleanup.get("usage", {}).get("samples", [])) and not (results / "rows-v5/work-v5").exists()):
        failures.append("cleanup:contract")
    plan = json.loads(plan_path.read_text())
    expected = expected_plan(results)
    if plan != expected:
        failures.append("chronology:plan")
    records = [json.loads(line) for line in chronology_path.read_text().splitlines() if line]
    observed = [{key: row.get(key) for key in ("event", "kind", "label", "command", "exit_code") if key in row} for row in records if row.get("event") in ("child-start", "child-complete")]
    events = []
    for child in expected:
        events.extend(({"event": "child-start", **child}, {"event": "child-complete", **child, "exit_code": 0}))
    if observed != events[:11]:
        failures.append("chronology:prefix")
    return failures, custody, cleanup, expected, proxy


def selected(row, keys):
    return {key: row.get(key) for key in keys}


def normalized_ledger(failures, sealed_rows, primary, observer, fresh_rows, custody, cleanup, plan, proxy):
    measured = [row for row in sealed_rows if row.get("workload") == "primary" and row.get("kind") == "measured"]
    pairs = []
    for pair_id in range(1, 5):
        pair = {row["arm"]: row for row in measured if row.get("pair") == pair_id}
        pairs.append({"pair": pair_id, "order": pair["A"]["order"], "control_ns": pair["A"]["reconstruction_wall_ns"], "instrumented_ns": pair["B"]["reconstruction_wall_ns"], "ratio": pair["B"]["reconstruction_wall_ns"] / pair["A"]["reconstruction_wall_ns"]})
    positions = {}
    for position in (1, 2):
        arms = {arm: statistics.median(row["reconstruction_wall_ns"] for row in measured if row.get("position") == position and row.get("arm") == arm) for arm in "AB"}
        positions[str(position)] = {**arms, "ratio": arms["B"] / arms["A"]}
    centers = {arm: statistics.mean(row["reconstruction_wall_ns"] for row in measured if row.get("arm") == arm) for arm in "AB"}
    read_keys = ("objects_authenticated", "canonical_bytes_authenticated", "sql_query_calls", "sql_rows_returned", "borrowed_row_blob_reads", "borrowed_row_blob_bytes", "leaf_batch_queries", "leaf_batch_references")
    read_work = []
    for row in measured:
        if row.get("arm") != "B":
            continue
        phase = next(item for item in row["phase_counters"] if item["phase"] == "read_operation")
        read_work.append({"label": row["label"], **selected(phase, read_keys), "operation_q_high_water": row["g2_decomposition"]["operation_q_high_water"], "maximum_resident_set_bytes": row["maximum_resident_set_bytes"], "timer_regions": row["g2_decomposition"]["timer_regions"], "direct_timer_sum_wall_ns": row["g2_decomposition"]["direct_timer_sum_wall_ns"], "raw_residual_wall_ns": row["g2_decomposition"]["raw_residual_wall_ns"]})
    guard_keys = ("label", "arm", "operation", "objects_authenticated", "canonical_bytes_authenticated", "sql_query_calls", "sql_rows_returned", "borrowed_row_blob_reads", "borrowed_row_blob_bytes", "leaf_batch_queries", "leaf_batch_references", "q_high_water", "q_current", "allocated_store_delta_bytes")
    guards = [selected(row, guard_keys) | {"range_measurements": row.get("range_measurements", [])} for row in sealed_rows if row.get("workload") == "guard" and row.get("operation") != "same-middle"]
    fresh_keys = tuple(EDIT_EXPECTED) + ("label", "arm", "sequence", "position", "order", "iteration", "binary_sha256", "base_copy_method", "maximum_resident_set_bytes", "durable_capture_total_wall_ns")
    fresh = []
    for row in sorted(fresh_rows, key=lambda item: item.get("sequence", 0)):
        item = selected(row, fresh_keys)
        item["removed_sha256"] = hashlib.sha256(bytes.fromhex(row.get("edit_removed_hex", ""))).hexdigest()
        item["inserted_sha256"] = hashlib.sha256(bytes.fromhex(row.get("edit_inserted_hex", ""))).hexdigest()
        item["decomposition"] = None if row.get("g2_decomposition") is None else selected(row["g2_decomposition"], ("enabled", "timer_regions", "direct_timer_sum_wall_ns", "operation_q_high_water"))
        fresh.append(item)
    sorted_failures = sorted(set(failures))
    gate_names = ("v1_custody_and_closure", "v3_historical_failure", "v4_rejected_preexec", "v1_observer", "v1_primary_pairs", "v1_positions", "v1_balanced_center", "v1_read_work", "v1_guard_work", "v1_operation_storage", "eligible_families", "fresh_semantics_and_edit", "fresh_storage", "fresh_durability", "fresh_q_rss_runtime", "operand_custody", "base_proxy_custody", "chronology", "cleanup", "retained_evidence")
    gates = {name: {"pass": not sorted_failures, "failure_ids": sorted_failures} for name in gate_names}
    return {
        "schema": "phase4-g2-v5-normalized-gate-ledger-v1",
        "failures": sorted_failures,
        "gates": gates,
        "v1": {"artifact_hashes": {path.name: file_hash(path) for path in EXPECTED_HASHES}, "payload_entries": 178, "payload_mismatches": 0, "complete_nodes": 181, "subtree_nonwritable": True, "lock_absent": True, "rows": len(sealed_rows), "historical_final_failure_ids": ["17-guard-same-middle-pos1-A:allocated-store-delta", "18-guard-same-middle-pos2-B:allocated-store-delta"], "historical_independent_status": "PASS", "observer": primary.get("observer", {"values": observer}), "pairs": pairs, "positions": positions, "balanced_center": {**centers, "ratio": centers["B"] / centers["A"]}, "timer_regions": primary.get("timer_regions", []), "read_work": read_work, "guards": guards, "eligible_families": primary.get("eligible_families", []), "family_values": primary.get("families", {})},
        "fresh": fresh,
        "operands": custody,
        "base_proxy": proxy,
        "v3_history": {"status": "REVISE", "fresh_rows": 0, "failure": "ValidationAuthorityUnavailable", "payload_manifest_sha256": V3_HASHES[V3 / "PAYLOAD-MANIFEST-v3.tsv"], "terminal_sha256": V3_HASHES[V3 / "TERMINAL-v3.json"], "terminal_verification_sha256": V3_HASHES[V3 / "TERMINAL-VERIFICATION-v3.txt"], "empty_raw_sha256": V3_HASHES[V3 / "rows-v3/G2-V3-RAW.jsonl"]},
        "v4_history": {"classification": "REJECTED_PRE_EXEC_NOT_A_CAMPAIGN", "actual_rows": 0, "result_root_absent": True, "reason": "database proxy symlink resolves sealed 0444 and fs::copy preserves permissions", "artifact_hashes": {path.name: digest for path, digest in V4_HASHES.items()}},
        "chronology": {"plan": plan, "required_interleaving": "prepare-B,prepare-A,row-B,row-A,primary,independent", "all_exits": 0},
        "cleanup": cleanup,
        "limits": {"child_seconds": 15, "global_ns": 59000000000, "transient_peak_bytes": 512 * 1024 * 1024, "retained_bytes": 10 * 1024 * 1024, "fresh_rss_bytes": 20 * 1024 * 1024},
        "timing_claim": "none",
        "g2_disposition": PASS_DISPOSITION if not sorted_failures else "G2 REVISE",
        "g3_eligible": False,
        "post_pass_static_closure_required": ["cargo test --workspace --offline --all-targets", "cargo clippy --workspace --offline --all-targets -- -D warnings", "cargo fmt --all -- --check", "git diff --check"],
    }


def recompute(results):
    failures = []
    for artifact, expected in EXPECTED_HASHES.items():
        if not artifact.is_file() or file_hash(artifact) != expected:
            failures.append(f"custody:{artifact.name}")
    if failures:
        return failures, {}
    failures.extend(validate_payload())
    failures.extend(validate_v3_history())
    failures.extend(validate_v4_history())
    sealed_rows = [json.loads(line) for line in RAW_V1.read_text().splitlines() if line]
    primary_artifact = json.loads(PRIMARY_V1.read_text())
    observer = json.loads(OBSERVER_V1.read_text())
    terminal = json.loads(TERMINAL_V1.read_text())
    if len(sealed_rows) != 18 or primary_artifact.get("status") != "PASS" or primary_artifact.get("disposition") != PASS_DISPOSITION or primary_artifact.get("failures") or primary_artifact.get("eligible_families"):
        failures.append("sealed-primary-artifact")
    if terminal.get("status") != "REVISE" or terminal.get("disposition") != "G2 REVISE":
        failures.append("sealed-terminal-history")
    final_v1 = json.loads(FINAL_V1.read_text())
    independent_v1 = json.loads(INDEPENDENT_V1.read_text())
    required_defect = ["17-guard-same-middle-pos1-A:allocated-store-delta", "18-guard-same-middle-pos2-B:allocated-store-delta"]
    if final_v1.get("status") != "REVISE" or final_v1.get("disposition") != "G2 REVISE" or final_v1.get("failures") != required_defect:
        failures.append("sealed-final-defect")
    if independent_v1.get("status") != "PASS" or independent_v1.get("disposition") != PASS_DISPOSITION or independent_v1.get("failures"):
        failures.append("sealed-independent")
    read_rows = [row for row in sealed_rows if row.get("operation") != "same-middle"]
    if len(read_rows) != 16:
        failures.append("sealed-read-count")
    for row in read_rows:
        failures.extend(check_read_row(row))
    historical_edits = [row for row in sealed_rows if row.get("operation") == "same-middle"]
    if len(historical_edits) != 2 or {row.get("arm") for row in historical_edits} != {"A", "B"}:
        failures.append("sealed-historical-edit-shape")
    else:
        for row in historical_edits:
            failures.extend(check_edit_row(row))
        edit_pair = {row["arm"]: row for row in historical_edits}
        for field in tuple(EDIT_EXPECTED) + ("residue_files",):
            if edit_pair["A"].get(field) != edit_pair["B"].get(field):
                failures.append(f"sealed-historical-edit-parity:{field}")
    comparison_fields = tuple(BASE_IDENTITIES) + (
        "source_fingerprint", "objects_created", "objects_reused", "objects_authenticated",
        "canonical_bytes_authenticated", "canonical_bytes_written", "mapping_bytes_rewritten",
        "transactions", "commits", "q_current", "post_database_sha256",
        "post_authority_sha256", "post_expectations_sha256",
        "sqlite_post_logical_database_bytes", "sqlite_post_logical_store_bytes",
        "sqlite_post_apparent_database_bytes", "sqlite_post_apparent_store_bytes",
        "sqlite_post_allocated_database_bytes", "sqlite_post_allocated_store_bytes",
    )
    for operation, wall_field in (
        ("materialize-fresh", "reconstruction_wall_ns"),
        ("read-range-1m", "range_verification_wall_ns"),
        ("reopen", "fresh_reopen_head_wall_ns"),
    ):
        pair = {row.get("arm"): row for row in read_rows if row.get("operation") == operation}
        if set(pair) != {"A", "B"}:
            failures.append(f"sealed-guard:{operation}:shape")
            continue
        failures.extend(f"sealed-guard:{operation}:parity:{field}" for field in comparison_fields if pair["A"].get(field) != pair["B"].get(field))
        allowed = pair["A"][wall_field] * 1.05
        if operation != "materialize-fresh":
            allowed = max(allowed, pair["A"][wall_field] + 200000)
        if pair["B"][wall_field] > allowed:
            failures.append(f"sealed-guard:{operation}:wall")
        expected_work = {
            "materialize-fresh": {"objects_authenticated": 5371, "canonical_bytes_authenticated": 105122401, "sql_query_calls": 173, "sql_rows_returned": 5374, "borrowed_row_blob_reads": 5284, "borrowed_row_blob_bytes": 104926292, "leaf_batch_queries": 83, "leaf_batch_references": 5284, "q_high_water": 32195},
            "read-range-1m": {"objects_authenticated": 62, "canonical_bytes_authenticated": 1086342, "sql_query_calls": 65, "sql_rows_returned": 65, "borrowed_row_blob_reads": 56, "borrowed_row_blob_bytes": 1078777, "leaf_batch_queries": 0, "leaf_batch_references": 0},
            "reopen": {"objects_authenticated": 0, "canonical_bytes_authenticated": 0, "sql_query_calls": 3, "sql_rows_returned": 3, "borrowed_row_blob_reads": 0, "borrowed_row_blob_bytes": 0, "leaf_batch_queries": 0, "leaf_batch_references": 0},
        }[operation]
        for arm, row in pair.items():
            failures.extend(mismatch(row, expected_work, f"sealed-guard:{operation}:{arm}:work"))
            if operation == "read-range-1m":
                ranges = row.get("range_measurements", [])
                if len(ranges) != 1 or mismatch(ranges[0], {"label": "sequential-1m", "start": 51904512, "end": 52953088, "returned_bytes": 1048576, "objects_authenticated": 60, "canonical_bytes_authenticated": 1086159}, "range"):
                    failures.append(f"sealed-guard:{operation}:{arm}:range")
    primary_rows = [row for row in sealed_rows if row.get("workload") == "primary"]
    measured = [row for row in primary_rows if row.get("kind") == "measured"]
    instrumented = [row for row in measured if row.get("arm") == "B"]
    if len(primary_rows) != 10 or len(measured) != 8 or len(instrumented) != 4:
        failures.append("sealed-primary-shape")
    else:
        for pair_number in range(1, 5):
            pair = {row["arm"]: row for row in measured if row.get("pair") == pair_number}
            if set(pair) != {"A", "B"} or pair["B"]["reconstruction_wall_ns"] / pair["A"]["reconstruction_wall_ns"] > 1.05:
                failures.append(f"sealed-primary-pair:{pair_number}")
        for row in instrumented:
            decomposition = row.get("g2_decomposition", {})
            timer_sum = sum(decomposition.get(field, -1) for field in TIMERS)
            if not decomposition.get("enabled") or decomposition.get("timer_regions") != 32307 or timer_sum != decomposition.get("direct_timer_sum_wall_ns") or timer_sum + decomposition.get("raw_residual_wall_ns", -1) != row.get("reconstruction_wall_ns"):
                failures.append(f"sealed-primary-timers:{row['label']}")
            if any(decomposition.get(field) != 0 for field in ("sqlite_cache_writes", "sqlite_cache_spills", "sqlite_status_errors")):
                failures.append(f"sealed-primary-sqlite:{row['label']}")
            read_phase = next((phase for phase in row.get("phase_counters", []) if phase.get("phase") == "read_operation"), {})
            failures.extend(mismatch(read_phase, {"objects_authenticated": 5371, "canonical_bytes_authenticated": 105122401, "sql_query_calls": 170, "sql_rows_returned": 5371, "borrowed_row_blob_reads": 5284, "borrowed_row_blob_bytes": 104926292, "leaf_batch_queries": 83, "leaf_batch_references": 5284}, f"sealed-primary-read:{row['label']}"))
            if not 0 < decomposition.get("operation_q_high_water", 0) <= 1048576 or row.get("maximum_resident_set_bytes", 20 * 1024 * 1024 + 1) > 20 * 1024 * 1024:
                failures.append(f"sealed-primary-resource:{row['label']}")
        if min(row["g2_decomposition"]["secondary_bytes_decode_wall_ns"] for row in instrumented) >= 33000000:
            failures.append("sealed-removable-family")
        warm_b = next(row for row in primary_rows if row.get("kind") == "warmup" and row.get("arm") == "B")
        observer_limit = min(5000000, warm_b["reconstruction_wall_ns"] // 100)
        if len(observer) != 5 or any(probe.get("status") != "PASS" or probe.get("regions") != 32307 or probe.get("probe_wall_ns", observer_limit + 1) > observer_limit for probe in observer):
            failures.append("sealed-observer")
        for position in (1, 2):
            medians = {arm: sorted(row["reconstruction_wall_ns"] for row in measured if row.get("position") == position and row.get("arm") == arm) for arm in "AB"}
            centers = {arm: sum(values) / len(values) for arm, values in medians.items()}
            if centers["B"] / centers["A"] > 1.05:
                failures.append(f"sealed-position:{position}")
        means = {arm: sum(row["reconstruction_wall_ns"] for row in measured if row.get("arm") == arm) / 4 for arm in "AB"}
        if means["B"] / means["A"] > 1.05:
            failures.append("sealed-balanced")
    raw_v5 = results / "rows-v5/G2-V5-RAW.jsonl"
    fresh_rows = [json.loads(line) for line in raw_v5.read_text().splitlines() if line] if raw_v5.is_file() else []
    if len(fresh_rows) != 2 or {row.get("arm") for row in fresh_rows} != {"A", "B"}:
        failures.append("fresh-edit-shape")
    else:
        for row in fresh_rows:
            failures.extend(check_edit_row(row))
        pair = {row["arm"]: row for row in fresh_rows}
        expected_schedule = {
            "B": {"sequence": 1, "position": 1, "order": "BA", "iteration": 983001, "arm": "B", "binary_sha256": "5d72b46d29a5b77494781f343cc6841a71879b5de426751afe744f27a033e8f5"},
            "A": {"sequence": 2, "position": 2, "order": "BA", "iteration": 983002, "arm": "A", "binary_sha256": "42e3ddeb15df298c978b14639690e366fbb26ee55851524d42c6c3e9c0e8bd55"},
        }
        for arm, row in pair.items():
            failures.extend(mismatch(row, {**expected_schedule[arm], "cli_operation": "edit-same", "kind": "measured", "workload": "v5-guard", "validation": "capture-only", "base_copy_method": "physical-byte-copy-identical-database-authority-expectations"}, f"fresh-edit:{arm}:schedule"))
            decomposition = row.get("g2_decomposition")
            if arm == "A" and decomposition is not None:
                failures.append("fresh-edit:A:decomposition")
            if arm == "B" and (not isinstance(decomposition, dict) or decomposition.get("enabled") is not False or decomposition.get("timer_regions") != 0 or decomposition.get("direct_timer_sum_wall_ns") != 0):
                failures.append("fresh-edit:B:decomposition")
        for field in tuple(EDIT_EXPECTED) + ("residue_files",):
            if pair["A"].get(field) != pair["B"].get(field):
                failures.append(f"fresh-edit-parity:{field}")
    campaign_failures, custody, cleanup, plan, proxy = artifact_failures(results)
    failures.extend(campaign_failures)
    ledger = normalized_ledger(failures, sealed_rows, primary_artifact, observer, fresh_rows, custody, cleanup, plan, proxy)
    return failures, ledger


def self_test():
    valid = next(json.loads(line) for line in RAW_V1.read_text().splitlines() if '"operation":"same-middle"' in line)
    assert check_edit_row(valid) == []
    invalid = dict(valid, allocated_store_delta_bytes=0)
    assert check_edit_row(invalid)
    read = {"operation": "reopen", "status": "PASS", "source_fingerprint": SOURCE, "q_current": 0, "transactions": 0, "commits": 0, "residue_files": [], "allocated_store_delta_bytes": 0, "sqlite_page_size_bytes": "Unavailable", "sqlite_runtime_journal_mode": "delete", "sqlite_runtime_synchronous": 2, "sqlite_runtime_temp_store": 1, "sqlite_runtime_mmap_size": 0, "post_modes": {"authority": "0600", "database": "0600", "expectations": "0400"}, **BASE_IDENTITIES}
    for measure in ("logical", "apparent", "allocated"):
        for scope in ("database", "store"):
            read[f"sqlite_pre_{measure}_{scope}_bytes"] = 7
            read[f"sqlite_post_{measure}_{scope}_bytes"] = 7
    for item in ("database", "authority", "expectations"):
        read[f"pre_edit_{item}_sha256"] = "x"
        read[f"post_{item}_sha256"] = "x"
    assert check_read_row(read) == []
    assert check_read_row(dict(read, allocated_store_delta_bytes=1))


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("results", type=Path, nargs="?")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
        print(json.dumps({"status": "PASS", "checks": 4}, sort_keys=True))
        return 0
    if args.results is None:
        parser.error("results is required unless --self-test is used")
    failures, composition = recompute(args.results)
    status = "PASS" if not failures else "REVISE"
    result = {
        "schema": "phase4-g2-protocol-closure-independent-recomputation-v5",
        "status": status,
        "disposition": PASS_DISPOSITION if status == "PASS" else "G2 REVISE",
        "failures": sorted(set(failures)),
        "composition": composition,
        "normalized_ledger": composition,
        "storage_predicate": "independent-operation-scoped-recomputation",
        "performance_claim": "none",
    }
    (args.results / "G2-V5-INDEPENDENT-RECOMPUTATION.json").write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
    print(json.dumps({"status": status, "disposition": result["disposition"]}, sort_keys=True))
    return 0 if status == "PASS" else 2


if __name__ == "__main__":
    raise SystemExit(main())
