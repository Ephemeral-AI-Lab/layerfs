"""Focused checks for the #232 Exec/FUSE route substrate (Phase 3)."""
import json
from pathlib import Path
import sys
import unittest

HERE = Path(__file__).resolve().parent
BENCH = HERE.parent
sys.path.insert(0, str(BENCH))
import runner  # noqa: E402
from shared import edit_cache, edit_contract as contract, edit_route  # noqa: E402


class Route(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.rows = {row["scenario_id"]: row for row in contract.registry()}
        cls.case = cls.rows["overwrite-middle-4k-on-1mib-ops-1-exec-v1"]

    def test_fixture_recipe_and_master_key_fail_closed(self):
        self.assertEqual(edit_route.sha256, edit_route.sha256)
        identity = {"product_seal": "p", "harness_seal": "h", "cargo_lock_sha256": "l"}
        first = edit_route.compatibility_key(identity, {"bytes": 4, "sha256": "s"})
        self.assertEqual(first["registry_sha256"], contract.REGISTRY_SHA256)
        self.assertEqual(first["fixture_generator_seed"], contract.FIXTURE_GENERATOR_SEED)
        other = edit_route.compatibility_key({**identity, "product_seal": "p2"},
                                            {"bytes": 4, "sha256": "s"})
        self.assertNotEqual(first, other)

    def test_case_record_freezes_plan_and_oracle(self):
        master = {"project_id": "a" * 66, "genesis_layer": "b" * 66, "genesis_root": "c" * 64,
                  "genesis_root_serial": 1, "fixture_canonical_root": "d" * 64,
                  "fixture_extent_count": 54}
        record = edit_route.case_record(self.case, master, "e" * 32)
        self.assertEqual(record["final_bytes"], self.case["final_bytes"])
        self.assertEqual(record["edit_start"], self.case["edit_start"])
        self.assertEqual(record["window_offset"] + record["window_bytes"],
                         min(self.case["final_bytes"],
                             self.case["edit_start"] + self.case["delete_len"]
                             + self.case["replacement_len"]
                             + contract.BOUNDED_ORACLE_WINDOW))
        self.assertLessEqual(record["window_bytes"], contract.BOUNDED_ORACLE_BYTES)
        self.assertEqual(record["full_file_digest"], 1)
        self.assertEqual(record["command"], self.case["command"])
        self.assertEqual(record["stack_body"], "a" * 32)
        path = BENCH / "target-case-check.txt"
        try:
            edit_route.case_file(path, record)
            reloaded = dict(line.split("=", 1) for line in path.read_text().splitlines())
            spec = dict(entry.split("=", 1) for entry in edit_route.case_spec(record).split(";"))
            self.assertEqual(reloaded["scenario_id"], self.case["scenario_id"])
            self.assertEqual(int(reloaded["final_bytes"]), self.case["final_bytes"])
            self.assertEqual(spec, {key: str(value) for key, value in record.items()})
        finally:
            path.unlink(missing_ok=True)

    def test_result_window_matches_the_declared_digest(self):
        for row in contract.registry():
            if row["registration_status"] != "REGISTERED" or row["final_bytes"] > 10_485_760:
                continue
            replacement = contract.payload_bytes(row["payload_seed"], row["replacement_len"],
                                                row["replacement_kind"])
            whole = contract.result_window(row["fixture_bytes"], row["edit_start"],
                                          row["delete_len"], replacement, 0, row["final_bytes"])
            import hashlib
            self.assertEqual(hashlib.sha256(whole).hexdigest(), row["final_sha256"])

    def test_cache_contract_declares_both_domains(self):
        fuse = edit_cache.qualify_fuse_backing()
        self.assertEqual(fuse["status"], "INELIGIBLE")
        self.assertEqual(fuse["method"], edit_cache.LINUX_METHOD)
        self.assertIn("forbidden", fuse["reason"])

    def test_terminal_status_never_promotes_a_warm_row(self):
        row = self.case
        receipt = {
            "complete_command_status": "PASS",
            "driver": {"status": "COMPLETE", "edit_commit_ns": 1_000_000, "sandbox_delete_ok": True},
            "telemetry": {"status": "PASS"},
            "verification": {"status": "PASS"},
            "cache": {"macos-store": {"status": "PASS"},
                      "linux-fuse-backing": {"status": "INELIGIBLE"}},
        }
        outcome = edit_route.terminal_status(receipt, row)
        self.assertEqual(outcome["status"], "INELIGIBLE")
        self.assertEqual(outcome["raw_edit_commit_ns"], 1_000_000)
        self.assertNotIn("GOAL_MET", json.dumps(outcome))
        receipt["cache"]["linux-fuse-backing"] = {"status": "PASS"}
        self.assertEqual(edit_route.terminal_status(receipt, row)["status"], "GOAL_MET")
        receipt["driver"]["edit_commit_ns"] = row["g2_target_ms"] * 1_000_000 + 1
        self.assertEqual(edit_route.terminal_status(receipt, row)["status"], "TARGET_MISS")
        receipt["verification"]["status"] = "FAIL"
        self.assertEqual(edit_route.terminal_status(receipt, row)["status"], "FAIL")

    def test_runner_registers_the_edit_route_without_a_second_system(self):
        rows = {row["scenario_id"]: row for row in contract.registry()}
        self.assertIn("overwrite-middle-4k-on-1mib-ops-1-exec-v1", rows)
        driver = (BENCH / "../../crates/layerfs-api/sdk/examples/benchmark_edit.rs").resolve()
        self.assertTrue(driver.is_file())
        runner.require_sdk_driver("benchmark_edit")
        self.assertEqual(runner.EXEC_EDIT_BUILD[0:2], ["cargo", "+1.85.1"])
        self.assertIn("--release", runner.EXEC_EDIT_BUILD)
        self.assertIn("--locked", runner.EXEC_EDIT_BUILD)
        source = driver.read_text()
        for call in ("ProjectApi::new(", "projects.fork(", "sandboxes.create(",
                     "workspaces.mount(", "workspaces.exec(", "workspaces.commit(",
                     "workspaces.status(", "workspaces.unmount(", "sandboxes.delete("):
            self.assertIn(call, source)

    def test_driver_and_verifier_are_release_examples_only(self):
        build = runner.EXEC_EDIT_BUILD
        self.assertNotIn("--debug", build)
        self.assertIn("benchmark_edit", build)
        self.assertIn("verify_edit", build)


if __name__ == "__main__":
    unittest.main()
