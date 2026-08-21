#!/usr/bin/env python3
"""Independent H05 private-screen analyzer; standard library only."""

import copy
import hashlib
import json
import re
import statistics
import sys
import tempfile
from pathlib import Path

PLAN = [
    (0, "warmup", "A", "AB"), (0, "warmup", "B", "AB"),
    (1, "measured", "A", "AB"), (1, "measured", "B", "AB"),
    (2, "measured", "B", "BA"), (2, "measured", "A", "BA"),
    (3, "measured", "A", "AB"), (3, "measured", "B", "AB"),
]
SMOKE_OPS = [
    "same-middle", "plus1-early", "plus1-middle", "materialize-warm",
    "materialize-fresh", "read-range-1m", "reopen",
]
PROFILE = "b0ebb845409ef995a5fa454bb23d10a80c6ecf44deb7832ca2ce1213eb0f4ba1"
FIXTURE_SHA = "63b3695b8c117b5bc39885e0df0dcd0af1d49e575482bab16577d84b4f40eff4"
SOURCE_FINGERPRINT = "bb883eecf4ea85d80432953791dcc352243da94175e7503e2c476afe9bd0bab7"
CDC_SEQUENCE = "5bb376c3c54d8724973a7b160acab599f2f5cee4b4a56e855ff0cbe987425994"
ROOT = "2d41c27f96b0332475fb8ec3c46a336c9c8a8084408bc545e5cbb24d51cb25d0"
TRANSITION = "ba15fd20469414de99c135fc90a5c5ad028f99f115b8c0d138ace9ec98536412"
CLOSURE = "d6aac6e40cc851dd6295dbeec6488f1c5ebefa7520f86b0cd12bdcdce1f0d54a"
SMOKE_NATIVE = {
    "same-middle": ("same-middle", "C1-changed-spine", 5284,
        "e6d6d858ab6ff9804839630df90a2e621ae06291e55ab12aea9957c566ec83f7"),
    "plus1-early": ("plus1-early", "C1-count-change-construction-proof", 5285,
        "17cc931a186ae7b3a69ea2f17e9f6f1047ed9816e15311b62f382f6cb5284cd3"),
    "plus1-middle": ("plus1-middle", "C1-count-change-construction-proof", 5285,
        "254945dbd7a5c2b10365a16590c6418310efe0a93d2354db14bac66a80709367"),
    "materialize-warm": ("materialize-warm", "not-applicable", 5284, CDC_SEQUENCE),
    "materialize-fresh": ("materialize-fresh", "not-applicable", 5284, CDC_SEQUENCE),
    "read-range-1m": ("read-range-1m", "not-applicable", 5284, CDC_SEQUENCE),
    "reopen": ("reopen", "not-applicable", 5284, CDC_SEQUENCE),
}
EXECUTABLES = {
    "A": "9cda87ee7fd92784281a6ec7ee3045eb661681d8b7b930dd36546119ae4749d7",
    "B": "15a668739e96de064a5a7dff1c0b1278406fa077f089687da210e83451e257dd",
}
SOURCES = {
    "A": "3284c3bdfe20426df78b4cb8ef310248e1e4f644b8422d79c4689653d870652a",
    "B": "e675d2fc7646745eaf709f61703ff84098949ce4319cb4e6882b96698d95d031",
}
EXPECTATION_VERSIONS = {"A": "LFS-WP4M-EXPECTATIONS-3", "B": "LFS-H05-EXPECTATIONS-1"}
NATIVE_RUNNER_SHA = "49acb9a70cc7ecdc9f57b931d3be29f497513773c5e9e50087d83f6425e45a33"
CPU_SCOPE = "whole-child-process; phase-local CPU unavailable"
CACHE_SCOPE = "fresh LayerFS process/connection where declared; OS/filesystem cache warm-or-unknown"
MEASUREMENT_BOUNDARIES = {
    "full": "durable-submit", "same-middle": "same-open-durable-edit",
    "plus1-early": "same-open-durable-edit", "plus1-middle": "same-open-durable-edit",
    "materialize-warm": "logical-materialization-warm",
    "materialize-fresh": "fresh-process-logical-materialization",
    "read-range-1m": "authenticated-sequential-1m-range",
    "reopen": "fresh-process-head-ready",
}
SMOKE_IDENTITIES = {
    "same-middle": (
        "d1a69475b0f8e25e44d7bd625a679b596ea2a8b3347ef8c15fafa13f654b299b",
        "f11cc9d84deae7f1871adca62cc562ab63dbb01e9c39771ed3522eab4007cee1",
        "c0f6a39bf9939c89301bedb564516c5ec851321a1d89c69b2e95d4b1844a9587",
    ),
    "plus1-early": (
        "4648eb987df7b46844135218cdbd73cbd8480d34b74a832f123fdfb1221869eb",
        "ac12e88bc47967043647484112ab5d1113d7f0ebbaa8c9026749b9123d8e949a",
        "e86efa7aaeaaf8f983c8fcaf48b5c206ce6d53d2be502cfc05a33dede544c5f1",
    ),
    "plus1-middle": (
        "41e9b48e1af960a4587027b929608d50686b59cd9dc22a625cbb5548379539b9",
        "bfcc3537f01f17265ecef026e5fc5ccf4a4da599c4659ddd4259a8bd63ff74a9",
        "4eb35ed21ded2bf3135d058a6a0da042db1af3c53d74d119e82c956a9c07110a",
    ),
}
HEX64 = re.compile(r"[0-9a-f]{64}").fullmatch
Q_EQUATION = "Q1"
Q_COMPONENTS = [
    "q_cdc_base_live_bytes", "q_cdc_old_window_bytes", "q_cdc_scan_input_bytes",
    "q_cdc_overlap_current", "q_cdc_old_chunk_slots_bytes", "leaf_batch_bound",
    "leaf_batch_queries", "leaf_batch_references", "leaf_batch_references_max",
    "leaf_batch_query_bytes_max",
]
UNAVAILABLE = {
    "host_physical_io": "U_PHYS_BYTES",
    "peak_journal_bytes": "U_JRN_PEAK",
    "peak_temporary_bytes": "U_TMP_PEAK",
    "sync_fsync_observations": "U_VFS_SYNC",
}
EQUAL_WORK = [
    "source_bytes_read", "source_cdc_bytes_read", "canonical_stage_source_bytes_read",
    "raw_bytes_hashed", "raw_hashes", "canonical_id_bytes_hashed",
    "canonical_id_hashes", "canonical_authentication_hash_bytes",
    "canonical_authentication_hashes", "canonical_new_write_bytes",
    "canonical_authenticated_nonnew_bytes", "canonical_bytes_authenticated",
    "canonical_bytes_written", "mapping_bytes_rewritten", "w_bytes", "d_bytes",
    "payload_io_bytes", "objects_created", "objects_reused", "objects_authenticated",
    "statement_cache_acquisitions", "sql_calls", "sql_query_calls", "sql_execute_calls",
    "sql_rows_returned", "sql_rows_changed", "row_blob_reads", "row_blob_writes",
    "row_blob_copy_bytes", "borrowed_row_blob_reads", "borrowed_row_blob_bytes",
    "blob_opens", "blob_reads", "blob_writes", "sqlite_main_db_dirty_pages_written",
    "sqlite_main_db_pager_write_bytes", "sqlite_cache_spill_pages", "transactions",
    "commits", "commit_dispatches", "commit_returns", "commit_return_successes",
    "commit_return_errors", "references", "pages", "branches", "chunks",
    "construction_cdc_entries", "construction_put_evidences",
    "construction_edges_covered", "construction_leaf_summaries",
    "construction_branch_summaries", "construction_file_summaries",
    "construction_workspace_summaries", "construction_transition_summaries",
    "construction_proof_consumptions",
]
INTENTIONAL_PHASE_FIELDS = {
    "construction_source_hash_bytes", "construction_source_hashes",
    "construction_canonical_commitment_bytes", "construction_canonical_commitment_entries",
    "construction_canonical_commitment_hashes",
}
STORAGE_FIELDS = [
    "sqlite_pre_logical_database_bytes", "sqlite_post_logical_database_bytes",
    "sqlite_pre_apparent_database_bytes", "sqlite_post_apparent_database_bytes",
    "sqlite_pre_allocated_database_bytes", "sqlite_post_allocated_database_bytes",
    "sqlite_pre_logical_store_bytes", "sqlite_post_logical_store_bytes",
    "sqlite_pre_apparent_store_bytes", "sqlite_post_apparent_store_bytes",
    "sqlite_pre_allocated_store_bytes", "sqlite_post_allocated_store_bytes",
    "physical_db_apparent_bytes", "physical_db_allocated_bytes",
    "physical_authority_sidecar_apparent_bytes", "physical_authority_sidecar_allocated_bytes",
    "physical_journal_apparent_bytes", "physical_journal_allocated_bytes",
    "physical_store_allocated_bytes",
]
PHASE_ORDER = ["same_open_authority", "canonical_cas_mapping", "precommit_closure", "sqlite_commit"]
MUTATION_SMOKE_PHASE_ORDER = PHASE_ORDER + [
    "fresh_reopen_head", "fresh_full_scrub", "reconstruction", "range_verification",
]
READ_SMOKE_PHASE_ORDER = ["fresh_reopen_head", "read_operation"]
MUTATION_RANGES = {
    "same-middle": [
        ("zero", 0, 0, 129, 1), ("first-byte", 0, 1, 22_286, 4),
        ("cross-chunk", 15_174, 15_176, 40_797, 5),
        ("leaf-boundary", 1_227_734, 1_227_736, 48_260, 6),
        ("branch-boundary", 81_445_751, 81_445_753, 45_639, 7),
        ("last-byte", 104_857_599, 104_857_600, 17_597, 4),
        ("eof", 104_857_600, 104_857_600, 129, 1),
    ],
    "plus1-early": [
        ("zero", 0, 0, 129, 1), ("first-byte", 0, 1, 7_112, 4),
        ("cross-chunk", 0, 2, 22_300, 5),
        ("leaf-boundary", 1_210_839, 1_210_841, 47_840, 6),
        ("branch-boundary", 81_428_808, 81_428_810, 43_278, 7),
        ("last-byte", 104_857_600, 104_857_601, 17_665, 4),
        ("eof", 104_857_601, 104_857_601, 129, 1),
    ],
    "plus1-middle": [
        ("zero", 0, 0, 129, 1), ("first-byte", 0, 1, 22_286, 4),
        ("cross-chunk", 15_174, 15_176, 40_797, 5),
        ("leaf-boundary", 1_227_734, 1_227_736, 48_260, 6),
        ("branch-boundary", 81_428_808, 81_428_810, 43_278, 7),
        ("last-byte", 104_857_600, 104_857_601, 17_665, 4),
        ("eof", 104_857_601, 104_857_601, 129, 1),
    ],
}
PHASE_NUMERIC_FIELDS = {
    "identity_bytes_hashed", "raw_bytes_hashed", "raw_hashes", "canonical_id_bytes_hashed",
    "canonical_id_hashes", "canonical_bytes_authenticated", "canonical_new_write_bytes",
    "canonical_authenticated_nonnew_bytes", "canonical_authentication_hash_bytes",
    "canonical_authentication_hashes", "reused_object_id_authentications",
    "reused_object_id_authentication_bytes", "borrowed_bytes_encode_calls",
    "borrowed_bytes_encode_input_bytes", "borrowed_source_encode_calls",
    "borrowed_source_encode_input_bytes", "objects_created", "objects_reused",
    "objects_authenticated", "statement_cache_acquisitions", "sql_query_calls",
    "sql_execute_calls", "sql_rows_returned", "sql_rows_changed", "row_blob_reads",
    "row_blob_writes", "row_blob_copy_bytes", "borrowed_row_blob_reads",
    "borrowed_row_blob_bytes", "incremental_blob_opens", "incremental_blob_reads",
    "incremental_blob_writes", "leaf_batch_queries", "leaf_batch_references",
    "leaf_batch_references_max", "leaf_batch_query_bytes_max", "commits", "references",
    "pages", "branches", "incremental_qualification_calls",
    "incremental_prior_spine_objects_authenticated", "incremental_prior_spine_bytes_authenticated",
    "incremental_replacement_spine_objects_authenticated",
    "incremental_replacement_spine_bytes_authenticated", "incremental_receipt_covered_edges",
    "incremental_new_or_different_edges", "incremental_new_subtree_objects_authenticated",
    "incremental_new_subtree_bytes_authenticated", "construction_put_evidences",
    "construction_edges_covered", "construction_leaf_summaries", "construction_branch_summaries",
    "construction_file_summaries", "construction_workspace_summaries",
    "construction_transition_summaries", "construction_proof_consumptions",
    "construction_source_hash_bytes", "construction_source_hashes", "construction_cdc_entries",
}


