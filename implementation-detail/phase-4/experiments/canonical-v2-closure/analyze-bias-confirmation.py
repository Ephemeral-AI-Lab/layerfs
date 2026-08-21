#!/usr/bin/env python3
"""Independent frozen-binary canonical-v2 position-bias confirmation."""

import csv
import hashlib
import json
import re
import statistics
import sys
from pathlib import Path

PROFILE_A = "b0ebb845409ef995a5fa454bb23d10a80c6ecf44deb7832ca2ce1213eb0f4ba1"
PROFILE_B = "94a03ba7b6c97b5ff37c0ec62ef1d801b9896494b45456bd3df23e2cb278d13b"
ROOT_B = "93d1b461b5cbf88e8d122ad4e90a15a6e3029c408f0fe02ef51c561e5a94c6d1"
TRANSITION_B = "2de8d2ce6b614373ba8fb8b29d3a3eccd535abfccc2654e7922514e5ca90fd89"
COMMITMENT_B = "5bf63ef8adad7bb373be4e968759997b385590d076d75a1488e1958b64a3f8e2"
CONTROL = "9cda87ee7fd92784281a6ec7ee3045eb661681d8b7b930dd36546119ae4749d7"
CANDIDATE = "7419acc21672cc92c698675db2e68f3b0281282c26623744d2d5c1be495a9b82"
CONTROL_SOURCE = "3284c3bdfe20426df78b4cb8ef310248e1e4f644b8422d79c4689653d870652a"
CANDIDATE_SOURCE = "e8b721013308bcd1ccce54e35f40026e12df067107b72431b00536e8328edd4a"
FIXTURE = "63b3695b8c117b5bc39885e0df0dcd0af1d49e575482bab16577d84b4f40eff4"
SCHEDULE = [
    (0, "A", "warmup", "AB", "AB"), (0, "B", "warmup", "AB", "AB"),
    (1, "B", "warmup", "BA", "BA"), (1, "A", "warmup", "BA", "BA"),
    (2, "A", "measured", "AB", "AB"), (2, "B", "measured", "AB", "AB"),
    (3, "B", "measured", "BA", "BA"), (3, "A", "measured", "BA", "BA"),
    (4, "A", "measured", "BA", "AB"), (4, "B", "measured", "BA", "AB"),
    (5, "B", "measured", "AB", "BA"), (5, "A", "measured", "AB", "BA"),
]
SMOKE = ["same-middle", "plus1-early", "plus1-middle", "materialize-warm",
         "materialize-fresh", "read-range-1m", "reopen"]
SMOKE_EXACT = {
    "same-middle": ("8df9bc09f9ba99351f11f3cb01b039713090120873b6dea8903e7d835a2a9faf", "b185f7670f748b5713d4d8538c513bce4b3019e17991840c369575f404fbf2ed", "d7614133f35f1a254d0d2222815cdbcbdcd69915baf30c3a801831e6497b1683", 5284, 2222803, 11078, 16289, 26, 649317, 45, 184320, 11, 5334, 108697),
    "plus1-early": ("f638dc6cdce75368dfce4d7496fe3c5cd3964c1880f879e1ac552398027805dd", "6b899425ac58eac832041b7a5a0019f9762743a3bd43238db32029f8224043d8", "9b74164bce3ffa57f7f2bbfd26329dfbd315e4dc81c310ea26cd280cc2175801", 5285, 49231, 11154, 16290, 184, 817062, 135, 552960, 90, 196375, 196389),
    "plus1-middle": ("c6f9c58fddea64cd80328b9dbf6c5ab25db7a24c74bea074fc78b669371530e0", "914f7998b42b90a3a8347b36c18194f95f9599c77f6fd7d6435ff2b10ac212cd", "4cdcd09b47447c6673d391bdbece5eb239bd26bb9320061223f44d22e56d104c", 5285, 49231, 11072, 16249, 102, 721450, 85, 348160, 49, 100763, 100777),
    "materialize-warm": (ROOT_B, TRANSITION_B, "29233d6018b031f6035c8e2c8175f1ab86fb721808f21a8858bc965567e9c0c1", 5284, 32195, 345, 10750, 0, 392594, "Unavailable", "Unavailable", 0, 0, 0),
    "materialize-fresh": (ROOT_B, TRANSITION_B, "29233d6018b031f6035c8e2c8175f1ab86fb721808f21a8858bc965567e9c0c1", 5284, 32195, 175, 5379, 0, 196485, "Unavailable", "Unavailable", 0, 0, 0),
    "read-range-1m": (ROOT_B, TRANSITION_B, "29233d6018b031f6035c8e2c8175f1ab86fb721808f21a8858bc965567e9c0c1", 5284, 2126026, 67, 70, 0, 7941, "Unavailable", "Unavailable", 0, 0, 0),
    "reopen": (ROOT_B, TRANSITION_B, "29233d6018b031f6035c8e2c8175f1ab86fb721808f21a8858bc965567e9c0c1", 5284, 17699, 5, 8, 0, 376, "Unavailable", "Unavailable", 0, 0, 0),
}


