"""When layerfs._native is missing or cannot be loaded, importing layerfs
must fail with a clear ImportError naming the missing module and platform (#39).
"""

import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
PACKAGE_SRC = HERE.parent / "src" / "layerfs"


class NativeMissing(unittest.TestCase):
    def test_missing_native_module_reports_clear_import_error(self):
        temp_dir = tempfile.mkdtemp(prefix="layerfs-py-missing-")
        try:
            target_pkg = Path(temp_dir) / "layerfs"
            # Copy source tree without bytecode or existing compiled binaries
            shutil.copytree(
                PACKAGE_SRC,
                target_pkg,
                ignore=shutil.ignore_patterns("__pycache__", "*.pyc", "_native*"),
            )

            cmd = [
                sys.executable,
                "-c",
                (
                    "import sys, layerfs\n"
                    "print('UNEXPECTED_SUCCESS')\n"
                ),
            ]
            env = os.environ.copy()
            env["PYTHONPATH"] = str(temp_dir)
            proc = subprocess.run(
                cmd,
                cwd=temp_dir,
                env=env,
                capture_output=True,
                text=True,
            )
            self.assertNotEqual(proc.returncode, 0)
            self.assertNotIn("UNEXPECTED_SUCCESS", proc.stdout)
            stderr = proc.stderr
            self.assertIn("layerfs._native", stderr)
            self.assertIn("wheel", stderr)
            self.assertNotIn("partially initialized module", stderr)
            self.assertNotIn("circular", stderr)
        finally:
            shutil.rmtree(temp_dir)


if __name__ == "__main__":
    unittest.main()
