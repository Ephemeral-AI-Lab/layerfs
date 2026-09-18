#!/usr/bin/env python3
"""The before/after counter comparison, through `unittest`."""

import unittest

import compare_runs


class CompareRunsSelfCheck(unittest.TestCase):
    def test_self_check(self):
        self.assertEqual(compare_runs.self_check(), [])

    def test_a_fallen_operation_is_a_rejection(self):
        base = {
            "case_id": "x",
            "status": "PASS",
            "counters": {},
            "resources": {},
            "phases": {"operation_ns": 1000},
        }
        after = dict(base)
        after["phases"] = {"operation_ns": 100}
        kinds = {f["kind"] for f in compare_runs.compare({"x": base}, {"x": after})["findings"]}
        self.assertIn("operation-fell", kinds)


if __name__ == "__main__":
    unittest.main()