def load_jsonl(path):
    return [json.loads(line) for line in Path(path).read_text().splitlines() if line.strip()]


def add(reasons, condition, reason):
    if not condition:
        reasons.append(reason)


def canonical_phase(row):
    return next((phase for phase in row.get("phase_counters", [])
                 if phase.get("phase") == "canonical_cas_mapping"), {})


def centers(rows, field):
    result = {}
    for arm in ("A", "B"):
        values = [row[field] for row in rows if row["screen_arm"] == arm]
        result[arm] = {"median": statistics.median(values), "min": min(values), "max": max(values)}
        positions = {}
        for position in (1, 2):
            position_values = [row[field] for row in rows if row["screen_arm"] == arm
                               and row["screen_execution_position"] == position]
            positions[str(position)] = {"values": position_values,
                                        "median": statistics.median(position_values)}
        result[arm]["execution_positions"] = positions
        result[arm]["position_balanced_center"] = (
            positions["1"]["median"] + positions["2"]["median"]) / 2
        preparation = {}
        for position in (1, 2):
            position_values = [row[field] for row in rows if row["screen_arm"] == arm
                               and row["screen_preparation_position"] == position]
            preparation[str(position)] = {"values": position_values,
                                           "median": statistics.median(position_values)}
        result[arm]["preparation_positions"] = preparation
    return result


def expected_invocations(result_dir):
    repo = result_dir.parents[3]
    exploration = repo / "target/phase4-canonical-v2-exploration-20260821-v1"
    control = exploration / "control/phase4_create_edit_benchmark-cp0009"
    candidate = exploration / "candidate/phase4_create_edit_benchmark-canonical-v2"
    work = result_dir / "work-v1"
    plan = []
    public_ops = ["edit-same", "edit-plus1-early", "edit-plus1-middle", "materialize-warm",
                  "materialize-fresh", "read-range-1m", "reopen"]
    internal_ops = SMOKE
    for index, (public, internal) in enumerate(zip(public_ops, internal_ops), 1):
        root = work / f"smoke-{index}-{internal}"
        iteration = 910_000 + index
        cli = "--count-change-scale" if internal.startswith("plus1-") else "--fast"
        plan.append(("prepare", -1, "B", "NA", "NA", CANDIDATE,
                     f"{candidate} {cli}-prepare {root} 104857600 {public} {iteration}"))
        plan.append(("smoke", -1, "B", "NA", "NA", CANDIDATE,
                     f"{candidate} {cli}-row {root} 104857600 {public} {iteration} false complete-roundtrip"))
    cells = [("warmup","AB","AB"),("warmup","BA","BA"),("measured","AB","AB"),
             ("measured","BA","BA"),("measured","BA","AB"),("measured","AB","BA")]
    for pair, (kind, prepare_order, execute_order) in enumerate(cells):
        iteration = 920_000 + pair
        for arm in prepare_order:
            executable = control if arm == "A" else candidate
            root = work / f"pair-{pair}" / f"prep-{arm}"
            plan.append(("prepare", pair, arm, prepare_order, execute_order,
                         CONTROL if arm == "A" else CANDIDATE,
                         f"{executable} --fast-prepare {root} 104857600 write {iteration}"))
        for arm in execute_order:
            executable = control if arm == "A" else candidate
            root = work / f"pair-{pair}" / f"row-{arm}"
            warmup = str(kind == "warmup").lower()
            plan.append((kind, pair, arm, prepare_order, execute_order,
                         CONTROL if arm == "A" else CANDIDATE,
                         f"{executable} --fast-row {root} 104857600 write {iteration} {warmup} capture-only"))
    fields = ("kind", "pair", "arm", "prepare_order", "execute_order", "executable_sha256", "command")
    return [{"sequence": str(index), **dict(zip(fields, map(str, item)))}
            for index, item in enumerate(plan, 1)]


