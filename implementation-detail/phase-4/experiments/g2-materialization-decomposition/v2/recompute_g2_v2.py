#!/usr/bin/env python3
"""Independent recomputation for the prospective G2-v2 protocol closure."""

import argparse
import hashlib
import json
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[4]
SEALED = REPO / "target/phase4-g2-materialization-decomposition-20260822-v1/results-v1"
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
    return problems


def check_read_row(row):
    label = f"sealed-read:{row.get('label', '?')}"
    failures = mismatch(row, {"status": "PASS", "source_fingerprint": SOURCE, "q_current": 0, "transactions": 0, "commits": 0, **BASE_IDENTITIES}, label)
    if row.get("residue_files"):
        failures.append(f"{label}:residue")
    failures.extend(mismatch(row, {"sqlite_page_size_bytes": 4096, "sqlite_runtime_journal_mode": "delete", "sqlite_runtime_synchronous": 2, "sqlite_runtime_temp_store": 1, "sqlite_runtime_mmap_size": 0, "post_modes": {"authority": "0600", "database": "0600", "expectations": "0400"}}, label))
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


def recompute(results):
    failures = []
    for artifact, expected in EXPECTED_HASHES.items():
        if not artifact.is_file() or file_hash(artifact) != expected:
            failures.append(f"custody:{artifact.name}")
    if failures:
        return failures, {}
    failures.extend(validate_payload())
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
    raw_v2 = results / "rows-v2/G2-V2-RAW.jsonl"
    fresh_rows = [json.loads(line) for line in raw_v2.read_text().splitlines() if line] if raw_v2.is_file() else []
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
            failures.extend(mismatch(row, {**expected_schedule[arm], "cli_operation": "edit-same", "kind": "measured", "workload": "v2-guard", "validation": "capture-only", "base_copy_method": "physical-byte-copy-identical-database-authority-expectations"}, f"fresh-edit:{arm}:schedule"))
        for field in tuple(EDIT_EXPECTED) + ("residue_files",):
            if pair["A"].get(field) != pair["B"].get(field):
                failures.append(f"fresh-edit-parity:{field}")
    ledger = {
        "v1_payload_entries": 178,
        "v1_payload_mismatches": len([item for item in failures if item.startswith("payload:")]),
        "v1_rows": len(sealed_rows),
        "v1_raw_sha256": file_hash(RAW_V1),
        "v1_primary_rows": len(primary_rows),
        "v1_read_only_rows": len(read_rows),
        "v1_historical_edit_rows": len(historical_edits),
        "v2_fresh_edit_rows": len(fresh_rows),
        "v2_fresh_order": [row.get("arm") for row in sorted(fresh_rows, key=lambda row: row.get("sequence", 0))],
        "v2_post_logical_store_bytes": EDIT_EXPECTED["sqlite_post_logical_store_bytes"],
        "v2_post_apparent_store_bytes": EDIT_EXPECTED["sqlite_post_apparent_store_bytes"],
        "v2_post_allocated_store_bytes": EDIT_EXPECTED["sqlite_post_allocated_store_bytes"],
        "v2_allocated_store_delta_bytes": EDIT_EXPECTED["allocated_store_delta_bytes"],
        "timing_claim": "none",
    }
    return failures, ledger


def self_test():
    valid = next(json.loads(line) for line in RAW_V1.read_text().splitlines() if '"operation":"same-middle"' in line)
    assert check_edit_row(valid) == []
    invalid = dict(valid, allocated_store_delta_bytes=0)
    assert check_edit_row(invalid)
    read = {"operation": "reopen", "status": "PASS", "source_fingerprint": SOURCE, "q_current": 0, "transactions": 0, "commits": 0, "residue_files": [], "allocated_store_delta_bytes": 0, "sqlite_page_size_bytes": 4096, "sqlite_runtime_journal_mode": "delete", "sqlite_runtime_synchronous": 2, "sqlite_runtime_temp_store": 1, "sqlite_runtime_mmap_size": 0, "post_modes": {"authority": "0600", "database": "0600", "expectations": "0400"}, **BASE_IDENTITIES}
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
        "schema": "phase4-g2-protocol-closure-independent-recomputation-v2",
        "status": status,
        "disposition": PASS_DISPOSITION if status == "PASS" else "G2 REVISE",
        "failures": sorted(set(failures)),
        "composition": composition,
        "normalized_ledger": composition,
        "storage_predicate": "independent-operation-scoped-recomputation",
        "performance_claim": "none",
    }
    (args.results / "G2-V2-INDEPENDENT-RECOMPUTATION.json").write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
    print(json.dumps({"status": status, "disposition": result["disposition"]}, sort_keys=True))
    return 0 if status == "PASS" else 2


if __name__ == "__main__":
    raise SystemExit(main())
