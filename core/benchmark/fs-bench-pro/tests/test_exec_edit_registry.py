"""Focused checks for the frozen #232 Workspace Exec/FUSE registry (Phase 1)."""
import hashlib
import json
from pathlib import Path
import sys
import unittest

HERE = Path(__file__).resolve().parent
BENCH = HERE.parent
sys.path.insert(0, str(BENCH))
from shared import edit_contract as contract  # noqa: E402
from families import (workspace_exec_edit_canonical_chunk_count as canonical,  # noqa: E402
                      workspace_exec_edit_length_changing as changing,
                      workspace_exec_edit_length_preserving as preserving)

STRUCTURAL = {"insert-middle-4k", "delete-middle-4k", "prepend-head-4k",
              "replace-grow-middle-2k-to-4k", "replace-shrink-middle-4k-to-2k"}


class Registry(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.rows = contract.registry()
        cls.by_id = {row["scenario_id"]: row for row in cls.rows}

    def test_cardinality_and_families(self):
        self.assertEqual(len(self.rows), 56)
        self.assertEqual(len(self.by_id), 56)
        counts = {family: sum(row["family_id"] == family for row in self.rows)
                  for family in (preserving.FAMILY_ID, changing.FAMILY_ID, canonical.FAMILY_ID)}
        self.assertEqual(counts[preserving.FAMILY_ID], 12)
        self.assertEqual(counts[changing.FAMILY_ID], 32)
        self.assertEqual(counts[canonical.FAMILY_ID], 12)
        self.assertEqual(contract.REPETITION, 1)
        self.assertTrue(all(row["repetition"] == 1 for row in self.rows))

    def test_distinct_workspace_exec_family_ids(self):
        for row in self.rows:
            self.assertTrue(row["family_id"].startswith("workspace_exec_edit_"))
            self.assertNotEqual(row["family_id"], row["historical_selection_id"].split("-on-")[0])
            self.assertTrue(row["scenario_id"].endswith("-exec-v1"))
            self.assertEqual(row["route"], "sdk-exec-fuse-edit-commit-v1")
            self.assertEqual(row["operation_entrypoint"], "WorkspaceApi::exec")
            self.assertEqual(row["operation_surface"], "workspace-posix-fuse")
            self.assertEqual(row["acknowledgement_boundary"], "WorkspaceApi::commit")
            self.assertEqual(row["scenario_version"], 1)

    def test_no_inherited_pass_status(self):
        """A matching family name never inherits the historical direct-SDK PASS."""
        for row in self.rows:
            self.assertIn(row["registration_status"], ("REGISTERED", "NOT_RUN"))
            self.assertNotIn("pass", json.dumps(row).lower().replace("payload", ""))

    def test_six_pristine_sizes_and_capped_inputs(self):
        self.assertEqual(sorted({row["fixture_bytes"] for row in self.rows}),
                         contract.PREPARED_SIZES)
        capped = {row["scenario_id"]: row["fixture_bytes"] for row in self.rows
                  if "result-capped-v2" in row["scenario_id"]}
        self.assertEqual(len(capped), 5)
        self.assertEqual(sorted(capped.values()), [524_283_904] * 4 + [524_285_952])
        grown = self.by_id["replace-grow-middle-2k-to-4k-on-500mib-result-capped-v2-ops-1-exec-v1"]
        self.assertEqual(grown["fixture_bytes"], 524_285_952)
        self.assertEqual(grown["final_bytes"], 524_288_000)

    def test_twenty_structural_cases_visible_as_not_run(self):
        structural = [row for row in self.rows if row["registration_status"] == "NOT_RUN"]
        self.assertEqual(len(structural), 20)
        self.assertEqual({row["operation_key"] for row in structural}, STRUCTURAL)
        for row in structural:
            self.assertIn(row["operation_key"], STRUCTURAL)
            self.assertEqual(row["not_run_reason"], contract.STRUCTURAL_NOT_RUN_REASON)
            self.assertIsNone(row["command"])
            self.assertEqual(row["editor_algorithm"], "unfrozen-in-place-window-shift")
            self.assertEqual(row["editor_syscalls"], [])
            self.assertIsNone(row["final_sha256"])

    def test_registered_cases_freeze_command_algorithm_and_oracle(self):
        registered = [row for row in self.rows if row["registration_status"] == "REGISTERED"]
        self.assertEqual(len(registered), 36)
        for row in registered:
            self.assertTrue(row["command"].startswith(contract.TOOL_PATH + " "))
            self.assertTrue(row["editor_syscalls"])
            self.assertLessEqual(row["edit_start"] + row["delete_len"], row["fixture_bytes"])
            self.assertEqual(row["final_bytes"], row["fixture_bytes"] - row["delete_len"]
                             + row["replacement_len"])
            self.assertEqual(row["oracle"]["final_bytes"], row["final_bytes"])
            self.assertLessEqual(row["oracle"]["bounded_window_bytes"],
                                 contract.BOUNDED_ORACLE_BYTES)
            self.assertEqual(row["oracle"]["full_file_digest"],
                             row["final_bytes"] <= contract.FULL_DIGEST_MAX_BYTES)
            self.assertEqual(len(row["final_sha256"]), 64)

    def test_engineering_targets_match_the_g2_table(self):
        for row in self.rows:
            family = {preserving.FAMILY_ID: preserving, changing.FAMILY_ID: changing,
                      canonical.FAMILY_ID: canonical}[row["family_id"]]
            self.assertEqual(row["g2_target_ms"],
                             family.TARGETS_MS[row["historical_selection_id"]])
            self.assertLess(row["g2_target_ms"], 15_000)
            self.assertEqual(row["performance_gate"],
                             "edit_commit_ns <= g2_target_ms at 0.01 ms precision")

    def test_fixture_recipe_reproduces_the_v016_digests(self):
        for size, expected in contract.FIXTURE_SHA256.items():
            self.assertEqual(contract.fixture_sha256(size), expected)

    def test_payload_and_result_recipes_are_reproducible(self):
        for row in self.rows:
            if row["registration_status"] != "REGISTERED":
                continue
            replacement = contract.payload_bytes(row["payload_seed"], row["replacement_len"],
                                                 row["replacement_kind"])
            self.assertEqual(hashlib.sha256(replacement).hexdigest(), row["replacement_sha256"])
            self.assertEqual(contract.final_sha256(row["fixture_bytes"], row["edit_start"],
                                                  row["delete_len"], replacement),
                             row["final_sha256"])

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
        path = BENCH / "registry/workspace-exec-edit-v1.json"
        self.assertEqual(path.read_bytes(), contract.registry_body())
        self.assertEqual(hashlib.sha256(path.read_bytes()).hexdigest(), contract.REGISTRY_SHA256)
        document = json.loads(path.read_text())
        self.assertEqual(len(document["cases"]), 56)
        self.assertEqual(document["route"], contract.ROUTE)
        self.assertEqual(document["capped_v1_duplicates_added"], 0)

    def test_no_timed_sample_exists_yet(self):
        """Phase 1 gate: the contract is committed before any sample is taken."""
        self.assertFalse((BENCH.parent.parent.parent / "benchmark-results/fs-bench-pro").joinpath(
            "exec-fuse-edit").exists())


if __name__ == "__main__":
    unittest.main()