def read_jsonl(path):
    raw = Path(path).read_bytes()
    return raw, [json.loads(line) for line in raw.decode().splitlines() if line.strip()]


def frozen_ids():
    values = [PROFILE, FIXTURE_SHA, SOURCE_FINGERPRINT, CDC_SEQUENCE, ROOT, TRANSITION,
              CLOSURE, *EXECUTABLES.values(), *SOURCES.values()]
    for identity in SMOKE_IDENTITIES.values():
        values.extend(identity)
    values.extend(native[3] for native in SMOKE_NATIVE.values())
    return values


def verify_cp0009_smoke_constants(cp_path):
    cp_path = Path(cp_path)
    reasons = []
    add(reasons, cp_path.is_file(), "evidence:cp0009:missing")
    if not cp_path.is_file():
        return reasons
    cp_rows = [json.loads(line) for line in cp_path.read_text().splitlines() if line.strip()]
    for operation, name in {
        "edit-same": "same-middle",
        "edit-plus1-early": "plus1-early",
        "edit-plus1-middle": "plus1-middle",
    }.items():
        identity = SMOKE_IDENTITIES[name]
        references, sequence = SMOKE_NATIVE[name][2:]
        matches = [row for row in cp_rows if row.get("operation") == operation
                   and row.get("size_bytes") == 104_857_600]
        add(reasons, bool(matches) and all((row.get("root_id"), row.get("transition_id"),
            row.get("ordered_closure_digest")) == identity
            and row.get("expected_cdc_references") == references
            and row.get("actual_cdc_references") == references
            and row.get("expected_cdc_sequence_fingerprint") == sequence
            for row in matches),
            f"evidence:cp0009:{operation}")
    return reasons


def audit_evidence(result_dir, rows, smoke):
    result_dir = Path(result_dir)
    reasons = []
    required = {
        "attempt": result_dir / "SCREEN-ATTEMPT-v1.txt",
        "status": result_dir / "RUN-STATUS-v1.txt",
        "lock": result_dir / "LOCK-TIMEOUT-v1.txt",
        "schedule": result_dir / "SCHEDULE-ASSERTION-EXECUTION-v1.txt",
        "custody": result_dir / "SCREEN-INPUT-CUSTODY-v1.tsv",
        "starts": result_dir / "ROW-STARTS-v1.txt",
        "command": result_dir / "COMMAND-v1.txt",
        "environment": result_dir / "ENVIRONMENT-v1.txt",
        "quiescence": result_dir / "QUIESCENCE-v1.txt",
        "quiescence_conflicts": result_dir / "QUIESCENCE-CONFLICTS-v1.txt",
        "execution_custody": result_dir / "EXECUTION-CUSTODY-RECHECK-v1.txt",
        "smoke_result": result_dir / "PROTECTED-SMOKE-RESULT-v1.txt",
    }
    for name, path in required.items():
        add(reasons, path.is_file(), f"evidence:{name}:missing")
    if reasons:
        return reasons
    attempt = required["attempt"].read_text()
    status_text = required["status"].read_text().strip()
    lock = required["lock"].read_text()
    schedule = required["schedule"].read_text()
    status = dict(token.split("=", 1) for token in status_text.split()
                  if token.count("=") == 1)
    add(reasons, re.fullmatch(r"attempt=1 started_utc=\S+ command=.+ --execute\n", attempt) is not None,
        "evidence:attempt")
    expected_status = {
        "status": "PASS", "timeout": "false", "screen_executed_exactly_once": "true",
        "warmup_rows": "2", "measured_rows": "6", "total_rows": "8",
        "protected_smoke_rows": "7",
    }
    for key, value in expected_status.items():
        add(reasons, status.get(key) == value, f"evidence:run-status:{key}")
    add(reasons, set(status) == set(expected_status) | {"wall_seconds"}
        and status.get("wall_seconds", "").isdigit()
        and int(status["wall_seconds"]) <= 120, "evidence:run-status:shape")
    for token in ("BENCHMARK_LOCK=H05_SCREEN", "complete_screen_wall_ceiling_seconds=120",
                  "lock_acquired_utc=", "lock_released_utc="):
        add(reasons, token in lock, f"evidence:lock:{token}")
    lock_paths = [line.removeprefix("lock_path=") for line in lock.splitlines()
                  if line.startswith("lock_path=")]
    add(reasons, len(lock_paths) == 1 and not Path(lock_paths[0]).exists(),
        "evidence:lock:released")
    expected_schedule = (
        "constructed plan:\npair 0  warmup   AB\npair 1  measured AB\n"
        "pair 2  measured BA\npair 3  measured AB\nexpected plan:\n"
        "pair 0  warmup   AB\npair 1  measured AB\npair 2  measured BA\n"
        "pair 3  measured AB\nschedule assertion: PASS\n"
        "row sequence: A B | A B | B A | A B\n"
    )
    add(reasons, schedule == expected_schedule, "evidence:schedule")
    add(reasons, required["command"].read_text().endswith("run-screen.sh --execute\n"),
        "evidence:command")
    environment = required["environment"].read_text()
    add(reasons, "branch=codex/empty-worktree\n" in environment
        and "head=febc20f046bba84ccdce1256363d77799eabf2db\n" in environment,
        "evidence:environment")
    add(reasons, required["quiescence"].read_text().startswith("quiescence=PASS ")
        and required["quiescence_conflicts"].read_bytes() == b"", "evidence:quiescence")
    execution_custody = required["execution_custody"].read_text()
    for digest in (*EXECUTABLES.values(), *SOURCES.values(), FIXTURE_SHA):
        add(reasons, digest in execution_custody, f"evidence:execution-custody:{digest[:8]}")
    add(reasons, required["smoke_result"].read_text()
        == "protected_smoke=PASS operations=7 gate=correctness/resource/non-controlling\n",
        "evidence:smoke-result")

    lines = required["custody"].read_text().splitlines()
    header = "scope\tpair\tarm\toperation\tsource_sha256\tbase_database_sha256\tbase_authority_sha256\tbase_expectations_sha256\texpectations_version\tcanonical_commitment"
    add(reasons, bool(lines) and lines[0] == header and len(lines) == 16,
        "evidence:custody:shape")
    custody = []
    for line in lines[1:]:
        values = line.split("\t")
        if len(values) != 10:
            reasons.append("evidence:custody:columns")
            continue
        custody.append(dict(zip(header.split("\t"), values)))
    screen_custody = {(int(item["pair"]), item["arm"]): item for item in custody
                      if item["scope"] == "screen"}
    smoke_custody = [item for item in custody if item["scope"] == "smoke"]
    add(reasons, len(screen_custody) == 8 and len(smoke_custody) == 7,
        "evidence:custody:membership")
    for row in rows:
        item = screen_custody.get((row.get("screen_pair"), row.get("screen_arm")))
        if item is None:
            reasons.append("evidence:custody:screen-row")
            continue
        commitment = item["canonical_commitment"]
        add(reasons, item["source_sha256"] == row.get("screen_source_sha256")
            and item["base_database_sha256"] == row.get("screen_base_database_sha256")
            and item["base_authority_sha256"] == row.get("screen_base_authority_sha256")
            and item["base_expectations_sha256"] == row.get("screen_base_expectations_sha256")
            and item["expectations_version"] == row.get("screen_expectations_version")
            and ((commitment == "-" and row.get("screen_canonical_commitment") is None)
                 or commitment == row.get("screen_canonical_commitment")),
            f"evidence:custody:screen:{row.get('screen_pair')}:{row.get('screen_arm')}")
        if row.get("screen_arm") == "B":
            add(reasons, HEX64(commitment) is not None,
                f"evidence:custody:commitment:{row.get('screen_pair')}")
        else:
            add(reasons, commitment == "-" and row.get("screen_canonical_commitment") is None,
                f"evidence:custody:control-commitment:{row.get('screen_pair')}")
    for row, item in zip(smoke, smoke_custody):
        add(reasons, item["operation"] == row.get("screen_smoke_operation")
            and item["source_sha256"] == row.get("screen_source_sha256")
            and item["base_database_sha256"] == row.get("screen_base_database_sha256")
            and item["base_authority_sha256"] == row.get("screen_base_authority_sha256")
            and item["base_expectations_sha256"] == row.get("screen_base_expectations_sha256")
            and item["expectations_version"] == row.get("screen_expectations_version")
            and item["canonical_commitment"] == row.get("screen_canonical_commitment")
            and HEX64(item["canonical_commitment"]) is not None,
            f"evidence:custody:smoke:{row.get('screen_smoke_operation')}")

    starts = required["starts"].read_text().splitlines()
    parsed_starts = []
    pattern = re.compile(
        r"row_(started|completed)_utc=\S+ scope=(smoke|screen) pair=(-?\d+) arm=([AB]) operation=(\S+)$")
    for line in starts:
        match = pattern.fullmatch(line)
        if not match:
            reasons.append("evidence:row-starts:syntax")
            continue
        parsed_starts.append((match.group(1), match.group(2), int(match.group(3)),
                              match.group(4), match.group(5)))
    expected_starts = []
    for row in smoke:
        item = ("smoke", -1, "B", row.get("screen_smoke_operation"))
        expected_starts.extend([("started", *item), ("completed", *item)])
    for row in rows:
        item = ("screen", row.get("screen_pair"), row.get("screen_arm"), "full")
        expected_starts.extend([("started", *item), ("completed", *item)])
    add(reasons, parsed_starts == expected_starts and len(parsed_starts) == 30,
        "evidence:row-starts:order-completeness")

    artifact = result_dir.parent
    cp_path = artifact / "control/cp-0009.raw.jsonl"
    reasons.extend(verify_cp0009_smoke_constants(cp_path))
    return reasons


