import fcntl
import io
import subprocess
import sys
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from types import SimpleNamespace
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

    def test_100k_selector_is_explicit_and_registered_case_stays_deferred(self):
        with patch.object(runner, "run", return_value=Path("receipt")) as run:
            with patch.object(sys, "argv", ["runner.py", "run", "--diagnostic-100k", "--out", "fresh"]):
                with redirect_stdout(io.StringIO()):
                    runner.main()
            run.assert_called_once_with("diagnostic-100k", "fresh")
            run.reset_mock()
            with patch.object(sys, "argv", ["runner.py", "run", "--diagnostic-100k-release", "--out", "fresh"]):
                with redirect_stdout(io.StringIO()):
                    runner.main()
            run.assert_called_once_with("diagnostic-100k-release", "fresh")
            with patch.object(sys, "argv", ["runner.py", "run", "--case", "namespace-100000", "--out", "fresh"]):
                with redirect_stdout(io.StringIO()), redirect_stderr(io.StringIO()):
                    with self.assertRaises(SystemExit):
                        runner.main()
            run.assert_called_once()
        self.assertNotIn("--release", runner.BUILD)
        self.assertIn("--release", runner.BUILD_RELEASE)

    def test_payload_residency_refuses_warmed_source(self):
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory)
            path = source / "payload"
            path.write_bytes(b"x" * runner.residency.page_size())
            manifest = source / "manifest.tsv"
            manifest.write_text(f"payload\tf\t420\t0\t{path.stat().st_size}\t-\n")
            self.assertEqual(runner.payload_pages(source, manifest, invalidate=True)["resident_pages"], 0)
            path.read_bytes()
            self.assertGreater(runner.payload_pages(source, manifest, invalidate=False)["resident_pages"], 0)

    def test_driver_resource_scope_and_frozen_not_run_rows(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            perf, child = runner.diagnostic_child(
                [sys.executable, "-c", 'print("{\\\"status\\\":\\\"COMPLETE\\\",\\\"operation_ns\\\":1}")'],
                root, "unused", "unused", timeout=2)
            self.assertEqual(perf["operation_ns"], 1)
            self.assertEqual(child["exit_code"], 0)
            self.assertGreater(child["external_resources"]["peak_rss_bytes"], 0)
            runner.fill_not_run(root, "diagnostic-100k")
            runner.fill_not_run(root, "diagnostic-100k-release")
            frozen = root / "sdk-host/init_namespace/namespace-100000/receipt.json"
            self.assertEqual(runner.json.loads(frozen.read_text())["status"], "NOT_RUN")

    def test_release_build_archives_release_examples(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            output, target = root / "output", root / "target"
            output.mkdir()
            for profile in ("debug", "release"):
                home = target / profile / "examples"
                home.mkdir(parents=True)
                for name in runner.BINARIES:
                    (home / name).write_bytes(profile.encode() + name.encode())
            with patch.object(runner, "RESULTS", root), patch.object(
                runner.subprocess, "run", return_value=SimpleNamespace(returncode=0)
            ) as call:
                result = runner.build(output, target, {"product_seal": "fixed"}, release=True)
            self.assertEqual(result["profile"], "release")
            self.assertIn("--release", call.call_args.args[0])
            for name in runner.BINARIES:
                self.assertEqual(Path(result["binaries"][name]["path"]).read_bytes(), b"release" + name.encode())

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
