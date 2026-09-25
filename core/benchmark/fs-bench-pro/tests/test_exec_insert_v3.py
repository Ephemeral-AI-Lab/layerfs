"""Focused source-only checks for the prospective four-case #241 selection."""
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile
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
        with self.assertRaisesRegex(ValueError, "expected canonical root/count absent"):
            edit_route.case_record(row, master, "e" * 32)
        # These test-only values exercise binding; production values must come
        # from final-source functional receipts before any registered sample.
        bound_row = {**row, "canonical_root_expected": "f" * 64,
                     "canonical_count_expected": 56}
        case = edit_route.case_record(bound_row, master, "e" * 32)
        self.assertEqual(case["operation_contract_id"], v3.OPERATION_CONTRACT_ID)
        self.assertEqual(case["carrier_protocol"], v3.ABI["contract"])
        self.assertEqual(case["full_file_digest"], 1)
        self.assertEqual(case["canonical_root_expected"], "f" * 64)
        self.assertEqual(case["canonical_count_expected"], "56")
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

    def test_all_v3_sizes_require_full_digest_and_pinned_root_count(self):
        master = {"project_id": "a" * 66, "genesis_layer": "b" * 66,
                  "genesis_root": "c" * 64, "genesis_root_serial": 1,
                  "fixture_canonical_root": "d" * 64, "fixture_extent_count": 54}
        for row in self.rows:
            with self.subTest(size=row["fixture_bytes"]):
                with self.assertRaisesRegex(ValueError, "expected canonical root/count absent"):
                    edit_route.case_record(row, master, "e" * 32)
                ready = {**row, "canonical_root_expected": "f" * 64,
                         "canonical_count_expected": 56}
                self.assertEqual(edit_route.case_record(ready, master, "e" * 32)
                                 ["full_file_digest"], 1)
                for invalid in ({"canonical_root_expected": "-"},
                                {"canonical_count_expected": 0}):
                    with self.assertRaisesRegex(ValueError, "expected canonical root/count absent"):
                        edit_route.case_record({**ready, **invalid}, master, "e" * 32)

    def test_release_verifier_refuses_incomplete_v3_case_before_store_open(self):
        verifier = runner.CORE / "target/release/examples/verify_edit"
        if not verifier.is_file():
            self.skipTest("build the locked release verify_edit example first")
        with tempfile.TemporaryDirectory() as scratch:
            case = Path(scratch) / "case.txt"
            for fields, expected in (
                ({"full_file_digest": "0", "canonical_root_expected": "-",
                  "canonical_count_expected": "-"}, "full-file digest"),
                ({"full_file_digest": "1", "canonical_root_expected": "-",
                  "canonical_count_expected": "1"}, "canonical root"),
                ({"full_file_digest": "1", "canonical_root_expected": "f" * 64,
                  "canonical_count_expected": "0"}, "canonical count"),
            ):
                case.write_text("operation_contract_id=workspace-exec-fuse-range-splice-commit-v3\n"
                                "scenario_id=insert-middle-4k-on-1mib-ops-1-exec-v3\n"
                                + "".join(f"{key}={value}\n" for key, value in fields.items()))
                result = subprocess.run([str(verifier), str(case), "/absent/store",
                                         "/absent/history"], capture_output=True, text=True)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn(expected, result.stderr)

    def test_v3_route_counts_fail_closed(self):
        row = self.rows[0]
        driver = {key: row[key] for key in ("scenario_id", "route", "operation_contract_id",
                    "fixture_bytes", "edit_start", "delete_len", "replacement_len",
                    "replacement_sha256", "final_bytes")}
        driver.update(status="COMPLETE", edit_commit_ns=1_000_000, unmount_ok=True,
                      sandbox_delete_ok=True,
                      projection_counts="range_state=2,range_edit=1,write=0,read=3",
                      range_accepted_payload_bytes=4096, range_shifted_suffix_bytes=0)
        receipt = {"complete_command_status": "PASS", "driver": driver,
                   "telemetry": {"status": "PASS"}, "verification": {"status": "PASS"},
                   "cache": {"macos-store": {"status": "PASS"},
                             "linux-fuse-backing": {"status": "INELIGIBLE"}}}
        self.assertEqual(edit_route.terminal_status(receipt, row)["status"], "INELIGIBLE")
        skipped = {**receipt, "verification": {"status": "SKIPPED"}}
        self.assertEqual(edit_route.terminal_status(skipped, row)["status"], "INCOMPLETE")
        for key, value in (("projection_counts", "range_edit=0,range_state=2,write=0"),
                           ("range_shifted_suffix_bytes", 1),
                           ("operation_contract_id", "wrong"),
                           ("unmount_ok", False)):
            with self.subTest(key=key):
                failed = {**receipt, "driver": {**driver, key: value}}
                self.assertEqual(edit_route.terminal_status(failed, row)["status"], "FAIL")

    def test_verifier_requires_bounded_wall_and_exact_commit_identity(self):
        row = self.rows[0]
        driver = {"branch_id": "branch", "head_commit": "head"}
        child = {"status": "PASS", "scenario_id": row["scenario_id"],
                 "branch_id": "branch", "head_commit": "head",
                 "store": "/case/store", "history": "/case/history"}
        def outcome(value=child, wall=14_000_000_000, code=0):
            return edit_route.verifier_result(code, False, wall, value, row, driver,
                                              "/case/store", "/case/history")[0]
        self.assertEqual(outcome(), "PASS")
        self.assertEqual(outcome(wall=15_000_000_001), "FAIL")
        self.assertEqual(outcome(value={**child, "scenario_id": "other"}), "FAIL")
        self.assertEqual(outcome(value={**child, "head_commit": "other"}), "FAIL")
        self.assertEqual(outcome(value={**child, "store": "/other/store"}), "FAIL")
        self.assertEqual(outcome(code=1), "FAIL")

    def test_verifier_refuses_to_replace_retained_attempt(self):
        with tempfile.TemporaryDirectory() as scratch:
            folder = Path(scratch)
            (folder / "verifier-case.txt").write_text("prior attempt")
            with self.assertRaisesRegex(ValueError, "already retained"):
                edit_route.verify(folder, {}, folder / "case.txt", folder / "store",
                                  folder / "history", self.rows[0], {}, "unused")

    def test_report_reclassifies_later_verifier_failure(self):
        row = self.rows[0]
        driver = {key: row[key] for key in ("scenario_id", "route", "operation_contract_id",
                    "fixture_bytes", "edit_start", "delete_len", "replacement_len",
                    "replacement_sha256", "final_bytes")}
        driver.update(status="COMPLETE", edit_commit_ns=1_000_000, unmount_ok=True,
                      sandbox_delete_ok=True, projection_counts="range_state=2,range_edit=1,write=0",
                      range_accepted_payload_bytes=4096, range_shifted_suffix_bytes=0,
                      branch_id="branch", head_commit="head")
        receipt = {"scenario_id": row["scenario_id"], "status": "INCOMPLETE",
                   "complete_command_status": "PASS", "driver": driver,
                   "telemetry": {"status": "PASS"}, "verification": {"status": "SKIPPED"},
                   "store": "/case/store", "history": "/case/history",
                   "cache": {"macos-store": {"status": "PASS"},
                             "linux-fuse-backing": {"status": "INELIGIBLE"}}}
        with tempfile.TemporaryDirectory() as scratch:
            folder = Path(scratch)
            (folder / "run.json").write_text(json.dumps({
                "selection": row["scenario_id"], "receipt": receipt,
                "image": {"registry_sha256": v3.REGISTRY_SHA256}}))
            (folder / "verification.json").write_text(json.dumps({
                "status": "FAIL", "scenario_id": row["scenario_id"],
                "exit_code": 1, "timeout": False, "wall_ns": 1_000_000,
                "child": None}))
            reported = runner.edit_report_row(row, folder)
            self.assertEqual(reported["verification"], "FAIL")
            self.assertEqual(reported["terminal"], "FAIL")
            self.assertEqual(reported["goal"], "FAIL")
            self.assertEqual(reported["recorded_terminal"], "INCOMPLETE")
            self.assertEqual(reported["derived_verification_gate"], "FAIL")
            self.assertEqual(reported["current_admission_status"], "FAIL")

    def test_master_seal_rejects_corruption_and_incomplete_entries(self):
        size = self.rows[0]["fixture_bytes"]
        key = {"fixture_bytes": size, "fixture_sha256": v2.FIXTURE_SHA256[size],
               "fixture_generator_seed": v2.FIXTURE_GENERATOR_SEED,
               "benchmark_init_sha256": "a" * 64, "cargo_lock_sha256": "b" * 64}
        with tempfile.TemporaryDirectory() as scratch:
            directory = Path(scratch) / "master-v3"
            directory.mkdir()
            store = directory / "store.sqlite"
            history = directory / "history.sqlite"
            store.write_bytes(b"sealed store")
            history.write_bytes(b"sealed history")
            created = {"status": "COMPLETE", "project_id": "c" * 66,
                       "genesis_layer": "d" * 66, "root": "e" * 64, "root_serial": 1}
            record = {
                "schema": "core-fs-bench-pro-exec-fuse-edit-master-v3",
                "fixture_bytes": size, "fixture_sha256": v2.FIXTURE_SHA256[size],
                "fixture_canonical_root": v2.FIXTURE_CANONICAL_ROOT[size],
                "fixture_extent_count": v2.FIXTURE_INITIAL_COUNT[size],
                "compatibility_key": key,
                "compatibility_key_sha256": hashlib.sha256(
                    json.dumps(key, sort_keys=True).encode()).hexdigest()[:16],
                "store": str(store), "history": str(history),
                "store_bytes": store.stat().st_size, "history_bytes": history.stat().st_size,
                "store_sha256": edit_route.sha256(store),
                "history_sha256": edit_route.sha256(history),
                "producer_source_commit": "f" * 40, "producer_source_tree": "1" * 40,
                "producer_binary_sha256": key["benchmark_init_sha256"],
                "project_id": created["project_id"], "genesis_layer": created["genesis_layer"],
                "genesis_root": created["root"], "genesis_root_serial": created["root_serial"],
                "init_receipt": created,
            }
            (directory / "master.json").write_text(json.dumps(record))
            self.assertEqual(edit_route._sealed_master(directory, key, size)["store_sha256"],
                             record["store_sha256"])
            cloned_store = Path(scratch) / "copy-store.sqlite"
            cloned_history = Path(scratch) / "copy-history.sqlite"
            cloned_store.write_bytes(store.read_bytes())
            cloned_history.write_bytes(history.read_bytes())
            self.assertTrue(edit_route._clone_seals(cloned_store, cloned_history, record)[1])
            cloned_history.write_bytes(b"wrong copy")
            self.assertFalse(edit_route._clone_seals(cloned_store, cloned_history, record)[1])
            store.write_bytes(b"corrupt store")
            with self.assertRaisesRegex(ValueError, "seal mismatch"):
                edit_route._sealed_master(directory, key, size)
            store.write_bytes(b"sealed store")
            (directory / "store.sqlite-wal").write_bytes(b"incomplete")
            with self.assertRaisesRegex(ValueError, "sidecars"):
                edit_route._sealed_master(directory, key, size)
            binary = Path(scratch) / "benchmark_init"
            binary.write_bytes(b"never execute this incomplete attempt")
            identity = {"cargo_lock_sha256": "b" * 64}
            pending_key = edit_route.compatibility_key(
                identity, {"bytes": size, "sha256": v2.FIXTURE_SHA256[size]}, self.rows[0],
                edit_route.sha256(binary))
            pending_digest = hashlib.sha256(json.dumps(pending_key, sort_keys=True).encode()
                                            ).hexdigest()[:16]
            (Path(scratch) / f".master-v3-{size}-{pending_digest}-interrupted").mkdir()
            with self.assertRaisesRegex(ValueError, "incomplete prior v3 master"):
                edit_route._master_v3(Path(scratch), size, {"benchmark_init": binary},
                                      identity, "unused", self.rows[0], 1_000_000_000)


if __name__ == "__main__":
    unittest.main()
