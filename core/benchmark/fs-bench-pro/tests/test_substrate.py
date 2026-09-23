import fcntl
import io
import subprocess
import sys
import tempfile
import unittest
from contextlib import redirect_stdout
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import runner


class Substrate(unittest.TestCase):
    def test_run_uses_sdk_only_and_always_verifies(self):
        case = runner.init.SELECTED[0]
        with patch.object(runner, "run", return_value=Path("receipt")) as run:
            with patch.object(sys, "argv", ["runner.py", "run", "--case", case, "--out", "fresh"]):
                with redirect_stdout(io.StringIO()):
                    runner.main()
            run.assert_called_once_with(case, "fresh")
        self.assertEqual(runner.BINARIES, ("benchmark_init", "verify_namespace"))
        self.assertFalse(hasattr(runner.init, "_route"))
        driver = (runner.CORE / "crates/layerfs-api/sdk/examples/benchmark_init.rs").read_text()
        self.assertIn("client.init_project(", driver)
        self.assertIn("Host::create(", driver)
        for backend in ("layerfs_bridge", "layerfs_history", "layerfs_service", "layerfs_storage"):
            self.assertNotIn(f"use {backend}", driver)

    def test_output_and_target_refusal(self):
        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaises(ValueError):
                runner.owned(Path(directory) / "foreign")
            with self.assertRaises(ValueError):
                runner.owned(runner.RESULTS / "../escape")
            original = runner.os.environ.get("CARGO_TARGET_DIR")
            try:
                runner.os.environ["CARGO_TARGET_DIR"] = directory
                with self.assertRaises(ValueError):
                    runner.target_path()
            finally:
                if original is None:
                    runner.os.environ.pop("CARGO_TARGET_DIR", None)
                else:
                    runner.os.environ["CARGO_TARGET_DIR"] = original

    def test_worktree_locks_are_local(self):
        with tempfile.TemporaryDirectory() as directory:
            one, two = Path(directory) / "one", Path(directory) / "two"
            one.touch()
            two.touch()
            with one.open("a+b") as lock:
                fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
                code = "import fcntl,sys; f=open(sys.argv[1], 'a+b'); fcntl.flock(f, fcntl.LOCK_EX | fcntl.LOCK_NB)"
                self.assertNotEqual(subprocess.run([sys.executable, "-c", code, str(one)], stderr=subprocess.DEVNULL).returncode, 0)
                self.assertEqual(subprocess.run([sys.executable, "-c", code, str(two)], stderr=subprocess.DEVNULL).returncode, 0)

    def test_one_sample_cardinality_and_retained_hash(self):
        with tempfile.TemporaryDirectory() as directory:
            run = Path(directory)
            for case in runner.init.CASES.values():
                folder = run / "sdk-host" / "init_namespace" / case.id
                folder.mkdir(parents=True)
                runner.write_json(folder / "receipt.json", {"status": "NOT_RUN", "sample_count": 0})
            target = run / "sdk-host" / "init_namespace" / runner.init.SELECTED[0] / "receipt.json"
            runner.write_json(target, {"status": "INCOMPLETE", "sample_count": 2})
            runner.manifest_run(run)
            with self.assertRaisesRegex(ValueError, "one-sample"):
                runner.verify_run(run)
            target.write_text("corrupt")
            with self.assertRaisesRegex(ValueError, "retained evidence"):
                runner.verify_run(run)


if __name__ == "__main__":
    unittest.main()