def phase(row, name):
    matches = [item for item in row.get("phase_counters", []) if item.get("phase") == name]
    return matches[0] if len(matches) == 1 else {}


def normalized_phase_counters(row):
    counters = copy.deepcopy(row.get("phase_counters", []))
    for counter in counters:
        for key in INTENTIONAL_PHASE_FIELDS:
            counter.pop(key, None)
    return counters


def add(reasons, condition, reason):
    if not condition:
        reasons.append(reason)


def has_ints(value, fields):
    return isinstance(value, dict) and all(
        isinstance(value.get(field), int) and not isinstance(value.get(field), bool)
        and value[field] >= 0
        for field in fields
    )


def native_envelope(row, operation):
    return (
        row.get("schema") == "phase4-current-baseline-v1"
        and row.get("acceptance_scope") == "baseline"
        and row.get("candidate_comparison") is False
        and row.get("measurement_boundary") == MEASUREMENT_BOUNDARIES[operation]
        and row.get("runner_sha256") == NATIVE_RUNNER_SHA
        and row.get("runner_wall_ceiling_seconds") == 120
        and row.get("runner_command_ceiling_seconds") == 60
        and row.get("cpu_scope") == CPU_SCOPE
        and row.get("cache_scope") == CACHE_SCOPE
    )


def phase_schema_contract(row, expected_order, screen_arm=None):
    counters = row.get("phase_counters")
    if not isinstance(counters, list) or [item.get("phase") for item in counters
                                          if isinstance(item, dict)] != expected_order:
        return False
    for counter in counters:
        if not has_ints(counter, PHASE_NUMERIC_FIELDS):
            return False
        if counter.get("native_sqlite_prepare_calls") != "U_NATIVE_PREP" \
                or counter.get("other_heap_copy_bytes") != "U_HEAP":
            return False
        phase_name = counter["phase"]
        commitment = [counter.get(key) for key in (
            "construction_canonical_commitment_bytes",
            "construction_canonical_commitment_entries",
            "construction_canonical_commitment_hashes",
        )]
        if screen_arm == "A":
            if any(key in counter for key in (
                "construction_canonical_commitment_bytes",
                "construction_canonical_commitment_entries",
                "construction_canonical_commitment_hashes",
            )):
                return False
            expected_source = (104_857_600, 1) if phase_name == "canonical_cas_mapping" else (0, 0)
            if (counter["construction_source_hash_bytes"],
                    counter["construction_source_hashes"]) != expected_source:
                return False
        elif screen_arm == "B":
            references = row.get("expected_cdc_references")
            expected_commitment = ((36 * references, references, 1)
                                   if phase_name == "canonical_cas_mapping"
                                   and isinstance(references, int) else (0, 0, 0))
            if tuple(commitment) != expected_commitment:
                return False
            if (counter["construction_source_hash_bytes"],
                    counter["construction_source_hashes"]) != (0, 0):
                return False
        else:
            if tuple(commitment) != (0, 0, 0):
                return False
            if (counter["construction_source_hash_bytes"],
                    counter["construction_source_hashes"]) != (0, 0):
                return False
    return True


def per_row_equations(row, mutation):
    numeric = set(STORAGE_FIELDS) | {
        "allocated_store_delta_bytes", "sql_calls", "sql_query_calls", "sql_execute_calls",
    }
    if mutation:
        numeric |= {"sqlite_page_size_bytes", "sqlite_main_db_dirty_pages_written",
                    "sqlite_main_db_pager_write_bytes", "sqlite_cache_spill_pages"}
    if not has_ints(row, numeric):
        return False
    authority_apparent = row["physical_authority_sidecar_apparent_bytes"]
    authority_allocated = row["physical_authority_sidecar_allocated_bytes"]
    ok = (
        authority_apparent == 32
        and row["sqlite_pre_logical_store_bytes"]
        == row["sqlite_pre_logical_database_bytes"] + authority_apparent
        and row["sqlite_post_logical_store_bytes"]
        == row["sqlite_post_logical_database_bytes"] + authority_apparent
        and row["sqlite_pre_apparent_store_bytes"]
        == row["sqlite_pre_apparent_database_bytes"] + authority_apparent
        and row["sqlite_post_apparent_store_bytes"]
        == row["sqlite_post_apparent_database_bytes"] + authority_apparent
        and row["sqlite_pre_allocated_store_bytes"]
        == row["sqlite_pre_allocated_database_bytes"] + authority_allocated
        and row["sqlite_post_allocated_store_bytes"]
        == row["sqlite_post_allocated_database_bytes"] + authority_allocated
        and row["allocated_store_delta_bytes"]
        == row["sqlite_post_allocated_store_bytes"] - row["sqlite_pre_allocated_store_bytes"]
        and row["physical_db_apparent_bytes"] == row["sqlite_post_apparent_database_bytes"]
        and row["physical_db_allocated_bytes"] == row["sqlite_post_allocated_database_bytes"]
        and row["physical_store_allocated_bytes"] == row["sqlite_post_allocated_store_bytes"]
        and row["sql_calls"] == row["sql_query_calls"] + row["sql_execute_calls"]
        and row.get("commit_reconciliation_calls") == 0
        and row.get("commit_reconciliation_wall_ns") == 0
        and row.get("commit_reconciliation_timer_nested") is True
    )
    if mutation:
        ok = ok and (
            row["sqlite_main_db_pager_write_bytes"]
            == row["sqlite_main_db_dirty_pages_written"] * row["sqlite_page_size_bytes"]
            and
            row.get("commit_return_db_apparent_bytes") == row["physical_db_apparent_bytes"]
            and row.get("commit_return_journal_apparent_bytes") == 0
            and row.get("commit_return_authority_apparent_bytes") == authority_apparent
            and row.get("commit_return_db_allocated_bytes") == row["physical_db_allocated_bytes"]
            and row.get("commit_return_journal_allocated_bytes") == 0
            and row.get("commit_return_authority_allocated_bytes") == authority_allocated
        )
    else:
        ok = ok and all(row.get(key) == "Unavailable" for key in (
            "sqlite_page_size_bytes", "sqlite_main_db_dirty_pages_written",
            "sqlite_main_db_pager_write_bytes", "sqlite_cache_spill_pages",
            "commit_return_db_apparent_bytes", "commit_return_journal_apparent_bytes",
            "commit_return_authority_apparent_bytes", "commit_return_db_allocated_bytes",
            "commit_return_journal_allocated_bytes", "commit_return_authority_allocated_bytes",
        ))
    return ok


def read_timer_contract(row, operation):
    durable_keys = (
        "capture_publish_wall_ns", "canonical_cas_mapping_stage_wall_ns",
        "precommit_closure_validation_wall_ns", "sqlite_commit_durability_wall_ns",
        "durable_capture_total_wall_ns", "durable_phase_sum_ns",
        "commit_publish_call_wall_ns", "commit_dispatch_to_return_wall_ns",
        "commit_pre_and_post_dispatch_wall_ns", "commit_caller_wrapper_wall_ns",
        "commit_observation_sum_wall_ns",
    )
    if not all(row.get(key) == 0 for key in durable_keys):
        return False
    reopen = row.get("fresh_reopen_head_wall_ns")
    scrub = row.get("fresh_full_scrub_wall_ns")
    reconstruction = row.get("reconstruction_wall_ns")
    ranges = row.get("range_verification_wall_ns")
    if not all(isinstance(value, int) and value >= 0 for value in (reopen, scrub, reconstruction, ranges)):
        return False
    patterns = {
        "materialize-warm": (False, True, False),
        "materialize-fresh": (True, True, False),
        "read-range-1m": (False, False, True),
        "reopen": (True, False, False),
    }
    want_reopen, want_reconstruction, want_ranges = patterns[operation]
    lifecycle = reopen + scrub + reconstruction + ranges
    complete = row.get("complete_lifecycle_total_wall_ns")
    lifecycle_matches = complete == lifecycle
    return (
        scrub == 0
        and (reopen > 0) is want_reopen
        and (reconstruction > 0) is want_reconstruction
        and (ranges > 0) is want_ranges
        and row.get("lifecycle_phase_sum_ns") == lifecycle
        and complete == row.get("sqlite_qualification_wall_ns") == row.get("elapsed_wall_ns")
        and ((operation == "materialize-fresh" and complete > lifecycle)
             or (operation != "materialize-fresh" and lifecycle_matches))
        and row.get("durable_phase_sum_matches") is True
        and row.get("lifecycle_phase_sum_matches") is lifecycle_matches
        and row.get("commit_timer_equation_matches") is True
        and row.get("source_cdc_nested_in_mapping_stage") is False
        and row.get("precommit_includes_reconstruction") is False
    )


def range_contract(row, operation):
    ranges = row.get("range_measurements")
    if not isinstance(ranges, list):
        return False
    if operation in ("same-middle", "plus1-early", "plus1-middle"):
        expected = MUTATION_RANGES[operation]
        if len(ranges) != len(expected):
            return False
        for item, wanted in zip(ranges, expected):
            if not has_ints(item, ("start", "end", "wall_ns", "returned_bytes",
                                   "canonical_bytes_authenticated", "objects_authenticated")):
                return False
            label, start, end, authenticated, objects = wanted
            if item["wall_ns"] <= 0 or (
                item.get("label"), item["start"], item["end"], item["returned_bytes"],
                item["canonical_bytes_authenticated"], item["objects_authenticated"],
            ) != (label, start, end, end - start, authenticated, objects):
                return False
        return True
    if operation == "read-range-1m":
        if len(ranges) != 1 or not isinstance(ranges[0], dict):
            return False
        item = ranges[0]
        return (
            item.get("label") == "sequential-1m"
            and item.get("start") == 51_904_512 and item.get("end") == 52_953_088
            and item.get("returned_bytes") == 1_048_576
            and isinstance(item.get("wall_ns"), int) and item["wall_ns"] > 0
            and isinstance(item.get("canonical_bytes_authenticated"), int)
            and item["canonical_bytes_authenticated"] == 1_090_255
            and item.get("objects_authenticated") == 60
        )
    return ranges == []


