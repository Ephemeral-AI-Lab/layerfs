"""A sampled maximum never becomes a continuous phase peak."""
import json
from pathlib import Path
import sys
import tempfile
import unittest
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from shared.pre_s8_phase_report import phase_report


class Phases(unittest.TestCase):
    def report(self, boundaries):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / "resources.jsonl"
            with path.open("w") as stream:
                for index, (boundary, rss) in enumerate(boundaries):
                    row = {"schema": "pre-s8-phase-observation-v1", "index": index, "phase": "commit", "boundary": boundary,
                           "process": {"scope": "macos-process", "memory_bytes": {"rss": rss}, "read_open_ns": index * 100, "read_closed_ns": index * 100 + 5},
                           "cgroup": None, "artifacts": []}
                    stream.write(json.dumps(row) + "\n")
            return phase_report(path)["phases"]["commit"]

    def test_sampled_maximum_scope(self):
        value = self.report([("baseline", 10), ("interior", 30), ("final", 20)])
        self.assertEqual(value["rss_observed_max_bytes"], 30)
        self.assertEqual(value["rss_baseline_bytes"], 10)
        self.assertEqual(value["rss_final_bytes"], 20)
        self.assertIsNone(value["phase_peak_bytes"])
        self.assertEqual(value["maximum_sample_gap_ns"], 100)
        self.assertEqual(value["numeric_acceptance"], "OWNER_DEFERRED")

    def test_missing_interior_stays_incomplete(self):
        value = self.report([("baseline", 10), ("final", 20)])
        self.assertEqual(value["coverage"], "INCOMPLETE_INTERIOR_OR_BOUNDARY")
        self.assertIsNone(value["phase_peak_bytes"])

    def test_duplicate_boundary_refused(self):
        with self.assertRaisesRegex(ValueError, "duplicate phase baseline"):
            self.report([("baseline", 10), ("baseline", 20)])


if __name__ == "__main__":
    unittest.main()
