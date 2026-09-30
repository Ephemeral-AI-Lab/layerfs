"""Independent retained-control oracle and fast-lane registry."""
import hashlib
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from families import workspace_commit as commit


class WorkspaceCommit(unittest.TestCase):
    def test_live_retained_oracles_and_limits(self):
        manifests, pin = commit.oracle()
        self.assertEqual(tuple(commit.CASES), commit.SELECTED)
        self.assertEqual(manifests[commit.SELECTED[0]], manifests["retained"])
        expected = bytearray(b"A" * (10 << 20))
        for i in range(4097):
            expected[(104729 + i * 2654435761) % len(expected)] = ord("B") + i % 24
        self.assertEqual(pin, hashlib.sha256(expected).hexdigest())
        expected[0] = ord("X")
        self.assertIn(hashlib.sha256(expected).hexdigest(), manifests[commit.SELECTED[1]])
        for case in commit.CASES.values():
            self.assertEqual(case.command_budget_ns, 15_000_000_000)
            self.assertEqual(case.verifier_budget_ns, 9_000_000_000)
        self.assertIsNone(commit.control_line(b"CONTROL\t{}\nCONTROL\t{}\n"))


if __name__ == "__main__":
    unittest.main()