def mutation_contract(row):
    return (
        row.get("transactions"), row.get("commits"), row.get("commit_dispatches"),
        row.get("commit_returns"), row.get("commit_return_successes"),
        row.get("commit_return_errors"), row.get("commit_return_status"),
        row.get("publication_status"),
    ) == (1, 1, 1, 1, 1, 0, "ok", "Committed")


def timer_contract(row, complete_roundtrip=False):
    durable = row.get("durable_capture_total_wall_ns")
    durable_sum = sum(row.get(key, -1) for key in (
        "canonical_cas_mapping_stage_wall_ns", "precommit_closure_validation_wall_ns",
        "sqlite_commit_durability_wall_ns",
    ))
    commit_sum = sum(row.get(key, -1) for key in (
        "commit_dispatch_to_return_wall_ns", "commit_pre_and_post_dispatch_wall_ns",
        "commit_caller_wrapper_wall_ns",
    ))
    ok = (
        durable == row.get("capture_publish_wall_ns")
        and durable == durable_sum == row.get("durable_phase_sum_ns")
        and row.get("durable_phase_sum_matches") is True
        and row.get("commit_observation_sum_wall_ns") == row.get("sqlite_commit_durability_wall_ns")
        and commit_sum == row.get("sqlite_commit_durability_wall_ns")
        and row.get("commit_publish_call_wall_ns")
        == row.get("commit_dispatch_to_return_wall_ns") + row.get("commit_pre_and_post_dispatch_wall_ns")
        and row.get("sqlite_commit_durability_wall_ns")
        == row.get("commit_publish_call_wall_ns") + row.get("commit_caller_wrapper_wall_ns")
        and row.get("commit_timer_equation_matches") is True
        and row.get("source_cdc_nested_in_mapping_stage") is True
        and row.get("precommit_includes_reconstruction") is False
    )
    if complete_roundtrip:
        lifecycle_sum = durable + sum(row.get(key, -1) for key in (
            "fresh_reopen_head_wall_ns", "fresh_full_scrub_wall_ns",
            "reconstruction_wall_ns", "range_verification_wall_ns",
        ))
        ok = (ok and row.get("complete_lifecycle_total_wall_ns") == lifecycle_sum
              == row.get("lifecycle_phase_sum_ns")
              == row.get("sqlite_qualification_wall_ns") == row.get("elapsed_wall_ns")
              and row.get("lifecycle_phase_sum_matches") is True)
    else:
        ok = ok and all(row.get(key) == 0 for key in (
            "fresh_reopen_head_wall_ns", "fresh_full_scrub_wall_ns",
            "reconstruction_wall_ns", "range_verification_wall_ns",
        )) and row.get("complete_lifecycle_total_wall_ns") == durable \
            == row.get("lifecycle_phase_sum_ns") \
            == row.get("sqlite_qualification_wall_ns") == row.get("elapsed_wall_ns") \
            and row.get("lifecycle_phase_sum_matches") is True
    return ok


def resource_storage_contract(row):
    ext = row.get("external_time", {})
    measurement = row.get("measurement_status", {})
    instrumentation = row.get("instrumentation", {})
    storage = (
        row.get("sqlite_runtime_journal_mode") == "delete"
        and row.get("sqlite_runtime_synchronous") == 2
        and row.get("sqlite_runtime_temp_store") == 1
        and row.get("sqlite_runtime_mmap_size") == 0
        and row.get("physical_journal_apparent_bytes") == 0
        and row.get("physical_journal_allocated_bytes") == 0
        and row.get("physical_authority_sidecar_apparent_bytes") == 32
        and row.get("physical_store_allocated_bytes")
        == row.get("physical_db_allocated_bytes", -1)
        + row.get("physical_authority_sidecar_allocated_bytes", -2)
        and row.get("screen_residue") == []
    )
    q = (
        row.get("q_equation") == Q_EQUATION and row.get("q_current") == 0
        and row.get("q_current_semantics") == "after_report_output_drop"
        and row.get("q_fixed_envelope_removed") is True
        and isinstance(row.get("q_high_water"), int) and row["q_high_water"] >= 0
        and isinstance(row.get("q_report_output_bytes"), int) and row["q_report_output_bytes"] > 0
        and all(isinstance(row.get(key), int) and row[key] >= 0 for key in Q_COMPONENTS)
        and row.get("leaf_batch_bound") == 64
    )
    external = all(isinstance(ext.get(key), (int, float)) and ext[key] >= 0 for key in (
        "user_seconds", "system_seconds", "maximum_resident_set_bytes",
        "peak_memory_footprint_bytes",
    ))
    unavailable = all(row.get(key) == value for key, value in UNAVAILABLE.items())
    measurement_contract = (
        all(measurement.get(key) == "O" for key in (
            "phase_counters", "identity_hash_bytes", "borrowed_bytes_encoding",
            "object_id_authentication_reuse", "logical_q", "w_d", "row_blob_copies",
            "borrowed_row_blob_path", "incremental_blob_api",
        ))
        and measurement.get("sqlite_page_cache")
        == ("O" if row.get("transactions") == 1 else "U_STATUS_API")
        and measurement.get("cpu_rss") == "O_EXT"
        and measurement.get("sqlite_page_cache_true_high_water") == "U_CACHE_HWM"
        and measurement.get("dirty_pages_current") == "U_DIRTY_CUR"
        and measurement.get("main_db_io_calls_bytes") == "U_VFS_IO"
        and measurement.get("journal_io_calls_bytes") == "U_VFS_IO"
        and measurement.get("sync_calls_wall") == "U_VFS_SYNC"
        and measurement.get("journal_true_peak") == "U_JRN_PEAK"
        and measurement.get("temporary_file_peak") == "U_TMP_PEAK"
        and measurement.get("host_physical_io_bytes") == "U_PHYS_BYTES"
        and measurement.get("other_heap_copy_bytes") == "U_HEAP"
        and measurement.get("query_plans") == "U_PLAN"
        and row.get("native_sqlite_prepare_calls") == "U_NATIVE_PREP"
        and row.get("query_plans") == "U_PLAN"
        and row.get("busy_events") == row.get("locked_events") == "Unavailable"
        and row.get("blob_api_status") == "O"
        and row.get("process_io") == "MIXED_IO"
        and row.get("physical_io_cache_sync_temp_journal_status") == "MIXED_IO"
        and row.get("measurement_status_schema") == "f1-v2-status-codes-v1"
        and instrumentation.get("c") == "O"
        and isinstance(instrumentation.get("sql"), list)
        and len(instrumentation["sql"]) == 8
        and all(isinstance(value, int) and value >= 0 for value in instrumentation["sql"])
        and isinstance(instrumentation.get("status"), list)
        and len(instrumentation["status"]) == 6
        and all(isinstance(value, int) and value >= 0 for value in instrumentation["status"])
        and instrumentation["status"][-1] == 0
    )
    return storage and q and external and unavailable and measurement_contract


