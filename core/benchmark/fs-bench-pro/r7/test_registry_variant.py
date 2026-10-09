"""Append-only oracle contract preserves matrix/bodies and historical identity."""
from pathlib import Path
import sys
import unittest
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from r7 import registry, registry_variant


class RegistryVariantContract(unittest.TestCase):
    def test_same_full_matrix_and_workloads_with_distinct_oracle_identity(self):
        old = registry.registration()
        new = registry_variant.document()
        self.assertEqual(len(new["selections"]),len(old["selections"]))
        for previous,current in zip(old["selections"],new["selections"]):
            expected = dict(previous)
            if previous["case_id"] == "E18" and previous["cache_class"] == "C":
                expected["reason"] = registry_variant.E18_C_REASON
            self.assertEqual(current,expected)
        self.assertEqual(new["workload_identity"],old["workload_identity"])
        self.assertNotEqual(registry_variant.identity(),registry.registry_identity())
        for previous,current in zip(old["cases"],new["cases"]):
            self.assertEqual(previous["workload"],current["workload"])
            self.assertEqual(previous["classes"],current["classes"])

    def test_owner_unrun_and_original_registry_remain_unchanged(self):
        before = registry.registry_identity()
        new = registry_variant.document()
        owner = next(case for case in new["cases"] if case["case_id"] == "E09")
        self.assertEqual(owner["not_run_reason"],"no authorized normalized oracle")
        self.assertNotIn("timing_oracle_variant",owner)
        self.assertEqual(before,registry.registry_identity())

    def test_variant_explicitly_retains_raw_stat_and_separate_l_proof_limit(self):
        contract = registry_variant.CONTRACT
        self.assertIn("stat-cache",contract["excluded_from_cross_filesystem_comparison"])
        self.assertIn("raw",contract["limitations"])
        self.assertIn("fresh mount",contract["separate_L_proof"])

    def test_unrefreshed_index_warmup_conflict_retains_all_unrun_rows(self):
        rows = [row for row in registry_variant.document()["selections"]
                if row["case_id"] == "E18" and row["cache_class"] == "C"]
        self.assertEqual({row["arm"] for row in rows},{"L","N","P"})
        for row in rows:
            self.assertEqual(row["status"],"NOT_RUN")
            self.assertEqual(row["sample_count"],0)
            self.assertEqual(row["reason"],registry_variant.E18_C_REASON)


if __name__ == "__main__":
    unittest.main()
