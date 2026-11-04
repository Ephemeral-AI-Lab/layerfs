"""``layerfs.doctor()`` over the real native module (#536 H7): one top-level
function, outside the call table, that reports and changes nothing.

Build first, as for test_calls.py."""

import os
import shutil
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent / "src"))

import layerfs  # noqa: E402


class Close(unittest.TestCase):
    def test_close_is_outside_the_table_and_closes_nothing_when_nothing_is_open(self):
        # outside the table: its families keep only their own calls (sandbox.close is one)
        self.assertNotIn("close", [spec["family"] for spec in layerfs.TABLE])
        layerfs.close()
        self.assertEqual(layerfs.close(), 0)


class Doctor(unittest.TestCase):
    def test_a_home_not_made_yet_is_ok_and_stays_absent(self):
        parent = tempfile.mkdtemp(prefix="layerfs-py-doctor-")
        home = os.path.join(parent, "home")
        try:
            self.assertEqual(layerfs.doctor(home), {"ok": True, "findings": []})
            self.assertFalse(os.path.exists(home))
            self.assertNotIn("doctor", [spec["call"] for spec in layerfs.TABLE])
        finally:
            shutil.rmtree(parent)

    def test_a_record_that_does_not_parse_is_an_error_with_its_fix(self):
        home = tempfile.mkdtemp(prefix="layerfs-py-doctor-")
        os.chmod(home, 0o700)
        try:
            layerfs.client(home=home).store.list()
            # each folder 0700: makedirs leaves the folders above to the umask
            folder = os.path.join(home, "store", "bad")
            os.makedirs(folder, exist_ok=True)
            for made in (os.path.dirname(folder), folder):
                os.chmod(made, 0o700)
            record = os.path.join(folder, "record.json")
            with open(record, "w") as file:
                file.write("{not json")
            os.chmod(record, 0o600)
            report = layerfs.doctor(home=home)
            self.assertFalse(report["ok"])
            [finding] = report["findings"]
            self.assertEqual(finding["severity"], "error")
            self.assertEqual(finding["entity"], {"kind": "store", "name": "bad"})
            self.assertIn("by hand", finding["fix"])
            with self.assertRaises(layerfs.LayerFSError) as refused:
                layerfs.doctor("relative")
            self.assertEqual(refused.exception.code, "InvalidInput")
        finally:
            shutil.rmtree(home)


if __name__ == "__main__":
    unittest.main()