def analyze_rows(rows, smoke, evidence_reasons=()):
    reasons = list(evidence_reasons)
    actual_plan = [(
        row.get("screen_pair"), row.get("screen_sample_kind"),
        row.get("screen_arm"), row.get("screen_order"),
    ) for row in rows]
    add(reasons, len(rows) == 8, "schedule:row-count")
    add(reasons, actual_plan == PLAN, "schedule:order")
    add(reasons, sum(row.get("screen_sample_kind") == "warmup" for row in rows) == 2,
        "schedule:warmup-count")
    add(reasons, sum(row.get("screen_sample_kind") == "measured" for row in rows) == 6,
        "schedule:measured-count")

    commitments = set()
    for index, row in enumerate(rows):
        prefix = f"row:{index}"
        arm = row.get("screen_arm")
        add(reasons, arm in EXECUTABLES, f"{prefix}:arm")
        if arm not in EXECUTABLES:
            continue
        add(reasons, row.get("status") == "PASS" and row.get("error") is None,
            f"{prefix}:semantic-status")
        add(reasons, row.get("screen_schema") == "h05-private-screen-row-v1"
            and row.get("qualification") is False and row.get("promotion") is False
            and row.get("rejection") is False
            and row.get("throughput_measurement_admissible") is False
            and row.get("purpose") == "profile_selection" and row.get("milestone") == "WP4-M"
            and row.get("candidate") == "K64-F64"
            and row.get("fixture") == "S1-100"
            and row.get("fixture_manifest") == "wp4m-retained-fixture-manifest.json"
            and row.get("qualification_mode") == "C1-construction-proof"
            and row.get("warmup") is (row.get("screen_sample_kind") == "warmup")
            and row.get("screen_smoke_operation") is None
            and row.get("source_cache_state") == "warm_or_unknown_after_manifest_preflight"
            and row.get("store_state") == "fresh_logical_store_cache_unknown",
            f"{prefix}:native-row-contract")
        add(reasons, row.get("operation") == "full" and row.get("size_bytes") == 104_857_600
            and row.get("input_size_bytes") == 104_857_600 and row.get("directory_entries") == 0,
            f"{prefix}:operand")
        add(reasons, native_envelope(row, "full"), f"{prefix}:native-envelope")
        add(reasons, row.get("profile_id") == PROFILE and row.get("build_profile") == "release"
            and row.get("debug_assertions") is False, f"{prefix}:profile")
        add(reasons, row.get("executable_sha256") == EXECUTABLES[arm]
            and row.get("screen_executable_sha256") == EXECUTABLES[arm]
            and row.get("screen_executable_source_sha256") == SOURCES[arm],
            f"{prefix}:executable-custody")
        add(reasons, row.get("screen_source_sha256") == FIXTURE_SHA
            and row.get("screen_source_size") == 104_857_600,
            f"{prefix}:fixture-custody")
        add(reasons, row.get("screen_expectations_version") == EXPECTATION_VERSIONS[arm],
            f"{prefix}:expectations-version")
        add(reasons, row.get("pre_edit_database_sha256") == row.get("screen_base_database_sha256")
            and row.get("pre_edit_authority_sha256") == row.get("screen_base_authority_sha256")
            and row.get("pre_edit_expectations_sha256") == row.get("screen_base_expectations_sha256"),
            f"{prefix}:base-custody")
        add(reasons, row.get("base_preparation_in_measured_interval") is False
            and row.get("base_copy_method") == "physical-byte-copy-identical-database-authority-expectations",
            f"{prefix}:preparation-boundary")
        add(reasons, (
            row.get("source_fingerprint"), row.get("expected_cdc_references"),
            row.get("expected_cdc_sequence_fingerprint"), row.get("actual_cdc_references"),
            row.get("root_id"), row.get("transition_id"), row.get("ordered_closure_digest"),
        ) == (SOURCE_FINGERPRINT, 5284, CDC_SEQUENCE, 5284, ROOT, TRANSITION, CLOSURE),
            f"{prefix}:identity")
        add(reasons, mutation_contract(row), f"{prefix}:transaction-commit")
        add(reasons, timer_contract(row), f"{prefix}:timers")
        add(reasons, resource_storage_contract(row), f"{prefix}:resource-storage")
        add(reasons, per_row_equations(row, True), f"{prefix}:equations")
        add(reasons, has_ints(row, EQUAL_WORK), f"{prefix}:required-work-fields")
        add(reasons, phase_schema_contract(row, PHASE_ORDER, arm), f"{prefix}:phase-schema")
        add(reasons, (
            row.get("raw_bytes_hashed"), row.get("raw_hashes"),
            row.get("canonical_id_bytes_hashed"), row.get("canonical_id_hashes"),
            row.get("canonical_new_write_bytes"), row.get("mapping_bytes_rewritten"),
        ) == (104_857_600, 5284, 105_291_554, 5372, 105_291_554, 365_262),
            f"{prefix}:unchanged-work")
        counters = phase(row, "canonical_cas_mapping")
        if arm == "A":
            add(reasons, row.get("screen_canonical_commitment") is None,
                f"{prefix}:control-commitment")
            add(reasons, counters.get("construction_source_hash_bytes") == 104_857_600
                and counters.get("construction_source_hashes") == 1,
                f"{prefix}:control-counters")
        else:
            commitment = row.get("screen_canonical_commitment")
            add(reasons, isinstance(commitment, str) and HEX64(commitment) is not None,
                f"{prefix}:prepared-commitment-format")
            if isinstance(commitment, str) and HEX64(commitment) is not None:
                commitments.add(commitment)
            add(reasons, (
                counters.get("construction_source_hash_bytes"),
                counters.get("construction_source_hashes"),
                counters.get("construction_canonical_commitment_bytes"),
                counters.get("construction_canonical_commitment_entries"),
                counters.get("construction_canonical_commitment_hashes"),
                counters.get("construction_cdc_entries"),
            ) == (0, 0, 190_224, 5284, 1, 5284), f"{prefix}:candidate-counters")

    add(reasons, len(commitments) == 1, "candidate:prepared-commitment")
    for pair in range(4):
        group = [row for row in rows if row.get("screen_pair") == pair]
        if len(group) != 2 or {row.get("screen_arm") for row in group} != {"A", "B"}:
            reasons.append(f"pair:{pair}:membership")
            continue
        a, b = (next(row for row in group if row["screen_arm"] == arm) for arm in ("A", "B"))
        add(reasons, a.get("screen_base_database_sha256") == b.get("screen_base_database_sha256")
            and a.get("screen_base_authority_sha256") == b.get("screen_base_authority_sha256"),
            f"pair:{pair}:byte-identical-start")
        add(reasons, a.get("screen_base_expectations_sha256") != b.get("screen_base_expectations_sha256"),
            f"pair:{pair}:versioned-expectations")
        add(reasons, a.get("screen_post_database_sha256") == b.get("screen_post_database_sha256")
            and a.get("screen_post_authority_sha256") == b.get("screen_post_authority_sha256"),
            f"pair:{pair}:durable-byte-equality")
        add(reasons, all(a.get(key) == b.get(key) for key in EQUAL_WORK),
            f"pair:{pair}:work-equality")
        add(reasons, normalized_phase_counters(a) == normalized_phase_counters(b),
            f"pair:{pair}:phase-counter-equality")
        add(reasons, b.get("q_high_water", -1) <= a.get("q_high_water", -2)
            and all(a.get(key) == b.get(key) for key in Q_COMPONENTS),
            f"pair:{pair}:q-nonincrease")
        add(reasons, all(a.get(key) == b.get(key) for key in STORAGE_FIELDS),
            f"pair:{pair}:storage-equality")

    add(reasons, len(smoke) == len(SMOKE_OPS), "smoke:row-count")
    add(reasons, [row.get("screen_smoke_operation") for row in smoke] == SMOKE_OPS,
        "smoke:order")
    for index, row in enumerate(smoke):
        op = row.get("screen_smoke_operation")
        prefix = f"smoke:{index}:{op}"
        native = SMOKE_NATIVE.get(op)
        add(reasons, row.get("status") == "PASS" and row.get("error") is None,
            f"{prefix}:status")
        add(reasons, native is not None and row.get("operation") == native[0]
            and row.get("qualification_mode") == native[1]
            and row.get("expected_cdc_references") == native[2]
            and row.get("actual_cdc_references") == native[2]
            and row.get("expected_cdc_sequence_fingerprint") == native[3],
            f"{prefix}:native-operation")
        add(reasons, op in MEASUREMENT_BOUNDARIES and native_envelope(row, op),
            f"{prefix}:native-envelope")
        add(reasons, row.get("screen_schema") == "h05-private-screen-row-v1"
            and row.get("screen_arm") == "B" and row.get("screen_pair") == -1
            and row.get("screen_sample_kind") == "smoke" and row.get("screen_order") == "NA"
            and row.get("size_bytes") == 104_857_600 and row.get("input_size_bytes") == 104_857_600
            and row.get("directory_entries") == 0 and row.get("warmup") is False
            and row.get("qualification") is False and row.get("promotion") is False
            and row.get("rejection") is False and row.get("throughput_measurement_admissible") is False
            and row.get("purpose") == "profile_selection" and row.get("milestone") == "WP4-M"
            and row.get("candidate") == "K64-F64" and row.get("profile_id") == PROFILE
            and row.get("fixture") == "S1-100"
            and row.get("fixture_manifest") == "wp4m-retained-fixture-manifest.json"
            and row.get("source_fingerprint") == SOURCE_FINGERPRINT
            and row.get("build_profile") == "release" and row.get("debug_assertions") is False
            and row.get("base_preparation_in_measured_interval") is False
            and row.get("base_copy_method") == "physical-byte-copy-identical-database-authority-expectations"
            and row.get("source_cache_state") == "warm_or_unknown_after_manifest_preflight"
            and row.get("store_state") == "fresh_logical_store_cache_unknown",
            f"{prefix}:native-row-contract")
        add(reasons, row.get("executable_sha256") == EXECUTABLES["B"]
            and row.get("screen_executable_sha256") == EXECUTABLES["B"]
            and row.get("screen_executable_source_sha256") == SOURCES["B"]
            and row.get("screen_source_sha256") == FIXTURE_SHA
            and row.get("screen_source_size") == 104_857_600
            and row.get("screen_expectations_version") == EXPECTATION_VERSIONS["B"]
            and isinstance(row.get("screen_canonical_commitment"), str)
            and HEX64(row["screen_canonical_commitment"]) is not None
            and row.get("pre_edit_database_sha256") == row.get("screen_base_database_sha256")
            and row.get("pre_edit_authority_sha256") == row.get("screen_base_authority_sha256")
            and row.get("pre_edit_expectations_sha256") == row.get("screen_base_expectations_sha256"),
            f"{prefix}:custody")
        expected_identity = SMOKE_IDENTITIES.get(op, (ROOT, TRANSITION, CLOSURE))
        add(reasons, (row.get("root_id"), row.get("transition_id"),
            row.get("ordered_closure_digest")) == expected_identity, f"{prefix}:identity")
        mutation = op in SMOKE_OPS[:3]
        add(reasons, resource_storage_contract(row), f"{prefix}:resource-storage")
        add(reasons, per_row_equations(row, mutation), f"{prefix}:equations")
        add(reasons, range_contract(row, op), f"{prefix}:ranges")
        if mutation:
            add(reasons, mutation_contract(row), f"{prefix}:transaction-commit")
            add(reasons, timer_contract(row, complete_roundtrip=True), f"{prefix}:timers")
            add(reasons, all(row.get(key, 0) > 0 for key in (
                "fresh_reopen_head_wall_ns", "fresh_full_scrub_wall_ns",
                "reconstruction_wall_ns", "range_verification_wall_ns",
            )), f"{prefix}:roundtrip")
            add(reasons, phase_schema_contract(row, MUTATION_SMOKE_PHASE_ORDER),
                f"{prefix}:phase-schema")
        else:
            add(reasons, (
                row.get("transactions"), row.get("commits"), row.get("commit_dispatches"),
                row.get("commit_returns"), row.get("commit_return_successes"),
                row.get("commit_return_errors"), row.get("commit_return_status"),
                row.get("publication_status"),
            ) == (0, 0, 0, 0, 0, 0, "NotApplicable", "Unavailable"),
                f"{prefix}:read-only")
            add(reasons, read_timer_contract(row, op), f"{prefix}:timers")
            add(reasons, phase_schema_contract(row, READ_SMOKE_PHASE_ORDER),
                f"{prefix}:phase-schema")

    pairs = []
    for pair in (1, 2, 3):
        group = [row for row in rows if row.get("screen_pair") == pair]
        if len(group) != 2:
            continue
        walls = {row.get("screen_arm"): row.get("durable_capture_total_wall_ns") for row in group}
        if set(walls) != {"A", "B"} or not all(isinstance(value, int) and value > 0 for value in walls.values()):
            reasons.append(f"pair:{pair}:wall")
            continue
        effect = walls["B"] - walls["A"]
        improvement = 100.0 * (walls["A"] - walls["B"]) / walls["A"]
        pairs.append({
            "pair": pair, "control_ns": walls["A"], "candidate_ns": walls["B"],
            "effect_ns": effect, "improvement_percent": improvement,
        })
    improvements = [pair["improvement_percent"] for pair in pairs]
    wins = sum(pair["effect_ns"] < 0 for pair in pairs)
    paired_median = statistics.median(improvements) if len(improvements) == 3 else None
    add(reasons, wins == 3, "performance:wins")
    add(reasons, paired_median is not None and paired_median >= 5.0,
        "performance:paired-median-threshold")
    reasons = sorted(set(reasons))
    measured = [row for row in rows if row.get("screen_sample_kind") == "measured"]
    return {
        "schema": "h05-private-screen-analysis-v1",
        "status": "PASS" if not reasons else "FAIL",
        "disposition": "RETAIN-FOR-FULL-CAMPAIGN" if not reasons else "REVERT / H05 LOCAL NO-GO",
        "reasons": reasons,
        "plan": [list(item) for item in actual_plan],
        "row_counts": {"warmup": len(rows) - len(measured), "measured": len(measured), "total": len(rows)},
        "protected_smoke": {"status": "PASS" if not any(reason.startswith("smoke:") for reason in reasons) else "FAIL",
                            "operations": [row.get("screen_smoke_operation") for row in smoke]},
        "prepared_canonical_commitment": next(iter(commitments), None) if len(commitments) == 1 else None,
        "pairs": pairs,
        "control_median_ns": statistics.median([row["durable_capture_total_wall_ns"] for row in measured if row.get("screen_arm") == "A"]) if len(measured) == 6 else None,
        "candidate_median_ns": statistics.median([row["durable_capture_total_wall_ns"] for row in measured if row.get("screen_arm") == "B"]) if len(measured) == 6 else None,
        "paired_median_improvement_percent": paired_median,
        "wins": wins,
        "effect_ns": {
            "min": min((pair["effect_ns"] for pair in pairs), default=None),
            "max": max((pair["effect_ns"] for pair in pairs), default=None),
            "spread": (max(pair["effect_ns"] for pair in pairs) - min(pair["effect_ns"] for pair in pairs)) if pairs else None,
        },
        "improvement_percent": {
            "min": min(improvements, default=None), "max": max(improvements, default=None),
            "spread": max(improvements) - min(improvements) if improvements else None,
        },
        "timer_components": [{key: row.get(key) for key in (
            "screen_pair", "screen_arm", "canonical_cas_mapping_stage_wall_ns",
            "precommit_closure_validation_wall_ns", "sqlite_commit_durability_wall_ns",
            "durable_capture_total_wall_ns",
        )} for row in rows],
        "resources": [{key: row.get(key) for key in (
            "screen_pair", "screen_arm", "q_high_water", "q_current", "external_time",
            "physical_db_apparent_bytes", "physical_db_allocated_bytes",
            "physical_authority_sidecar_apparent_bytes", "physical_authority_sidecar_allocated_bytes",
            "host_physical_io", "peak_journal_bytes", "peak_temporary_bytes",
        )} for row in rows],
        "limitations": [
            "Three measured pairs on one host and one retained fixture are a local kill screen, not a full campaign or portability claim.",
            "OS/filesystem cache state is warm-or-unknown; host physical I/O, fsync wall, true journal peak, and temporary-file peak are unavailable from the supported observations.",
            "CPU and RSS are whole-child observations; phase-local CPU attribution is unavailable.",
            "The prepared canonical commitment is custody evidence from the frozen candidate expectation oracle; row PASS is the frozen pre-COMMIT actual-versus-prepared comparison, not an independent implementation recomputation.",
            "A retained result authorizes only a separately preregistered full campaign and is not H05 product PASS.",
        ],
    }


