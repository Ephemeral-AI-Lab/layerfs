"""Tamper/omission tests over retained actual terminal-engine receipts."""
import copy
import gzip
import json
from pathlib import Path
import sys
import tempfile
import unittest
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from shared import evidence_engine as engine

ROOT = Path(__file__).resolve().parents[4]
FIXTURE = ROOT / "core/docs/issues/307/checks/pre-s8-finite-engine-20261007/12-host-terminal-jobs.jsonl.gz"


class EngineReceipts(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        with gzip.open(FIXTURE, "rt") as stream:
            cls.original = [json.loads(line) for line in stream]

    def rejected(self, edit, message):
        rows = copy.deepcopy(self.original)
        edit(rows)
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / "changed.jsonl"
            with path.open("w") as stream:
                for row in rows:
                    stream.write(json.dumps(row) + "\n")
            with self.assertRaisesRegex(ValueError, message):
                engine.validate(path)

    def test_real_original_stream(self):
        result = engine.validate(FIXTURE)
        self.assertEqual(result["functional_count_status"], "PASS")
        self.assertEqual(result["jobs"], 1060)
        self.assertEqual(result["closed_namespaces"], 4)
        self.assertEqual(result["numeric_acceptance"], "OWNER_DEFERRED")

    def test_missing_job(self):
        self.rejected(lambda rows: rows.pop(9), "job index")

    def test_duplicate_job(self):
        self.rejected(lambda rows: rows.insert(9, copy.deepcopy(rows[9])), "job index")

    def test_missing_summary(self):
        self.rejected(lambda rows: rows.pop(), "terminal summary")

    def test_after_summary(self):
        self.rejected(lambda rows: rows.append(copy.deepcopy(rows[0])), "after terminal")

    def test_family_tamper(self):
        def edit(rows):
            rows[0]["sql"]["families"][0]["vm_steps"] += 1
            rows[0]["sql"]["total"]["vm_steps"] += 1
        self.rejected(edit, "family sums")

    def test_total_tamper(self):
        self.rejected(lambda rows: rows[0]["sql"]["total"].__setitem__("vm_steps", 0), "total differs")

    def test_wrong_workspace(self):
        def edit(rows):
            row = next(row for row in rows if row.get("phase") == "wave")
            row["workspace"] = 3
        self.rejected(edit, "per-Workspace/class")

    def test_attempt_replay(self):
        self.rejected(lambda rows: rows[0].__setitem__("attempt_count", 2), "one original")

    def test_original_failure(self):
        self.rejected(lambda rows: rows[0].__setitem__("outcome", "original_failure"), "did not succeed")

    def test_retained_credit(self):
        self.rejected(lambda rows: rows[-1]["owner"]["counters"].__setitem__("outstanding", 1), "retain credits")

    def test_only_logical_close(self):
        self.rejected(lambda rows: rows[-1].__setitem__("closed_namespaces", 0), "terminal drain")

    def test_numeric_promotion(self):
        self.rejected(lambda rows: rows[-1].__setitem__("numeric_acceptance", "PASS"), "substitution")

    def test_duplicate_json_key(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / "duplicates.jsonl"
            path.write_text('{"schema":"a","schema":"b"}\n')
            with self.assertRaisesRegex(ValueError, "duplicate JSON"):
                engine.validate(path)


if __name__ == "__main__":
    unittest.main()
