"""``layerfs.doctor()`` over the real native module (#536 H7): one top-level
function, outside the call table, that reports and changes nothing; and
``doctor.fix(id)``, the ``doctor`` family's one call, which removes a
leftover the report listed (layerfs-project#180).

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
    def test_host_target_has_explicit_coverage_without_creating_a_home(self):
        with tempfile.TemporaryDirectory(prefix="layerfs-py-doctor-target-") as parent:
            home = os.path.join(parent, "absent")
            report = layerfs.doctor(home, target="host", timeout="1s")
            self.assertEqual(report["target"], {"kind": "host", "name": "local"})
            self.assertIsInstance(report["complete"], bool)
            self.assertTrue(report["checks"])
            self.assertFalse(os.path.exists(home))

    def test_invalid_target_options_refuse_before_home_effects(self):
        with tempfile.TemporaryDirectory(prefix="layerfs-py-doctor-invalid-") as parent:
            home = os.path.join(parent, "absent")
            for options in (
                {"target": "sandbox"},
                {"target": "host", "quick": False},
                {"target": "machine", "name": "dev", "write_test": False},
                {"target": "host", "timeout": "0s"},
            ):
                with self.subTest(options=options):
                    with self.assertRaises(layerfs.LayerFSError) as refused:
                        layerfs.doctor(home, **options)
                    self.assertEqual(refused.exception.code, "InvalidInput")
                    self.assertFalse(os.path.exists(home))

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

    def test_a_leftover_is_listed_and_doctor_fix_removes_it(self):
        home = tempfile.mkdtemp(prefix="layerfs-py-doctor-")
        os.chmod(home, 0o700)
        try:
            lfs = layerfs.client(home=home)
            lfs.store.list()
            clean = {"machine": "native", "complete": True, "image_locks": 0, "unmount_markers": 0}
            self.assertEqual(layerfs.doctor(home)["leftovers"], [clean])
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

            report = layerfs.doctor(home)
            self.assertTrue(report["ok"], "a leftover is a warning")
            [finding] = report["findings"]
            self.assertEqual(finding["entity"], {"kind": "machine", "name": "native"})
            left = finding["leftover"]
            fix_id = f"native:image-part:{digest}"
            self.assertEqual((left["id"], left["kind"], left["path"], left["bytes"]), (fix_id, "image-part", received, 50))
            self.assertGreaterEqual(left["age"], 7190)
            self.assertTrue(left["fixable"])
            self.assertIn(f"layerfs doctor fix {fix_id}", finding["fix"])
            self.assertEqual(report["leftovers"], [dict(clean, image_locks=1)])

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
            self.assertEqual(layerfs.doctor(home)["findings"], [])
            with self.assertRaises(layerfs.LayerFSError) as refused:
                lfs.doctor.fix(fix_id)
            self.assertEqual(refused.exception.code, "NotFound")
        finally:
            shutil.rmtree(home)


if __name__ == "__main__":
    unittest.main()