def structured_failure(reason):
    return {
        "schema": "h05-private-screen-analysis-v1",
        "status": "FAIL",
        "disposition": "REVERT / H05 LOCAL NO-GO",
        "reasons": [reason],
        "row_counts": {"warmup": 0, "measured": 0, "total": 0},
        "protected_smoke": {"status": "FAIL", "operations": []},
        "pairs": [], "wins": 0, "paired_median_improvement_percent": None,
        "limitations": ["Malformed or incomplete evidence was rejected before statistics."],
    }


def safe_analyze_rows(rows, smoke, evidence_reasons=()):
    try:
        return analyze_rows(rows, smoke, evidence_reasons)
    except Exception as error:
        return structured_failure(f"malformed-evidence:{type(error).__name__}:{error}")


def synthetic_rows():
    base = {
        "status": "PASS", "error": None, "operation": "full", "size_bytes": 104_857_600,
        "input_size_bytes": 104_857_600, "directory_entries": 0,
        "screen_schema": "h05-private-screen-row-v1", "qualification": False,
        "promotion": False, "rejection": False, "throughput_measurement_admissible": False,
        "purpose": "profile_selection", "milestone": "WP4-M", "candidate": "K64-F64",
        "fixture": "S1-100", "fixture_manifest": "wp4m-retained-fixture-manifest.json",
        "qualification_mode": "C1-construction-proof", "warmup": False,
        "source_cache_state": "warm_or_unknown_after_manifest_preflight",
        "store_state": "fresh_logical_store_cache_unknown",
        "schema": "phase4-current-baseline-v1", "acceptance_scope": "baseline",
        "candidate_comparison": False, "measurement_boundary": "durable-submit",
        "runner_sha256": NATIVE_RUNNER_SHA, "runner_wall_ceiling_seconds": 120,
        "runner_command_ceiling_seconds": 60, "cpu_scope": CPU_SCOPE,
        "cache_scope": CACHE_SCOPE,
        "profile_id": PROFILE, "build_profile": "release", "debug_assertions": False,
        "screen_source_sha256": FIXTURE_SHA, "screen_source_size": 104_857_600,
        "source_fingerprint": SOURCE_FINGERPRINT, "expected_cdc_references": 5284,
        "expected_cdc_sequence_fingerprint": CDC_SEQUENCE, "actual_cdc_references": 5284,
        "root_id": ROOT, "transition_id": TRANSITION, "ordered_closure_digest": CLOSURE,
        "base_preparation_in_measured_interval": False,
        "base_copy_method": "physical-byte-copy-identical-database-authority-expectations",
        "transactions": 1, "commits": 1,
        "commit_dispatches": 1, "commit_returns": 1, "commit_return_successes": 1,
        "commit_return_errors": 0, "commit_return_status": "ok", "publication_status": "Committed",
        "commit_reconciliation_calls": 0, "commit_reconciliation_wall_ns": 0,
        "commit_reconciliation_timer_nested": True,
        "canonical_cas_mapping_stage_wall_ns": 80, "precommit_closure_validation_wall_ns": 1,
        "sqlite_commit_durability_wall_ns": 19, "durable_capture_total_wall_ns": 100,
        "capture_publish_wall_ns": 100, "sqlite_qualification_wall_ns": 100,
        "elapsed_wall_ns": 100, "durable_phase_sum_ns": 100,
        "durable_phase_sum_matches": True, "commit_dispatch_to_return_wall_ns": 17,
        "commit_pre_and_post_dispatch_wall_ns": 1, "commit_publish_call_wall_ns": 18,
        "commit_caller_wrapper_wall_ns": 1,
        "commit_observation_sum_wall_ns": 19, "commit_timer_equation_matches": True,
        "source_cdc_nested_in_mapping_stage": True, "precommit_includes_reconstruction": False,
        "fresh_reopen_head_wall_ns": 0, "fresh_full_scrub_wall_ns": 0,
        "reconstruction_wall_ns": 0, "range_verification_wall_ns": 0,
        "complete_lifecycle_total_wall_ns": 100, "lifecycle_phase_sum_ns": 100,
        "lifecycle_phase_sum_matches": True, "raw_bytes_hashed": 104_857_600,
        "raw_hashes": 5284, "canonical_id_bytes_hashed": 105_291_554,
        "canonical_id_hashes": 5372, "canonical_new_write_bytes": 105_291_554,
        "mapping_bytes_rewritten": 365_262, "objects_created": 5372, "references": 5284,
        "pages": 83, "branches": 2, "chunks": 5284, "construction_cdc_entries": 5284,
        "construction_put_evidences": 5372, "construction_edges_covered": 5371,
        "construction_leaf_summaries": 83, "construction_branch_summaries": 2,
        "construction_file_summaries": 1, "construction_workspace_summaries": 1,
        "construction_transition_summaries": 1, "construction_proof_consumptions": 1,
        "q_equation": Q_EQUATION, "q_current": 0, "q_high_water": 90_000,
        "q_current_semantics": "after_report_output_drop", "q_report_output_bytes": 80_000,
        "q_cdc_base_live_bytes": 0, "q_cdc_old_window_bytes": 0,
        "q_cdc_scan_input_bytes": 0, "q_cdc_overlap_current": 0,
        "q_cdc_old_chunk_slots_bytes": 0, "q_fixed_envelope_removed": True,
        "leaf_batch_bound": 64, "leaf_batch_queries": 0, "leaf_batch_references": 0,
        "leaf_batch_references_max": 0, "leaf_batch_query_bytes_max": 0,
        "sqlite_runtime_journal_mode": "delete", "sqlite_runtime_synchronous": 2,
        "sqlite_runtime_temp_store": 1, "sqlite_runtime_mmap_size": 0,
        "sqlite_pre_logical_database_bytes": 1000, "sqlite_post_logical_database_bytes": 2000,
        "sqlite_pre_apparent_database_bytes": 1000, "sqlite_post_apparent_database_bytes": 2000,
        "sqlite_pre_allocated_database_bytes": 4096, "sqlite_post_allocated_database_bytes": 8192,
        "sqlite_pre_logical_store_bytes": 1032, "sqlite_post_logical_store_bytes": 2032,
        "sqlite_pre_apparent_store_bytes": 1032, "sqlite_post_apparent_store_bytes": 2032,
        "sqlite_pre_allocated_store_bytes": 8192, "sqlite_post_allocated_store_bytes": 12288,
        "allocated_store_delta_bytes": 4096,
        "physical_db_apparent_bytes": 2000, "physical_db_allocated_bytes": 8192,
        "physical_journal_apparent_bytes": 0, "physical_journal_allocated_bytes": 0,
        "physical_authority_sidecar_apparent_bytes": 32,
        "physical_authority_sidecar_allocated_bytes": 4096,
        "physical_store_allocated_bytes": 12288,
        "commit_return_db_apparent_bytes": 2000, "commit_return_journal_apparent_bytes": 0,
        "commit_return_authority_apparent_bytes": 32,
        "commit_return_db_allocated_bytes": 8192, "commit_return_journal_allocated_bytes": 0,
        "commit_return_authority_allocated_bytes": 4096,
        "sqlite_page_size_bytes": 4096, "sqlite_main_db_dirty_pages_written": 2,
        "sqlite_main_db_pager_write_bytes": 8192, "sqlite_cache_spill_pages": 0,
        "sql_query_calls": 4, "sql_execute_calls": 5, "sql_calls": 9,
        "screen_residue": [], "external_time": {"user_seconds": 1.0, "system_seconds": 0.1,
            "maximum_resident_set_bytes": 1000, "peak_memory_footprint_bytes": 900},
        "measurement_status": {
            "phase_counters": "O", "identity_hash_bytes": "O",
            "borrowed_bytes_encoding": "O", "object_id_authentication_reuse": "O",
            "logical_q": "O", "w_d": "O", "row_blob_copies": "O",
            "borrowed_row_blob_path": "O", "incremental_blob_api": "O",
            "sqlite_page_cache": "O", "cpu_rss": "O_EXT",
            "sqlite_page_cache_true_high_water": "U_CACHE_HWM",
            "dirty_pages_current": "U_DIRTY_CUR", "main_db_io_calls_bytes": "U_VFS_IO",
            "journal_io_calls_bytes": "U_VFS_IO", "sync_calls_wall": "U_VFS_SYNC",
            "journal_true_peak": "U_JRN_PEAK", "temporary_file_peak": "U_TMP_PEAK",
            "host_physical_io_bytes": "U_PHYS_BYTES",
            "other_heap_copy_bytes": "U_HEAP", "query_plans": "U_PLAN",
        },
        "native_sqlite_prepare_calls": "U_NATIVE_PREP", "query_plans": "U_PLAN",
        "busy_events": "Unavailable", "locked_events": "Unavailable",
        "blob_api_status": "O", "process_io": "MIXED_IO",
        "physical_io_cache_sync_temp_journal_status": "MIXED_IO",
        "measurement_status_schema": "f1-v2-status-codes-v1",
        "instrumentation": {"c": "O", "sql": [2, 2, 1, 1, 2, 2, 5, 5],
                            "status": [4, 5, 5, 5, 19, 0]},
        **UNAVAILABLE,
    }
    for key in EQUAL_WORK:
        base.setdefault(key, 0)
    for key in STORAGE_FIELDS:
        base.setdefault(key, 1000)

    def counter(phase_name):
        value = {field: 0 for field in PHASE_NUMERIC_FIELDS}
        value.update({"phase": phase_name, "native_sqlite_prepare_calls": "U_NATIVE_PREP",
                      "other_heap_copy_bytes": "U_HEAP"})
        return value

    rows = []
    for index, (pair, kind, arm, order) in enumerate(PLAN):
        row = copy.deepcopy(base)
        row.update({"screen_pair": pair, "screen_sample_kind": kind, "screen_arm": arm,
                    "screen_order": order, "warmup": kind == "warmup",
                    "executable_sha256": EXECUTABLES[arm],
                    "screen_executable_sha256": EXECUTABLES[arm],
                    "screen_executable_source_sha256": SOURCES[arm],
                    "screen_expectations_version": EXPECTATION_VERSIONS[arm],
                    "screen_base_database_sha256": f"db{pair}",
                    "screen_base_authority_sha256": f"auth{pair}",
                    "screen_base_expectations_sha256": f"exp{arm}{pair}",
                    "pre_edit_database_sha256": f"db{pair}", "pre_edit_authority_sha256": f"auth{pair}",
                    "pre_edit_expectations_sha256": f"exp{arm}{pair}",
                    "screen_post_database_sha256": f"post{pair}",
                    "screen_post_authority_sha256": f"postauth{pair}",
                    "screen_canonical_commitment": None if arm == "A" else "ab" * 32,
                    "screen_smoke_operation": None})
        phases = [counter(name) for name in PHASE_ORDER]
        canonical = phases[1]
        canonical.update({
            "raw_bytes_hashed": 104_857_600, "raw_hashes": 5284,
            "canonical_id_bytes_hashed": 105_291_554, "canonical_id_hashes": 5372,
            "canonical_bytes_authenticated": 105_291_554,
            "canonical_new_write_bytes": 105_291_554,
            "objects_created": 5372, "objects_authenticated": 5372,
            "statement_cache_acquisitions": 5372, "sql_execute_calls": 5373,
            "sql_rows_changed": 5372, "row_blob_writes": 10_744,
            "references": 5284, "pages": 83, "branches": 2,
            "construction_put_evidences": 5372, "construction_edges_covered": 5371,
            "construction_leaf_summaries": 83, "construction_branch_summaries": 2,
            "construction_file_summaries": 1, "construction_workspace_summaries": 1,
            "construction_transition_summaries": 1, "construction_cdc_entries": 5284,
        })
        phases[2]["construction_proof_consumptions"] = 1
        phases[3].update({"commits": 1, "sql_query_calls": 1, "sql_execute_calls": 2,
                          "sql_rows_changed": 1, "row_blob_writes": 4})
        if arm == "A":
            canonical.update({"construction_source_hash_bytes": 104_857_600,
                              "construction_source_hashes": 1})
        if arm == "B":
            for phase_counter in phases:
                phase_counter.update({"construction_canonical_commitment_bytes": 0,
                                      "construction_canonical_commitment_entries": 0,
                                      "construction_canonical_commitment_hashes": 0})
            canonical.update({"construction_canonical_commitment_bytes": 190_224,
                              "construction_canonical_commitment_entries": 5284,
                              "construction_canonical_commitment_hashes": 1})
            for key in ("durable_capture_total_wall_ns", "capture_publish_wall_ns", "elapsed_wall_ns",
                        "durable_phase_sum_ns", "complete_lifecycle_total_wall_ns",
                        "lifecycle_phase_sum_ns", "sqlite_qualification_wall_ns"):
                row[key] = 90
            row["canonical_cas_mapping_stage_wall_ns"] = 70
            row["sqlite_commit_durability_wall_ns"] = 19
        row["phase_counters"] = phases
        rows.append(row)
    smoke = []
    for op in SMOKE_OPS:
        row = copy.deepcopy(rows[1])
        native_operation, qualification_mode, references, sequence = SMOKE_NATIVE[op]
        row.update({"screen_smoke_operation": op, "screen_pair": -1,
                    "screen_sample_kind": "smoke", "screen_order": "NA",
                    "operation": native_operation, "qualification_mode": qualification_mode,
                    "expected_cdc_references": references, "actual_cdc_references": references,
                    "expected_cdc_sequence_fingerprint": sequence,
                    "measurement_boundary": MEASUREMENT_BOUNDARIES[op],
                    "screen_residue": [], "warmup": False,
                    "screen_expectations_version": EXPECTATION_VERSIONS["B"],
                    "root_id": SMOKE_IDENTITIES.get(op, (ROOT, TRANSITION, CLOSURE))[0],
                    "transition_id": SMOKE_IDENTITIES.get(op, (ROOT, TRANSITION, CLOSURE))[1],
                    "ordered_closure_digest": SMOKE_IDENTITIES.get(op, (ROOT, TRANSITION, CLOSURE))[2]})
        smoke_phases = [counter(name) for name in
                        (MUTATION_SMOKE_PHASE_ORDER if op in SMOKE_OPS[:3] else READ_SMOKE_PHASE_ORDER)]
        for phase_counter in smoke_phases:
            phase_counter.update({"construction_canonical_commitment_bytes": 0,
                                  "construction_canonical_commitment_entries": 0,
                                  "construction_canonical_commitment_hashes": 0})
        row["phase_counters"] = smoke_phases
        if op in SMOKE_OPS[:3]:
            ranges = []
            for label, start, end, authenticated, objects in MUTATION_RANGES[op]:
                returned = end - start
                ranges.append({"label": label, "start": start, "end": end, "wall_ns": 1,
                               "returned_bytes": returned,
                               "canonical_bytes_authenticated": authenticated,
                               "objects_authenticated": objects,
                               "throughput_mib_s": 1.0})
            row.update({"fresh_reopen_head_wall_ns": 1, "fresh_full_scrub_wall_ns": 1,
                        "reconstruction_wall_ns": 1, "range_verification_wall_ns": 1,
                        "complete_lifecycle_total_wall_ns": 94,
                        "lifecycle_phase_sum_ns": 94, "lifecycle_phase_sum_matches": True,
                        "sqlite_qualification_wall_ns": 94, "elapsed_wall_ns": 94,
                        "range_measurements": ranges})
        else:
            read_measurement = copy.deepcopy(row["measurement_status"])
            read_measurement["sqlite_page_cache"] = "U_STATUS_API"
            row.update({"capture_publish_wall_ns": 0, "canonical_cas_mapping_stage_wall_ns": 0,
                        "precommit_closure_validation_wall_ns": 0,
                        "sqlite_commit_durability_wall_ns": 0, "durable_capture_total_wall_ns": 0,
                        "durable_phase_sum_ns": 0, "commit_publish_call_wall_ns": 0,
                        "commit_dispatch_to_return_wall_ns": 0,
                        "commit_pre_and_post_dispatch_wall_ns": 0,
                        "commit_caller_wrapper_wall_ns": 0, "commit_observation_sum_wall_ns": 0,
                        "source_cdc_nested_in_mapping_stage": False,
                        "transactions": 0, "commits": 0, "commit_dispatches": 0,
                        "commit_returns": 0, "commit_return_successes": 0,
                        "commit_return_errors": 0, "commit_return_status": "NotApplicable",
                        "publication_status": "Unavailable", "fresh_reopen_head_wall_ns": 0,
                        "fresh_full_scrub_wall_ns": 0, "reconstruction_wall_ns": 0,
                        "range_verification_wall_ns": 0, "range_measurements": [],
                        "measurement_status": read_measurement,
                        "sqlite_page_size_bytes": "Unavailable",
                        "sqlite_main_db_dirty_pages_written": "Unavailable",
                        "sqlite_main_db_pager_write_bytes": "Unavailable",
                        "sqlite_cache_spill_pages": "Unavailable",
                        "commit_return_db_apparent_bytes": "Unavailable",
                        "commit_return_journal_apparent_bytes": "Unavailable",
                        "commit_return_authority_apparent_bytes": "Unavailable",
                        "commit_return_db_allocated_bytes": "Unavailable",
                        "commit_return_journal_allocated_bytes": "Unavailable",
                        "commit_return_authority_allocated_bytes": "Unavailable"})
            if op == "materialize-warm":
                row.update({"reconstruction_wall_ns": 5, "complete_lifecycle_total_wall_ns": 5,
                            "lifecycle_phase_sum_ns": 5, "sqlite_qualification_wall_ns": 5,
                            "elapsed_wall_ns": 5})
            elif op == "materialize-fresh":
                row.update({"fresh_reopen_head_wall_ns": 1, "reconstruction_wall_ns": 5,
                            "complete_lifecycle_total_wall_ns": 7, "lifecycle_phase_sum_ns": 6,
                            "lifecycle_phase_sum_matches": False,
                            "sqlite_qualification_wall_ns": 7, "elapsed_wall_ns": 7})
            elif op == "read-range-1m":
                row.update({"range_verification_wall_ns": 3, "complete_lifecycle_total_wall_ns": 3,
                            "lifecycle_phase_sum_ns": 3, "sqlite_qualification_wall_ns": 3,
                            "elapsed_wall_ns": 3,
                            "range_measurements": [{"label": "sequential-1m",
                                "start": 51_904_512, "end": 52_953_088, "wall_ns": 3,
                                "returned_bytes": 1_048_576,
                                "canonical_bytes_authenticated": 1_090_255,
                                "objects_authenticated": 60, "throughput_mib_s": 1.0}]})
            elif op == "reopen":
                row.update({"fresh_reopen_head_wall_ns": 1, "complete_lifecycle_total_wall_ns": 1,
                            "lifecycle_phase_sum_ns": 1, "sqlite_qualification_wall_ns": 1,
                            "elapsed_wall_ns": 1})
        smoke.append(row)
    return rows, smoke


