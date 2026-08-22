#!/usr/bin/env python3
"""Primary analyzer for the prospective G2-v3 protocol closure."""

import argparse
import hashlib
import json
import statistics
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[4]
SEALED = REPO / "target/phase4-g2-materialization-decomposition-20260822-v1/results-v1"
V1_RAW = SEALED / "rows-v1/G2-RAW-v1.jsonl"
V1_PRIMARY = SEALED / "G2-PRIMARY-ANALYSIS-v1.json"
V1_OBSERVER = SEALED / "OBSERVER-PROBES-v1.json"
V1_TERMINAL = SEALED / "TERMINAL-v1.json"
V1_PAYLOAD = SEALED / "PAYLOAD-MANIFEST-v1.tsv"
V1_FINAL = SEALED / "G2-ANALYSIS-v1.json"
V1_INDEPENDENT = SEALED / "INDEPENDENT-RECOMPUTATION-v1.json"
V1_STATUS = SEALED / "STATUS-v1.json"
V1_TERMINAL_VERIFICATION = SEALED / "TERMINAL-VERIFICATION-v1.txt"
V1_HASHES = {
    V1_RAW: "6f7124cc8d4fdd248b89770da5576f2546f105304e3d486ddb2f9c7ce5352af2",
    V1_PRIMARY: "0840dcf353eff15a53eaa07f748678bfcab5b02b732ec9c592c12d0f38127282",
    V1_OBSERVER: "bfe2e85b7a1fd61d84699cab4f1f3727731e955965a1370e0cfad8d8a406e717",
    V1_TERMINAL: "b859de6dce9aef9caba43dbf43fd5eb2b7ea24630f7f18ff206749d431e6f2a1",
    V1_PAYLOAD: "28c1b86a3fd3715785617da84195e5ed2cbd5a880dcc883f57f8e51d5edd2d13",
    V1_FINAL: "e926187cd9da28647b3b4695616efe237192721105270844750ac49e5a35bb21",
    V1_INDEPENDENT: "803c9658a3a3ab4238a15a03fe8b7ec8dcef7a313bfee5af4530533fdd5ee5d7",
    V1_STATUS: "1f4112e0bd48a44000f0096b2c7db5a1d9ac3892672ce2ff7356035ba513e97c",
    V1_TERMINAL_VERIFICATION: "d004339854fded0c39af5a7b05a6fea78e398a703846e5eec43ad180f971b1be",
}
DISPOSITION = "G2 PASS / INSUFFICIENT_EVIDENCE FOR A CONSTANT-FACTOR CANDIDATE"
BASELINE = {
    "source_fingerprint": "bb883eecf4ea85d80432953791dcc352243da94175e7503e2c476afe9bd0bab7",
    "root_id": "93d1b461b5cbf88e8d122ad4e90a15a6e3029c408f0fe02ef51c561e5a94c6d1",
    "transition_id": "2de8d2ce6b614373ba8fb8b29d3a3eccd535abfccc2654e7922514e5ca90fd89",
    "ordered_closure_digest": "29233d6018b031f6035c8e2c8175f1ab86fb721808f21a8858bc965567e9c0c1",
    "expected_cdc_sequence_fingerprint": "5bf63ef8adad7bb373be4e968759997b385590d076d75a1488e1958b64a3f8e2",
    "actual_cdc_references": 5284,
}
EDIT = {
    "source_fingerprint": BASELINE["source_fingerprint"],
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
    "post_modes": {"authority": "0600", "database": "0600", "expectations": "0400"},
    "commit_timer_equation_matches": True,
    "durable_phase_sum_matches": True,
    "commit_reconciliation_calls": 0,
    "commit_reconciliation_wall_ns": 0,
    "physical_journal_apparent_bytes": 0,
    "physical_journal_allocated_bytes": 0,
    "q_equation": "Q1",
}
FAMILIES = (
    "sqlite_blob_acquisition_wall_ns",
    "canonical_authentication_wall_ns",
    "mapping_validation_wall_ns",
    "closure_commitment_wall_ns",
    "occurrence_commitment_wall_ns",
    "source_fingerprint_wall_ns",
    "secondary_bytes_decode_wall_ns",
)
ENDPOINT_KINDS = ("logical", "apparent", "allocated")
PAIR_FIELDS = tuple(EDIT) + ("status",)


