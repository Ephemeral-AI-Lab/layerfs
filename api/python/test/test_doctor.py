"""The doctors over the real native module (call surface 2.0):
``home.doctor()`` and its five siblings are table calls that report and
change nothing; and ``home.leftovers()`` and ``home.prune(leftover)``,
which list what earlier runs left and remove one (layerfs-project#180).

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
            shutil.rmtree(home)

    def test_home_prune_removes_a_leftover_home_leftovers_listed(self):
        home = tempfile.mkdtemp(prefix="layerfs-py-doctor-")
        os.chmod(home, 0o700)
        try:
            lfs = layerfs.client(home=home)
            lfs.store.list()
            # an unpack's temporary folder, its image's lock, and a partly
            # received image (no leftover since spec v19: `machine prune` owns
            # it); each folder 0700: makedirs leaves the folders above to the
            # umask
            images = home
            for part in ("machine", "native", "run", "images"):
                images = os.path.join(images, part)
                os.makedirs(images, mode=0o700, exist_ok=True)
                os.chmod(images, 0o700)
            digest = "ab" * 32
            unpack = os.path.join(images, f".tmp-{digest}.2-ab12")
            folder = os.path.join(unpack, "usr", "bin")
            os.makedirs(folder, mode=0o700)
            for made in (unpack, os.path.dirname(folder), folder):
                os.chmod(made, 0o700)
            with open(os.path.join(folder, "tool"), "w") as file:
                file.write("x" * 50)
            received = os.path.join(images, f".recv-{digest}.part")
            with open(received, "w") as file:
                file.write("x" * 50)
            then = time.time() - 7200
            os.utime(received, (then, then))
            lock = os.path.join(images, f"{digest}.2.lock")
            open(lock, "w").close()

            # the list names the temporary by its id; the lock and the part
            # are no leftover
            leftover = f"native:image_tmp:{digest}.2-ab12"
            listed = lfs.home.leftovers(machine="native")
            self.assertEqual([item["leftover"] for item in listed["items"]], [leftover])

            # the same call on the module's function, on a client and as a coroutine
            self.assertTrue(callable(layerfs.home.prune))
            self.assertFalse(hasattr(layerfs, "doctor"))
            for wrong in (
                f"native:image_lock:{digest}.2",
                "native:image_tmp:../../x",
                f"native:image_part:{digest}",
            ):
                with self.assertRaises(layerfs.LayerFSError) as refused:
                    lfs.home.prune(wrong)
                self.assertEqual(refused.exception.code, "NotFound")
            with self.assertRaises(layerfs.LayerFSError) as refused:
                asyncio.run(aio.client(home=home).home.prune(f"native:image-tmp:{digest}.2-ab12"))
            self.assertEqual(refused.exception.code, "NotFound")
            removed = lfs.home.prune(leftover, detail=True)
            self.assertEqual(
                removed,
                {
                    "ok": True,
                    # a temporary is removed without a count
                    "freed_bytes": None,
                    "outcome": "removed",
                    "leftover": leftover,
                    "kind": "image_tmp",
                    "machine": "native",
                    "path": unpack,
                },
            )
            self.assertFalse(os.path.exists(unpack))
            self.assertTrue(os.path.exists(received), "the part stays for `machine prune`")
            self.assertTrue(os.path.exists(lock), "the lock file stays")
            with self.assertRaises(layerfs.LayerFSError) as refused:
                lfs.home.prune(leftover)
            self.assertEqual(refused.exception.code, "NotFound")
        finally:
            shutil.rmtree(home)


if __name__ == "__main__":
    unittest.main()
