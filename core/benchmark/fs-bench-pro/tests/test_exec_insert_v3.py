"""Focused source-only checks for the prospective four-case #241 selection."""
import hashlib
from pathlib import Path
import sys
import unittest

BENCH = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(BENCH))
import runner  # noqa: E402
from shared import edit_contract as v2, edit_insert_v3 as v3, edit_route  # noqa: E402


class InsertV3(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.rows = v3.registry()
        cls.old = {row["scenario_id"]: row for row in v2.registry()
                   if row["operation_key"] == "insert-middle-4k"}

    def test_registry_and_exact_v2_input_identity(self):
        v3.validate_registry()
        self.assertEqual(len(self.rows), 4)
        self.assertEqual(len({row["scenario_id"] for row in self.rows}), 4)
        self.assertEqual(hashlib.sha256((BENCH / v2.REGISTRY_PATH).read_bytes()).hexdigest(),
                         v2.REGISTRY_SHA256)
        for row in self.rows:
            old = self.old[row["scenario_id"].replace("-exec-v3", "-exec-v2")]
            for field in ("family_id", "fixture_bytes", "fixture_sha256", "fixture_generator_seed",
                          "edit_start", "delete_len", "replacement_len", "replacement_sha256",
                          "payload_seed", "payload_source", "final_bytes", "final_sha256",
                          "oracle", "g2_target_ms", "cache_contract", "clone_method"):
                self.assertEqual(row[field], old[field], field)
            self.assertEqual(row["scenario_version"], 3)
            self.assertEqual(row["route"], v3.ROUTE)
            self.assertEqual(row["operation_contract_id"], v3.OPERATION_CONTRACT_ID)
            self.assertEqual(row["editor_algorithm"], "linux-fuse-ioctl-range-splice-v2")
            self.assertEqual(row["expected_range_state_callbacks"], 2)
            self.assertEqual(row["expected_range_callbacks"], 1)
            self.assertEqual(row["expected_write_callbacks"], 0)
            self.assertEqual(row["expected_shifted_suffix_bytes"], 0)
            self.assertEqual(row["command"],
                f"{v2.TOOL_PATH} splice --file {v2.FIXTURE_PATH} "
                f"--offset {row['edit_start']} --delete-length 0 --length 4096 "
                f"--payload {row['payload_source']} --expect-size {row['fixture_bytes']}")
        self.assertEqual(v3.ABI["state"]["magic"], "LFS2")
        self.assertEqual(v3.ABI["edit"], {"magic": "LFE2", "command": "0x5060f541",
                                           "input_bytes": 4192, "output_bytes": 0,
                                           "max_replacement_bytes": 4096})

    def test_runner_resolves_v3_without_changing_v2(self):
        contract, row = runner.edit_selection(self.rows[0]["scenario_id"])
        self.assertIs(contract, v3)
        self.assertEqual(row, self.rows[0])
        contract, row = runner.edit_selection(next(iter(self.old)))
        self.assertIs(contract, v2)
        self.assertEqual(row["scenario_version"], 2)
        with self.assertRaisesRegex(ValueError, "unregistered"):
            runner.edit_selection("unknown-exec-v3")
        self.assertEqual(runner.report_edit(BENCH / "absent", scenario_version=3)[1]
                         ["counts"]["rows"], 4)

    def test_case_and_image_bind_to_v3(self):
        row = self.rows[0]
        master = {"project_id": "a" * 66, "genesis_layer": "b" * 66,
                  "genesis_root": "c" * 64, "genesis_root_serial": 1,
                  "fixture_canonical_root": "d" * 64, "fixture_extent_count": 54}
        case = edit_route.case_record(row, master, "e" * 32)
        self.assertEqual(case["operation_contract_id"], v3.OPERATION_CONTRACT_ID)
        self.assertEqual(case["carrier_protocol"], v3.ABI["contract"])
        identity = {"product_seal": "p", "harness_seal": "h", "cargo_lock_sha256": "l"}
        fixture = {"bytes": row["fixture_bytes"], "sha256": row["fixture_sha256"]}
        key = edit_route.compatibility_key(identity, fixture, row, "init-binary-hash")
        self.assertEqual(key["benchmark_init_sha256"], "init-binary-hash")
        self.assertNotIn("harness_seal", key)
        self.assertNotIn("registry_sha256", key)
        image = {"schema": "core-fs-bench-pro-exec-fuse-insert-image-v3",
                 "registry_sha256": v3.REGISTRY_SHA256, "profile": "release",
                 "scenario_version": 3, "carrier_abi": v3.ABI,
                 "target": "aarch64-unknown-linux-musl", "image_id": "sha256:" + "a" * 64,
                 "lockfile_sha256": runner.digest(runner.CORE / "Cargo.lock"),
                 "cargo_config_sha256": runner.digest(runner.ROOT / ".cargo/config.toml"),
                 "product_seal": "product", "workload_source_seal": runner.edit_workload_seal(),
                 "daemon_sha256": "b" * 64, "edit_tool_sha256": "c" * 64,
                 "dockerfile_sha256": "d" * 64,
                 "payloads": {"insert-middle-4k.bin": {"bytes": 4096,
                             "sha256": row["replacement_sha256"]}}}
        identity = {"product_seal": "product"}
        runner.require_edit_image(image, v3, row, identity)
        for changed in ({"registry_sha256": v2.REGISTRY_SHA256},
                        {"profile": "debug"}, {"payloads": {}}, {"carrier_abi": {}},
                        {"workload_source_seal": "stale"}, {"product_seal": "stale"}):
            with self.subTest(changed=changed), self.assertRaisesRegex(ValueError, "image"):
                runner.require_edit_image({**image, **changed}, v3, row, identity)

    def test_v3_route_counts_fail_closed(self):
        row = self.rows[0]
        driver = {key: row[key] for key in ("scenario_id", "route", "operation_contract_id",
                    "fixture_bytes", "edit_start", "delete_len", "replacement_len",
                    "replacement_sha256", "final_bytes")}
        driver.update(status="COMPLETE", edit_commit_ns=1_000_000, sandbox_delete_ok=True,
                      projection_counts="range_state=2,range_edit=1,write=0,read=3",
                      range_accepted_payload_bytes=4096, range_shifted_suffix_bytes=0)
        receipt = {"complete_command_status": "PASS", "driver": driver,
                   "telemetry": {"status": "PASS"}, "verification": {"status": "PASS"},
                   "cache": {"macos-store": {"status": "PASS"},
                             "linux-fuse-backing": {"status": "INELIGIBLE"}}}
        self.assertEqual(edit_route.terminal_status(receipt, row)["status"], "INELIGIBLE")
        for key, value in (("projection_counts", "range_edit=0,range_state=2,write=0"),
                           ("range_shifted_suffix_bytes", 1),
                           ("operation_contract_id", "wrong")):
            with self.subTest(key=key):
                failed = {**receipt, "driver": {**driver, key: value}}
                self.assertEqual(edit_route.terminal_status(failed, row)["status"], "FAIL")


if __name__ == "__main__":
    unittest.main()
