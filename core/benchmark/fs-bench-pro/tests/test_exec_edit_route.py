"""Focused checks for the #232 Exec/FUSE route substrate and shift algorithm."""
import hashlib
import json
from pathlib import Path
import sys
import tempfile
import unittest

HERE = Path(__file__).resolve().parent
BENCH = HERE.parent
sys.path.insert(0, str(BENCH))
import runner  # noqa: E402
from shared import edit_cache, edit_contract as contract, edit_route  # noqa: E402

SHIFT_OPS = {"insert-middle-4k", "delete-middle-4k", "prepend-head-4k",
             "replace-grow-middle-2k-to-4k", "replace-shrink-middle-4k-to-2k"}


def inplace_shift(data, start, delete_len, length, replacement, block):
    """The declared window shift applied block by block to one mutable buffer.

    This mirrors the frozen tool algorithm: a grow extends the buffer and moves
    the suffix backward from the old end, a shrink moves it forward from the
    splice point and then truncates, and either direction ends by writing the
    declared replacement over the stale window.
    """
    initial = len(data)
    final = initial - delete_len + length
    tail_start = start + delete_len
    if final > initial:
        delta = final - initial
        data.extend(b"\0" * delta)
        end = initial
        while end > tail_start:
            take = min(block, end - tail_start)
            source = end - take
            data[source + delta:source + delta + take] = data[source:source + take]
            end = source
    elif final < initial:
        delta = initial - final
        cursor = tail_start
        while cursor < initial:
            take = min(block, initial - cursor)
            data[cursor - delta:cursor - delta + take] = data[cursor:cursor + take]
            cursor += take
        del data[final:]
    if length:
        data[start:start + length] = replacement
    return bytes(data)


class ShiftAlgorithm(unittest.TestCase):
    """Development checks: direction, boundaries, exact bytes before sampling."""

    def test_block_wise_shift_equals_the_declared_splice(self):
        block = 1024
        pristine = bytes(range(256)) * 41  # 10,496 bytes, byte pattern wraps
        starts = (0, 1, 7, block - 1, block, block + 1, 2 * block - 3, 3 * block)
        deletes = (0, 1, 17, block, block + 5, 2 * block + 7)
        lengths = (0, 1, 31, block, block + 9, 2 * block + 1)
        checked = 0
        for start in starts:
            for delete in deletes:
                if start + delete > len(pristine):
                    continue
                for length in lengths:
                    if delete == 0 and length == 0:
                        continue
                    replacement = bytes((7 * start + 3 * length + index) % 251
                                        for index in range(length))
                    expected = (pristine[:start] + replacement
                                + pristine[start + delete:])
                    observed = inplace_shift(bytearray(pristine), start, delete, length,
                                             replacement, block)
                    self.assertEqual(len(observed), len(expected))
                    self.assertEqual(observed, expected,
                                     f"start={start} delete={delete} length={length}")
                    checked += 1
        self.assertGreater(checked, 200)

    def test_direction_follows_the_sign_of_the_size_change(self):
        block = 64
        pristine = bytes(range(64)) * 4
        grown = inplace_shift(bytearray(pristine), 100, 0, 5, b"abcde", block)
        self.assertEqual(len(grown), len(pristine) + 5)
        self.assertEqual(grown[:100], pristine[:100])
        self.assertEqual(grown[100:105], b"abcde")
        self.assertEqual(grown[105:], pristine[100:])
        shrunk = inplace_shift(bytearray(pristine), 100, 7, 3, b"xyz", block)
        self.assertEqual(len(shrunk), len(pristine) - 4)
        self.assertEqual(shrunk[:100], pristine[:100])
        self.assertEqual(shrunk[100:103], b"xyz")
        self.assertEqual(shrunk[103:], pristine[107:])
        prepended = inplace_shift(bytearray(pristine), 0, 0, 4, b"head", block)
        self.assertEqual(prepended, b"head" + pristine)
        deleted = inplace_shift(bytearray(pristine), 8, 16, 0, b"", block)
        self.assertEqual(deleted, pristine[:8] + pristine[24:])

    def test_declared_rows_shift_to_their_declared_digest(self):
        for row in contract.registry():
            if row["operation_key"] not in SHIFT_OPS or row["fixture_bytes"] > 1_048_576:
                continue
            pristine = contract.splitmix_bytes(contract.FIXTURE_GENERATOR_SEED,
                                               row["fixture_bytes"])
            replacement = contract.payload_bytes(row["payload_seed"], row["replacement_len"],
                                                 row["replacement_kind"])
            observed = inplace_shift(bytearray(pristine), row["edit_start"], row["delete_len"],
                                     row["replacement_len"], replacement,
                                     contract.SHIFT_BLOCK_BYTES)
            self.assertEqual(len(observed), row["final_bytes"])
            self.assertEqual(hashlib.sha256(observed).hexdigest(), row["final_sha256"])
            for window in row["oracle"]["windows"]:
                self.assertEqual(
                    hashlib.sha256(observed[window["offset"]:window["offset"]
                                           + window["bytes"]]).hexdigest(),
                    window["sha256"])


