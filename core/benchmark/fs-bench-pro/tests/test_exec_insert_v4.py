"""Frozen v4 splice selection and independent oracle binding."""
import hashlib
import json
from pathlib import Path
import sys
import tempfile
import unittest

BENCH = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(BENCH))
import runner  # noqa: E402
from shared import edit_insert_v3 as v3, edit_insert_v4 as v4, edit_route  # noqa: E402


class InsertV4(unittest.TestCase):
    def test_forward_only_registry(self):
        self.assertEqual(hashlib.sha256((BENCH / v3.REGISTRY_PATH).read_bytes()).hexdigest(),
                         v3.REGISTRY_SHA256)
        v4.validate_registry()
        old = {row["fixture_bytes"]: row for row in v3.registry()}
        rows = v4.registry()
        self.assertEqual(len(rows), 4)
        self.assertEqual({row["fixture_bytes"] for row in rows}, set(v4.EXPECTED_CANONICAL))
        for row in rows:
            source = old[row["fixture_bytes"]]
            with self.subTest(size=row["fixture_bytes"]):
                for field in ("fixture_sha256", "fixture_generator_seed", "edit_start",
                              "delete_len", "replacement_len", "replacement_sha256",
                              "payload_seed", "payload_source", "final_bytes", "final_sha256",
                              "g2_target_ms", "cache_contract", "clone_method", "carrier_protocol"):
                    self.assertEqual(row[field], source[field], field)
                self.assertEqual(row["scenario_id"],
                                 source["scenario_id"].replace("-exec-v3", "-exec-v4"))
                self.assertEqual(row["command"], source["command"] + " --output-version 4")
                self.assertEqual(row["operation_contract_id"], v4.OPERATION_CONTRACT_ID)
                self.assertEqual(row["tool_output_version"], 4)
                self.assertEqual(row["file_mode"], 416)
                self.assertEqual((row["canonical_root_expected"],
                                  row["canonical_count_expected"]),
                                 v4.EXPECTED_CANONICAL[row["fixture_bytes"]])
                self.assertTrue(row["oracle"]["full_file_digest"])
                self.assertTrue(row["oracle"]["full_file_bytes_verified"])
                self.assertEqual(row["oracle"]["windows"], source["oracle"]["windows"])

    def test_case_runner_and_image_bind_to_v4(self):
        row = v4.registry()[0]
        selected, actual = runner.edit_selection(row["scenario_id"])
        self.assertIs(selected, v4)
        self.assertEqual(actual, row)
        self.assertIs(runner.edit_selection(row["scenario_id"].replace("-exec-v4", "-exec-v3"))[0],
                      v3)
        master = {"project_id": "a" * 66, "genesis_layer": "b" * 66,
                  "genesis_root": "c" * 64, "genesis_root_serial": 1,
                  "fixture_canonical_root": "d" * 64, "fixture_extent_count": 54}
        case = edit_route.case_record(row, master, "e" * 32)
        self.assertEqual(case["full_file_digest"], 1)
        self.assertEqual(case["fixture_mode"], 416)
        self.assertEqual(case["expected_mode"], 416)
        self.assertEqual(case["canonical_root_expected"], row["canonical_root_expected"])
        self.assertEqual(case["canonical_count_expected"], str(row["canonical_count_expected"]))
        image = {"schema": "core-fs-bench-pro-exec-fuse-insert-image-v4",
                 "registry_sha256": v4.REGISTRY_SHA256, "profile": "release",
                 "scenario_version": 4, "carrier_abi": v4.ABI,
                 "target": "aarch64-unknown-linux-musl", "image_id": "sha256:" + "a" * 64,
                 "lockfile_sha256": runner.digest(runner.CORE / "Cargo.lock"),
                 "cargo_config_sha256": runner.digest(runner.ROOT / ".cargo/config.toml"),
                 "product_seal": "product", "workload_source_seal": runner.edit_workload_seal(),
                 "daemon_sha256": "b" * 64, "edit_tool_sha256": "c" * 64,
                 "dockerfile_sha256": "d" * 64,
                 "payloads": {"insert-middle-4k.bin": {"bytes": 4096,
                             "sha256": row["replacement_sha256"]}}}
        runner.require_edit_image(image, v4, row, {"product_seal": "product"})
        for change in ({"registry_sha256": v3.REGISTRY_SHA256},
                       {"scenario_version": 3}, {"carrier_abi": {}}, {"payloads": {}}):
            with self.subTest(change=change), self.assertRaisesRegex(ValueError, "image"):
                runner.require_edit_image({**image, **change}, v4, row,
                                          {"product_seal": "product"})
        self.assertEqual(runner.report_edit(BENCH / "absent", scenario_version=4)[1]
                         ["counts"]["rows"], 4)

    def test_report_rejects_wrong_v4_image(self):
        row = v4.registry()[0]
        with tempfile.TemporaryDirectory() as scratch:
            folder = Path(scratch)
            (folder / "run.json").write_text(json.dumps({
                "selection": row["scenario_id"],
                "receipt": {"scenario_id": row["scenario_id"], "status": "INCOMPLETE"},
                "image": {"registry_sha256": v4.REGISTRY_SHA256,
                          "schema": "core-fs-bench-pro-exec-fuse-insert-image-v3",
                          "scenario_version": 3, "carrier_abi": v4.ABI},
            }))
            self.assertEqual(runner.edit_report_row(row, folder)["terminal"],
                             "INVALID_EVIDENCE")

    def test_verifier_result_requires_bound_v4_metadata(self):
        row = v4.registry()[0]
        driver = {"branch_id": "branch", "head_commit": "head",
                  "observed_mtime_seconds": 17, "observed_mtime_nanoseconds": 42}
        child = {"status": "PASS", "scenario_id": row["scenario_id"],
                 "branch_id": "branch", "head_commit": "head", "store": "store",
                 "history": "history", "full_file_bytes_verified": True,
                 "content_match": True, "canonical_root_match": True,
                 "canonical_count_match": True,
                 "canonical_root": row["canonical_root_expected"],
                 "extent_count": row["canonical_count_expected"],
                 "published_metadata_match": True, "historical_metadata_match": True,
                 "expected_mode": 416, "fixture_mode": 416,
                 "expected_mtime_seconds": 17, "expected_mtime_nanoseconds": 42}
        self.assertEqual(edit_route.verifier_result(0, False, 1, child, row, driver,
                                                    "store", "history")[0], "PASS")
        self.assertEqual(edit_route.verifier_result(0, False, 1,
                         {**child, "expected_mtime_nanoseconds": 43}, row, driver,
                         "store", "history")[0], "FAIL")


if __name__ == "__main__":
    unittest.main()