def sha256(file_path):
    digest = hashlib.sha256()
    with file_path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def mode(file_path):
    return f"{file_path.stat().st_mode & 0o7777:04o}"


def exact_failures(row, expected, prefix):
    return [f"{prefix}:{key}" for key, value in expected.items() if row.get(key) != value]


def payload_failures():
    failures = []
    rows = V1_PAYLOAD.read_text().splitlines()
    if len(rows) != 179 or rows[0] != "path\tsha256\tsize_bytes":
        return ["v1-payload-manifest-shape"]
    for index, line in enumerate(rows[1:], 1):
        relative, expected_hash, expected_size = line.split("\t")
        artifact = (SEALED / relative).resolve()
        try:
            artifact.relative_to(SEALED.resolve())
        except ValueError:
            failures.append(f"v1-payload-{index}:path")
            continue
        if not artifact.is_file() or artifact.stat().st_size != int(expected_size) or sha256(artifact) != expected_hash:
            failures.append(f"v1-payload-{index}:custody")
    expected_nodes = {line.split("\t")[0] for line in rows[1:]} | {"PAYLOAD-MANIFEST-v1.tsv", "TERMINAL-v1.json", "TERMINAL-VERIFICATION-v1.txt"}
    actual_nodes = {str(item.relative_to(SEALED)) for item in SEALED.rglob("*") if not item.is_dir()}
    if actual_nodes != expected_nodes:
        failures.append("v1-complete-file-closure")
    if any(item.stat().st_mode & 0o222 for item in (SEALED.parent, *SEALED.parent.rglob("*")) if not item.is_symlink()):
        failures.append("v1-writable-subtree")
    if (REPO / "target/phase4-g2-materialization-decomposition-20260822-v1.lock").exists():
        failures.append("v1-lock-present")
    return failures


def storage_failures(row, prefix):
    failures = []
    if row.get("operation") == "same-middle":
        failures.extend(exact_failures(row, EDIT, prefix))
        for kind in ENDPOINT_KINDS:
            expected_delta = 16777216 if kind == "allocated" else 114688
            for scope in ("database", "store"):
                before = row.get(f"sqlite_pre_{kind}_{scope}_bytes")
                after = row.get(f"sqlite_post_{kind}_{scope}_bytes")
                if before is None or after is None or after - before != expected_delta:
                    failures.append(f"{prefix}:{kind}-{scope}-delta")
    else:
        if row.get("allocated_store_delta_bytes") != 0:
            failures.append(f"{prefix}:read-only-allocated-store-delta")
        for kind in ENDPOINT_KINDS:
            for scope in ("database", "store"):
                if row.get(f"sqlite_pre_{kind}_{scope}_bytes") != row.get(f"sqlite_post_{kind}_{scope}_bytes"):
                    failures.append(f"{prefix}:read-only-{kind}-{scope}-endpoint")
        for scope in ("database", "authority", "expectations"):
            if row.get(f"pre_edit_{scope}_sha256") != row.get(f"post_{scope}_sha256"):
                failures.append(f"{prefix}:read-only-{scope}-hash")
        if row.get("transactions") != 0 or row.get("commits") != 0:
            failures.append(f"{prefix}:read-only-transaction-shape")
    return failures


