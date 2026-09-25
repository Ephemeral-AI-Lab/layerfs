"""Focused checks for the frozen #232 Workspace Exec/FUSE registry (scenario 2).

These are development checks: they prove the declared edit shapes, shift
directions, boundaries and expected bytes/size/chunk count before any benchmark
sample is collected. They are not performance samples and never substitute for
one.
"""
import hashlib
import json
from pathlib import Path
import sys
import unittest

HERE = Path(__file__).resolve().parent
BENCH = HERE.parent
sys.path.insert(0, str(BENCH))
from shared import edit_contract as contract  # noqa: E402
from families import (edit_canonical_chunk_count as canonical,  # noqa: E402
                      edit_length_changing as changing,
                      edit_length_preserving as preserving)

SHIFT_OPS = {"insert-middle-4k", "delete-middle-4k", "prepend-head-4k",
             "replace-grow-middle-2k-to-4k", "replace-shrink-middle-4k-to-2k"}
FAMILIES = {preserving.FAMILY_ID: (preserving, 12), changing.FAMILY_ID: (changing, 32),
            canonical.FAMILY_ID: (canonical, 12)}


class Registry(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.rows = contract.registry()
        cls.by_id = {row["scenario_id"]: row for row in cls.rows}

    def test_cardinality_and_families(self):
        self.assertEqual(len(self.rows), 56)
        self.assertEqual(len(self.by_id), 56)
        for family, (_, expected) in FAMILIES.items():
            self.assertEqual(sum(row["family_id"] == family for row in self.rows), expected)
        self.assertEqual(contract.REPETITION, 1)
        self.assertTrue(all(row["repetition"] == 1 for row in self.rows))
        self.assertEqual(sorted({row["fixture_bytes"] for row in self.rows}),
                         contract.PREPARED_SIZES)

    def test_every_case_is_registered_with_a_frozen_command(self):
        for row in self.rows:
            self.assertEqual(row["registration_status"], "REGISTERED")
            self.assertIsNone(row["not_run_reason"])
            self.assertTrue(row["command"].startswith(contract.TOOL_PATH + " "))
            self.assertTrue(row["editor_syscalls"])
            self.assertEqual(len(row["final_sha256"]), 64)
            self.assertEqual(row["oracle"]["final_bytes"], row["final_bytes"])
            self.assertEqual(row["scenario_version"], 2)
            self.assertTrue(row["scenario_id"].endswith("-exec-v2"))
            self.assertNotIn("exec-v1", row["scenario_id"])
            self.assertEqual(row["route"], "sdk-exec-fuse-edit-commit-v1")
            self.assertEqual(row["operation_contract_id"],
                             "workspace-exec-fuse-edit-commit-v2")
            self.assertEqual(row["operation_entrypoint"], "WorkspaceApi::exec")
            self.assertEqual(row["operation_surface"], "workspace-posix-fuse")
            self.assertEqual(row["acknowledgement_boundary"], "WorkspaceApi::commit")

    def test_structural_cases_freeze_the_shift_algorithm(self):
        structural = [row for row in self.rows if row["operation_key"] in SHIFT_OPS]
        self.assertEqual(len(structural), 20)
        for row in structural:
            grow = row["final_bytes"] > row["fixture_bytes"]
            direction = "grow" if grow else "shrink"
            self.assertEqual(row["editor_direction"], direction)
            self.assertEqual(row["editor_algorithm"], f"in-place-window-shift-{direction}")
            self.assertEqual(row["editor_block_bytes"], contract.SHIFT_BLOCK_BYTES)
            self.assertIn("shift", row["command"])
            self.assertIn(f"--direction {direction}", row["command"])
            self.assertIn(f"--delete-length {row['delete_len']}", row["command"])
            self.assertIn(f"--offset {row['edit_start']}", row["command"])
            self.assertIn(f"--expect-size {row['fixture_bytes']}", row["command"])
            self.assertIn(f"--length {row['replacement_len']}", row["command"])
            self.assertEqual(row["final_bytes"],
                             row["fixture_bytes"] - row["delete_len"] + row["replacement_len"])
            if row["replacement_len"]:
                self.assertIn(f"--payload {contract.PAYLOAD_DIRECTORY}/"
                              f"{row['operation_key']}.bin", row["command"])
            else:
                self.assertNotIn("--payload", row["command"])
            # Direction follows the declared sizes, and the two syscall sets keep
            # their declared order of truncate and I/O.
            self.assertIn("pread", row["editor_syscalls"])
            self.assertIn("pwrite", row["editor_syscalls"])
            self.assertIn("ftruncate", row["editor_syscalls"])

    def test_shift_boundaries_match_the_historical_shapes(self):
        expected = {
            "insert-middle-4k": lambda size: (size // 2, 0, 4096),
            "delete-middle-4k": lambda size: (size // 2 - 2048, 4096, 0),
            "prepend-head-4k": lambda size: (0, 0, 4096),
            "replace-grow-middle-2k-to-4k": lambda size: (size // 2 - 1024, 2048, 4096),
            "replace-shrink-middle-4k-to-2k": lambda size: (size // 2 - 2048, 4096, 2048),
        }
        for row in self.rows:
            if row["operation_key"] not in SHIFT_OPS:
                continue
            start, delete_len, replacement_len = expected[row["operation_key"]](
                row["fixture_bytes"])
            self.assertEqual(row["edit_start"], start)
            self.assertEqual(row["delete_len"], delete_len)
            self.assertEqual(row["replacement_len"], replacement_len)
            self.assertLessEqual(row["edit_start"] + row["delete_len"], row["fixture_bytes"])

    def test_oracle_windows_are_declared_and_bounded(self):
        for row in self.rows:
            windows = row["oracle"]["windows"]
            self.assertTrue(1 <= len(windows) <= 2)
            total = 0
            previous_end = -1
            for window in windows:
                self.assertLess(window["bytes"], contract.BOUNDED_ORACLE_BYTES)
                self.assertEqual(len(window["sha256"]), 64)
                self.assertGreaterEqual(window["offset"], 0)
                self.assertLessEqual(window["offset"] + window["bytes"], row["final_bytes"])
                self.assertGreaterEqual(window["offset"], previous_end)
                previous_end = window["offset"] + window["bytes"]
                total += window["bytes"]
            self.assertLessEqual(total, contract.BOUNDED_ORACLE_BYTES)
            self.assertEqual(total, row["oracle"]["bounded_window_bytes"])
            self.assertEqual(row["oracle"]["full_file_digest"],
                             row["final_bytes"] <= contract.FULL_DIGEST_MAX_BYTES)

    def test_shift_window_covers_the_splice_and_the_tail(self):
        for row in self.rows:
            if row["operation_key"] not in SHIFT_OPS or row["replacement_len"] == 0:
                continue
            seam = row["oracle"]["windows"][0]
            self.assertLessEqual(seam["offset"], row["edit_start"])
            self.assertGreaterEqual(seam["offset"] + seam["bytes"],
                                    row["edit_start"] + row["replacement_len"])

    def test_engineering_targets_match_the_g2_table(self):
        for row in self.rows:
            family = FAMILIES[row["family_id"]][0]
            self.assertEqual(row["g2_target_ms"],
                             family.TARGETS_MS[row["historical_selection_id"]])
            self.assertLess(row["g2_target_ms"], 15_000)
            self.assertEqual(row["performance_gate"],
                             "edit_commit_ns <= g2_target_ms at 0.01 ms precision")

    def test_capped_inputs_and_the_grown_500mib_case(self):
        capped = {row["scenario_id"]: row["fixture_bytes"] for row in self.rows
                  if "result-capped-v2" in row["scenario_id"]}
        self.assertEqual(len(capped), 5)
        self.assertEqual(sorted(capped.values()), [524_283_904] * 4 + [524_285_952])
        grown = self.by_id[
            "replace-grow-middle-2k-to-4k-on-500mib-result-capped-v2-ops-1-exec-v2"]
        self.assertEqual(grown["fixture_bytes"], 524_285_952)
        self.assertEqual(grown["final_bytes"], 524_288_000)

    def test_fixture_recipe_reproduces_the_v016_digests(self):
        for size, expected in contract.FIXTURE_SHA256.items():
            self.assertEqual(contract.fixture_sha256(size), expected)

    def test_payload_and_result_recipes_are_reproducible(self):
        for row in self.rows:
            replacement = contract.payload_bytes(row["payload_seed"], row["replacement_len"],
                                                 row["replacement_kind"])
            self.assertEqual(hashlib.sha256(replacement).hexdigest(), row["replacement_sha256"])
            self.assertEqual(contract.final_sha256(row["fixture_bytes"], row["edit_start"],
                                                  row["delete_len"], replacement),
                             row["final_sha256"])

    def test_declared_window_digests_match_the_result_recipe(self):
        for row in self.rows:
            if row["final_bytes"] > 20_971_520:
                continue
            replacement = contract.payload_bytes(row["payload_seed"], row["replacement_len"],
                                                 row["replacement_kind"])
            for window in row["oracle"]["windows"]:
                observed = contract.result_window(
                    row["fixture_bytes"], row["edit_start"], row["delete_len"], replacement,
                    window["offset"], window["bytes"])
                self.assertEqual(hashlib.sha256(observed).hexdigest(), window["sha256"])

    def test_canonical_rows_pin_root_and_count(self):
        rows = [row for row in self.rows if row["family_id"] == canonical.FAMILY_ID]
        self.assertEqual(len(rows), 12)
        for row in rows:
            shape = row["operation_key"].rsplit("-", 1)[-1]
            initial, final, sha256, root = canonical.EXPECTED[(shape, row["fixture_bytes"])]
            self.assertEqual(row["initial_count"], initial)
            self.assertEqual(row["canonical_count_expected"], final)
            self.assertEqual(row["canonical_root_expected"], root)
            self.assertEqual(row["final_sha256"], sha256)
            self.assertNotEqual(final, initial) if shape != "preserve" else None

    def test_cache_contract_and_eligibility_are_frozen(self):
        document = contract.document()
        self.assertEqual(document["cache_contract"], "exec-fuse-edit-cache-v1")
        self.assertEqual(document["clone_method"], "independent-writable-byte-copy")
        self.assertEqual(sorted(document["cache_domains"]),
                         ["linux-fuse-backing", "macos-store"])
        self.assertEqual(document["cache_domains"]["macos-store"]["eligibility"],
                         "PASS only when the reported resident page count is 0 for the whole domain")
        self.assertIn("INELIGIBLE", document["cache_domains"]["linux-fuse-backing"]["eligibility"])
        self.assertEqual(len(document["eligibility_rules"]), 10)
        for row in self.rows:
            self.assertEqual(row["cache_contract"], contract.CACHE_CONTRACT)
            self.assertEqual(row["eligibility_rule"], contract.ELIGIBILITY_INELIGIBLE_REASON)
        self.assertEqual(document["budgets"]["complete_command_budget_ns"], 15_000_000_000)
        self.assertEqual(document["budgets"]["verifier_hard_budget_ns"], 15_000_000_000)

    def test_committed_registry_document_and_digest(self):
        path = BENCH / contract.REGISTRY_PATH
        self.assertEqual(path.read_bytes(), contract.registry_body())
        self.assertEqual(hashlib.sha256(path.read_bytes()).hexdigest(), contract.REGISTRY_SHA256)
        document = json.loads(path.read_text())
        self.assertEqual(len(document["cases"]), 56)
        self.assertEqual(document["route"], contract.ROUTE)
        self.assertEqual(document["scenario_version"], 2)
        self.assertEqual(document["capped_v1_duplicates_added"], 0)

    def test_scenario_one_registry_is_retained_unrewritten(self):
        """The frozen version 1 registry and its NOT_RUN rows stay historical."""
        path = BENCH / contract.HISTORICAL_REGISTRY_PATH
        self.assertEqual(hashlib.sha256(path.read_bytes()).hexdigest(),
                         contract.HISTORICAL_REGISTRY_SHA256)
        document = json.loads(path.read_text())
        self.assertEqual(document["scenario_version"], 1)
        not_run = [row for row in document["cases"] if row["registration_status"] == "NOT_RUN"]
        self.assertEqual(len(not_run), 20)
        for row in not_run:
            self.assertIn("structural-shift-algorithm-unfrozen", row["not_run_reason"])
            self.assertTrue(row["scenario_id"].endswith("-exec-v1"))

    def test_prospective_all_ioctl_registry_is_distinct_and_complete(self):
        path = BENCH / "registry/workspace-exec-edit-v3.json"
        self.assertEqual(hashlib.sha256(path.read_bytes()).hexdigest(),
                         "469bf23363083980cf2e424394eba994ed7a6009116d215b3d96c379e66b975c")
        document = json.loads(path.read_text())
        self.assertEqual(document["scenario_version"], 3)
        self.assertEqual(document["historical_registry"]["sha256"], contract.REGISTRY_SHA256)
        rows = document["cases"]
        self.assertEqual(len(rows), 56)
        self.assertEqual(len({row["scenario_id"] for row in rows}), 56)
        self.assertEqual({family: sum(row["family_id"] == family for row in rows)
                          for family in FAMILIES},
                         {family: count for family, (_, count) in FAMILIES.items()})
        self.assertEqual(sum(row["replacement_len"] == 65536 for row in rows), 12)
        self.assertEqual(max(row["replacement_len"] for row in rows), 65536)
        self.assertEqual(sum(row["phase1_selection"] for row in rows), 8)
        self.assertEqual(set(document["minimal_phase1_selection"]),
                         {row["scenario_id"] for row in rows if row["phase1_selection"]})
        for row in rows:
            old = self.by_id[row["scenario_id"].replace("-exec-ioctl-v3", "-exec-v2")]
            for key in ("fixture_bytes", "fixture_sha256", "replacement_sha256",
                        "final_bytes", "final_sha256", "oracle"):
                self.assertEqual(row[key], old[key])
            self.assertEqual(row["route"], document["route"])
            self.assertEqual(row["operation_surface"], "workspace-linux-fuse-ioctl")
            self.assertEqual(row["expected_workspace_revisions"], 1)
            self.assertEqual(row["expected_callback_total"],
                             sum(row["expected_callbacks"].values()))
            self.assertIn(" splice ", row["command"])
            self.assertIn(" --stream ", row["command"])
            self.assertIn(" --output-version 5", row["command"])
            self.assertNotIn("pwrite", row["command"])
            self.assertEqual(row["final_bytes"], row["fixture_bytes"]
                             - row["delete_len"] + row["replacement_len"])
            staged = row["replacement_stream"][0]["kind"] == "zero" or row["replacement_len"] > 4096
            self.assertEqual(row["expected_callbacks"]["begin"], int(staged))
            self.assertEqual(row["expected_callbacks"]["apply"], int(staged))
            self.assertEqual(row["expected_callbacks"]["inline_edit"], int(not staged))


if __name__ == "__main__":
    unittest.main()
