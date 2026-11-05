"""Public Python refusal matrix: existing records survive, no worker is dialled."""

import json
import socket
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent / "src"))
import layerfs

FIXTURE = (HERE.parent.parent / "test/disabled-platforms.json").read_text()


class DisabledPlatforms(unittest.TestCase):
    def test_new_work_is_refused_before_any_effect(self):
        for platform in ("firecracker", "ssh"):
            with (
                self.subTest(platform=platform),
                tempfile.TemporaryDirectory(prefix="lfs-disabled-py-") as folder,
                socket.socket() as listener,
            ):
                listener.bind(("127.0.0.1", 0))
                listener.listen()
                listener.setblocking(False)
                home = Path(folder)
                data = json.loads(
                    FIXTURE.replace("$platform", platform).replace("$address", f"127.0.0.1:{listener.getsockname()[1]}")
                )
                for file, value in data["files"].items():
                    path = home / file
                    path.parent.mkdir(parents=True, exist_ok=True)
                    path.write_text(json.dumps(value))

                def snapshot():
                    return {str(p.relative_to(home)): p.read_bytes() for p in home.rglob("*") if p.is_file()}

                before = snapshot()
                lfs = layerfs.client(home=home)
                for row in data["cases"]:
                    family, call = row["call"].split(".")
                    disabled = "ssh" if row["call"] == "machine.connect" else platform
                    with self.assertRaises(layerfs.LayerFSError, msg=row["call"]) as caught:
                        getattr(getattr(lfs, family), call)(**row["args"])
                    self.assertEqual(caught.exception.code, "Unsupported", row["call"])
                    self.assertEqual(
                        caught.exception.message,
                        f"{disabled} is permanently disabled by product policy (layerfs-project#7)",
                    )
                    self.assertEqual(snapshot(), before, row["call"])
                    with self.assertRaises(BlockingIOError):
                        listener.accept()
