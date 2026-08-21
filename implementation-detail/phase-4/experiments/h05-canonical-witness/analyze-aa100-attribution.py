#!/usr/bin/env python3
"""Independent CP-0009 100-MiB A/A allocation-attribution analyzer."""

import copy
import csv
import hashlib
import importlib.util
import json
import re
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[3]
HELPER = HERE / "analyze-screen.py"
HELPER_SHA = "9819e7bc36fe4bb2ba5ad422dd584044f8d78d205bf892a59dd9071f9b34379c"


def sha256(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


if sha256(HELPER) != HELPER_SHA:
    raise RuntimeError("frozen analyzer helper SHA-256 mismatch")
SPEC = importlib.util.spec_from_file_location("h05_screen_contract", HELPER)
H05 = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(H05)

CONTROL = "9cda87ee7fd92784281a6ec7ee3045eb661681d8b7b930dd36546119ae4749d7"
CONTROL_SOURCE = "3284c3bdfe20426df78b4cb8ef310248e1e4f644b8422d79c4689653d870652a"
FIXTURE = "63b3695b8c117b5bc39885e0df0dcd0af1d49e575482bab16577d84b4f40eff4"
RUNNER = "7dac1e29dd553aecf46ca98e9768dd0ae69fdd42e7c0c72a035094c60481b97b"
RUNNER_PATH = HERE / "run-aa100-attribution.zsh"
RUNNER_TEST_PATH = HERE / "test-aa100-attribution.zsh"
CONTROL_PATH = REPO / "target/phase4-h05-canonical-witness-screen-20260821-v1/control/phase4_create_edit_benchmark-cp0009"
HISTORICAL_MANIFEST_SHA = "90595c15c0fb3992ef19110f197555d978f323f06bab0b1469b7517973a528ba"
PROFILE = H05.PROFILE
SOURCE_FINGERPRINT = H05.SOURCE_FINGERPRINT
CDC_SEQUENCE = H05.CDC_SEQUENCE
ROOT = H05.ROOT
TRANSITION = H05.TRANSITION
CLOSURE = H05.CLOSURE
EXPECTATIONS = "LFS-WP4M-EXPECTATIONS-3"
ORDERS = ["AB", "BA", "AB", "BA", "AB", "BA"]
PLAN = [(pair, arm, ORDERS[pair]) for pair in range(6) for arm in ORDERS[pair]]
LOGICAL_APPARENT_FIELDS = (
    "sqlite_pre_logical_database_bytes", "sqlite_post_logical_database_bytes",
    "sqlite_pre_apparent_database_bytes", "sqlite_post_apparent_database_bytes",
    "sqlite_pre_logical_store_bytes", "sqlite_post_logical_store_bytes",
    "sqlite_pre_apparent_store_bytes", "sqlite_post_apparent_store_bytes",
    "physical_db_apparent_bytes", "physical_authority_sidecar_apparent_bytes",
    "physical_journal_apparent_bytes",
)
HEX64 = re.compile(r"[0-9a-f]{64}").fullmatch
UTC = re.compile(r"2026-08-21T\d{2}:\d{2}:\d{2}Z").fullmatch


def add(reasons, condition, reason):
    if not condition:
        reasons.append(reason)


def read_jsonl(path):
    raw = Path(path).read_bytes()
    return raw, [json.loads(line) for line in raw.decode().splitlines() if line.strip()]


def read_tsv(path):
    return list(csv.DictReader(Path(path).open(), delimiter="\t"))


def ints(row, keys):
    return all(isinstance(row.get(key), int) and not isinstance(row.get(key), bool)
               and row[key] >= 0 for key in keys)


def row_contract(row):
    phase = H05.phase(row, "canonical_cas_mapping")
    return (
        row.get("status") == "PASS" and row.get("error") is None
        and row.get("operation") == "full" and row.get("size_bytes") == 104_857_600
        and row.get("input_size_bytes") == 104_857_600 and row.get("directory_entries") == 0
        and row.get("qualification_mode") == "C1-construction-proof"
        and row.get("qualification") is False and row.get("promotion") is False
        and row.get("rejection") is False and row.get("throughput_measurement_admissible") is False
        and row.get("purpose") == "profile_selection" and row.get("milestone") == "WP4-M"
        and row.get("candidate") == "K64-F64" and row.get("profile_id") == PROFILE
        and row.get("fixture") == "S1-100"
        and row.get("fixture_manifest") == "wp4m-retained-fixture-manifest.json"
        and row.get("build_profile") == "release" and row.get("debug_assertions") is False
        and row.get("base_preparation_in_measured_interval") is False
        and row.get("base_copy_method") == "physical-byte-copy-identical-database-authority-expectations"
        and row.get("source_cache_state") == "warm_or_unknown_after_manifest_preflight"
        and row.get("store_state") == "fresh_logical_store_cache_unknown"
        and row.get("schema") == "phase4-current-baseline-v1"
        and row.get("acceptance_scope") == "baseline" and row.get("candidate_comparison") is False
        and row.get("measurement_boundary") == "durable-submit"
        and row.get("runner_sha256") == RUNNER
        and row.get("runner_wall_ceiling_seconds") == 120
        and row.get("runner_command_ceiling_seconds") == 60
        and row.get("cpu_scope") == H05.CPU_SCOPE and row.get("cache_scope") == H05.CACHE_SCOPE
        and row.get("aa_schema") == "h05c-aa100-row-v1"
        and row.get("aa_label") in ("A", "B") and row.get("aa_sample_kind") == "placebo"
        and isinstance(row.get("aa_pair"), int) and row["aa_pair"] in range(6)
        and row.get("aa_order") == ORDERS[row["aa_pair"]]
        and row.get("iteration") == 930_000 + row["aa_pair"] and row.get("warmup") is False
        and row.get("aa_optimization_claim") is False and row.get("aa_validation_scope") == "capture-only"
        and row.get("executable_sha256") == CONTROL and row.get("aa_executable_sha256") == CONTROL
        and row.get("aa_executable_source_sha256") == CONTROL_SOURCE
        and row.get("aa_fixture_sha256") == FIXTURE and row.get("aa_fixture_size") == 104_857_600
        and row.get("aa_expectations_version") == EXPECTATIONS
        and row.get("pre_edit_database_sha256") == row.get("aa_base_database_sha256")
        and row.get("pre_edit_authority_sha256") == row.get("aa_base_authority_sha256")
        and row.get("pre_edit_expectations_sha256") == row.get("aa_base_expectations_sha256")
        and row.get("source_fingerprint") == SOURCE_FINGERPRINT
        and row.get("expected_cdc_references") == row.get("actual_cdc_references") == 5284
        and row.get("expected_cdc_sequence_fingerprint") == CDC_SEQUENCE
        and row.get("root_id") == ROOT and row.get("transition_id") == TRANSITION
        and row.get("ordered_closure_digest") == CLOSURE
        and H05.mutation_contract(row) and H05.timer_contract(row)
        and H05.resource_storage_contract(row) and H05.per_row_equations(row, True)
        and H05.has_ints(row, H05.EQUAL_WORK)
        and H05.phase_schema_contract(row, H05.PHASE_ORDER, "A")
        and (row.get("raw_bytes_hashed"), row.get("raw_hashes"),
             row.get("canonical_id_bytes_hashed"), row.get("canonical_id_hashes"),
             row.get("canonical_new_write_bytes"), row.get("mapping_bytes_rewritten"))
            == (104_857_600, 5284, 105_291_554, 5372, 105_291_554, 365_262)
        and (phase.get("construction_source_hash_bytes"),
             phase.get("construction_source_hashes"), phase.get("construction_cdc_entries"))
            == (104_857_600, 1, 5284)
        and not any("canonical_commitment" in key for counter in row.get("phase_counters", [])
                    for key in counter)
        and row.get("screen_residue") == []
    )


def analyze(rows, snapshots, evidence_reasons=()):
    reasons = list(evidence_reasons)
    actual_plan = [(row.get("aa_pair"), row.get("aa_label"), row.get("aa_order")) for row in rows]
    add(reasons, len(rows) == 12, "schedule:row-count")
    add(reasons, actual_plan == PLAN, "schedule:order")
    for index, row in enumerate(rows):
        add(reasons, row_contract(row), f"row:{index}:contract")

    pairs = []
    for pair, order in enumerate(ORDERS):
        group = [row for row in rows if row.get("aa_pair") == pair]
        if len(group) != 2 or {row.get("aa_label") for row in group} != {"A", "B"}:
            reasons.append(f"pair:{pair}:membership")
            continue
        a = next(row for row in group if row["aa_label"] == "A")
        b = next(row for row in group if row["aa_label"] == "B")
        add(reasons, a.get("aa_order") == b.get("aa_order") == order, f"pair:{pair}:order")
        add(reasons, a.get("aa_base_database_sha256") == b.get("aa_base_database_sha256")
            and a.get("aa_base_authority_sha256") == b.get("aa_base_authority_sha256")
            and a.get("aa_base_expectations_sha256") == b.get("aa_base_expectations_sha256"),
            f"pair:{pair}:byte-identical-start")
        add(reasons, a.get("aa_post_database_sha256") == b.get("aa_post_database_sha256")
            and a.get("aa_post_authority_sha256") == b.get("aa_post_authority_sha256"),
            f"pair:{pair}:byte-identical-final")
        add(reasons, all(a.get(key) == b.get(key) for key in H05.EQUAL_WORK),
            f"pair:{pair}:work-equality")
        add(reasons, a.get("phase_counters") == b.get("phase_counters"),
            f"pair:{pair}:phase-equality")
        add(reasons, all(a.get(key) == b.get(key) for key in H05.Q_COMPONENTS)
            and a.get("q_high_water") == b.get("q_high_water")
            and all(isinstance(row.get("q_report_output_bytes"), int)
                    and row["q_report_output_bytes"] > 0 for row in (a, b)),
            f"pair:{pair}:q-equality")
        add(reasons, all(a.get(key) == b.get(key) for key in LOGICAL_APPARENT_FIELDS),
            f"pair:{pair}:logical-apparent-equality")
        pairs.append({"pair": pair, "order": order,
                      "A_ns": a.get("durable_capture_total_wall_ns"),
                      "B_ns": b.get("durable_capture_total_wall_ns")})

    expected_snapshot_order = []
    for pair, order in enumerate(ORDERS):
        expected_snapshot_order += [(pair, "A", "PRE"), (pair, "B", "PRE")]
        expected_snapshot_order += [(pair, arm, "T0") for arm in order]
    for pair in range(6):
        expected_snapshot_order += [(pair, "A", "T1"), (pair, "B", "T1")]
    parsed = []
    for row in snapshots:
        try:
            parsed.append((int(row["pair"]), row["arm"], row["snapshot"]))
        except Exception:
            reasons.append("snapshot:syntax")
    add(reasons, len(snapshots) == 36 and parsed == expected_snapshot_order
        and len(set(parsed)) == 36, "snapshot:shape-order")
    numeric = (
        "monotonic_ns", "database_logical_bytes", "database_apparent_bytes",
        "database_allocated_bytes", "authority_apparent_bytes", "authority_allocated_bytes",
        "journal_apparent_bytes", "journal_allocated_bytes", "store_logical_bytes",
        "store_apparent_bytes", "store_allocated_bytes", "expectations_apparent_bytes",
        "expectations_allocated_bytes", "journal_present", "wal_present", "shm_present",
    )
    snapshot_map = {}
    for item in snapshots:
        try:
            item = dict(item)
            for key in numeric:
                item[key] = int(item[key])
            key = (int(item["pair"]), item["arm"], item["snapshot"])
            snapshot_map[key] = item
            add(reasons, key[0] in range(6) and item.get("order") == ORDERS[key[0]]
                and item.get("arm") in ("A", "B") and UTC(item.get("snapshot_utc", "")),
                f"snapshot:{key}:envelope")
            add(reasons, all(item[field] >= 0 for field in numeric), f"snapshot:{key}:nonnegative")
            add(reasons, item["integrity_check"] == "ok"
                and (item["journal_present"], item["wal_present"], item["shm_present"]) == (0, 0, 0),
                f"snapshot:{key}:integrity-residue")
            add(reasons, HEX64(item["database_sha256"]) and HEX64(item["authority_sha256"])
                and HEX64(item["expectations_sha256"]) and item["fixture_sha256"] == FIXTURE,
                f"snapshot:{key}:hashes")
            add(reasons, item["authority_apparent_bytes"] == 32
                and item["store_logical_bytes"] == item["database_logical_bytes"] + 32
                and item["store_apparent_bytes"] == item["database_apparent_bytes"] + 32
                and item["store_allocated_bytes"] == item["database_allocated_bytes"]
                    + item["authority_allocated_bytes"] + item["journal_allocated_bytes"],
                f"snapshot:{key}:equations")
        except Exception:
            reasons.append("snapshot:malformed")
    monotonic = [snapshot_map[key]["monotonic_ns"] for key in parsed if key in snapshot_map]
    add(reasons, len(monotonic) == 36 and all(a < b for a, b in zip(monotonic, monotonic[1:])),
        "snapshot:monotonic-order")

    pair_unstable = []
    time_unstable = []
    cross_pair_unstable = []
    allocation_rows = []
    for pair in range(6):
        group = [row for row in rows if row.get("aa_pair") == pair]
        by_arm = {row["aa_label"]: row for row in group if row.get("aa_label") in ("A", "B")}
        for snapshot in ("PRE", "T0", "T1"):
            a = snapshot_map.get((pair, "A", snapshot))
            b = snapshot_map.get((pair, "B", snapshot))
            if not a or not b:
                continue
            if snapshot == "PRE":
                add(reasons, a["database_sha256"] == b["database_sha256"]
                    and a["authority_sha256"] == b["authority_sha256"]
                    and a["expectations_sha256"] == b["expectations_sha256"],
                    f"snapshot:{pair}:PRE:byte-equality")
            else:
                add(reasons, a["database_sha256"] == b["database_sha256"]
                    and a["authority_sha256"] == b["authority_sha256"],
                    f"snapshot:{pair}:{snapshot}:byte-equality")
            add(reasons, a["database_logical_bytes"] == b["database_logical_bytes"]
                and a["database_apparent_bytes"] == b["database_apparent_bytes"]
                and a["store_logical_bytes"] == b["store_logical_bytes"]
                and a["store_apparent_bytes"] == b["store_apparent_bytes"],
                f"snapshot:{pair}:{snapshot}:logical-apparent")
            differences = {}
            for field in ("database_allocated_bytes", "store_allocated_bytes"):
                if a[field] != b[field]:
                    differences[field] = {"A": a[field], "B": b[field]}
            if differences:
                pair_unstable.append({"pair": pair, "snapshot": snapshot,
                                      "differences": differences})
            allocation_rows.append({"pair": pair, "snapshot": snapshot,
                                    "A_database": a["database_allocated_bytes"],
                                    "B_database": b["database_allocated_bytes"],
                                    "A_store": a["store_allocated_bytes"],
                                    "B_store": b["store_allocated_bytes"]})
        for arm in ("A", "B"):
            pre = snapshot_map.get((pair, arm, "PRE"))
            t0 = snapshot_map.get((pair, arm, "T0"))
            t1 = snapshot_map.get((pair, arm, "T1"))
            row = by_arm.get(arm)
            if not pre or not t0 or not t1 or not row:
                continue
            add(reasons, pre["database_sha256"] == row.get("aa_base_database_sha256")
                and pre["authority_sha256"] == row.get("aa_base_authority_sha256")
                and pre["expectations_sha256"] == row.get("aa_base_expectations_sha256"),
                f"snapshot:{pair}:{arm}:PRE-custody")
            add(reasons, pre["expectations_sha256"] == t0["expectations_sha256"] == t1["expectations_sha256"]
                and pre["fixture_sha256"] == t0["fixture_sha256"] == t1["fixture_sha256"] == FIXTURE,
                f"snapshot:{pair}:{arm}:input-custody")
            add(reasons, t0["database_sha256"] == row.get("aa_post_database_sha256")
                and t0["authority_sha256"] == row.get("aa_post_authority_sha256")
                and t0["database_logical_bytes"] == row.get("sqlite_post_logical_database_bytes")
                and t0["database_apparent_bytes"] == row.get("physical_db_apparent_bytes")
                and t0["database_allocated_bytes"] == row.get("physical_db_allocated_bytes")
                and t0["authority_apparent_bytes"] == row.get("physical_authority_sidecar_apparent_bytes")
                and t0["authority_allocated_bytes"] == row.get("physical_authority_sidecar_allocated_bytes")
                and t0["store_logical_bytes"] == row.get("sqlite_post_logical_store_bytes")
                and t0["store_apparent_bytes"] == row.get("sqlite_post_apparent_store_bytes")
                and t0["store_allocated_bytes"] == row.get("physical_store_allocated_bytes"),
                f"snapshot:{pair}:{arm}:T0-native")
            stable_fields = (
                "database_sha256", "authority_sha256", "expectations_sha256", "fixture_sha256",
                "database_logical_bytes", "database_apparent_bytes", "authority_apparent_bytes",
                "journal_apparent_bytes", "store_logical_bytes", "store_apparent_bytes",
                "expectations_apparent_bytes", "integrity_check", "journal_present",
                "wal_present", "shm_present",
            )
            add(reasons, all(t0[field] == t1[field] for field in stable_fields),
                f"snapshot:{pair}:{arm}:T0-T1-content")
            add(reasons, t1["monotonic_ns"] - t0["monotonic_ns"] >= 2_000_000_000,
                f"snapshot:{pair}:{arm}:T1-delay")
            changes = {}
            for field in ("database_allocated_bytes", "store_allocated_bytes"):
                if t0[field] != t1[field]:
                    changes[field] = {"T0": t0[field], "T1": t1[field]}
            if changes:
                time_unstable.append({"pair": pair, "arm": arm, "changes": changes})

    t0_times = [item["monotonic_ns"] for key, item in snapshot_map.items() if key[2] == "T0"]
    t1_times = [item["monotonic_ns"] for key, item in snapshot_map.items() if key[2] == "T1"]
    add(reasons, len(t0_times) == len(t1_times) == 12
        and min(t1_times) - max(t0_times) >= 2_000_000_000,
        "snapshot:shared-T1-delay")
    for pair in range(6):
        for arm in ("A", "B"):
            pre = snapshot_map.get((pair, arm, "PRE"))
            t0 = snapshot_map.get((pair, arm, "T0"))
            t1 = snapshot_map.get((pair, arm, "T1"))
            add(reasons, bool(pre and t0 and t1 and pre["monotonic_ns"] < t0["monotonic_ns"] < t1["monotonic_ns"]),
                f"snapshot:{pair}:{arm}:time-order")
    for snapshot in ("PRE", "T0", "T1"):
        groups = {}
        for (pair, arm, label), item in snapshot_map.items():
            if label != snapshot:
                continue
            identity = tuple(item[field] for field in (
                "database_sha256", "authority_sha256", "database_logical_bytes",
                "database_apparent_bytes", "store_logical_bytes", "store_apparent_bytes"))
            groups.setdefault(identity, []).append((pair, arm, item))
        for identity, items in groups.items():
            if len({pair for pair, _, _ in items}) < 2:
                continue
            values = {(item["database_allocated_bytes"], item["store_allocated_bytes"])
                      for _, _, item in items}
            if len(values) > 1:
                cross_pair_unstable.append({
                    "snapshot": snapshot, "identity": list(identity),
                    "observations": [{"pair": pair, "arm": arm,
                                      "database": item["database_allocated_bytes"],
                                      "store": item["store_allocated_bytes"]}
                                     for pair, arm, item in items],
                })

    reasons = sorted(set(reasons))
    valid = not reasons
    unstable = valid and bool(pair_unstable or time_unstable or cross_pair_unstable)
    classification = (
        "EXACT_ALLOCATED_EQUALITY_UNSTABLE_ON_CP0009" if unstable else
        "H05 CLOSED / A/A EXACT-EQUALITY STABLE" if valid else
        "H05C_PHASE1_INVALID"
    )
    return {
        "schema": "h05c-aa100-analysis-v1",
        "status": "PASS" if valid else "FAIL",
        "classification": classification,
        "phase2_eligible": unstable,
        "reasons": reasons,
        "row_count": len(rows), "snapshot_count": len(snapshots),
        "plan": [list(item) for item in actual_plan],
        "pair_unstable": pair_unstable, "time_unstable": time_unstable,
        "cross_pair_unstable": cross_pair_unstable,
        "allocation_rows": allocation_rows, "placebo_timings": pairs,
        "optimization_claim": False,
        "historical_dispositions": {
            "H05_v7": "H05 MEASURED NO-GO / REVERT",
            "H05b": "H05B_NOT_JUSTIFIED / STOP",
        },
        "next_eligible_action": (
            "prospectively write H05c Phase-2 amendment" if unstable else
            "canonical-v2 recommendation; do not start it" if valid else
            "repair orchestration only in a fresh namespace"
        ),
        "limitations": [
            "Both labels are CP-0009 control; wall differences are placebo observations, not an optimization result.",
            "st_blocks*512 is allocated-block evidence, not physical I/O or exclusive-extent ownership.",
            "T1 is one shared delayed batch snapshot; earlier rows have longer T0-to-T1 intervals.",
            "No privileged kernel exec observer is available; no-candidate proof is bounded to the single-operand runner, invocation ledger, row-start hashes, and native self-report.",
        ],
    }


def audit_evidence(result_dir, rows):
    result_dir = Path(result_dir).resolve()
    artifact = result_dir.parent
    reasons = []
    required = {
        "attempt": result_dir / "AA100-ATTEMPT-v1.txt",
        "status": result_dir / "RUN-STATUS-v1.txt",
        "lock": result_dir / "LOCK-TIMEOUT-v1.txt",
        "schedule": result_dir / "SCHEDULE-ASSERTION-v1.txt",
        "custody": result_dir / "AA100-INPUT-CUSTODY-v1.tsv",
        "invocations": result_dir / "AA100-INVOCATION-PLAN-v1.tsv",
        "actual": result_dir / "AA100-ACTUAL-INVOCATIONS-v1.tsv",
        "starts": result_dir / "ROW-STARTS-v1.txt",
        "command": result_dir / "COMMAND-v1.txt",
        "environment": result_dir / "ENVIRONMENT-v1.txt",
        "quiescence": result_dir / "QUIESCENCE-v1.txt",
        "conflicts": result_dir / "QUIESCENCE-CONFLICTS-v1.txt",
        "custody_check": result_dir / "EXECUTION-CUSTODY-v1.txt",
        "candidate_post": result_dir / "CANDIDATE-NONINVOCATION-CUSTODY-POST-v1.txt",
        "history_post": result_dir / "HISTORICAL-H05-H05B-VERIFICATION-POST-v1.txt",
    }
    for name, path in required.items():
        add(reasons, path.is_file(), f"evidence:{name}:missing")
    if reasons:
        return reasons
    methodology = artifact / "PROSPECTIVE-METHODOLOGY-CUSTODY-v2.tsv"
    add(reasons, methodology.is_file(), "evidence:methodology-custody:missing")
    method_sha = sha256(methodology) if methodology.is_file() else "MISSING"
    expected_command = f"/usr/bin/env H05C_METHOD_CUSTODY_SHA256={method_sha} {RUNNER_PATH} --execute"
    add(reasons, re.fullmatch(r"attempt=1 started_utc=2026-08-21T\d{2}:\d{2}:\d{2}Z command="
                              + re.escape(expected_command) + r"\n",
                              required["attempt"].read_text()) is not None, "evidence:attempt")
    status = dict(token.split("=", 1) for token in required["status"].read_text().split()
                  if token.count("=") == 1)
    expected = {"status": "PASS", "timeout": "false", "study_executed_exactly_once": "true",
                "placebo_rows": "12", "snapshots": "36"}
    add(reasons, all(status.get(key) == value for key, value in expected.items())
        and status.get("wall_seconds", "").isdigit() and int(status["wall_seconds"]) <= 120,
        "evidence:status")
    lock = required["lock"].read_text()
    add(reasons, "BENCHMARK_LOCK=H05C_AA100" in lock
        and "complete_study_wall_ceiling_seconds=120" in lock
        and "lock_acquired_utc=" in lock and "lock_released_utc=" in lock,
        "evidence:lock")
    lock_paths = [line.split("=", 1)[1] for line in lock.splitlines() if line.startswith("lock_path=")]
    add(reasons, len(lock_paths) == 1 and not Path(lock_paths[0]).exists(), "evidence:lock-released")
    expected_schedule = "constructed plan:\n" + "\n".join(f"pair {i}  {order}" for i, order in enumerate(ORDERS)) \
        + "\nexpected plan:\n" + "\n".join(f"pair {i}  {order}" for i, order in enumerate(ORDERS)) \
        + "\nschedule assertion: PASS\nrow sequence: A B | B A | A B | B A | A B | B A\n"
    add(reasons, required["schedule"].read_text() == expected_schedule, "evidence:schedule")
    command = required["command"].read_text()
    add(reasons, command == expected_command + "\n", "evidence:command")
    add(reasons, f"methodology_custody_sha256={method_sha}\n" in required["environment"].read_text(),
        "evidence:environment-anchor")
    add(reasons, required["quiescence"].read_text().startswith("quiescence=PASS ")
        and required["conflicts"].read_bytes() == b"", "evidence:quiescence")
    custody = read_tsv(required["custody"])
    row_map = {(row.get("aa_pair"), row.get("aa_label")): row for row in rows}
    expected_custody = []
    for pair, order in enumerate(ORDERS):
        for arm in ("A", "B"):
            row = row_map.get((pair, arm), {})
            expected_custody.append({
                "pair": str(pair), "order": order, "arm": arm,
                "iteration": str(930_000 + pair), "fixture_sha256": FIXTURE,
                "base_database_sha256": str(row.get("aa_base_database_sha256", "")),
                "base_authority_sha256": str(row.get("aa_base_authority_sha256", "")),
                "base_expectations_sha256": str(row.get("aa_base_expectations_sha256", "")),
                "expectations_version": EXPECTATIONS, "executable_sha256": CONTROL,
            })
    add(reasons, custody == expected_custody, "evidence:custody-exact")
    invocations = read_tsv(required["invocations"])
    work = result_dir / "work-v1"
    expected_invocations = []
    sequence = 0
    for pair, order in enumerate(ORDERS):
        iteration = 930_000 + pair
        sequence += 1
        expected_invocations.append({"sequence": str(sequence), "kind": "prepare", "pair": str(pair),
            "arm": "-", "executable_sha256": CONTROL,
            "command": f"{CONTROL_PATH} --fast-prepare {work}/pair-{pair}/prep 104857600 write {iteration}"})
        for arm in order:
            sequence += 1
            expected_invocations.append({"sequence": str(sequence), "kind": "row", "pair": str(pair),
                "arm": arm, "executable_sha256": CONTROL,
                "command": f"{CONTROL_PATH} --fast-row {work}/pair-{pair}/{arm} 104857600 write {iteration} false capture-only"})
    add(reasons, invocations == expected_invocations, "evidence:invocation-plan-exact")
    forbidden = ("phase4_create_edit_benchmark-h05", "15a668739e96", "LFS-H05-EXPECTATIONS")
    add(reasons, all(all(token not in item.get("command", "") for token in forbidden)
                     for item in invocations), "evidence:single-control-operand")
    actual = read_tsv(required["actual"])
    expected_actual = []
    for planned in expected_invocations:
        pair = int(planned["pair"])
        for event, exit_code in (("started", "-"), ("completed", "0")):
            expected_actual.append({"sequence": planned["sequence"], "event": event,
                "kind": planned["kind"], "pair": planned["pair"], "order": ORDERS[pair],
                "arm": planned["arm"], "iteration": str(930_000 + pair),
                "executable_sha256": CONTROL, "command": planned["command"], "exit": exit_code})
    actual_without_utc = [{key: item.get(key, "") for key in expected_actual[0]} for item in actual] if expected_actual else []
    add(reasons, actual_without_utc == expected_actual and len(actual) == 36
        and all(UTC(item.get("utc", "")) for item in actual), "evidence:actual-invocations-exact")
    starts = required["starts"].read_text().splitlines()
    pattern = re.compile(r"row_(started|completed)_utc=\S+ pair=(\d+) order=(AB|BA) arm=([AB]) executable_sha256=([0-9a-f]{64})$")
    parsed = []
    for line in starts:
        match = pattern.fullmatch(line)
        if not match:
            reasons.append("evidence:starts-syntax")
        else:
            parsed.append((match.group(1), int(match.group(2)), match.group(3), match.group(4), match.group(5)))
    expected_starts = []
    for pair, arm, order in PLAN:
        expected_starts += [("started", pair, order, arm, CONTROL),
                            ("completed", pair, order, arm, CONTROL)]
    add(reasons, parsed == expected_starts and len(parsed) == 24, "evidence:starts-order")
    custody_text = required["custody_check"].read_text()
    add(reasons, CONTROL in custody_text and CONTROL_SOURCE in custody_text and FIXTURE in custody_text,
        "evidence:execution-custody")
    add(reasons, "methodology_custody=PASS" in custody_text, "evidence:methodology-execution-custody")
    if methodology.is_file():
        method_rows = read_tsv(methodology)
        expected_paths = [RUNNER_PATH, RUNNER_TEST_PATH, Path(__file__).resolve(), HELPER,
                          artifact / "PROSPECTIVE-AA100-PREREGISTRATION-v1.md",
                          artifact / "PROSPECTIVE-AA100-REPAIR-v2.md",
                          artifact / "HISTORICAL-H05-H05B-MANIFEST-v1.tsv",
                          artifact / "HISTORICAL-H05-H05B-VERIFICATION-PRE-v1.txt",
                          artifact / "CANDIDATE-NONINVOCATION-CUSTODY-PRE-v1.txt",
                          artifact / "AA100-PHASE1-v1-MANIFEST.tsv"]
        labels = ["runner", "runner-test", "analyzer", "analyzer-helper", "original-preregistration",
                  "repair-preregistration", "historical-manifest", "historical-verification-pre",
                  "candidate-custody-pre", "phase1-v1-manifest"]
        expected_methods = [{"label": label, "sha256": sha256(path), "path": str(path)}
                            for label, path in zip(labels, expected_paths)]
        add(reasons, method_rows == expected_methods, "evidence:methodology-custody-exact")
    runner_text = RUNNER_PATH.read_text()
    add(reasons, sha256(RUNNER_PATH) == RUNNER and all(token not in runner_text for token in forbidden),
        "evidence:runner-single-operand")
    history = artifact / "HISTORICAL-H05-H05B-MANIFEST-v1.tsv"
    add(reasons, history.is_file() and sha256(history) == HISTORICAL_MANIFEST_SHA,
        "evidence:historical-manifest")
    if history.is_file():
        for item in read_tsv(history):
            path = Path(item["path"])
            if not path.is_absolute():
                path = REPO / path
            actual = hashlib.sha256(path.read_bytes()).hexdigest() if path.is_file() else "MISSING"
            size = path.stat().st_size if path.is_file() else -1
            add(reasons, actual == item["sha256"] and size == int(item["size_bytes"]),
                "evidence:historical-custody")
    v1_manifest = artifact / "AA100-PHASE1-v1-MANIFEST.tsv"
    add(reasons, v1_manifest.is_file(), "evidence:phase1-v1-manifest")
    if v1_manifest.is_file():
        for item in read_tsv(v1_manifest):
            path = REPO / item["path"]
            actual = sha256(path) if path.is_file() else "MISSING"
            size = path.stat().st_size if path.is_file() else -1
            add(reasons, actual == item["sha256"] and size == int(item["size_bytes"]),
                "evidence:phase1-v1-custody")
    verification_fields = {"status": "PASS", "entries": "97", "mismatches": "0",
        "manifest_sha256": HISTORICAL_MANIFEST_SHA, "H05_v7": "H05 MEASURED NO-GO / REVERT",
        "H05b": "H05B_NOT_JUSTIFIED / STOP", "reopened": "false"}
    for label, path in (("pre", artifact / "HISTORICAL-H05-H05B-VERIFICATION-PRE-v1.txt"),
                        ("post", required["history_post"])):
        values = dict(line.split("=", 1) for line in path.read_text().splitlines() if "=" in line)
        add(reasons, values == verification_fields, f"evidence:historical-verification-{label}")
    candidate_pre = artifact / "CANDIDATE-NONINVOCATION-CUSTODY-PRE-v1.txt"
    pre = dict(line.split("=", 1) for line in candidate_pre.read_text().splitlines() if "=" in line)
    post = dict(line.split("=", 1) for line in required["candidate_post"].read_text().splitlines() if "=" in line)
    add(reasons, pre.pop("classification", None) == "READ_ONLY_NONINVOCATION_CUSTODY_PRE"
        and post.pop("classification", None) == "READ_ONLY_NONINVOCATION_CUSTODY_POST"
        and pre == post and set(pre) == {f"candidate_{kind}_{field}" for kind in ("executable", "source")
                                        for field in ("path", "sha256", "size", "mtime")}
        and all(HEX64(pre[f"candidate_{kind}_sha256"]) for kind in ("executable", "source")),
        "evidence:candidate-pre-post-custody")
    return reasons


def synthetic():
    source_rows, _ = H05.synthetic_rows()
    template = copy.deepcopy(next(row for row in source_rows if row["screen_arm"] == "A"))
    rows = []
    snapshots = []
    for pair, order in enumerate(ORDERS):
        base_db = f"{pair + 1:064x}"
        base_auth = f"{pair + 101:064x}"
        base_exp = f"{pair + 201:064x}"
        post_db = f"{pair + 301:064x}"
        post_auth = f"{pair + 401:064x}"
        for arm in order:
            row = copy.deepcopy(template)
            row.update({
                "runner_sha256": RUNNER, "aa_schema": "h05c-aa100-row-v1",
                "aa_label": arm, "aa_pair": pair, "aa_order": order,
                "iteration": 930_000 + pair, "warmup": False,
                "aa_sample_kind": "placebo", "aa_optimization_claim": False,
                "aa_validation_scope": "capture-only", "aa_executable_sha256": CONTROL,
                "aa_executable_source_sha256": CONTROL_SOURCE, "aa_fixture_sha256": FIXTURE,
                "aa_fixture_size": 104_857_600, "aa_expectations_version": EXPECTATIONS,
                "aa_base_database_sha256": base_db, "aa_base_authority_sha256": base_auth,
                "aa_base_expectations_sha256": base_exp, "aa_post_database_sha256": post_db,
                "aa_post_authority_sha256": post_auth, "executable_sha256": CONTROL,
                "pre_edit_database_sha256": base_db, "pre_edit_authority_sha256": base_auth,
                "pre_edit_expectations_sha256": base_exp, "screen_residue": [],
            })
            rows.append(row)
        for arm in ("A", "B"):
            snapshots.append({
                "pair": str(pair), "order": order, "arm": arm, "snapshot": "PRE",
                "snapshot_utc": "2026-08-21T00:00:00Z", "monotonic_ns": str(pair * 10_000_000_000),
                "database_sha256": base_db, "authority_sha256": base_auth,
                "expectations_sha256": base_exp, "fixture_sha256": FIXTURE,
                "database_logical_bytes": "1000", "database_apparent_bytes": "1000",
                "database_allocated_bytes": "4096", "authority_apparent_bytes": "32",
                "authority_allocated_bytes": "4096", "journal_apparent_bytes": "0",
                "journal_allocated_bytes": "0", "store_logical_bytes": "1032",
                "store_apparent_bytes": "1032", "store_allocated_bytes": "8192",
                "expectations_apparent_bytes": "100", "expectations_allocated_bytes": "4096",
                "integrity_check": "ok", "journal_present": "0", "wal_present": "0", "shm_present": "0",
            })
        for arm in order:
            row = next(row for row in rows if row["aa_pair"] == pair and row["aa_label"] == arm)
            snapshots.append({
                "pair": str(pair), "order": order, "arm": arm, "snapshot": "T0",
                "snapshot_utc": "2026-08-21T00:00:01Z", "monotonic_ns": str(pair * 10_000_000_000 + 1_000_000_000),
                "database_sha256": post_db, "authority_sha256": post_auth,
                "expectations_sha256": base_exp, "fixture_sha256": FIXTURE,
                "database_logical_bytes": str(row["sqlite_post_logical_database_bytes"]),
                "database_apparent_bytes": str(row["physical_db_apparent_bytes"]),
                "database_allocated_bytes": str(row["physical_db_allocated_bytes"]),
                "authority_apparent_bytes": str(row["physical_authority_sidecar_apparent_bytes"]),
                "authority_allocated_bytes": str(row["physical_authority_sidecar_allocated_bytes"]),
                "journal_apparent_bytes": "0", "journal_allocated_bytes": "0",
                "store_logical_bytes": str(row["sqlite_post_logical_store_bytes"]),
                "store_apparent_bytes": str(row["sqlite_post_apparent_store_bytes"]),
                "store_allocated_bytes": str(row["physical_store_allocated_bytes"]),
                "expectations_apparent_bytes": "100", "expectations_allocated_bytes": "4096",
                "integrity_check": "ok", "journal_present": "0", "wal_present": "0", "shm_present": "0",
            })
    for pair, order in enumerate(ORDERS):
        for arm in ("A", "B"):
            t0 = next(item for item in snapshots if item["pair"] == str(pair) and item["arm"] == arm and item["snapshot"] == "T0")
            item = dict(t0)
            item.update({"snapshot": "T1", "snapshot_utc": "2026-08-21T00:00:03Z",
                         "monotonic_ns": "0"})
            snapshots.append(item)
    for index, item in enumerate(snapshots):
        item["monotonic_ns"] = str(index * 100_000_000 if index < 24
                                   else 4_400_000_000 + (index - 24) * 100_000_000)
    return rows, snapshots


def synthetic_bundle(artifact, rows):
    artifact = Path(artifact)
    result = artifact / "aa-results-v1"
    result.mkdir(parents=True)
    result = result.resolve()
    artifact = result.parent
    prereg = artifact / "PROSPECTIVE-AA100-PREREGISTRATION-v1.md"
    prereg.write_bytes((REPO / "target/phase4-h05c-aa100-attribution-20260821-v1/PROSPECTIVE-AA100-PREREGISTRATION-v1.md").read_bytes())
    repair = artifact / "PROSPECTIVE-AA100-REPAIR-v2.md"
    repair.write_bytes((REPO / "target/phase4-h05c-aa100-attribution-20260821-v1/PROSPECTIVE-AA100-REPAIR-v2.md").read_bytes())
    history = artifact / "HISTORICAL-H05-H05B-MANIFEST-v1.tsv"
    history.write_bytes((REPO / "target/phase4-h05c-aa100-attribution-20260821-v1/HISTORICAL-H05-H05B-MANIFEST-v1.tsv").read_bytes())
    candidate_pre = artifact / "CANDIDATE-NONINVOCATION-CUSTODY-PRE-v1.txt"
    candidate_pre.write_bytes((REPO / "target/phase4-h05c-aa100-attribution-20260821-v1/CANDIDATE-NONINVOCATION-CUSTODY-PRE-v1.txt").read_bytes())
    history_text = ("status=PASS\nentries=97\nmismatches=0\n"
                    f"manifest_sha256={HISTORICAL_MANIFEST_SHA}\n"
                    "H05_v7=H05 MEASURED NO-GO / REVERT\n"
                    "H05b=H05B_NOT_JUSTIFIED / STOP\nreopened=false\n")
    (artifact / "HISTORICAL-H05-H05B-VERIFICATION-PRE-v1.txt").write_text(history_text)
    v1_manifest = artifact / "AA100-PHASE1-v1-MANIFEST.tsv"
    v1_manifest.write_bytes((REPO / "target/phase4-h05c-aa100-attribution-20260821-v1/AA100-PHASE1-v1-MANIFEST.tsv").read_bytes())
    method_paths = [RUNNER_PATH, RUNNER_TEST_PATH, Path(__file__).resolve(), HELPER, prereg, repair,
                    history, artifact / "HISTORICAL-H05-H05B-VERIFICATION-PRE-v1.txt",
                    candidate_pre, v1_manifest]
    labels = ["runner", "runner-test", "analyzer", "analyzer-helper", "original-preregistration",
              "repair-preregistration", "historical-manifest", "historical-verification-pre",
              "candidate-custody-pre", "phase1-v1-manifest"]
    methodology = artifact / "PROSPECTIVE-METHODOLOGY-CUSTODY-v2.tsv"
    methodology.write_text("label\tsha256\tpath\n" + "".join(
        f"{label}\t{sha256(path)}\t{path}\n" for label, path in zip(labels, method_paths)))
    method_sha = sha256(methodology)
    command = f"/usr/bin/env H05C_METHOD_CUSTODY_SHA256={method_sha} {RUNNER_PATH} --execute"
    (result / "AA100-ATTEMPT-v1.txt").write_text(
        f"attempt=1 started_utc=2026-08-21T00:00:00Z command={command}\n")
    (result / "RUN-STATUS-v1.txt").write_text(
        "status=PASS timeout=false study_executed_exactly_once=true placebo_rows=12 snapshots=36 wall_seconds=1\n")
    (result / "LOCK-TIMEOUT-v1.txt").write_text(
        f"BENCHMARK_LOCK=H05C_AA100\nlock_path={artifact / 'absent.lock'}\n"
        "lock_acquired_utc=2026-08-21T00:00:00Z\ncomplete_study_wall_ceiling_seconds=120\n"
        "per_command_ceiling_seconds=60\nlock_released_utc=2026-08-21T00:00:01Z\n")
    schedule = "constructed plan:\n" + "\n".join(f"pair {i}  {order}" for i, order in enumerate(ORDERS)) \
        + "\nexpected plan:\n" + "\n".join(f"pair {i}  {order}" for i, order in enumerate(ORDERS)) \
        + "\nschedule assertion: PASS\nrow sequence: A B | B A | A B | B A | A B | B A\n"
    (result / "SCHEDULE-ASSERTION-v1.txt").write_text(schedule)
    (result / "COMMAND-v1.txt").write_text(command + "\n")
    (result / "ENVIRONMENT-v1.txt").write_text(f"methodology_custody_sha256={method_sha}\n")
    (result / "QUIESCENCE-v1.txt").write_text("quiescence=PASS no prohibited task matched\n")
    (result / "QUIESCENCE-CONFLICTS-v1.txt").write_bytes(b"")
    (result / "EXECUTION-CUSTODY-v1.txt").write_text(
        f"{CONTROL}\n{CONTROL_SOURCE}\n{FIXTURE}\nmethodology_custody=PASS\n")
    custody = ["pair\torder\tarm\titeration\tfixture_sha256\tbase_database_sha256\tbase_authority_sha256\tbase_expectations_sha256\texpectations_version\texecutable_sha256"]
    row_map = {(row["aa_pair"], row["aa_label"]): row for row in rows}
    for pair, order in enumerate(ORDERS):
        for arm in ("A", "B"):
            row = row_map[(pair, arm)]
            custody.append("\t".join(map(str, (pair, order, arm, 930_000 + pair, FIXTURE,
                row["aa_base_database_sha256"], row["aa_base_authority_sha256"],
                row["aa_base_expectations_sha256"], EXPECTATIONS, CONTROL))))
    (result / "AA100-INPUT-CUSTODY-v1.tsv").write_text("\n".join(custody) + "\n")
    work = result / "work-v1"
    plan = []
    for pair, order in enumerate(ORDERS):
        iteration = 930_000 + pair
        plan.append(("prepare", pair, "-", f"{CONTROL_PATH} --fast-prepare {work}/pair-{pair}/prep 104857600 write {iteration}"))
        plan.extend(("row", pair, arm,
            f"{CONTROL_PATH} --fast-row {work}/pair-{pair}/{arm} 104857600 write {iteration} false capture-only")
            for arm in order)
    (result / "AA100-INVOCATION-PLAN-v1.tsv").write_text(
        "sequence\tkind\tpair\tarm\texecutable_sha256\tcommand\n" + "".join(
            f"{sequence}\t{kind}\t{pair}\t{arm}\t{CONTROL}\t{command_text}\n"
            for sequence, (kind, pair, arm, command_text) in enumerate(plan, 1)))
    actual = ["sequence\tevent\tutc\tkind\tpair\torder\tarm\titeration\texecutable_sha256\tcommand\texit"]
    for sequence, (kind, pair, arm, command_text) in enumerate(plan, 1):
        for event, exit_code in (("started", "-"), ("completed", "0")):
            actual.append("\t".join(map(str, (sequence, event, "2026-08-21T00:00:00Z", kind,
                pair, ORDERS[pair], arm, 930_000 + pair, CONTROL, command_text, exit_code))))
    (result / "AA100-ACTUAL-INVOCATIONS-v1.tsv").write_text("\n".join(actual) + "\n")
    starts = []
    for pair, arm, order in PLAN:
        for event in ("started", "completed"):
            starts.append(f"row_{event}_utc=2026-08-21T00:00:00Z pair={pair} order={order} arm={arm} executable_sha256={CONTROL}")
    (result / "ROW-STARTS-v1.txt").write_text("\n".join(starts) + "\n")
    post = candidate_pre.read_text().replace("READ_ONLY_NONINVOCATION_CUSTODY_PRE",
                                              "READ_ONLY_NONINVOCATION_CUSTODY_POST", 1)
    (result / "CANDIDATE-NONINVOCATION-CUSTODY-POST-v1.txt").write_text(post)
    (result / "HISTORICAL-H05-H05B-VERIFICATION-POST-v1.txt").write_text(history_text)
    return result


def self_test():
    rows, snapshots = synthetic()
    result = analyze(rows, snapshots)
    assert result["status"] == "PASS" and result["classification"] == "H05 CLOSED / A/A EXACT-EQUALITY STABLE"
    report_width_rows = copy.deepcopy(rows)
    report_width_rows[1]["q_report_output_bytes"] += 4
    assert analyze(report_width_rows, snapshots)["status"] == "PASS"
    for snapshot in ("PRE", "T1"):
        changed = copy.deepcopy(snapshots)
        item = next(item for item in changed if item["pair"] == "0" and item["arm"] == "B"
                    and item["snapshot"] == snapshot)
        item["database_allocated_bytes"] = str(int(item["database_allocated_bytes"]) + 4096)
        item["store_allocated_bytes"] = str(int(item["store_allocated_bytes"]) + 4096)
        result = analyze(rows, changed)
        assert result["status"] == "PASS" and result["phase2_eligible"], snapshot
    changed_rows = copy.deepcopy(rows)
    changed_snapshots = copy.deepcopy(snapshots)
    changed_row = next(row for row in changed_rows if row["aa_pair"] == 0 and row["aa_label"] == "B")
    for key in ("sqlite_post_allocated_database_bytes", "sqlite_post_allocated_store_bytes",
                "allocated_store_delta_bytes", "commit_return_db_allocated_bytes",
                "physical_db_allocated_bytes", "physical_store_allocated_bytes"):
        changed_row[key] += 4096
    item = next(item for item in changed_snapshots if item["pair"] == "0"
                and item["arm"] == "B" and item["snapshot"] == "T0")
    item["database_allocated_bytes"] = str(int(item["database_allocated_bytes"]) + 4096)
    item["store_allocated_bytes"] = str(int(item["store_allocated_bytes"]) + 4096)
    result = analyze(changed_rows, changed_snapshots)
    assert result["status"] == "PASS" and result["phase2_eligible"], "T0"
    changed = copy.deepcopy(snapshots)
    item = next(item for item in changed if item["pair"] == "0" and item["arm"] == "A" and item["snapshot"] == "T1")
    item["database_allocated_bytes"] = str(int(item["database_allocated_bytes"]) + 4096)
    item["store_allocated_bytes"] = str(int(item["store_allocated_bytes"]) + 4096)
    assert analyze(rows, changed)["phase2_eligible"]
    cross_rows, cross_snapshots = copy.deepcopy(rows), copy.deepcopy(snapshots)
    pair0 = {(item["arm"], item["snapshot"]): item for item in cross_snapshots if item["pair"] == "0"}
    for item in (item for item in cross_snapshots if item["pair"] == "1" and item["snapshot"] == "PRE"):
        source = pair0[(item["arm"], "PRE")]
        for key in ("database_sha256", "authority_sha256",
                    "database_logical_bytes", "database_apparent_bytes", "store_logical_bytes",
                    "store_apparent_bytes"):
            item[key] = source[key]
        item["database_allocated_bytes"] = str(int(source["database_allocated_bytes"]) + 4096)
        item["store_allocated_bytes"] = str(int(source["store_allocated_bytes"]) + 4096)
    for row in (row for row in cross_rows if row["aa_pair"] == 1):
        source = next(item for item in cross_snapshots if item["pair"] == "1"
                      and item["arm"] == row["aa_label"] and item["snapshot"] == "PRE")
        row["aa_base_database_sha256"] = row["pre_edit_database_sha256"] = source["database_sha256"]
        row["aa_base_authority_sha256"] = row["pre_edit_authority_sha256"] = source["authority_sha256"]
        row["aa_base_expectations_sha256"] = row["pre_edit_expectations_sha256"] = source["expectations_sha256"]
    result = analyze(cross_rows, cross_snapshots)
    assert result["status"] == "PASS" and result["phase2_eligible"] and result["cross_pair_unstable"], result
    invalid_cases = {
        "missing-snapshot": lambda r, s: s.pop(),
        "wrong-order": lambda r, s: r.__setitem__(0, r[1]),
        "final-hash": lambda r, s: r[1].__setitem__("aa_post_database_sha256", "f" * 64),
        "authority-hash": lambda r, s: r[1].__setitem__("aa_post_authority_sha256", "f" * 64),
        "apparent": lambda r, s: next(item for item in s if item["pair"] == "0" and item["arm"] == "B" and item["snapshot"] == "T0").__setitem__("database_apparent_bytes", "9"),
        "residue": lambda r, s: next(item for item in s if item["pair"] == "0" and item["arm"] == "A" and item["snapshot"] == "T0").__setitem__("journal_present", "1"),
        "row-status": lambda r, s: r[0].__setitem__("status", "FAIL"),
        "wrong-executable": lambda r, s: r[0].__setitem__("aa_executable_sha256", "f" * 64),
        "expectation-drift": lambda r, s: next(item for item in s if item["pair"] == "0" and item["arm"] == "A" and item["snapshot"] == "T1").__setitem__("expectations_sha256", "f" * 64),
        "shared-delay": lambda r, s: next(item for item in s if item["snapshot"] == "T1").__setitem__("monotonic_ns", str(max(int(x["monotonic_ns"]) for x in s if x["snapshot"] == "T0") + 1)),
        "work": lambda r, s: r[0].__setitem__("raw_bytes_hashed", r[0]["raw_bytes_hashed"] + 1),
        "phase": lambda r, s: r[0]["phase_counters"][0].__setitem__("phase", "bad"),
        "q": lambda r, s: r[0].__setitem__("q_current", 1),
        "timer": lambda r, s: r[0].__setitem__("durable_capture_total_wall_ns", r[0]["durable_capture_total_wall_ns"] + 1),
        "transaction": lambda r, s: r[0].__setitem__("commits", 2),
        "storage": lambda r, s: r[0].__setitem__("physical_store_allocated_bytes", r[0]["physical_store_allocated_bytes"] + 1),
        "compact-label": lambda r, s: r[0].__setitem__("measurement_status_schema", "bad"),
    }
    for name, mutate in invalid_cases.items():
        broken_rows, broken_snapshots = copy.deepcopy(rows), copy.deepcopy(snapshots)
        mutate(broken_rows, broken_snapshots)
        assert analyze(broken_rows, broken_snapshots)["status"] == "FAIL", name
    assert sha256(HELPER) == HELPER_SHA
    with tempfile.TemporaryDirectory() as directory:
        result_dir = synthetic_bundle(Path(directory), rows)
        audit = audit_evidence(result_dir, rows)
        assert audit == [], audit
    evidence_cases = {
        "custody": ("AA100-INPUT-CUSTODY-v1.tsv", CONTROL, "f" * 64),
        "actual-ledger": ("AA100-ACTUAL-INVOCATIONS-v1.tsv", "\tcompleted\t", "\tstarted\t"),
        "invocation": ("AA100-INVOCATION-PLAN-v1.tsv", "capture-only", "capture-only candidate-token"),
        "starts": ("ROW-STARTS-v1.txt", "row_completed_utc", "row_BAD_utc"),
        "status": ("RUN-STATUS-v1.txt", "status=PASS", "status=PASS_BAD"),
        "lock": ("LOCK-TIMEOUT-v1.txt", "lock_released_utc", "lock_BAD_utc"),
        "candidate-post": ("CANDIDATE-NONINVOCATION-CUSTODY-POST-v1.txt", "candidate_executable_size=1318592", "candidate_executable_size=1"),
        "history-post": ("HISTORICAL-H05-H05B-VERIFICATION-POST-v1.txt", "reopened=false", "reopened=true"),
    }
    for name, (filename, old, new) in evidence_cases.items():
        with tempfile.TemporaryDirectory() as directory:
            result_dir = synthetic_bundle(Path(directory), rows)
            path = result_dir / filename
            path.write_text(path.read_text().replace(old, new, 1))
            assert audit_evidence(result_dir, rows), name
    print("self-test PASS cases=stable,pair/time/cross-instability,schedule,hash,apparent,residue,row/native,expectation,delay,work,phase,Q,timer,transaction,storage,compact,bundle-custody,actual-ledger,invocation,starts,status,lock,history,candidate")


def main():
    if sys.argv[1:] == ["--self-test"]:
        self_test()
        return 0
    if len(sys.argv) != 2:
        raise SystemExit(f"usage: {sys.argv[0]} RESULTS-DIR | --self-test")
    result_dir = Path(sys.argv[1])
    try:
        raw_bytes, rows = read_jsonl(result_dir / "AA100-RAW-v1.jsonl")
        snapshot_bytes = (result_dir / "AA100-STORAGE-SNAPSHOTS-v1.tsv").read_bytes()
        snapshots = list(csv.DictReader(snapshot_bytes.decode().splitlines(), delimiter="\t"))
        evidence = audit_evidence(result_dir, rows)
        result = analyze(rows, snapshots, evidence)
        result["raw_sha256"] = hashlib.sha256(raw_bytes).hexdigest()
        result["snapshots_sha256"] = hashlib.sha256(snapshot_bytes).hexdigest()
    except Exception as error:
        result = {"schema": "h05c-aa100-analysis-v1", "status": "FAIL",
                  "classification": "H05C_PHASE1_INVALID", "phase2_eligible": False,
                  "reasons": [f"malformed-bundle:{type(error).__name__}:{error}"]}
    print(json.dumps(result, sort_keys=True, separators=(",", ":")))
    return result["status"] != "PASS"


if __name__ == "__main__":
    raise SystemExit(main())
