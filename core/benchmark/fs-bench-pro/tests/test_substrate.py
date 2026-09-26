import fcntl
import io
import subprocess
import sys
import tempfile
import unittest
from contextlib import redirect_stdout
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
        self.assertEqual(runner.BUILD_PROFILE, "release")
        self.assertEqual(runner.BINARY_DIR, "release/examples")
        self.assertIn("--release", runner.BUILD)
        self.assertGreater(runner.VERIFY_TIMEOUT_S, 5)
        self.assertLess(runner.VERIFY_TIMEOUT_S, 10)
        self.assertFalse(hasattr(runner.init, "_route"))
        driver = (runner.CORE / "crates/layerfs-api/sdk/examples/benchmark_init.rs").read_text()
        self.assertIn("ProjectApi::new(", driver)
        self.assertIn("Server::create(", driver)
        for backend in ("layerfs_bridge", "layerfs_history", "layerfs_sandbox", "layerfs_storage"):
            self.assertNotIn(f"use {backend}", driver)
        runner.require_sdk_driver("benchmark_init")

    def test_sdk_driver_refuses_backend_and_host_mutation(self):
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / "benchmark_exec2edit.rs"
            source.write_text("use layerfs_sdk::WorkspaceApi;\nuse layerfs_storage::Store;\n")
            with self.assertRaisesRegex(ValueError, "public layerfs-sdk"):
                runner.require_sdk_driver("benchmark_exec2edit", source)
            source.write_text("use layerfs_sdk::WorkspaceApi;\nuse layerfs_server::Service;\n")
            with self.assertRaisesRegex(ValueError, "public layerfs-sdk"):
                runner.require_sdk_driver("benchmark_exec2edit", source)
            source.write_text("use layerfs_sdk::WorkspaceApi;\nstd::fs::write(\"note\", b\"x\");\n")
            with self.assertRaisesRegex(ValueError, "public layerfs-sdk"):
                runner.require_sdk_driver("benchmark_exec2edit", source)
            source.write_text("use layerfs_sdk::WorkspaceApi;\nstd::process::Command::new(\"docker\");\n")
            with self.assertRaisesRegex(ValueError, "public layerfs-sdk"):
                runner.require_sdk_driver("benchmark_exec2edit", source)

    def test_explicit_large_cases_remain_separate_from_default_family(self):
        self.assertEqual(len(runner.init.SELECTED), 2)
        for case in tuple(runner.init.CASES)[2:]:
            with self.subTest(case=case), patch.object(runner, "run", return_value=Path("receipt")) as run:
                with patch.object(sys, "argv", ["runner.py", "run", "--case", case, "--out", "fresh"]):
                    with redirect_stdout(io.StringIO()):
                        runner.main()
                run.assert_called_once_with(case, "fresh")

    def test_release_build_refuses_an_old_debug_cache(self):
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory)
            results, target = home / "results", home / "target"
            results.mkdir()
            (target / runner.BINARY_DIR).mkdir(parents=True)
            old = home / "debug"
            old.mkdir()
            binaries = {}
            for name in runner.BINARIES:
                (old / name).write_bytes(b"debug")
                (target / runner.BINARY_DIR / name).write_bytes(b"release")
                binaries[name] = {"path": str(old / name), "sha256": runner.digest(old / name)}
            runner.write_json(results / "sdk-build-release.json", {
                "build_profile": "debug", "product_seal": "same", "binaries": binaries,
            })
            output = results / "out"
            output.mkdir()
            with patch.object(runner, "RESULTS", results), patch.object(
                runner.subprocess, "run", return_value=SimpleNamespace(returncode=0)
            ) as build:
                result = runner.build(output, target, {"product_seal": "same"})
            build.assert_called_once()
            self.assertEqual(result["build_profile"], "release")
            self.assertEqual(result["mode"], "changed-product")
            self.assertEqual(result["binaries"]["benchmark_init"]["sha256"],
                             runner.digest(target / runner.BINARY_DIR / "benchmark_init"))

    def test_lite_receipt_requires_sampled_content_and_full_path_counts(self):
        case = runner.init.CASES[runner.init.SELECTED[0]]
        sample, fixture = {"root": "root"}, {"manifest_sha256": "manifest"}
        child = {
            "status": "PASS", "paths": 102, "discovered_files": 100,
            "directories": 2, "manifest_bytes": 5_000_000,
            "sampled_files": 66, "sampled_bytes": 1_000_000,
            "sample_policy": runner.SAMPLE_POLICY, "workers": 4,
            "root": "root", "manifest_sha256": "manifest",
        }
        self.assertTrue(runner.lite_verification_pass(child, case, sample, fixture))
        for field in ("sampled_files", "sample_policy", "discovered_files"):
            missing = dict(child)
            del missing[field]
            self.assertFalse(runner.lite_verification_pass(missing, case, sample, fixture))

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
