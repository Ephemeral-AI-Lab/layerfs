"""Product-free registry, evidence tamper and independent-oracle checks."""
import copy
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from r7 import changes, oracle, receipts, registry, workloads


def unrun_rows():
    return [{"schema": receipts.SCHEMA, "family_id": "r7-optimization", "mode": "exploratory",
             "admission_eligible": False, "registry_identity": registry.registry_identity(),
             "selection_id": item["selection_id"], "row_status": "NOT_RUN", "reason": item["reason"],
             "attempted_operation_count": 0, "completed_operation_count": 0, "sample_count": 0}
            for item in registry.selections()]


def diagnostic(selection_id="E02:C:L"):
    case_id, cls, arm = selection_id.split(":")
    case = next(item for item in registry.cases() if item["case_id"] == case_id)
    row = unrun_rows()[0]
    row.update({"selection_id": selection_id, "row_status": "INCOMPLETE", "reason": "synthetic parser test, never product evidence",
                "attempted_operation_count": 1, "completed_operation_count": 1, "sample_count": 1,
                "identities": dict.fromkeys(receipts.IDENTITIES, "synthetic-test-identity"),
                "build_profile": "release", "dirty": False, "construction_workers": 1,
                "store_profile": "Disposable/WAL/OFF", "overlay_profile": "MEMORY/OFF/EXCLUSIVE",
                "exec_registration_count": 0, "command_launcher": "ordinary external bash", "uid": 501, "gid": 20,
                "setup_method": "clone", "benchmark_verification_count_in_performance": 0,
                "declared_interference": "synthetic parser test", "exact_command": case["workload"],
                "performance_status": "PRICED_DIAGNOSTIC" if case_id in registry.DIAGNOSTIC else "DIAGNOSTIC",
                "verification_status": "SKIPPED", "resource_status": "INCOMPLETE", "cleanup_status": "INCOMPLETE",
                "custody_status": "TEST_ONLY", "failure_class": "missing observation",
                "cache": {"class": cls, "scopes": dict.fromkeys(registry.PHASES, "declared synthetic phase cache"), "warmup_gap_ns": 1,
                          "same_mount": True, "warmup_receipt": "synthetic"},
                "phases": {name: receipts.unavailable("synthetic missing observation") for name in registry.PHASES},
                "observations": {name: receipts.unavailable("synthetic missing observation") for name in registry.COUNTERS},
                "complete_command_wall_stop_ns": case["complete_command_wall_stop_ns"], "complete_command_ns": 1,
                "oracle_scope": case["oracle_scope"], "raw_artifacts": {"original.json": "0" * 64}})
    row.update(dict.fromkeys(receipts.OPERATION_FIELDS, "synthetic-declared-operation"))
    row.update({"public_api_call_count": 1, "forbidden_route_count": 0, "fallback_count": 0})
    row.update({"source_arm": arm, "scenario_id": case_id, "scenario_version": 1,
                "seed_or_repetition": 1, "contract_commit": "synthetic-test-contract", "treatment": "synthetic-parser-test"})
    row["identities"]["image_id"] = registry.IMAGE
    return row


class Registry(unittest.TestCase):
    def test_whole_membership(self):
        cases = registry.cases()
        self.assertEqual(len(cases), 38)
        self.assertEqual(len({case["case_id"] for case in cases}), 38)
        self.assertEqual({f"E{i:02}" for i in range(1, 20)} | {f"C{i:02}" for i in range(1, 13)} |
                         {f"K{i:02}" for i in range(1, 6)} | {"W01", "W02"}, {case["case_id"] for case in cases})

    def test_complete_preregistration_is_unrun(self):
        rows = registry.selections()
        self.assertEqual(len(rows), len({row["selection_id"] for row in rows}))
        self.assertTrue(all(row["status"] == "NOT_RUN" and row["sample_count"] == 0 and row["admission_eligible"] is False for row in rows))
        self.assertEqual(receipts.validate_campaign(unrun_rows())["rows"], len(rows))

    def test_budget_classification(self):
        for case in registry.cases():
            if case["case_id"] in registry.DIAGNOSTIC:
                self.assertFalse(case["numeric_budget_verdict"])
                self.assertEqual(case["complete_command_wall_stop_ns"], 120_000_000_000)
            else:
                self.assertEqual(case["complete_command_wall_stop_ns"], 15_000_000_000)
            self.assertLess(case["verifier_wall_stop_ns"], 10_000_000_000)

    def test_exact_body_bytes_and_preparation(self):
        self.assertEqual(workloads.workload("E16")["argv"], ["/bin/bash", "-o", "pipefail", "-c", "python3 /code/workload.py append"])
        self.assertEqual(workloads.workload("C09")["preparation_command"], workloads.DD)
        self.assertEqual(workloads.workload("E18")["argv"], workloads.workload("E04")["argv"])
        self.assertIsNone(workloads.workload("E03")["oracle_argv"])
        self.assertIn("wc-only", workloads.workload("E03")["oracle_unavailable_reason"])

    def test_nonrepeatable_class_c_preserved(self):
        rows = registry.selections()
        for case in ("E12", "E13", "C12"):
            row = next(item for item in rows if item["selection_id"] == case + ":C:L")
            self.assertIn("non-repeatable", row["reason"])

    def test_commit_and_concurrency_schedules_exact(self):
        cases = {case["case_id"]: case for case in registry.cases()}
        self.assertEqual(cases["K04"]["workload"]["commit_count"], 5)
        self.assertEqual(len(cases["K04"]["workload"]["commands"]), 5)
        self.assertEqual(len(cases["W01"]["workload"]["commands"]), 2)
        self.assertTrue(all(case["workload"] for case in cases.values()))

    def test_scaling_dimensions_not_dropped(self):
        self.assertEqual(registry.SCALING["factors"], [1, 2, 4])
        self.assertEqual(len(registry.SCALING["dimensions"]), 16)

    def test_cache_b_has_no_control_analogue(self):
        rows = [row for row in registry.selections() if row["cache_class"] == "B" and row["arm"] != "L"]
        self.assertTrue(rows)
        self.assertTrue(all("L-only" in row["reason"] for row in rows))


