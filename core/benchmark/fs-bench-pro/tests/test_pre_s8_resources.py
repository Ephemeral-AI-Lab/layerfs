"""Observer failure keeps its original cause and joins only its owned child."""
import hashlib
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from diagnostics import pre_s8_resources as observer


class ObserverCustody(unittest.TestCase):
    def test_original_observer_failure_is_terminal_and_child_is_joined(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            binary = root / "engine_finite"
            binary.write_text(f"#!{sys.executable}\nimport os,sys\nprint('ENGINE_PHASE startup pid='+str(os.getpid()),flush=True)\nsys.stdin.readline()\n")
            binary.chmod(0o700)
            registration = root / "registration.json"
            registration.write_text(json.dumps({"case": "engine-finite", "binary": {"path": str(binary), "sha256": observer.digest(binary)}}))
            with patch.object(observer, "admit", return_value={"status": "PASS"}), patch.dict(os.environ, {"LAYERFS_CONSTRUCTION_WORKERS": "1"}), patch.object(observer.resources, "macos_process", side_effect=OSError("original observer refusal")), patch.object(observer.resources, "linux_process", side_effect=OSError("original observer refusal")):
                result = observer.collect(binary, root / "out", root / "database", "engine-finite", registration)
            self.assertEqual(result["status"], "FAILED")
            self.assertEqual(result["original_failure"], {"type": "OSError", "message": "original observer refusal"})
            self.assertTrue(result["child_joined"])
            self.assertEqual(result["resource_records"], 0)
            self.assertTrue((root / "out/stdout.txt").read_text().startswith("ENGINE_PHASE startup"))
            with self.assertRaises(ProcessLookupError):
                os.kill(result["child_pid"], 0)

    def test_streaming_digest(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "input"
            value = b"a" * 131073
            path.write_bytes(value)
            self.assertEqual(observer.digest(path), hashlib.sha256(value).hexdigest())

    def test_exact_case_shape_before_child_creation(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            with self.assertRaisesRegex(ValueError, "exact fixed case"):
                observer.collect(root / "none", root / "out", root / "db", "store-commit", root / "registration")
            self.assertFalse((root / "out").exists())


if __name__ == "__main__":
    unittest.main()
