import sys
from pathlib import Path
import unittest
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from families import workspace_commit_native as native


class NativeSelection(unittest.TestCase):
    def test_frozen_order_and_routes(self):
        self.assertEqual(len(native.SELECTED), 8)
        first = native.CASES[native.SELECTED[0]]
        self.assertEqual(first.master, 'large')
        self.assertEqual(first.test, 'phase_b_commit::full_lowering_64mib')
        self.assertTrue(all(case.budget_ns == 60_000_000_000 for case in native.CASES.values()))
        self.assertEqual(sum(case.clones for case in native.CASES.values()), 9)

    def test_retained_custody_is_explicit(self):
        retained = [case for case in native.CASES.values() if case.retained]
        self.assertEqual(len(retained), 2)
        self.assertEqual({case.clones for case in retained}, {1, 2})


if __name__ == '__main__':
    unittest.main()
