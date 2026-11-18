"""The doctors over the real native module (call surface 2.0):
``home.doctor()`` and its five siblings are table calls that report and
change nothing; and ``doctor.fix(id)``, the ``doctor`` family's one call,
which removes one leftover (layerfs-project#180).

Build first, as for test_calls.py."""

import asyncio
import os
import shutil
import sys
import tempfile
import time
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent / "src"))

import layerfs  # noqa: E402
from layerfs import aio  # noqa: E402


class Close(unittest.TestCase):
    def test_close_is_outside_the_table_and_closes_nothing_when_nothing_is_open(self):
        # outside the table: its families keep only their own calls (sandbox.close is one)
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
            report = layerfs.client(home=home).home.doctor(detail=True)
            self.assertEqual(list(report), ["ok", "findings", "not_checked", "status", "counts", "observed_at", "tree"])
            self.assertEqual(report["tree"]["coverage"], "shallow")
            self.assertFalse(os.path.exists(home))
        finally:
            shutil.rmtree(parent)

    def test_a_record_that_does_not_parse_is_an_error_with_its_fix(self):
        home = tempfile.mkdtemp(prefix="layerfs-py-doctor-")
        os.chmod(home, 0o700)
        try:
            lfs = layerfs.client(home=home)
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
            [finding] = [found for found in report["findings"] if found.get("store") == "bad"]
            self.assertEqual((finding["check"], finding["severity"]), ("host.records", "error"))
            self.assertIn("by hand", finding["fix"])
        finally:
            shutil.rmtree(home)

    def test_doctor_fix_removes_a_leftover_by_its_id(self):
        home = tempfile.mkdtemp(prefix="layerfs-py-doctor-")
        os.chmod(home, 0o700)
        try:
            lfs = layerfs.client(home=home)
            lfs.store.list()
            # a partly received image, last written two hours ago, and its lock
            # each folder 0700: makedirs leaves the folders above to the umask
            images = home
            for part in ("machine", "native", "run", "images"):
                images = os.path.join(images, part)
                os.makedirs(images, mode=0o700, exist_ok=True)
                os.chmod(images, 0o700)
            digest = "ab" * 32
            received = os.path.join(images, f".recv-{digest}.part")
            with open(received, "w") as file:
                file.write("x" * 50)
            then = time.time() - 7200
            os.utime(received, (then, then))
            lock = os.path.join(images, f"{digest}.2.lock")
            open(lock, "w").close()

            # no doctor's report lists a leftover: its id is the leftover walk's
            fix_id = f"native:image-part:{digest}"

            # the same call on the module's function, on a client and as a coroutine
            self.assertTrue(callable(layerfs.doctor.fix))
            for wrong, code in ((f"native:image-lock:{digest}.2", "InvalidInput"), ("native:image-part:../x", "NotFound")):
                with self.assertRaises(layerfs.LayerFSError) as refused:
                    lfs.doctor.fix(wrong)
                self.assertEqual(refused.exception.code, code)
            with self.assertRaises(layerfs.LayerFSError) as refused:
                asyncio.run(aio.client(home=home).doctor.fix(f"native:image-lock:{digest}.2"))
            self.assertEqual(refused.exception.code, "InvalidInput")
            removed = lfs.doctor.fix(fix_id)
            self.assertEqual(
                removed,
                {"kind": "image-part", "path": received, "removed": True, "bytes": 50, "machine": "native", "id": fix_id},
            )
            self.assertFalse(os.path.exists(received))
            self.assertTrue(os.path.exists(lock), "the lock file stays")
            with self.assertRaises(layerfs.LayerFSError) as refused:
                lfs.doctor.fix(fix_id)
            self.assertEqual(refused.exception.code, "NotFound")
        finally:
            shutil.rmtree(home)


if __name__ == "__main__":
    unittest.main()