def analyze(result_dir):
    result_dir = Path(result_dir).resolve()
    rows = load_jsonl(result_dir / "SCREEN-RAW-v1.jsonl")
    smoke = load_jsonl(result_dir / "PROTECTED-SMOKE-v1.jsonl")
    reasons = []
    actual_schedule = [(row.get("screen_pair"), row.get("screen_arm"),
                        row.get("screen_sample_kind"), row.get("screen_prepare_order"),
                        row.get("screen_order")) for row in rows]
    add(reasons, actual_schedule == SCHEDULE, "schedule")
    add(reasons, len(smoke) == 7 and [row.get("screen_smoke_operation") for row in smoke] == SMOKE,
        "protected-smoke-shape")

    for index, row in enumerate(rows):
        arm = row.get("screen_arm")
        add(reasons, row.get("status") == "PASS" and row.get("error") is None,
            f"row:{index}:status")
        add(reasons, row.get("q_current") == 0 and row.get("screen_residue") == [],
            f"row:{index}:cleanup")
        add(reasons, (row.get("transactions"), row.get("commits"), row.get("commit_dispatches"),
                      row.get("commit_returns"), row.get("commit_return_successes"),
                      row.get("commit_return_errors")) == (1, 1, 1, 1, 1, 0),
            f"row:{index}:transaction")
        add(reasons, row.get("durable_phase_sum_matches") is True
            and row.get("commit_timer_equation_matches") is True,
            f"row:{index}:timers")
        add(reasons, row.get("screen_executable_sha256") == (CONTROL if arm == "A" else CANDIDATE),
            f"row:{index}:executable")
        add(reasons, row.get("screen_executable_source_sha256") ==
            (CONTROL_SOURCE if arm == "A" else CANDIDATE_SOURCE)
            and row.get("screen_source_sha256") == FIXTURE
            and row.get("screen_source_size") == 104_857_600,
            f"row:{index}:source-custody")
        add(reasons, row.get("base_copy_method") ==
            "physical-byte-copy-identical-database-authority-expectations"
            and row.get("base_preparation_in_measured_interval") is False
            and row.get("runner_wall_ceiling_seconds") == 120
            and row.get("runner_command_ceiling_seconds") == 60,
            f"row:{index}:runner-envelope")
        add(reasons, row.get("q_equation") == "Q1" and row.get("q_fixed_envelope_removed") is True
            and row.get("q_current_semantics") == "after_report_output_drop"
            and isinstance(row.get("q_report_output_bytes"), int) and row["q_report_output_bytes"] > 0
            and all(isinstance(row.get(key), int) and row[key] >= 0 for key in (
                "q_high_water", "q_cdc_base_live_bytes",
                "q_cdc_old_window_bytes", "q_cdc_scan_input_bytes", "q_cdc_overlap_current")),
            f"row:{index}:q")
        add(reasons, (row.get("sql_calls"), row.get("sql_query_calls"), row.get("sql_execute_calls"),
                      row.get("sql_rows_returned"), row.get("sql_rows_changed"),
                      row.get("row_blob_reads"), row.get("row_blob_writes"),
                      row.get("row_blob_copy_bytes"), row.get("blob_opens"), row.get("blob_reads"),
                      row.get("blob_writes")) == (5_381, 4, 5_377, 2, 5_373, 4, 10_748, 88, 0, 0, 0),
            f"row:{index}:sql-blob")
        add(reasons, row.get("physical_journal_apparent_bytes") == 0
            and row.get("physical_authority_sidecar_apparent_bytes") == 32,
            f"row:{index}:storage-residue")
        add(reasons, row.get("screen_execution_position") == row["screen_order"].index(arm) + 1
            and row.get("screen_preparation_position") == row["screen_prepare_order"].index(arm) + 1,
            f"row:{index}:position")
        add(reasons, row.get("operation") == "full" and row.get("size_bytes") == 104_857_600
            and row.get("input_size_bytes") == 104_857_600 and row.get("directory_entries") == 0
            and row.get("source_fingerprint") == "bb883eecf4ea85d80432953791dcc352243da94175e7503e2c476afe9bd0bab7"
            and row.get("expected_cdc_references") == row.get("actual_cdc_references") == 5_284,
            f"row:{index}:identity-envelope")
        add(reasons, row.get("screen_expectations_version") ==
            ("LFS-WP4M-EXPECTATIONS-3" if arm == "A" else "LFS-CANONICAL-V2-EXPECTATIONS-1")
            and (row.get("screen_canonical_commitment") is None if arm == "A"
                 else row.get("screen_canonical_commitment") == COMMITMENT_B),
            f"row:{index}:expectations")
        add(reasons, row.get("expected_cdc_sequence_fingerprint") ==
            ("5bb376c3c54d8724973a7b160acab599f2f5cee4b4a56e855ff0cbe987425994"
             if arm == "A" else COMMITMENT_B), f"row:{index}:cdc-sequence")
        arm_exact = ({"w_bytes":210_493_394,
                      "canonical_bytes_authenticated":105_291_554,
                      "canonical_bytes_written":105_291_554,
                      "sqlite_post_logical_database_bytes":109_268_992,
                      "sqlite_post_apparent_database_bytes":109_268_992,
                      "sqlite_post_logical_store_bytes":109_269_024,
                      "sqlite_post_apparent_store_bytes":109_269_024}
                     if arm == "A" else
                     {"w_bytes":210_324_306,
                      "canonical_bytes_authenticated":105_122_466,
                      "canonical_bytes_written":105_122_466,
                      "sqlite_post_logical_database_bytes":109_199_360,
                      "sqlite_post_apparent_database_bytes":109_199_360,
                      "sqlite_post_logical_store_bytes":109_199_392,
                      "sqlite_post_apparent_store_bytes":109_199_392})
        arm_exact.update({"d_bytes":0, "payload_io_bytes":104_857_600,
                          "sqlite_pre_logical_database_bytes":20_480,
                          "sqlite_pre_apparent_database_bytes":20_480,
                          "sqlite_pre_logical_store_bytes":20_512,
                          "sqlite_pre_apparent_store_bytes":20_512,
                          "q_cdc_base_live_bytes":0, "q_cdc_old_window_bytes":0,
                          "q_cdc_scan_input_bytes":0, "q_cdc_overlap_current":0})
        add(reasons, all(row.get(key)==value for key,value in arm_exact.items()),
            f"row:{index}:exact-q-work-storage")
        phase = canonical_phase(row)
        add(reasons, (phase.get("construction_put_evidences"), phase.get("construction_edges_covered"),
                      phase.get("construction_leaf_summaries"), phase.get("construction_branch_summaries"),
                      phase.get("construction_file_summaries"), phase.get("construction_workspace_summaries"),
                      phase.get("construction_transition_summaries"), phase.get("construction_cdc_entries"))
            == (5_372, 5_371, 83, 2, 1, 1, 1, 5_284), f"row:{index}:construction-proof")
        if arm == "A":
            add(reasons, row.get("profile_id") == PROFILE_A, f"row:{index}:control-profile")
            add(reasons, (row.get("root_id"), row.get("transition_id"),
                          row.get("ordered_closure_digest")) ==
                ("2d41c27f96b0332475fb8ec3c46a336c9c8a8084408bc545e5cbb24d51cb25d0",
                 "ba15fd20469414de99c135fc90a5c5ad028f99f115b8c0d138ace9ec98536412",
                 "d6aac6e40cc851dd6295dbeec6488f1c5ebefa7520f86b0cd12bdcdce1f0d54a"),
                f"row:{index}:control-identities")
            add(reasons, (row.get("raw_bytes_hashed"), row.get("raw_hashes"),
                          row.get("mapping_bytes_rewritten"), row.get("canonical_new_write_bytes"),
                          row.get("source_bytes_read"), row.get("source_cdc_bytes_read"),
                          row.get("references"), row.get("chunks"), row.get("objects_created"),
                          row.get("objects_reused"), row.get("pages"), row.get("branches"),
                          row.get("q_high_water")) ==
                (104_857_600, 5_284, 365_262, 105_291_554, 104_857_600, 104_857_600,
                 5_284, 5_284, 5_372, 0, 83, 2, 88_093), f"row:{index}:control-work")
            add(reasons, (row.get("sqlite_main_db_dirty_pages_written"),
                          row.get("sqlite_main_db_pager_write_bytes"), row.get("sqlite_cache_spill_pages"))
                == (26_676, 109_264_896, 6_675), f"row:{index}:control-pager")
            add(reasons, (phase.get("construction_source_hash_bytes"),
                          phase.get("construction_source_hashes")) == (104_857_600, 1),
                f"row:{index}:control-source-proof")
        else:
            exact = {"profile_id": PROFILE_B, "root_id": ROOT_B,
                     "transition_id": TRANSITION_B,
                     "ordered_closure_digest":"29233d6018b031f6035c8e2c8175f1ab86fb721808f21a8858bc965567e9c0c1",
                     "screen_canonical_commitment": COMMITMENT_B,
                     "source_bytes_read": 104_857_600, "source_cdc_bytes_read": 104_857_600,
                     "references": 5_284, "chunks": 5_284, "raw_bytes_hashed": 0,
                     "raw_hashes": 0, "mapping_bytes_rewritten": 196_174,
                     "canonical_new_write_bytes": 105_122_466, "objects_created": 5_372,
                     "objects_reused": 0, "pages": 83, "branches": 2, "q_high_water": 86_045}
            add(reasons, all(row.get(key) == value for key, value in exact.items()),
                f"row:{index}:candidate-work")
            phase_exact = (phase.get("construction_source_hash_bytes"),
                           phase.get("construction_source_hashes"),
                           phase.get("construction_canonical_commitment_bytes"),
                           phase.get("construction_canonical_commitment_entries"),
                           phase.get("construction_canonical_commitment_hashes"),
                           phase.get("construction_cdc_entries"))
            add(reasons, phase_exact == (0, 0, 190_224, 5_284, 1, 5_284),
                f"row:{index}:candidate-proof")
            add(reasons, (row.get("sqlite_main_db_dirty_pages_written"),
                          row.get("sqlite_main_db_pager_write_bytes"), row.get("sqlite_cache_spill_pages"))
                == (26_659, 109_195_264, 6_658), f"row:{index}:candidate-pager")
            add(reasons, row.get("physical_store_allocated_bytes", 1) * 4
                <= row.get("sqlite_post_apparent_store_bytes", 0) * 5,
                f"row:{index}:allocation")

    for index, row in enumerate(smoke):
        operation = row.get("screen_smoke_operation")
        mutation = operation in {"same-middle", "plus1-early", "plus1-middle"}
        add(reasons, row.get("status") == "PASS" and row.get("error") is None
            and row.get("profile_id") == PROFILE_B and row.get("q_current") == 0
            and isinstance(row.get("q_report_output_bytes"), int) and row["q_report_output_bytes"] > 0
            and row.get("screen_residue") == [], f"smoke:{index}:contract")
        add(reasons, (row.get("transactions"), row.get("commits")) == ((1, 1) if mutation else (0, 0)),
            f"smoke:{index}:transaction")
        add(reasons, row.get("raw_bytes_hashed") == 0 and row.get("raw_hashes") == 0,
            f"smoke:{index}:raw-hash")
        expected = SMOKE_EXACT.get(operation)
        actual = (row.get("root_id"), row.get("transition_id"), row.get("ordered_closure_digest"),
                  row.get("actual_cdc_references"), row.get("q_high_water"), row.get("sql_calls"),
                  row.get("row_blob_reads"), row.get("row_blob_writes"), row.get("row_blob_copy_bytes"),
                  row.get("sqlite_main_db_dirty_pages_written"), row.get("sqlite_main_db_pager_write_bytes"),
                  row.get("objects_created"), row.get("mapping_bytes_rewritten"),
                  row.get("canonical_new_write_bytes"))
        add(reasons, expected is not None and actual == expected, f"smoke:{index}:exact-work")
        timer_ok = row.get("durable_phase_sum_matches") is True and row.get("commit_timer_equation_matches") is True
        if operation == "materialize-fresh":
            timer_ok = timer_ok and row.get("durable_capture_total_wall_ns") == 0 \
                and row.get("fresh_reopen_head_wall_ns", 0) > 0 \
                and row.get("reconstruction_wall_ns", 0) > 0 \
                and row.get("elapsed_wall_ns") == row.get("complete_lifecycle_total_wall_ns")
        else:
            timer_ok = timer_ok and row.get("lifecycle_phase_sum_matches") is True
        add(reasons, timer_ok,
            f"smoke:{index}:timers")
        ranges = row.get("range_measurements", [])
        if mutation:
            add(reasons, [item.get("label") for item in ranges] ==
                ["zero", "first-byte", "cross-chunk", "leaf-boundary", "branch-boundary", "last-byte", "eof"]
                and [item.get("returned_bytes") for item in ranges] == [0, 1, 2, 2, 2, 1, 0]
                and all(item.get("canonical_bytes_authenticated", -1) >= item.get("returned_bytes", 0)
                        for item in ranges), f"smoke:{index}:ranges")
        elif operation == "read-range-1m":
            add(reasons, len(ranges) == 1 and ranges[0].get("label") == "sequential-1m"
                and (ranges[0].get("start"), ranges[0].get("end"), ranges[0].get("returned_bytes"))
                == (51_904_512, 52_953_088, 1_048_576)
                and ranges[0].get("canonical_bytes_authenticated", 0) >= 1_048_576,
                f"smoke:{index}:range-1m")
        else:
            add(reasons, ranges == [], f"smoke:{index}:unexpected-ranges")

    measured = [row for row in rows if row.get("screen_sample_kind") == "measured"]
    pairs = []
    for pair in range(2, 6):
        a = next(row for row in measured if row["screen_pair"] == pair and row["screen_arm"] == "A")
        b = next(row for row in measured if row["screen_pair"] == pair and row["screen_arm"] == "B")
        pairs.append({"pair": pair - 1, "prepare_order": a["screen_prepare_order"],
                      "execute_order": a["screen_order"],
                      "control_ns": a["durable_capture_total_wall_ns"],
                      "candidate_ns": b["durable_capture_total_wall_ns"],
                      "improvement_percent": (a["durable_capture_total_wall_ns"]
                                               - b["durable_capture_total_wall_ns"]) * 100
                                              / a["durable_capture_total_wall_ns"],
                      "control_mapping_ns": a["canonical_cas_mapping_stage_wall_ns"],
                      "candidate_mapping_ns": b["canonical_cas_mapping_stage_wall_ns"],
                      "control_commit_ns": a["sqlite_commit_durability_wall_ns"],
                      "candidate_commit_ns": b["sqlite_commit_durability_wall_ns"]})

    durable = centers(measured, "durable_capture_total_wall_ns")
    mapping = centers(measured, "canonical_cas_mapping_stage_wall_ns")
    commit = centers(measured, "sqlite_commit_durability_wall_ns")
    control_center = durable["A"]["position_balanced_center"]
    candidate_center = durable["B"]["position_balanced_center"]
    external = {}
    for arm in ("A", "B"):
        external[arm] = {}
        for position in (1, 2):
            group = [row["external_time"] for row in measured if row["screen_arm"] == arm
                     and row["screen_execution_position"] == position]
            external[arm][str(position)] = {
                "user_seconds_median": statistics.median([item["user_seconds"] for item in group]),
                "system_seconds_median": statistics.median([item["system_seconds"] for item in group]),
                "rss_bytes_median": statistics.median([item["maximum_resident_set_bytes"] for item in group]),
                "peak_bytes_median": statistics.median([item["peak_memory_footprint_bytes"] for item in group]),
                "instructions": sorted({item["instructions"] for item in group}),
                "cycles": sorted({item["cycles"] for item in group}),
            }

    required = ["SCREEN-ATTEMPT-v1.txt", "RUN-STATUS-v1.txt", "LOCK-TIMEOUT-v1.txt",
                "SCHEDULE-ASSERTION-EXECUTION-v1.txt", "SCREEN-INPUT-CUSTODY-v1.tsv",
                "ACTUAL-INVOCATIONS-v1.tsv", "INVOCATION-PLAN-v1.tsv", "ROW-STARTS-v1.txt",
                "COMMAND-v1.txt", "ENVIRONMENT-v1.txt", "QUIESCENCE-v1.txt",
                "QUIESCENCE-CONFLICTS-v1.txt", "EXECUTION-CUSTODY-RECHECK-v1.txt",
                "PROTECTED-SMOKE-RESULT-v1.txt"]
    add(reasons, all((result_dir / name).is_file() for name in required), "bundle:missing")
    if all((result_dir / name).is_file() for name in required):
        status = dict(token.split("=", 1) for token in (result_dir / "RUN-STATUS-v1.txt").read_text().split()
                      if token.count("=") == 1)
        expected_status = {"status":"PASS", "timeout":"false",
            "confirmation_executed_exactly_once":"true", "warmup_rows":"4",
            "measured_rows":"8", "total_rows":"12", "protected_smoke_rows":"7"}
        add(reasons, set(status) == set(expected_status) | {"wall_seconds"}
            and all(status.get(key) == value for key, value in expected_status.items())
            and status.get("wall_seconds", "").isdigit() and int(status["wall_seconds"]) <= 120,
            "bundle:status")
        actual = list(csv.DictReader((result_dir / "ACTUAL-INVOCATIONS-v1.tsv").open(), delimiter="\t"))
        plan = list(csv.DictReader((result_dir / "INVOCATION-PLAN-v1.tsv").open(), delimiter="\t"))
        owned_plan = expected_invocations(result_dir)
        add(reasons, len(actual) == 76 and sum(item["event"] == "started" for item in actual) == 38
            and sum(item["event"] == "completed" for item in actual) == 38
            and all(item["executable_sha256"] in {CONTROL, CANDIDATE} for item in actual),
            "bundle:invocations")
        fields = ["sequence", "kind", "pair", "arm", "prepare_order", "execute_order",
                  "executable_sha256", "command"]
        projected = [{field: item[field] for field in fields} for item in actual]
        expected = [row for item in plan for row in (item, item)]
        add(reasons, plan == owned_plan and projected == expected
            and [item["event"] for item in actual] == [event for _ in plan for event in ("started", "completed")]
            and all(item["exit"] == ("-" if item["event"] == "started" else "0") for item in actual),
            "bundle:plan-actual")
        starts = (result_dir / "ROW-STARTS-v1.txt").read_text().splitlines()
        start_pattern = re.compile(r"row_(started|completed)_utc=\S+ scope=(smoke|screen) pair=(-?\d+) arm=([AB]) operation=([a-z0-9-]+)")
        parsed_starts=[]
        for line in starts:
            match=start_pattern.fullmatch(line)
            if match:
                parsed_starts.append((match.group(1),match.group(2),int(match.group(3)),match.group(4),match.group(5)))
        expected_rows=[("smoke",-1,"B",operation) for operation in SMOKE]
        expected_rows += [("screen",pair,arm,"full") for pair,arm,_,_,_ in SCHEDULE]
        expected_starts=[(event,*row) for row in expected_rows for event in ("started","completed")]
        add(reasons, parsed_starts == expected_starts, "bundle:row-starts")
        custody = list(csv.DictReader((result_dir / "SCREEN-INPUT-CUSTODY-v1.tsv").open(), delimiter="\t"))
        custody_map={(item["scope"],int(item["pair"]),item["arm"],item["operation"]):item for item in custody}
        add(reasons, len(custody) == len(custody_map) == 19, "bundle:input-custody-shape")
        for scope,row in [("smoke",row) for row in smoke]+[("screen",row) for row in rows]:
            operation=row["screen_smoke_operation"] if scope=="smoke" else "full"
            item=custody_map.get((scope,row["screen_pair"],row["screen_arm"],operation))
            commitment="-" if row["screen_canonical_commitment"] is None else row["screen_canonical_commitment"]
            add(reasons, item is not None and item["source_sha256"]==row["screen_source_sha256"]
                and item["base_database_sha256"]==row["screen_base_database_sha256"]
                and item["base_authority_sha256"]==row["screen_base_authority_sha256"]
                and item["base_expectations_sha256"]==row["screen_base_expectations_sha256"]
                and item["expectations_version"]==row["screen_expectations_version"]
                and item["canonical_commitment"]==commitment,
                f"bundle:custody:{scope}:{row['screen_pair']}:{row['screen_arm']}:{operation}")
            add(reasons, re.fullmatch(r"[0-9a-f]{64}",row.get("screen_post_database_sha256","")) is not None
                and re.fullmatch(r"[0-9a-f]{64}",row.get("screen_post_authority_sha256","")) is not None,
                f"bundle:post-custody:{scope}:{row['screen_pair']}:{row['screen_arm']}:{operation}")
        add(reasons, (result_dir / "QUIESCENCE-v1.txt").read_text().startswith("quiescence=PASS ")
            and (result_dir / "QUIESCENCE-CONFLICTS-v1.txt").read_bytes() == b"",
            "bundle:quiescence")
        lock = (result_dir / "LOCK-TIMEOUT-v1.txt").read_text()
        lock_paths = [Path(line.split("=", 1)[1]) for line in lock.splitlines() if line.startswith("lock_path=")]
        add(reasons, "BENCHMARK_LOCK=CANONICAL_V2_BIAS_CONFIRMATION" in lock
            and "complete_confirmation_wall_ceiling_seconds=120" in lock
            and lock.count("lock_acquired_utc=")==1 and lock.count("lock_released_utc=")==1
            and lock.index("lock_acquired_utc=") < lock.index("lock_released_utc=")
            and len(lock_paths) == 1 and not lock_paths[0].exists(),
            "bundle:lock")
        study = result_dir.parent
        methodology = study / "PROSPECTIVE-METHODOLOGY-CUSTODY-v2.tsv"
        add(reasons, methodology.is_file(), "bundle:methodology")
        if methodology.is_file():
            method_sha = hashlib.sha256(methodology.read_bytes()).hexdigest()
            method_rows = list(csv.DictReader(methodology.open(), delimiter="\t"))
            labels=["runner","runner-test","analyzer","manifest-tool","repair-preregistration",
                    "historical-manifest","bias-v1-manifest","control-executable","control-source","candidate-executable",
                    "candidate-source","candidate-codec","fixture"]
            add(reasons, [item["label"] for item in method_rows]==labels
                and len({item["path"] for item in method_rows})==13
                and all(Path(item["path"]).is_file()
                and hashlib.sha256(Path(item["path"]).read_bytes()).hexdigest() == item["sha256"]
                for item in method_rows), "bundle:methodology-custody")
            runner_path=next((item["path"] for item in method_rows if item["label"]=="runner"),"")
            expected_command=f"/usr/bin/env CANONICAL_V2_BIAS_CUSTODY_SHA256={method_sha} {runner_path} --execute"
            add(reasons, (result_dir / "COMMAND-v1.txt").read_text()==expected_command+"\n",
                "bundle:command-anchor")
            attempt=(result_dir / "SCREEN-ATTEMPT-v1.txt").read_text()
            add(reasons, re.fullmatch(r"attempt=1 started_utc=\S+ command="+re.escape(expected_command)+r"\n",attempt) is not None,
                "bundle:attempt")
            runner_sha=next((item["sha256"] for item in method_rows if item["label"]=="runner"),None)
            add(reasons, all(row.get("runner_sha256")==runner_sha for row in rows+smoke),
                "bundle:row-runner-sha")
        expected_schedule="constructed plan:\n"+"\n".join([
            "pair 0  warmup   prepare=AB execute=AB","pair 1  warmup   prepare=BA execute=BA",
            "pair 2  measured prepare=AB execute=AB","pair 3  measured prepare=BA execute=BA",
            "pair 4  measured prepare=BA execute=AB","pair 5  measured prepare=AB execute=BA"])
        expected_schedule+="\nexpected plan:\n"+"\n".join([
            "pair 0  warmup   prepare=AB execute=AB","pair 1  warmup   prepare=BA execute=BA",
            "pair 2  measured prepare=AB execute=AB","pair 3  measured prepare=BA execute=BA",
            "pair 4  measured prepare=BA execute=AB","pair 5  measured prepare=AB execute=BA"])
        expected_schedule+="\nschedule assertion: PASS\nrow sequence: A B | B A | A B | B A | A B | B A\n"
        add(reasons,(result_dir/"SCHEDULE-ASSERTION-EXECUTION-v1.txt").read_text()==expected_schedule,
            "bundle:schedule-artifact")
        custody_text=(result_dir/"EXECUTION-CUSTODY-RECHECK-v1.txt").read_text()
        add(reasons, all(value in custody_text for value in (CONTROL,CANDIDATE,CONTROL_SOURCE,
            CANDIDATE_SOURCE,FIXTURE,"methodology_custody=PASS entries=13",
            "exploratory_historical_manifest=PASS entries=83",
            "bias_confirmation_v1_manifest=PASS entries=223")),"bundle:execution-custody")
        add(reasons,(result_dir/"PROTECTED-SMOKE-RESULT-v1.txt").read_text()==
            "protected_smoke=PASS operations=7 gate=correctness/resource/non-controlling\n",
            "bundle:smoke-result")
        historical = study.parent / "EXPLORATORY-HISTORICAL-MANIFEST-v1.tsv"
        if historical.is_file():
            repo = study.parents[2]
            history = list(csv.DictReader(historical.open(), delimiter="\t"))
            add(reasons, len(history) == 83 and all((repo / item["path"]).is_file()
                and hashlib.sha256((repo / item["path"]).read_bytes()).hexdigest() == item["sha256"]
                and (repo / item["path"]).stat().st_size == int(item["size_bytes"])
                for item in history), "bundle:historical-custody")
        else:
            reasons.append("bundle:historical-missing")
        bias_v1=study.parent/"BIAS-CONFIRMATION-V1-MANIFEST.tsv"
        if bias_v1.is_file():
            repo=study.parents[2]
            prior=list(csv.DictReader(bias_v1.open(),delimiter="\t"))
            add(reasons,len(prior)==223 and all((repo/item["path"]).is_file()
                and hashlib.sha256((repo/item["path"]).read_bytes()).hexdigest()==item["sha256"]
                and (repo/item["path"]).stat().st_size==int(item["size_bytes"])
                for item in prior),"bundle:bias-v1-custody")
        else:
            reasons.append("bundle:bias-v1-missing")

    reasons = sorted(set(reasons))
    candidate_ms = candidate_center / 1_000_000
    return {"schema": "canonical-v2-bias-confirmation-analysis-v1",
            "status": "PASS" if not reasons else "FAIL", "reasons": reasons,
            "classification": "POSITION_BALANCED_CONFIRMATION" if not reasons else "INVALID",
            "threshold": None, "row_count": len(rows), "smoke_count": len(smoke),
            "schedule": [list(item) for item in actual_schedule], "pairs": pairs,
            "durable": durable, "mapping": mapping, "commit": commit,
            "position_balanced": {"control_ns": control_center, "candidate_ns": candidate_center,
                "improvement_percent": (control_center - candidate_center) * 100 / control_center,
                "candidate_throughput_mib_s": 100_000_000_000 / candidate_center,
                "candidate_gap_ms": {str(target): candidate_ms - target
                                     for target in (500, 400, 333.333, 250)}},
            "external_by_execution_position": external,
            "work": {"control_q": sorted({row["q_high_water"] for row in measured if row["screen_arm"] == "A"}),
                     "candidate_q": sorted({row["q_high_water"] for row in measured if row["screen_arm"] == "B"}),
                     "control_allocated_store": sorted({row["physical_store_allocated_bytes"] for row in measured if row["screen_arm"] == "A"}),
                     "candidate_allocated_store": sorted({row["physical_store_allocated_bytes"] for row in measured if row["screen_arm"] == "B"}),
                     "candidate_sql_calls": sorted({row["sql_calls"] for row in measured if row["screen_arm"] == "B"}),
                     "candidate_blob_writes": sorted({row["row_blob_writes"] for row in measured if row["screen_arm"] == "B"}),
                     "candidate_pager_bytes": sorted({row["sqlite_main_db_pager_write_bytes"] for row in measured if row["screen_arm"] == "B"})},
            "limitations": ["Confirmation only; no promotion threshold.",
                "OS/filesystem cache warm-or-unknown.",
                "Instructions/cycles unavailable on this host.",
                "Allocated bytes are not physical-I/O or exclusive-extent evidence."]}


