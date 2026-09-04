"""The family launchers are the only supported shell entrypoints."""
from pathlib import Path
import os
import re
import subprocess
import unittest


BENCH = Path(__file__).resolve().parent.parent


class LayoutTests(unittest.TestCase):
    def test_help_does_not_require_runtime_or_selection(self):
        for name in ("setup.sh", "perf.sh", "verify.sh"):
            result = subprocess.run(
                ["bash", str(BENCH / "families/init_namespace" / name), "--help"],
                env={**os.environ, "PYTHONDONTWRITEBYTECODE": "1"},
                capture_output=True, text=True, timeout=5,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn("--image", result.stdout)

    def test_no_root_shell_or_retired_pipeline(self):
        self.assertEqual(list(BENCH.glob("*.sh")), [])
        for name in ("workspace-runner.py", "sdk-edit-custody.py",
                     "generate-workspace-report.py", "generate-sdk-edit-report.py"):
            self.assertFalse((BENCH / name).exists(), name)

    def test_family_entrypoints_and_docker_source(self):
        families = sorted(path.parent for path in (BENCH / "families").glob("*/mod.rs"))
        self.assertEqual(len(families), 18)
        for family in families:
            for name in ("setup.sh", "perf.sh", "verify.sh"):
                path = family / name
                self.assertTrue(os.access(path, os.X_OK), str(path))
                subprocess.run(["bash", "-n", str(path)], check=True, timeout=5)
                text = path.read_text()
                target = "verify-selected.py" if name == "verify.sh" else "shared/runner.py"
                self.assertIn(target, text)
                self.assertIn("--family " + family.name, text)
        dockerfile = (BENCH / "Dockerfile.layerfs").read_text()
        copy = re.search(r"^COPY (\S+) /usr/local/bin/layerfs-daemon-entrypoint$", dockerfile, re.M)
        self.assertIsNotNone(copy)
        entry = BENCH.parent.parent / copy.group(1)
        self.assertEqual(entry, BENCH / "shared/daemon-entrypoint.sh")
        self.assertTrue(os.access(entry, os.X_OK))
        subprocess.run(["sh", "-n", str(entry)], check=True, timeout=5)


if __name__ == "__main__":
    unittest.main()