def write_synthetic_bundle(result_dir, rows, smoke):
    result_dir.mkdir(parents=True)
    artifact = result_dir.parent
    (artifact / "control").mkdir()
    (result_dir / "SCREEN-ATTEMPT-v1.txt").write_text(
        "attempt=1 started_utc=2026-08-21T00:00:00Z command=run-screen.sh --execute\n")
    (result_dir / "RUN-STATUS-v1.txt").write_text(
        "status=PASS timeout=false screen_executed_exactly_once=true warmup_rows=2 "
        "measured_rows=6 total_rows=8 protected_smoke_rows=7 wall_seconds=10\n")
    lock_path = artifact / "H05_SCREEN.lock"
    (result_dir / "LOCK-TIMEOUT-v1.txt").write_text(
        f"BENCHMARK_LOCK=H05_SCREEN\nlock_path={lock_path}\n"
        "lock_acquired_utc=2026-08-21T00:00:00Z\n"
        "complete_screen_wall_ceiling_seconds=120\n"
        "lock_released_utc=2026-08-21T00:00:10Z\n")
    (result_dir / "SCHEDULE-ASSERTION-EXECUTION-v1.txt").write_text(
        "constructed plan:\npair 0  warmup   AB\npair 1  measured AB\n"
        "pair 2  measured BA\npair 3  measured AB\nexpected plan:\n"
        "pair 0  warmup   AB\npair 1  measured AB\npair 2  measured BA\n"
        "pair 3  measured AB\nschedule assertion: PASS\n"
        "row sequence: A B | A B | B A | A B\n")
    (result_dir / "COMMAND-v1.txt").write_text("run-screen.sh --execute\n")
    (result_dir / "ENVIRONMENT-v1.txt").write_text(
        "branch=codex/empty-worktree\nhead=febc20f046bba84ccdce1256363d77799eabf2db\n")
    (result_dir / "QUIESCENCE-v1.txt").write_text("quiescence=PASS synthetic\n")
    (result_dir / "QUIESCENCE-CONFLICTS-v1.txt").write_text("")
    (result_dir / "EXECUTION-CUSTODY-RECHECK-v1.txt").write_text(
        "\n".join((*EXECUTABLES.values(), *SOURCES.values(), FIXTURE_SHA)) + "\n")
    (result_dir / "PROTECTED-SMOKE-RESULT-v1.txt").write_text(
        "protected_smoke=PASS operations=7 gate=correctness/resource/non-controlling\n")
    start_lines = []
    for row in smoke:
        for event in ("started", "completed"):
            start_lines.append(f"row_{event}_utc=2026-08-21T00:00:00Z scope=smoke pair=-1 "
                               f"arm=B operation={row['screen_smoke_operation']}")
    for row in rows:
        for event in ("started", "completed"):
            start_lines.append(f"row_{event}_utc=2026-08-21T00:00:00Z scope=screen "
                               f"pair={row['screen_pair']} arm={row['screen_arm']} operation=full")
    (result_dir / "ROW-STARTS-v1.txt").write_text("\n".join(start_lines) + "\n")
    header = "scope\tpair\tarm\toperation\tsource_sha256\tbase_database_sha256\tbase_authority_sha256\tbase_expectations_sha256\texpectations_version\tcanonical_commitment"
    custody = [header]
    for row in smoke:
        custody.append("\t".join(map(str, ("smoke", -1, "B", row["screen_smoke_operation"],
            row["screen_source_sha256"], row["screen_base_database_sha256"],
            row["screen_base_authority_sha256"], row["screen_base_expectations_sha256"],
            row["screen_expectations_version"], row["screen_canonical_commitment"]))))
    for pair in range(4):
        for arm in ("A", "B"):
            row = next(item for item in rows if item["screen_pair"] == pair and item["screen_arm"] == arm)
            custody.append("\t".join(map(str, ("screen", pair, arm, "full",
                row["screen_source_sha256"], row["screen_base_database_sha256"],
                row["screen_base_authority_sha256"], row["screen_base_expectations_sha256"],
                row["screen_expectations_version"], row["screen_canonical_commitment"] or "-"))))
    (result_dir / "SCREEN-INPUT-CUSTODY-v1.tsv").write_text("\n".join(custody) + "\n")
    cp_rows = []
    for operation, name in (("edit-same", "same-middle"),
                            ("edit-plus1-early", "plus1-early"),
                            ("edit-plus1-middle", "plus1-middle")):
        root, transition, closure = SMOKE_IDENTITIES[name]
        references, sequence = SMOKE_NATIVE[name][2:]
        cp_rows.append({"operation": operation, "size_bytes": 104_857_600,
                        "root_id": root, "transition_id": transition,
                        "ordered_closure_digest": closure,
                        "expected_cdc_references": references,
                        "actual_cdc_references": references,
                        "expected_cdc_sequence_fingerprint": sequence})
    (artifact / "control/cp-0009.raw.jsonl").write_text(
        "".join(json.dumps(row) + "\n" for row in cp_rows))


