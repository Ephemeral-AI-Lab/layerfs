"""The doctors over the real native module (call surface 2.0):
``home.doctor()`` and its five siblings are table calls that report and
change nothing; ``home.leftovers()`` and ``home.prune()`` are gone (spec
v20: LayerFS removes leftovers itself).

Build first, as for test_calls.py."""

import os
import shutil
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parents[1]  # api/python/test
sys.path.insert(0, str(HERE.parent / "src"))

import layerfs  # noqa: E402
from layerfs import aio  # noqa: E402


class Close(unittest.TestCase):
    def test_close_is_outside_the_table_and_closes_nothing_when_nothing_is_open(self):
        # outside the table: its families keep only their own calls (sandbox.remove is one)
        self.assertNotIn("close", [spec["family"] for spec in layerfs.TABLE])
        layerfs.close()
        self.assertEqual(layerfs.close(), 0)


class Doctor(unittest.TestCase):
    def test_the_six_doctors_are_table_calls(self):
        names = sorted(f"{spec['family']}.{spec['call']}" for spec in layerfs.TABLE if spec["call"] == "doctor")
        self.assertEqual(
            names,
            ["backup.doctor", "home.doctor", "machine.doctor", "project.doctor", "sandbox.doctor", "store.doctor"],
        )

    def test_a_home_not_made_yet_is_reported_and_stays_absent(self):
        parent = tempfile.mkdtemp(prefix="layerfs-py-doctor-")
        home = os.path.join(parent, "home")
        try:
            report = layerfs.client(root=parent).home.doctor(detail=True)
            self.assertEqual(list(report), ["ok", "findings", "not_checked", "status", "counts", "observed_at", "tree"])
            self.assertEqual(report["tree"]["coverage"], "shallow")
            self.assertFalse(os.path.exists(home))
        finally:
            shutil.rmtree(parent)

    def test_a_record_that_does_not_parse_is_an_error_with_its_fix(self):
        root = tempfile.mkdtemp(prefix="layerfs-py-doctor-")
        os.chmod(root, 0o700)
        home = os.path.join(root, "home")
        try:
            lfs = layerfs.client(root=root)
            lfs.store.list()
            with self.assertRaises(layerfs.LayerFSError) as refused:
                lfs.machine.doctor("missing")
            self.assertEqual(refused.exception.code, "NotFound")
            # each folder 0700: makedirs leaves the folders above to the umask
            folder = os.path.join(home, "store", "bad")
            os.makedirs(folder, exist_ok=True)
            for made in (os.path.dirname(folder), folder):
                os.chmod(made, 0o700)
            record = os.path.join(folder, "record.json")
            with open(record, "w") as file:
                file.write("{not json")
            os.chmod(record, 0o600)
            report = lfs.home.doctor()
            self.assertNotIn("ok", report)
            bad = [found for found in report["findings"] if found.get("store") == "bad"]
            # the home's records check names it, and so does the Store's own
            # record check on its broken node (conformance R4 F5)
            [finding] = [found for found in bad if found["check"] == "home.records"]
            self.assertEqual(finding["severity"], "error")
            [own] = [found for found in bad if found["check"] == "store.record"]
            self.assertEqual(own["severity"], "error")
            self.assertIn("by hand", finding["message"])
            self.assertNotIn("fix", finding)
        finally:
            shutil.rmtree(root)

    def test_home_leftovers_and_prune_are_gone(self):
        # spec v20: LayerFS removes leftovers itself; the calls are gone
        home = [spec["call"] for spec in layerfs.TABLE if spec["family"] == "home"]
        self.assertEqual(home, ["init", "get", "log", "clear_log", "doctor", "move"])
        self.assertFalse(hasattr(layerfs.home, "leftovers"))
        self.assertFalse(hasattr(layerfs.home, "prune"))
        client = layerfs.client(root=os.path.join(tempfile.gettempdir(), "layerfs-py-gone"))
        self.assertFalse(hasattr(client.home, "leftovers"))
        self.assertFalse(hasattr(client.home, "prune"))
        self.assertFalse(hasattr(aio.client(root=tempfile.gettempdir()).home, "prune"))

if __name__ == "__main__":
    unittest.main()