def row_common_failures(row, prefix):
    failures = []
    if row.get("status") != "PASS" or row.get("q_current") != 0:
        failures.append(f"{prefix}:status-or-terminal-q")
    if row.get("residue_files"):
        failures.append(f"{prefix}:residue")
    runtime = {
        "sqlite_page_size_bytes": 4096 if row.get("operation") == "same-middle" else "Unavailable",
        "sqlite_runtime_journal_mode": "delete",
        "sqlite_runtime_synchronous": 2,
        "sqlite_runtime_temp_store": 1,
        "sqlite_runtime_mmap_size": 0,
        "post_modes": {"authority": "0600", "database": "0600", "expectations": "0400"},
    }
    failures.extend(exact_failures(row, runtime, prefix))
    if row.get("operation") == "same-middle" and row.get("maximum_resident_set_bytes", 20 * 1024 * 1024 + 1) > 20 * 1024 * 1024:
        failures.append(f"{prefix}:rss-bound")
    if row.get("operation") == "same-middle":
        try:
            removed = bytes.fromhex(row.get("edit_removed_hex", ""))
            inserted = bytes.fromhex(row.get("edit_inserted_hex", ""))
        except ValueError:
            failures.append(f"{prefix}:edit-hex")
        else:
            if len(removed) != 18854 or hashlib.sha256(removed).hexdigest() != "fdc04dd5bea39e9480dd5559068fd72e0e9c2c3ce5b92fa8c62798ee3425a8fa":
                failures.append(f"{prefix}:removed-bytes")
            if len(inserted) != 18854 or hashlib.sha256(inserted).hexdigest() != "8a4df9c28dcf4e0625ba08fa5f92c4ea2e462274c081072823900ab0e75611d6":
                failures.append(f"{prefix}:inserted-bytes")
    return failures + storage_failures(row, prefix)


def read_only_evidence_failures(rows):
    failures = []
    read_rows = [row for row in rows if row.get("operation") != "same-middle"]
    if len(rows) != 18 or len(read_rows) != 16:
        failures.append("v1-row-shape")
    for row in read_rows:
        prefix = f"v1:{row.get('label', 'unlabeled')}"
        failures.extend(row_common_failures(row, prefix))
        failures.extend(exact_failures(row, BASELINE, prefix))
    historical_edits = [row for row in rows if row.get("operation") == "same-middle"]
    if len(historical_edits) != 2 or {row.get("arm") for row in historical_edits} != {"A", "B"}:
        failures.append("v1-historical-edit-shape")
    else:
        for row in historical_edits:
            failures.extend(row_common_failures(row, f"v1-historical-edit:{row['label']}"))
        historical_by_arm = {row["arm"]: row for row in historical_edits}
        failures.extend(f"v1-historical-edit:parity:{key}" for key in PAIR_FIELDS if historical_by_arm["A"].get(key) != historical_by_arm["B"].get(key))
    parity_fields = tuple(BASELINE) + (
        "objects_created", "objects_reused", "objects_authenticated",
        "canonical_bytes_authenticated", "canonical_bytes_written",
        "mapping_bytes_rewritten", "transactions", "commits", "q_current",
        "sqlite_post_logical_database_bytes", "sqlite_post_logical_store_bytes",
        "sqlite_post_apparent_database_bytes", "sqlite_post_apparent_store_bytes",
        "sqlite_post_allocated_database_bytes", "sqlite_post_allocated_store_bytes",
        "post_database_sha256", "post_authority_sha256", "post_expectations_sha256",
    )
    for operation in ("materialize-fresh", "read-range-1m", "reopen"):
        pair = [row for row in read_rows if row.get("operation") == operation]
        if len(pair) != 2 or {row.get("arm") for row in pair} != {"A", "B"}:
            failures.append(f"v1-guard-{operation}:shape")
            continue
        by_arm = {row["arm"]: row for row in pair}
        failures.extend(
            f"v1-guard-{operation}:parity:{key}"
            for key in parity_fields
            if by_arm["A"].get(key) != by_arm["B"].get(key)
        )
        wall_field = (
            "reconstruction_wall_ns" if operation == "materialize-fresh"
            else "range_verification_wall_ns" if operation == "read-range-1m"
            else "fresh_reopen_head_wall_ns"
        )
        control, candidate = by_arm["A"][wall_field], by_arm["B"][wall_field]
        allowance = control * 1.05
        if operation != "materialize-fresh":
            allowance = max(allowance, control + 200000)
        if candidate > allowance:
            failures.append(f"v1-guard-{operation}:wall")
        work = {
            "materialize-fresh": {"objects_authenticated": 5371, "canonical_bytes_authenticated": 105122401, "sql_query_calls": 173, "sql_rows_returned": 5374, "borrowed_row_blob_reads": 5284, "borrowed_row_blob_bytes": 104926292, "leaf_batch_queries": 83, "leaf_batch_references": 5284, "q_high_water": 32195},
            "read-range-1m": {"objects_authenticated": 62, "canonical_bytes_authenticated": 1086342, "sql_query_calls": 65, "sql_rows_returned": 65, "borrowed_row_blob_reads": 56, "borrowed_row_blob_bytes": 1078777, "leaf_batch_queries": 0, "leaf_batch_references": 0},
            "reopen": {"objects_authenticated": 0, "canonical_bytes_authenticated": 0, "sql_query_calls": 3, "sql_rows_returned": 3, "borrowed_row_blob_reads": 0, "borrowed_row_blob_bytes": 0, "leaf_batch_queries": 0, "leaf_batch_references": 0},
        }[operation]
        for row in pair:
            failures.extend(exact_failures(row, work, f"v1-guard-{operation}:{row['arm']}:work"))
            if operation == "read-range-1m":
                ranges = row.get("range_measurements", [])
                if len(ranges) != 1 or any(ranges[0].get(key) != value for key, value in {"label": "sequential-1m", "start": 51904512, "end": 52953088, "returned_bytes": 1048576, "objects_authenticated": 60, "canonical_bytes_authenticated": 1086159}.items()):
                    failures.append(f"v1-guard-{operation}:{row['arm']}:range-work")
    return failures