class Receipts(unittest.TestCase):
    def test_missing_ledger_row(self):
        with self.assertRaisesRegex(ValueError, "missing or unexpected"):
            receipts.validate_campaign(unrun_rows()[1:])

    def test_duplicate_is_not_resampled(self):
        rows = unrun_rows()
        with self.assertRaisesRegex(ValueError, "duplicate/resampled"):
            receipts.validate_campaign(rows + [copy.deepcopy(rows[0])])

    def test_valid_unavailable_diagnostic_is_not_zero(self):
        row = diagnostic()
        self.assertEqual(receipts.validate_receipt(row)["row_status"], "INCOMPLETE")
        self.assertIsNone(row["observations"]["daemon_vmhwm_bytes"]["value"])

    def reject(self, edit, text):
        row = diagnostic()
        edit(row)
        with self.assertRaisesRegex(ValueError, text):
            receipts.validate_receipt(row)

    def test_no_admission_promotion(self):
        self.reject(lambda row: row.__setitem__("admission_eligible", True), "cannot be promoted")

    def test_no_retry(self):
        self.reject(lambda row: row.__setitem__("attempted_operation_count", 2), "one original")

    def test_no_warm_cache_expiry(self):
        self.reject(lambda row: row["cache"].__setitem__("warmup_gap_ns", 60_000_000_000), "cache lifetime")

    def test_no_debug(self):
        self.reject(lambda row: row.__setitem__("build_profile", "debug"), "pinned release")

    def test_no_durable(self):
        self.reject(lambda row: row.__setitem__("store_profile", "Durable/WAL/FULL"), "profile/worker")

    def test_no_worker_increase(self):
        self.reject(lambda row: row.__setitem__("construction_workers", 2), "profile/worker")

    def test_no_registered_command(self):
        self.reject(lambda row: row.__setitem__("exec_registration_count", 1), "external execution")

    def test_no_verifier_inside_performance(self):
        self.reject(lambda row: row.__setitem__("benchmark_verification_count_in_performance", 1), "contaminated")

    def test_no_made_up_observation(self):
        self.reject(lambda row: row["observations"]["daemon_vmhwm_bytes"].__setitem__("value", 0), "null and reason")

    def test_no_missing_counter(self):
        self.reject(lambda row: row["observations"].pop("statement_work"), "missing counter")

    def test_no_resource_pass_with_unavailable_bytes(self):
        self.reject(lambda row: row.__setitem__("resource_status", "PASS"), "actual memory and disk")

    def test_no_fallback_route(self):
        self.reject(lambda row: row.__setitem__("fallback_count", 1), "forbidden/fallback")

    def test_no_missing_sidecar_residency(self):
        row = diagnostic("E02:A:L")
        row["cache"] = {"class": "A", "scopes": dict.fromkeys(registry.PHASES, "synthetic test"), "fresh_daemon_cache": True, "fresh_kernel_connection": True,
                        "store_path":"store", "overlay_path":"overlay",
                        "required_residency_paths": ["store", "store-wal"],
                        "residency": {"cache_class": "A", "status":"ELIGIBLE", "files": [{"path": "store", "present": True,
                                        "resident_pages": 0, "eviction_hint_attempts": 1}], "resident_pages": 0,
                                      "payload_bytes_read": 0, "attempts": 0, "method": "fadvise and mincore"}}
        with self.assertRaisesRegex(ValueError, "complete input and sidecar"):
            receipts.validate_receipt(row)

    def test_no_cold_attempt_if_resident(self):
        cache = {"class": "A", "scopes": dict.fromkeys(registry.PHASES, "synthetic test"), "fresh_daemon_cache": True, "fresh_kernel_connection": True,
                 "store_path": "store", "overlay_path":"overlay", "required_residency_paths": ["store", "store-wal", "store-shm", "store-journal","overlay","overlay-wal","overlay-shm","overlay-journal"], "residency": {"cache_class": "A", "status":"INELIGIBLE",
                 "files": [{"path": "store", "present": True, "resident_pages": 1, "eviction_hint_attempts": 1}] +
                          [{"path": "store" + suffix, "present": False, "resident_pages": 0} for suffix in ("-wal", "-shm", "-journal")] +
                          [{"path":"overlay","present":True,"resident_pages":0,"eviction_hint_attempts":1}] +
                          [{"path":"overlay"+suffix,"present":False,"resident_pages":0} for suffix in ("-wal","-shm","-journal")],
                 "resident_pages": 1, "payload_bytes_read": 0, "attempts": 0, "method": "fadvise and mincore"}}
        with self.assertRaisesRegex(ValueError, "resident cold input attempted"):
            receipts.validate_cache(cache, "L", 1)

    def test_phase_arithmetic(self):
        value = {"status": "AVAILABLE", "provenance": "synthetic-clock", "value": {"start_ns": 2, "end_ns": 4,
                 "duration_ns": 3, "clock_id": "monotonic", "start_event": "launch", "end_event": "exit"}}
        with self.assertRaisesRegex(ValueError, "arithmetic"):
            receipts.phase(value, "command")

    def test_no_diagnostic_latency_pass(self):
        row = diagnostic("E03:C:L")
        row["performance_status"] = "PASS"
        with self.assertRaisesRegex(ValueError, "no latency pass/fail"):
            receipts.validate_receipt(row)