def self_test():
    assert all(HEX64(value) is not None for value in frozen_ids())
    rows, smoke = synthetic_rows()
    assert analyze_rows(rows, smoke)["status"] == "PASS"
    cases = {
        "missing-row": lambda r: r.pop(),
        "wrong-order": lambda r: r.__setitem__(0, r[1]),
        "wrong-hash": lambda r: r[0].__setitem__("screen_source_sha256", "0" * 64),
        "wrong-counter": lambda r: r[1]["phase_counters"][0].__setitem__("construction_canonical_commitment_bytes", 1),
        "phase-counter-drift": lambda r: r[1]["phase_counters"][0].__setitem__("raw_hashes", 5283),
        "phase-shape": lambda r: r[0]["phase_counters"].pop(),
        "missing-work-field": lambda r: r[0].pop("sql_calls"),
        "storage-equation": lambda r: r[0].__setitem__("sqlite_post_logical_store_bytes", 1),
        "compact-code": lambda r: r[0]["measurement_status"].__setitem__("other_heap_copy_bytes", "wrong"),
        "base-copy-method": lambda r: r[0].__setitem__("base_copy_method", "wrong"),
        "native-row-contract": lambda r: r[0].__setitem__("purpose", "wrong"),
        "native-envelope": lambda r: r[0].__setitem__("runner_command_ceiling_seconds", 61),
        "negative-work": lambda r: [item.__setitem__("source_bytes_read", -1) for item in r],
        "transaction-commit": lambda r: r[0].__setitem__("commits", 2),
        "q": lambda r: r[0].__setitem__("q_current", 1),
        "candidate-q-regression": lambda r: r[1].__setitem__("q_high_water", 90_001),
        "work-drift": lambda r: r[1].__setitem__("sql_query_calls", 1),
        "commitment-format": lambda r: r[1].__setitem__("screen_canonical_commitment", "A" * 64),
        "control-commitment": lambda r: r[0].__setitem__("screen_canonical_commitment", "ab" * 32),
        "threshold": lambda r: [item.update({"durable_capture_total_wall_ns": 99,
            "capture_publish_wall_ns": 99, "elapsed_wall_ns": 99,
            "durable_phase_sum_ns": 99, "complete_lifecycle_total_wall_ns": 99,
            "lifecycle_phase_sum_ns": 99, "canonical_cas_mapping_stage_wall_ns": 79})
            for item in r if item["screen_arm"] == "B"],
    }
    for name, mutate in cases.items():
        broken = copy.deepcopy(rows)
        mutate(broken)
        assert analyze_rows(broken, smoke)["status"] == "FAIL", name
    broken_smoke = copy.deepcopy(smoke)
    broken_smoke[0]["lifecycle_phase_sum_ns"] += 1
    assert analyze_rows(rows, broken_smoke)["status"] == "FAIL", "smoke-lifecycle"
    broken_smoke = copy.deepcopy(smoke)
    broken_smoke[3]["physical_journal_apparent_bytes"] = 1
    assert analyze_rows(rows, broken_smoke)["status"] == "FAIL", "smoke-resource"
    broken_smoke = copy.deepcopy(smoke)
    broken_smoke[0]["range_measurements"][2]["label"] = "wrong"
    assert analyze_rows(rows, broken_smoke)["status"] == "FAIL", "smoke-range"
    broken_smoke = copy.deepcopy(smoke)
    broken_smoke[0]["phase_counters"][1]["construction_canonical_commitment_bytes"] = 1
    assert analyze_rows(rows, broken_smoke)["status"] == "FAIL", "smoke-commitment-counter"
    malformed = copy.deepcopy(rows)
    malformed[0]["durable_capture_total_wall_ns"] = "bad"
    malformed_result = safe_analyze_rows(malformed, smoke)
    assert malformed_result["status"] == "FAIL" and malformed_result["reasons"]
    with tempfile.TemporaryDirectory() as directory:
        result_dir = Path(directory) / "artifact/results"
        write_synthetic_bundle(result_dir, rows, smoke)
        assert audit_evidence(result_dir, rows, smoke) == []
        (result_dir / "RUN-STATUS-v1.txt").write_text("status=PASS timeout=true\n")
        assert audit_evidence(result_dir, rows, smoke)
        (result_dir / "RUN-STATUS-v1.txt").write_text(
            "status=PASS timeout=false screen_executed_exactly_once=true warmup_rows=2 "
            "measured_rows=6 total_rows=8 protected_smoke_rows=7 wall_seconds=10\n")
        custody = result_dir / "SCREEN-INPUT-CUSTODY-v1.tsv"
        custody.write_text(custody.read_text().replace("expB0", "wrong", 1))
        assert audit_evidence(result_dir, rows, smoke)
        write_synthetic_bundle(Path(directory) / "artifact2/results", rows, smoke)
        starts = Path(directory) / "artifact2/results/ROW-STARTS-v1.txt"
        starts.write_text("\n".join(starts.read_text().splitlines()[:-1]) + "\n")
        assert audit_evidence(Path(directory) / "artifact2/results", rows, smoke)
    print("self-test PASS cases=frozen-id-format,schedule,missing-row,wrong-order,wrong-hash,"
          "wrong-counter,phase-counter-drift,phase-shape,missing-work-field,storage-equation,"
          "compact-code,base-copy-method,native-row-contract,"
          "native-envelope,negative-work,transaction-commit,Q,candidate-Q-regression,"
          "normalized-work-drift,"
          "commitment-format,control-commitment,smoke-lifecycle,smoke-resource,smoke-range,"
          "smoke-commitment-counter,malformed-evidence,threshold,bundle-evidence,"
          "row-start-reconciliation")


def main():
    if sys.argv[1:] == ["--self-test"]:
        self_test()
        return 0
    if len(sys.argv) == 3 and sys.argv[1] == "--verify-cp0009":
        reasons = verify_cp0009_smoke_constants(sys.argv[2])
        print("CP-0009 smoke constants PASS" if not reasons else "CP-0009 smoke constants FAIL " + ",".join(reasons))
        return bool(reasons)
    if len(sys.argv) != 2:
        raise SystemExit(f"usage: {sys.argv[0]} RESULTS-DIR | --self-test")
    result_dir = Path(sys.argv[1])
    try:
        raw_bytes, rows = read_jsonl(result_dir / "SCREEN-RAW-v1.jsonl")
        smoke_bytes, smoke = read_jsonl(result_dir / "PROTECTED-SMOKE-v1.jsonl")
        evidence_reasons = audit_evidence(result_dir, rows, smoke)
        result = safe_analyze_rows(rows, smoke, evidence_reasons)
        result["raw_sha256"] = hashlib.sha256(raw_bytes).hexdigest()
        result["protected_smoke_sha256"] = hashlib.sha256(smoke_bytes).hexdigest()
    except Exception as error:
        result = structured_failure(f"malformed-bundle:{type(error).__name__}:{error}")
    print(json.dumps(result, sort_keys=True, separators=(",", ":")))
    return result["status"] != "PASS"


if __name__ == "__main__":
    raise SystemExit(main())