def decomposition_failures(rows, primary, observer):
    failures = []
    expected_primary = {
        "status": "PASS", "disposition": DISPOSITION, "failures": [],
        "primary_rows": 10, "measured_rows": 8, "eligible_families": [],
        "timer_regions": [32307],
    }
    failures.extend(exact_failures(primary, expected_primary, "v1-primary-artifact"))
    primary_rows = [row for row in rows if row.get("workload") == "primary"]
    measured = [row for row in primary_rows if row.get("kind") == "measured"]
    b_rows = [row for row in measured if row.get("arm") == "B"]
    if len(primary_rows) != 10 or len(measured) != 8 or len(b_rows) != 4:
        failures.append("v1-primary-recomputed-shape")
        return failures
    for pair_id in range(1, 5):
        pair = {row["arm"]: row for row in measured if row.get("pair") == pair_id}
        if set(pair) != {"A", "B"} or pair["B"]["reconstruction_wall_ns"] > pair["A"]["reconstruction_wall_ns"] * 1.05:
            failures.append(f"v1-primary-pair-{pair_id}")
    for row in b_rows:
        g2 = row.get("g2_decomposition", {})
        if not g2.get("enabled") or g2.get("timer_regions") != 32307:
            failures.append(f"v1:{row['label']}:decomposition-shape")
            continue
        direct = sum(g2.get(name, -1) for name in FAMILIES)
        if direct != g2.get("direct_timer_sum_wall_ns") or direct + g2.get("raw_residual_wall_ns", -1) != row.get("reconstruction_wall_ns"):
            failures.append(f"v1:{row['label']}:timer-equation")
        if any(g2.get(key) != 0 for key in ("sqlite_cache_writes", "sqlite_cache_spills", "sqlite_status_errors")):
            failures.append(f"v1:{row['label']}:sqlite-read-status")
        phase = next((item for item in row.get("phase_counters", []) if item.get("phase") == "read_operation"), {})
        read_work = {"objects_authenticated": 5371, "canonical_bytes_authenticated": 105122401, "sql_query_calls": 170, "sql_rows_returned": 5371, "borrowed_row_blob_reads": 5284, "borrowed_row_blob_bytes": 104926292, "leaf_batch_queries": 83, "leaf_batch_references": 5284}
        failures.extend(exact_failures(phase, read_work, f"v1:{row['label']}:read-operation"))
        if not 0 < g2.get("operation_q_high_water", 0) <= 1048576 or row.get("maximum_resident_set_bytes", 20 * 1024 * 1024 + 1) > 20 * 1024 * 1024:
            failures.append(f"v1:{row['label']}:resource-bound")
    removable = [row["g2_decomposition"]["secondary_bytes_decode_wall_ns"] for row in b_rows]
    if min(removable) >= 33000000:
        failures.append("v1-unexpected-removable-family")
    warmup_b = next(row for row in primary_rows if row.get("kind") == "warmup" and row.get("arm") == "B")
    observer_gate = min(5000000, warmup_b["reconstruction_wall_ns"] // 100)
    if len(observer) != 5 or any(item.get("status") != "PASS" or item.get("regions") != 32307 or item.get("probe_wall_ns", observer_gate + 1) > observer_gate for item in observer):
        failures.append("v1-observer-gate")
    for position in (1, 2):
        arms = {arm: statistics.median(row["reconstruction_wall_ns"] for row in measured if row.get("position") == position and row.get("arm") == arm) for arm in "AB"}
        if arms["B"] / arms["A"] > 1.05:
            failures.append(f"v1-position-{position}-observer")
    centers = {arm: statistics.mean(row["reconstruction_wall_ns"] for row in measured if row.get("arm") == arm) for arm in "AB"}
    if centers["B"] / centers["A"] > 1.05:
        failures.append("v1-balanced-observer")
    return failures


def edit_pair_failures(rows):
    failures = []
    if len(rows) != 2 or {row.get("arm") for row in rows} != {"A", "B"}:
        return ["v3-edit-pair-shape"]
    by_arm = {row["arm"]: row for row in rows}
    for arm, row in by_arm.items():
        prefix = f"v3:{arm}:{row.get('label', 'unlabeled')}"
        if row.get("operation") != "same-middle" or row.get("warmup") is not False:
            failures.append(f"{prefix}:operation-shape")
        failures.extend(row_common_failures(row, prefix))
        expected_schedule = {
            "B": {"sequence": 1, "position": 1, "order": "BA", "iteration": 983001, "arm": "B", "binary_sha256": "5d72b46d29a5b77494781f343cc6841a71879b5de426751afe744f27a033e8f5"},
            "A": {"sequence": 2, "position": 2, "order": "BA", "iteration": 983002, "arm": "A", "binary_sha256": "42e3ddeb15df298c978b14639690e366fbb26ee55851524d42c6c3e9c0e8bd55"},
        }[arm]
        failures.extend(exact_failures(row, {**expected_schedule, "cli_operation": "edit-same", "kind": "measured", "workload": "v3-guard", "validation": "capture-only", "base_copy_method": "physical-byte-copy-identical-database-authority-expectations"}, prefix))
        decomposition = row.get("g2_decomposition")
        if arm == "A" and decomposition is not None:
            failures.append(f"{prefix}:control-decomposition")
        if arm == "B" and (not isinstance(decomposition, dict) or decomposition.get("enabled") is not False or decomposition.get("timer_regions") != 0 or decomposition.get("direct_timer_sum_wall_ns") != 0):
            failures.append(f"{prefix}:candidate-decomposition")
    failures.extend(
        f"v3-edit-pair:parity:{key}"
        for key in PAIR_FIELDS
        if by_arm["A"].get(key) != by_arm["B"].get(key)
    )
    return failures


def expected_plan(results):
    operands = results / "operands-v3"
    work = results / "rows-v3/work-v3"
    candidate = operands / "phase4_create_edit_benchmark-instrumented"
    control = operands / "phase4_create_edit_benchmark-control"
    rows = (
        ("01-measured-same-middle-pos1-B", 983001, candidate),
        ("02-measured-same-middle-pos2-A", 983002, control),
    )
    plan = [
        {"kind": "prepare", "label": f"prepare-{label}", "command": [str(candidate), "--fast-prepare", str(work / label), "104857600", "edit-same", str(iteration)]}
        for label, iteration, _ in rows
    ]
    plan.extend({"kind": "row", "label": label, "command": ["/usr/bin/time", "-l", str(binary), "--fast-row", str(work / label), "104857600", "edit-same", str(iteration), "false", "capture-only"]} for label, iteration, binary in rows)
    plan.extend((
        {"kind": "analyzer", "label": "primary-analysis", "command": [sys.executable, str(HERE / "analyze_g2_v3.py"), str(results)]},
        {"kind": "analyzer", "label": "independent-recomputation", "command": [sys.executable, str(HERE / "recompute_g2_v3.py"), str(results)]},
    ))
    return plan


def artifact_failures(results, analyzer_label):
    failures = []
    custody_path = results / "OPERAND-CUSTODY-v3.json"
    cleanup_path = results / "TRANSIENT-VERIFICATION-v3.json"
    plan_path = results / "CHRONOLOGY-PLAN-v3.json"
    chronology_path = results / "CHRONOLOGY-v3.jsonl"
    if not all(path.is_file() for path in (custody_path, cleanup_path, plan_path, chronology_path)):
        return ["v3-artifact-set-missing"], [], {}, []
    custody = json.loads(custody_path.read_text())
    expected_operands = {"control": ("42e3ddeb15df298c978b14639690e366fbb26ee55851524d42c6c3e9c0e8bd55", 1372784), "candidate": ("5d72b46d29a5b77494781f343cc6841a71879b5de426751afe744f27a033e8f5", 1390512)}
    if len(custody) != 2 or {item.get("name") for item in custody} != set(expected_operands):
        failures.append("v3-operand-custody-shape")
    for item in custody:
        digest, size = expected_operands.get(item.get("name"), (None, None))
        copied = Path(item.get("copy_path", ""))
        source = Path(item.get("source_path", ""))
        if not copied.is_file() or not source.is_file() or sha256(copied) != digest or copied.stat().st_size != size or mode(copied) != "0500" or item.get("sha256") != digest or item.get("size_bytes") != size or item.get("copy_mode") != "0500" or item.get("execution_path") != "snapshot-only":
            failures.append(f"v3-operand-{item.get('name')}:bytes-mode")
            continue
        if (source.stat().st_dev, source.stat().st_ino, copied.stat().st_dev, copied.stat().st_ino) != (item.get("source_device"), item.get("source_inode"), item.get("copy_device"), item.get("copy_inode")) or (source.stat().st_dev, source.stat().st_ino) == (copied.stat().st_dev, copied.stat().st_ino):
            failures.append(f"v3-operand-{item.get('name')}:inode")
    cleanup = json.loads(cleanup_path.read_text())
    cleanup_expected = cleanup.get("status") == "PASS" and cleanup.get("declared_deletions") == ["rows-v3/work-v3"] and cleanup.get("deleted") == ["rows-v3/work-v3"] and cleanup.get("work_path_existed") is True and cleanup.get("work_path_absent") is True and cleanup.get("rows_validated") is True and cleanup.get("usage", {}).get("within_ceiling") is True and not (results / "rows-v3/work-v3").exists()
    if not cleanup_expected:
        failures.append("v3-cleanup-contract")
    plan = json.loads(plan_path.read_text())
    expected = expected_plan(results)
    if plan != expected:
        failures.append("v3-chronology-plan")
    records = [json.loads(line) for line in chronology_path.read_text().splitlines() if line]
    observed = [{key: row.get(key) for key in ("event", "kind", "label", "command", "exit_code") if key in row} for row in records if row.get("event") in ("child-start", "child-complete")]
    events = []
    for child in expected:
        events.extend(({"event": "child-start", **child}, {"event": "child-complete", **child, "exit_code": 0}))
    prefix_length = 9 if analyzer_label == "primary-analysis" else 11
    if observed != events[:prefix_length]:
        failures.append("v3-chronology-prefix")
    return failures, custody, cleanup, expected


def selected(row, keys):
    return {key: row.get(key) for key in keys}


def normalized_ledger(results, failures, v1_rows, primary, observer, fresh_rows, custody, cleanup, plan):
    measured = [row for row in v1_rows if row.get("workload") == "primary" and row.get("kind") == "measured"]
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
    guards = [selected(row, guard_keys) | {"range_measurements": row.get("range_measurements", [])} for row in v1_rows if row.get("workload") == "guard" and row.get("operation") != "same-middle"]
    fresh_keys = tuple(EDIT) + ("status", "label", "arm", "sequence", "position", "order", "iteration", "binary_sha256", "base_copy_method", "maximum_resident_set_bytes", "durable_capture_total_wall_ns")
    fresh = []
    for row in sorted(fresh_rows, key=lambda item: item.get("sequence", 0)):
        item = selected(row, fresh_keys)
        item["removed_sha256"] = hashlib.sha256(bytes.fromhex(row.get("edit_removed_hex", ""))).hexdigest()
        item["inserted_sha256"] = hashlib.sha256(bytes.fromhex(row.get("edit_inserted_hex", ""))).hexdigest()
        item["decomposition"] = None if row.get("g2_decomposition") is None else selected(row["g2_decomposition"], ("enabled", "timer_regions", "direct_timer_sum_wall_ns", "operation_q_high_water"))
        fresh.append(item)
    sorted_failures = sorted(set(failures))
    gate_names = ("v1_custody_and_closure", "v1_observer", "v1_primary_pairs", "v1_positions", "v1_balanced_center", "v1_read_work", "v1_guard_work", "v1_operation_storage", "eligible_families", "fresh_semantics_and_edit", "fresh_storage", "fresh_durability", "fresh_q_rss_runtime", "operand_custody", "chronology", "cleanup", "retained_evidence")
    gates = {name: {"pass": not sorted_failures, "failure_ids": sorted_failures} for name in gate_names}
    return {
        "schema": "phase4-g2-v3-normalized-gate-ledger-v1",
        "failures": sorted_failures,
        "gates": gates,
        "v1": {"artifact_hashes": {path.name: sha256(path) for path in V1_HASHES}, "payload_entries": 178, "payload_mismatches": 0, "complete_nodes": 181, "subtree_nonwritable": True, "lock_absent": True, "rows": len(v1_rows), "historical_final_failure_ids": ["17-guard-same-middle-pos1-A:allocated-store-delta", "18-guard-same-middle-pos2-B:allocated-store-delta"], "historical_independent_status": "PASS", "observer": primary.get("observer", {"values": observer}), "pairs": pairs, "positions": positions, "balanced_center": {**centers, "ratio": centers["B"] / centers["A"]}, "timer_regions": primary.get("timer_regions", []), "read_work": read_work, "guards": guards, "eligible_families": primary.get("eligible_families", []), "family_values": primary.get("families", {})},
        "fresh": fresh,
        "operands": custody,
        "chronology": {"plan": plan, "required_interleaving": "prepare-B,prepare-A,row-B,row-A,primary,independent", "all_exits": 0},
        "cleanup": cleanup,
        "limits": {"child_seconds": 15, "global_ns": 59000000000, "transient_peak_bytes": 300 * 1024 * 1024, "retained_bytes": 10 * 1024 * 1024, "fresh_rss_bytes": 20 * 1024 * 1024},
        "timing_claim": "none",
        "g2_disposition": DISPOSITION if not sorted_failures else "G2 REVISE",
        "g3_eligible": False,
        "post_pass_static_closure_required": ["cargo test --workspace --offline --all-targets", "cargo clippy --workspace --offline --all-targets -- -D warnings", "cargo fmt --all -- --check", "git diff --check"],
    }


def analyze(results):
    failures = []
    for file_path, expected in V1_HASHES.items():
        if not file_path.is_file() or sha256(file_path) != expected:
            failures.append(f"v1-custody:{file_path.name}")
    if failures:
        return failures, {}
    failures.extend(payload_failures())
    v1_rows = [json.loads(line) for line in V1_RAW.read_text().splitlines() if line]
    primary = json.loads(V1_PRIMARY.read_text())
    observer = json.loads(V1_OBSERVER.read_text())
    terminal = json.loads(V1_TERMINAL.read_text())
    if terminal.get("status") != "REVISE" or terminal.get("disposition") != "G2 REVISE":
        failures.append("v1-historical-terminal-drift")
    final = json.loads(V1_FINAL.read_text())
    independent = json.loads(V1_INDEPENDENT.read_text())
    exact_final_failures = ["17-guard-same-middle-pos1-A:allocated-store-delta", "18-guard-same-middle-pos2-B:allocated-store-delta"]
    if final.get("status") != "REVISE" or final.get("disposition") != "G2 REVISE" or final.get("failures") != exact_final_failures:
        failures.append("v1-final-defect-shape")
    if independent.get("status") != "PASS" or independent.get("disposition") != DISPOSITION or independent.get("failures"):
        failures.append("v1-independent-shape")
    failures.extend(read_only_evidence_failures(v1_rows))
    failures.extend(decomposition_failures(v1_rows, primary, observer))
    raw_v3 = results / "rows-v3/G2-V3-RAW.jsonl"
    if not raw_v3.is_file():
        failures.append("v3-raw-missing")
        v3_rows = []
    else:
        v3_rows = [json.loads(line) for line in raw_v3.read_text().splitlines() if line]
    failures.extend(edit_pair_failures(v3_rows))
    campaign_failures, custody, cleanup, plan = artifact_failures(results, "primary-analysis")
    failures.extend(campaign_failures)
    ledger = normalized_ledger(results, failures, v1_rows, primary, observer, v3_rows, custody, cleanup, plan)
    return failures, ledger


def self_test():
    valid_edit = {"operation": "same-middle", **EDIT}
    assert storage_failures(valid_edit, "valid") == []
    wrong_edit = dict(valid_edit, sqlite_post_allocated_store_bytes=EDIT["sqlite_post_allocated_store_bytes"] - 4096)
    assert any("sqlite_post_allocated_store_bytes" in item or "allocated-store-delta" in item for item in storage_failures(wrong_edit, "wrong"))
    valid_read = {"operation": "reopen", "allocated_store_delta_bytes": 0, "transactions": 0, "commits": 0}
    for kind in ENDPOINT_KINDS:
        for scope in ("database", "store"):
            valid_read[f"sqlite_pre_{kind}_{scope}_bytes"] = 1
            valid_read[f"sqlite_post_{kind}_{scope}_bytes"] = 1
    for scope in ("database", "authority", "expectations"):
        valid_read[f"pre_edit_{scope}_sha256"] = "same"
        valid_read[f"post_{scope}_sha256"] = "same"
    assert storage_failures(valid_read, "valid") == []
    assert storage_failures(dict(valid_read, allocated_store_delta_bytes=1), "wrong") == ["wrong:read-only-allocated-store-delta"]


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
    failures, composition = analyze(args.results)
    status = "PASS" if not failures else "REVISE"
    output = {
        "schema": "phase4-g2-protocol-closure-primary-analysis-v3",
        "status": status,
        "disposition": DISPOSITION if status == "PASS" else "G2 REVISE",
        "failures": sorted(set(failures)),
        "composition": composition,
        "normalized_ledger": composition,
        "storage_predicate": "read-only-zero-delta; same-middle-exact-prospective-endpoints-and-ab-parity",
        "performance_claim": "none",
    }
    (args.results / "G2-V3-ANALYSIS.json").write_text(json.dumps(output, indent=2, sort_keys=True) + "\n")
    print(json.dumps({"status": status, "disposition": output["disposition"]}, sort_keys=True))
    return 0 if status == "PASS" else 2


if __name__ == "__main__":
    raise SystemExit(main())
