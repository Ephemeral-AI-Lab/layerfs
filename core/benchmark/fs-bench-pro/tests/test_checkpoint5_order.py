"""External checkpoint-5 observer checks (no benchmark product hooks)."""
import importlib.util
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

HERE = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(HERE))
spec = importlib.util.spec_from_file_location("checkpoint5_273", HERE / "checkpoint5_273.py")
checkpoint = importlib.util.module_from_spec(spec)
spec.loader.exec_module(checkpoint)
from separated_writes import backing_samples, progress


class Ordering(unittest.TestCase):
    def test_all_inputs_hashed_before_eviction_and_all_evicted_before_final_check(self):
        with tempfile.TemporaryDirectory() as temp:
            paths = [Path(temp) / name for name in ("store.sqlite", "history.sqlite")]
            for path in paths:
                path.write_bytes(b"a" * 4096)
            events = []
            real_digest = checkpoint.digest

            def digest(path):
                events.append(("hash", Path(path).name))
                return real_digest(path)

            class Residency:
                def check(self, fd, size, evict=False):
                    events.append(("evict" if evict else "final", fd))
                    return 1, 0

            def launch(*args):
                events.append(("launch", None))
                return {"started_ns": checkpoint.time.monotonic_ns(),
                        "complete_command_wall_ns": 1}

            with patch.object(checkpoint, "Residency", Residency), patch.object(
                    checkpoint, "digest", side_effect=digest), patch.object(
                    checkpoint, "run_measured", side_effect=launch):
                result, measured = checkpoint.run_cold_measured(paths, [], 15, {}, Path(temp))
            self.assertEqual([name for name, _ in events],
                             ["hash", "hash", "evict", "evict", "final", "final", "launch"])
            self.assertEqual(result["status"], "PASS")
            self.assertEqual(set(result["files"]), {path.name for path in paths})
            self.assertIn("final_check_ns", result)
            self.assertLessEqual(result["launch_gap_ns"], 1_000_000_000)

    def test_c1_explicit_zero_is_not_missing_or_interleaved(self):
        valid = {"c1_edit": [{"v": 1, "nodes_read": 0,
                             "draft_nodes_read": 0, "stored_nodes_read": 0}],
                 "file_input": [{"v": 1, "edits": 0, "record_lookups": 0,
                                 "record_reads": 0, "replacement_bytes_read": 0,
                                 "replacement_reads": 0, "spool_resident": "false"}]}
        self.assertEqual(checkpoint.c1_observation(valid)["c1_work"], 0)
        self.assertTrue(checkpoint.c1_observation(valid)["complete"])
        for invalid in ({}, {"c1_edit": valid["c1_edit"]},
                        {**valid, "c1_edit": valid["c1_edit"] * 2},
                        {**valid, "file_input": [{"v": 1, "edits": 0}]}):
            self.assertFalse(checkpoint.c1_observation(invalid)["complete"])
            self.assertIsNone(checkpoint.c1_observation(invalid)["c1_work"])

    def test_source_row_is_complete_or_explicitly_incomplete(self):
        raw = (b"LFS_ACTIVE_SOURCE v=2 complete=true windows=1 references=2 "
               b"distinct_packs=2 fill_ns=4 pack_loads=2 pack_hits=0 "
               b"locator_lookups=2 index_reads=4 index_seeks=2 locator_ns=3 "
               b"pack_ns=2 decoded_records=80 decoded_bytes=4000 copied_bytes=2 "
               b"ref_limit=1024 byte_limit=32768\n")
        result = checkpoint.source_observation(raw)
        self.assertEqual(result["status"], "PASS")
        self.assertEqual(result["counts"]["pack_loads"], 2)
        for damaged in (b"", raw + raw,
                        raw.replace(b"v=2", b"v=1"),
                        raw.replace(b"ref_limit=1024", b"ref_limit=256"), raw.replace(b"pack_hits=0", b"pack_hits=5"),
                        raw.replace(b"decoded_records", b"LFT1 {}decoded_records")):
            self.assertEqual(checkpoint.source_observation(damaged)["status"], "INCOMPLETE")
            self.assertIsNone(checkpoint.source_observation(damaged)["counts"])

    def test_atomic_write_sample_refuses_interleaving_without_repair(self):
        line = (b"LFS_WRITE_SAMPLE v=4 write_class=Some(100) elapsed_ns=123 "
                b"backing=Ok(BackingStatus { a: 2 }) "
                b"metadata=Ok(MetadataStatus { b: 3 }) "
                b"acquisition_ns=0 publication_ns=9")
        self.assertEqual(backing_samples(line + b"\n")[0]["write_class"], 100)
        self.assertEqual(backing_samples(line.replace(b"metadata=", b"LFT1 {}metadata=")
                                         + b"\n"), [])
        self.assertEqual(backing_samples(b"LFS_WRITE_SAMPLE v=4 status=INCOMPLETE_OVERFLOW\n"), [])
        self.assertEqual(backing_samples(line.replace(b" publication_ns=9", b"") + b"\n"), [])

    def test_unqualified_pair_has_no_numeric_ratio(self):
        row = {"row_status": "INELIGIBLE", "cache_status": "PASS",
               "numeric_admission": False}
        self.assertFalse(checkpoint.numeric_pair_eligible(row, row, 100, 10))
        self.assertFalse(checkpoint.numeric_pair_eligible(None, row, 100, 10))
        admitted = {**row, "row_status": "PASS", "numeric_admission": True}
        self.assertTrue(checkpoint.numeric_pair_eligible(admitted, admitted, 100, 10))
        self.assertFalse(checkpoint.numeric_pair_eligible(admitted, admitted, 0, 10))

    def test_single_write_has_only_one_reachable_checkpoint(self):
        self.assertEqual(progress({"exec_stdout_hex": b"PROGRESS\t1\t12\n".hex()}, 1),
                         [{"writes": 1, "writer_elapsed_ns": 12}])
        with self.assertRaises(ValueError):
            progress({"exec_stdout_hex": b"PROGRESS\t1\t12\n".hex()}, 100)


if __name__ == "__main__":
    unittest.main()
