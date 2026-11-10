"""Prepared images publish only verified artifacts and never inherit host identity."""

import json
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import machine_image as images
from cleanup_state import Refused


class MachineImageTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.manifest = {
            "hash": "a" * 64,
            "platform": "linux-aarch64",
            "protocol": 3,
            "image": "sha256:" + "b" * 64,
            "runtime": "c" * 64,
        }
        self.work = self.root / "work"
        self.work.mkdir()
        self.receipt = {"work": str(self.work), "provider": "lima", "identity": "i" * 64, "steps": []}

    def test_seed_template_uses_a_fixed_disposable_user_and_no_host_mounts(self):
        value = images.seed_template(Path("/stock/ubuntu.img"))
        self.assertEqual(value["mounts"], [])
        self.assertEqual(value["user"]["name"], "layerfs-image-build")
        self.assertEqual(value["user"]["uid"], 1000)
        self.assertFalse(value["ssh"]["loadDotSSHPubKeys"])
        self.assertFalse(value["ssh"]["forwardAgent"])

    def test_identity_changes_for_runtime_base_and_provider_but_not_host_path(self):
        identity = images.image_identity("lima", self.manifest, "stock-digest")
        self.assertEqual(identity, images.image_identity("lima", dict(self.manifest), "stock-digest"))
        for provider, manifest, base in [
            ("apple-container", self.manifest, "stock-digest"),
            ("lima", {**self.manifest, "hash": "d" * 64}, "stock-digest"),
            ("lima", self.manifest, "another-stock-digest"),
        ]:
            self.assertNotEqual(identity, images.image_identity(provider, manifest, base))

    def test_failed_command_retains_receipt_and_never_publishes(self):
        failed = subprocess.CompletedProcess(["provider"], 1, b"", b"failed")
        with patch.object(images.subprocess, "run", return_value=failed), self.assertRaises(Refused):
            images.run(["provider"], {}, self.receipt, "seed-create")
        value = json.loads((self.work / "receipt.json").read_text())
        self.assertEqual(value["steps"][0]["exit"], 1)
        self.assertFalse(list(self.root.rglob("current.json")))

    def test_timeout_preserves_unknown_outcome_and_partial_log(self):
        error = subprocess.TimeoutExpired(["provider"], 120, output=b"partial evidence")
        with patch.object(images.subprocess, "run", side_effect=error) as run, self.assertRaises(Refused):
            images.run(["provider"], {}, self.receipt, "seed-create")
        self.assertEqual(run.call_count, 1)
        self.assertEqual(self.receipt["status"], "uncertain")
        self.assertEqual(self.receipt["steps"][0]["outcome"], "Unknown")
        self.assertEqual((self.work / "seed-create.log").read_bytes(), b"partial evidence")

    def test_guest_worker_verification_requires_authenticated_protocol_and_bundle(self):
        script = images.worker_check(self.manifest)
        self.assertIn("secrets.token_hex", script)
        self.assertIn("socket.create_connection", script)
        self.assertIn("assert hello['protocol'] == 3", script)
        self.assertIn("assert hello['bundle'] == '" + "a" * 64, script)
        self.assertIn("p.wait(timeout=5)", script)


if __name__ == "__main__":
    unittest.main()
