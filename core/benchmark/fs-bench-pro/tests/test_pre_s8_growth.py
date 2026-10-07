"""Growth successor selection and independent omission/tamper checks."""
import copy
import json
from pathlib import Path
import sys
import tempfile
import unittest
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from shared.pre_s8_growth import validate
from shared.pre_s8_registration import GROWTH_REGISTRY
ROOT = Path(__file__).resolve().parents[4]


class Growth(unittest.TestCase):
    def setUp(self):
        self.registry = json.loads((ROOT / GROWTH_REGISTRY).read_text())
        self.case = self.registry["cases"]["growth-n1000"]
        point = {"schema": "pre-s8-growth-point-v1", "stage": "bound", "index": 0,
                 "read_handles": 4, "cache_bytes": 200, "credited_bytes": 0, "outstanding": 0,
                 "queued": 0, "receipt_overruns": 0, "closed_namespaces": 0,
                 "rust_live_requested": 1000}
        self.values = [{"schema": "pre-s8-growth-start-v1", "mode": "commit", "files": 1000,
                        "amount": 1, "profile": "sqlite-wal-off-v2"}, point,
                       {"schema": "pre-s8-growth-operation-v1", "index": 0, "attempts": 1,
                        "root": "a" * 64, "head": "12" + "b" * 64, "reserve": 1,
                        "initial": 1, "refills": 0, "publish": 1, "history": 1,
                        "writes": 3, "kind": "commit", "engine_jobs": 2},
                       {**point, "stage": "released", "index": 1},
                       {**point, "stage": "drained", "index": 1, "closed_namespaces": 1},
                       {"schema": "pre-s8-growth-terminal-v1", "owner_stopped": True,
                        "store_released": True, "rust_live_requested": 100}]

    def check(self, values):
        with tempfile.TemporaryDirectory() as d:
            p = Path(d) / "rows.jsonl"
            p.write_text("".join(json.dumps(v) + "\n" for v in values))
            return validate(p, self.case)

    def test_diagnostic_never_numeric_pass(self):
        value = self.check(self.values)
        self.assertEqual(value["functional_counts"], "PASS")
        self.assertEqual(value["memory_growth_verdict"], "REQUIRES_CROSS_CASE_AND_SOURCE_ANALYSIS")

    def test_missing_release(self):
        del self.values[3]
        with self.assertRaisesRegex(ValueError, "point order"):
            self.check(self.values)

    def test_retained_credit(self):
        self.values[3]["credited_bytes"] = 1
        with self.assertRaisesRegex(ValueError, "retains credits"):
            self.check(self.values)

    def test_missing_terminal_cleanup(self):
        self.values[4]["closed_namespaces"] = 0
        with self.assertRaisesRegex(ValueError, "terminal namespace"):
            self.check(self.values)

    def test_replayed_operation(self):
        self.values[2]["attempts"] = 2
        with self.assertRaisesRegex(ValueError, "original operation"):
            self.check(self.values)

    def test_wrong_transaction_count(self):
        self.values[2]["writes"] = 4
        with self.assertRaisesRegex(ValueError, "write arithmetic"):
            self.check(self.values)

    def test_workload_cannot_shrink(self):
        self.values[0]["files"] = 10
        with self.assertRaisesRegex(ValueError, "selected workload"):
            self.check(self.values)

    def test_frozen_cases_and_phase_inventory(self):
        self.assertEqual(len(self.registry["cases"]), 7)
        repeated = self.registry["cases"]["growth-repeat64"]
        self.assertEqual(repeated["amount"], 64)
        self.assertEqual(len(repeated["phases"]), 135)
        self.assertEqual(repeated["arguments"], ["repeat", "100000", "64"])


if __name__ == "__main__":
    unittest.main()
