"""Fail-closed strict thresholds and preserved historical case identities."""
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from shared import history_storage as storage
from families import history_retention as history


class HistoryStorage(unittest.TestCase):
    def test_registry_and_independent_ledgers(self):
        self.assertEqual([c.states for c in history.CASES.values()], [17, 53, 157] * 2)
        self.assertEqual([c.ceiling_bytes for c in history.CASES.values()],
                         [49_344_512, 64_024_576, 83_947_520] * 2)
        self.assertEqual([c.verification_budget_ns for c in history.CASES.values()],
                         [10_000_000_000, 20_000_000_000, 30_000_000_000] * 2)
        self.assertEqual(history.SELECTED, tuple(history.CASES)[3:5])
        import hashlib
        import json
        for case in history.CASES.values():
            pins = json.loads((case.pin_root / "manifest.json").read_text())
            file = case.pin_root / f"{case.backend_id}.tsv"
            self.assertEqual(hashlib.sha256(file.read_bytes()).hexdigest(),
                             pins["cases"][case.backend_id]["roots_sha256"])
            self.assertEqual(len(file.read_text().splitlines()) - 1, case.states)
            expected = pins["cases"][case.backend_id]
            required = ([expected["canonical_bytes"], expected["canonical_objects"]]
                        if case.version == "v1" else expected["canonical_required"])
            self.assertEqual(len(required), 2)

    def test_strict_gate_and_missing_owners(self):
        for ceiling in (49_344_512, 64_024_576, 83_947_520):
            self.assertEqual(storage.evaluate(ceiling - 1, ceiling, storage.CREATION), "PASS")
            self.assertEqual(storage.evaluate(ceiling, ceiling, storage.CREATION), "FAIL")
            self.assertEqual(storage.evaluate(ceiling + 1, ceiling, storage.CREATION), "FAIL")
            for missing in (None, -1, False, 0.0):
                self.assertEqual(storage.evaluate(missing, ceiling, storage.CREATION), "INCOMPLETE")
            for attribution in ("shared", "reflink", "unknown"):
                self.assertEqual(storage.evaluate(1, ceiling, attribution), "INELIGIBLE")
        with tempfile.TemporaryDirectory() as directory:
            result = storage.collect(directory, 49_344_512, 17)
            self.assertEqual(result["status"], "INCOMPLETE")
            self.assertIsNone(result["total_retained_allocated_bytes"])


if __name__ == "__main__":
    unittest.main()
