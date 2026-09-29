"""The nine registered schedules and the frozen independent master."""

import importlib.util
from pathlib import Path
import unittest


HERE = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("workspace_write", HERE / "families/workspace_write.py")
write = importlib.util.module_from_spec(SPEC)
import sys
sys.modules[SPEC.name] = write
SPEC.loader.exec_module(write)


class WorkspaceWriteRegistry(unittest.TestCase):
    def test_nine_schedules(self):
        self.assertEqual(len(write.CASES), 9)
        self.assertEqual(tuple(write.CASES), write.SELECTED)
        manifests = write.manifests()
        self.assertEqual(len(manifests), 10)
        for case in write.CASES.values():
            data = write.expected(case.pattern, case.writes)
            self.assertEqual(len(data), write.SIZE + (case.writes if case.pattern == "append" else 0))
            self.assertIn(str(len(data)), manifests[case.id])
            self.assertEqual(case.command_budget_ns, (25 if case.writes == 4097 else 15) * 1_000_000_000)
        self.assertEqual(write.expected("repeated", 4097)[5 << 20], ord("B") + 4096 % 24)


if __name__ == "__main__":
    unittest.main()