class Oracles(unittest.TestCase):
    def test_full_empty_root_create_bytes_and_symlinks(self):
        with tempfile.TemporaryDirectory() as folder:
            folder = Path(folder)
            root = folder / "root"
            root.mkdir()
            (root / "file").write_bytes(b"abc")
            (root / "alias").symlink_to("file")
            manifest = folder / "manifest.jsonl"
            row = oracle.observe(root, "C01", manifest)
            self.assertEqual(row["hashed_files"], 1)
            self.assertEqual(row["hashed_bytes"], 3)
            self.assertEqual(oracle.compare(manifest, manifest, "C01")["status"], "PASS")

    def test_hard_link_relationship_cannot_be_faked_by_equal_bytes(self):
        with tempfile.TemporaryDirectory() as folder:
            folder = Path(folder)
            expected, actual = folder / "expected", folder / "actual"
            expected.mkdir()
            actual.mkdir()
            (expected / "file").write_bytes(b"abc")
            os.link(expected / "file", expected / "alias")
            (actual / "file").write_bytes(b"abc")
            (actual / "alias").write_bytes(b"abc")
            left, right = folder / "left.jsonl", folder / "right.jsonl"
            oracle.observe(expected, "C01", left)
            oracle.observe(actual, "C01", right)
            result = oracle.compare(left, right, "C01")
            self.assertEqual(result["status"], "FAIL")
            self.assertTrue(any(row.get("reason") == "hard-link equivalence classes" for row in result["differences"]))

    def test_full_payload_mutation_detected(self):
        with tempfile.TemporaryDirectory() as folder:
            folder = Path(folder)
            root = folder / "root"
            root.mkdir()
            (root / "experiment.log").write_bytes(b"abc")
            left, right = folder / "before.jsonl", folder / "after.jsonl"
            oracle.observe(root, "E16", left)
            (root / "experiment.log").write_bytes(b"abd")
            oracle.observe(root, "E16", right)
            self.assertEqual(oracle.compare(left, right, "E16")["status"], "FAIL")

    def test_oracle_cannot_write_into_mounted_root(self):
        with tempfile.TemporaryDirectory() as folder:
            with self.assertRaisesRegex(ValueError, "outside verification root"):
                oracle.observe(folder, "C01", Path(folder) / "oracle.jsonl")

    def test_stdout_case_cannot_be_promoted_to_tree_oracle(self):
        with self.assertRaisesRegex(ValueError, "unavailable"):
            oracle.selected("E07", "package.json")

    def test_tracked_content_scope_mandatory(self):
        with tempfile.TemporaryDirectory() as folder:
            with self.assertRaisesRegex(ValueError, "sealed tracked"):
                oracle.observe(folder, "E19", Path(folder).parent / "unused.jsonl")

    def test_e19_source_checkout_protected_before_any_write(self):
        with self.assertRaisesRegex(ValueError, "protected"):
            changes.prepare(changes.PROTECTED_FIXTURE, [])

    def test_e19_cannot_silently_reduce_changed_count(self):
        with tempfile.TemporaryDirectory() as folder:
            with self.assertRaisesRegex(ValueError, "14090"):
                changes.prepare(folder, [])


if __name__ == "__main__":
    unittest.main()
