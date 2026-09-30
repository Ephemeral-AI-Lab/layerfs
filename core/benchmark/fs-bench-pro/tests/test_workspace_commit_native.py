import sys
from pathlib import Path
import unittest
import json
import tempfile
from types import SimpleNamespace
from unittest.mock import patch
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from families import workspace_commit_native as native


class NativeSelection(unittest.TestCase):
    def test_frozen_order_and_routes(self):
        self.assertEqual(len(native.SELECTED), 8)
        first = native.CASES[native.SELECTED[0]]
        self.assertEqual(first.master, 'large')
        self.assertEqual(first.test, 'phase_b_commit::full_lowering_64mib')
        self.assertTrue(all(case.budget_ns == 60_000_000_000 for case in native.CASES.values()))
        self.assertEqual(sum(native.CASES[name].clones for name in native.SELECTED), 9)

    def test_owner_deferred_case_acquires_no_resources(self):
        with tempfile.TemporaryDirectory() as temporary:
            out = Path(temporary) / "deferred"
            common = SimpleNamespace(owned=lambda path: out,
                                     identities=lambda: {"source_dirty": False},
                                     manifest_run=lambda path: None)
            with patch.object(native, "build", side_effect=AssertionError("no deferred build")):
                native.run("workspace-commit-native-count-writes-10240-v2", str(out), common)
            row = json.loads((out / "run.json").read_text())["rows"][0]
            self.assertEqual(row["status"], "SKIPPED / OWNER-DEFERRED")
            self.assertEqual(row["sample_count"], 0)

    def test_retained_custody_is_explicit(self):
        retained = [case for case in native.CASES.values() if case.retained]
        self.assertEqual(len(retained), 2)
        self.assertEqual({case.clones for case in retained}, {1, 2})


if __name__ == '__main__':
    unittest.main()
