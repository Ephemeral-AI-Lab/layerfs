"""Fail-closed strict thresholds and preserved historical case identities."""
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from shared import history_storage as storage
from families import history_retention as history


class HistoryStorage(unittest.TestCase):
    def test_native_semantics_do_not_inherit_the_resource_failure(self):
        import gzip
        archived = (history.ROOT / "core/docs/issues/286/experiments"
                    / "20260930-history-stride10-v2-r007"
                    / "history-retention-stride-10-total-storage-v2/native/trace.jsonl.gz")
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "trace.jsonl"
            path.write_bytes(gzip.decompress(archived.read_bytes()))
            parsed = history.trace_module.read(path)
        self.assertEqual(parsed.status(), "FAIL")
        self.assertTrue(history.semantic_gates_pass(parsed.gates(),
            "g1.o6-total-retained-below-v016-v2"))

    def test_registry_and_independent_ledgers(self):
        self.assertEqual([c.states for c in history.CASES.values()], [17, 53, 157] * 3)
        self.assertEqual([c.ceiling_bytes for c in history.CASES.values()],
                         [49_344_512, 64_024_576, 83_947_520] * 3)
        self.assertEqual([c.verification_budget_ns for c in history.CASES.values()],
                         [10_000_000_000, 20_000_000_000, 30_000_000_000] * 3)
        self.assertEqual(history.SELECTED, tuple(history.CASES)[6:8])
        import hashlib
        import json
        for case in history.CASES.values():
            pins = json.loads((case.pin_root / "manifest.json").read_text())
            file = case.pin_root / f"{case.backend_id}.tsv"
            self.assertEqual(hashlib.sha256(file.read_bytes()).hexdigest(),
                             pins["cases"][case.backend_id]["roots_sha256"])
            self.assertEqual(len(file.read_text().splitlines()) - 1, case.states)
            expected = pins["cases"][case.backend_id]
            required = (history.V3_CANONICAL[case.backend_id] if case.version == "v3" else
                        [expected["canonical_bytes"], expected["canonical_objects"]]
                        if case.version == "v1" else expected["canonical_required"])
            self.assertEqual(len(required), 2)
        self.assertEqual(history.V3_CANONICAL["history-stride3"], (589_480_854, 73_447))
        self.assertEqual(history.V3_CANONICAL["history-stride1"], (871_337_620, 104_618))
        self.assertEqual(json.loads((history.CASES["history-retention-stride-3-total-storage-v2"].pin_root
                         / "manifest.json").read_text())["cases"]["history-stride3"]["canonical_required"],
                         [589_423_458, 73_476])

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
