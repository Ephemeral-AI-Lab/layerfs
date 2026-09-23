import hashlib
import tempfile
import unittest
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from families import init_namespace as init


class InitCases(unittest.TestCase):
    def test_frozen_case_plans(self):
        for case in init.CASES.values():
            planned = init.plan(case)
            self.assertEqual(len(planned), case.files)
            self.assertEqual(sum(size for _, _, size in planned), case.logical_bytes)
            self.assertEqual(len({path for path, _, _ in planned}), case.files)
        self.assertEqual(init.SELECTED, tuple(init.CASES)[:3])
        self.assertIn("first-pass", init.NOT_RUN_REASON)

    def test_seal_and_inventory_refuse_corruption(self):
        with tempfile.TemporaryDirectory() as directory:
            case = init.CASES[init.SELECTED[0]]
            first = init.prepare(case, Path(directory))
            reused = init.prepare(case, Path(directory))
            self.assertTrue(reused["reused"])
            self.assertEqual(first["manifest_sha256"], reused["manifest_sha256"])
            manifest = Path(first["manifest"])
            rows = manifest.read_text().splitlines()
            file_row = next(row for row in rows if "\tf\t" in row)
            path = Path(first["source"]) / file_row.split("\t")[0]
            self.assertEqual(hashlib.sha256(path.read_bytes()).hexdigest(), file_row.split("\t")[-1])
            path.write_bytes(b"corrupt")
            with self.assertRaises(ValueError):
                init.prepare(case, Path(directory))
            path.unlink()
            with self.assertRaises(ValueError):
                init.prepare(case, Path(directory))


if __name__ == "__main__":
    unittest.main()