class Route(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.rows = {row["scenario_id"]: row for row in contract.registry()}
        cls.case = cls.rows["overwrite-middle-4k-on-1mib-ops-1-exec-v2"]
        cls.shift = cls.rows["insert-middle-4k-on-1mib-ops-1-exec-v2"]

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
        for case in (self.case, self.shift):
            record = edit_route.case_record(case, master, "e" * 32)
            self.assertEqual(record["final_bytes"], case["final_bytes"])
            self.assertEqual(record["edit_start"], case["edit_start"])
            self.assertEqual(record["window_count"], len(case["oracle"]["windows"]))
            for index, window in enumerate(case["oracle"]["windows"]):
                self.assertEqual(record[f"window{index}_offset"], window["offset"])
                self.assertEqual(record[f"window{index}_bytes"], window["bytes"])
                self.assertEqual(record[f"window{index}_sha256"], window["sha256"])
            self.assertLessEqual(
                sum(record[f"window{index}_bytes"] for index in range(record["window_count"])),
                contract.BOUNDED_ORACLE_BYTES)
            self.assertEqual(record["full_file_digest"], 1)
            self.assertEqual(record["command"], case["command"])
            self.assertEqual(record["stack_body"], "a" * 32)
            path = BENCH / "target-case-check.txt"
            try:
                edit_route.case_file(path, record)
                reloaded = dict(line.split("=", 1) for line in path.read_text().splitlines())
                spec = dict(entry.split("=", 1) for entry in edit_route.case_spec(record).split(";"))
                self.assertEqual(reloaded["scenario_id"], case["scenario_id"])
                self.assertEqual(int(reloaded["final_bytes"]), case["final_bytes"])
                self.assertEqual(spec, {key: str(value) for key, value in record.items()})
            finally:
                path.unlink(missing_ok=True)

    def test_result_window_matches_the_declared_digest(self):
        for row in contract.registry():
            if row["final_bytes"] > 10_485_760:
                continue
            replacement = contract.payload_bytes(row["payload_seed"], row["replacement_len"],
                                                row["replacement_kind"])
            whole = contract.result_window(row["fixture_bytes"], row["edit_start"],
                                          row["delete_len"], replacement, 0, row["final_bytes"])
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
            "driver": {"status": "COMPLETE", "edit_commit_ns": 1_000_000,
                       "unmount_ok": True, "sandbox_delete_ok": True},
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
        receipt["verification"]["status"] = "PASS"
        receipt["driver"]["unmount_ok"] = False
        self.assertEqual(edit_route.terminal_status(receipt, row)["status"], "FAIL")
        receipt["driver"]["unmount_ok"] = True
        receipt["verification"]["status"] = "SKIPPED"
        self.assertEqual(edit_route.terminal_status(receipt, row)["status"], "INCOMPLETE")

    def test_historical_v2_terminal_is_retained_but_failed_verifier_is_audited(self):
        row = self.case
        receipt = {"status": "INELIGIBLE", "scenario_id": row["scenario_id"],
                   "complete_command_status": "PASS",
                   "driver": {"status": "COMPLETE", "edit_commit_ns": 1_000_000,
                              "unmount_ok": True, "sandbox_delete_ok": True,
                              "branch_id": "branch", "head_commit": "head"},
                   "telemetry": {"status": "PASS"},
                   "verification": {"status": "SKIPPED"},
                   "store": "/case/store", "history": "/case/history",
                   "cache": {"macos-store": {"status": "PASS"},
                             "linux-fuse-backing": {"status": "INELIGIBLE"}}}
        with tempfile.TemporaryDirectory() as scratch:
            folder = Path(scratch)
            (folder / "run.json").write_text(json.dumps({"receipt": receipt}))
            (folder / "verification.json").write_text(json.dumps({
                "status": "PASS", "scenario_id": row["scenario_id"],
                "exit_code": 0, "timeout": False, "wall_ns": 1_000_000,
                "child": None}))
            reported = runner.edit_report_row(row, folder)
            self.assertEqual(reported["terminal"], "INELIGIBLE")
            self.assertEqual(reported["recorded_terminal"], "INELIGIBLE")
            self.assertEqual(reported["current_admission_status"], "FAIL")
            self.assertEqual(reported["verification"], "PASS")
            self.assertEqual(reported["derived_verification_gate"], "FAIL")

    def test_runner_registers_the_edit_route_without_a_second_system(self):
        rows = {row["scenario_id"]: row for row in contract.registry()}
        self.assertIn("overwrite-middle-4k-on-1mib-ops-1-exec-v2", rows)
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

    def test_report_edit_covers_every_registered_row(self):
        runs = BENCH.parent.parent.parent / "benchmark-results/fs-bench-pro/absent-campaign"
        table, document = runner.report_edit(runs)
        lines = table.strip().splitlines()
        self.assertEqual(len(lines), 57)
        self.assertEqual(document["counts"]["rows"], 56)
        self.assertEqual(document["counts"]["attempted"], 0)
        self.assertEqual(document["counts"]["terminal"]["NOT_RUN"], 56)
        for row in contract.registry():
            self.assertIn(row["scenario_id"], table)


if __name__ == "__main__":
    unittest.main()
