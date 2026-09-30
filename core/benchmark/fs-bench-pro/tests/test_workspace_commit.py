"""Independent retained-control oracle and fast-lane registry."""
import hashlib
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from families import workspace_commit as commit


class WorkspaceCommit(unittest.TestCase):
    def test_refusal_before_ownership_needs_no_invented_delete(self):
        self.assertTrue(commit.cleanup_complete({"stage": "sandbox_create", "resources_owned": False}, None, b""))
        self.assertFalse(commit.cleanup_complete({"stage": "sandbox_create", "resources_owned": True}, None, b""))
        self.assertFalse(commit.cleanup_complete(None, None, b""))

    def test_sdk_reordered_copy_and_stopping_oracles(self):
        manifests, _ = commit.oracle()
        retained = bytearray(b"A" * (10 << 20))
        for i in range(4097):
            retained[(104729 + i * 2654435761) % len(retained)] = ord("B") + i % 24
        first, second = retained[:8192], retained[1 << 20:(1 << 20) + 8192]
        self.assertNotEqual(first, second)
        retained[:8192], retained[1 << 20:(1 << 20) + 8192] = second, first
        self.assertIn(hashlib.sha256(retained).hexdigest(), manifests["workspace-commit-sdk-reordered-base-copy-8kib-v1"])
        self.assertEqual(manifests["workspace-commit-sdk-stopping-refusal-v1"], manifests["baseline"])

    def test_live_retained_oracles_and_limits(self):
        manifests, pin = commit.oracle()
        self.assertEqual(len(commit.SELECTED), 2)
        self.assertTrue(all(name.endswith("-v3") for name in commit.SELECTED))
        self.assertEqual(manifests[commit.SELECTED[0]], manifests["retained"])
        expected = bytearray(b"A" * (10 << 20))
        for i in range(4097):
            expected[(104729 + i * 2654435761) % len(expected)] = ord("B") + i % 24
        self.assertEqual(pin, hashlib.sha256(expected).hexdigest())
        expected[0] = ord("X")
        self.assertIn(hashlib.sha256(expected).hexdigest(), manifests[commit.SELECTED[1]])
        for case in commit.CASES.values():
            self.assertEqual(case.command_budget_ns, (60 if case.role == "functional-oracle" and not case.stopping else 15) * 1_000_000_000)
            self.assertEqual(case.verifier_budget_ns, 9_000_000_000)
            if case.id in commit.SELECTED:
                self.assertEqual(case.pin_read_bytes, 0)
        full = commit.CASES["workspace-commit-full-pin-retained-writes-4097-read-31kib-v1"]
        self.assertEqual(full.pin_read_bytes, 31_744)
        self.assertIn(hashlib.sha256(expected).hexdigest(), manifests[full.id])
        self.assertIsNone(commit.control_line(b"CONTROL\t{}\nCONTROL\t{}\n"))


if __name__ == "__main__":
    unittest.main()