def main():
    if sys.argv[1:] == ["--self-test"]:
        rows=[]
        for pair, prep, order in [(2,"AB","AB"),(3,"BA","BA"),(4,"BA","AB"),(5,"AB","BA")]:
            for arm in order:
                rows.append({"screen_pair":pair,"screen_arm":arm,"screen_prepare_order":prep,
                             "screen_order":order,"screen_execution_position":order.index(arm)+1,
                             "screen_preparation_position":prep.index(arm)+1,"x":100+pair*10+(arm=="B")})
        value=centers(rows,"x")
        assert set(value)=={"A","B"} and all(len(value[arm]["execution_positions"][str(pos)]["values"])==2
                                                 for arm in ("A","B") for pos in (1,2))
        repo=Path(__file__).resolve().parents[4]
        plan=expected_invocations(repo/"target/phase4-canonical-v2-closure-20260821-v1/bias-confirmation-v1/results-v1")
        assert len(plan)==38 and len(SMOKE_EXACT)==7
        assert {(item["prepare_order"],item["execute_order"]) for item in plan if item["kind"]=="measured"}=={("AB","AB"),("BA","BA"),("BA","AB"),("AB","BA")}
        print("self-test PASS orthogonal_positions=4 pair_centers=balanced plan=38 smoke_contracts=7")
        return 0
    if len(sys.argv) != 2:
        raise SystemExit(f"usage: {sys.argv[0]} RESULTS-DIR | --self-test")
    try:
        result = analyze(sys.argv[1])
    except Exception as error:
        result = {"schema":"canonical-v2-bias-confirmation-analysis-v1","status":"FAIL",
                  "classification":"INVALID","threshold":None,
                  "reasons":[f"malformed-bundle:{type(error).__name__}:{error}"]}
    print(json.dumps(result, sort_keys=True, separators=(",", ":")))
    return result["status"] != "PASS"


if __name__ == "__main__":
    raise SystemExit(main())
