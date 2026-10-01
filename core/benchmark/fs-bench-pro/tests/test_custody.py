"""External build-custody and source-classification regression proofs."""
import hashlib
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from shared import custody
import runner


class Custody(unittest.TestCase):
    def test_runtime_sql_and_nested_sources_are_build_inputs(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            core, harness = root / "core", root / "core/benchmark/fs-bench-pro"
            names = ("core/crates/c/sql/schema.sql", "core/crates/c/src/format.bin",
                     "core/crates/layerfs-api/sdk/examples/driver.rs",
                     "core/crates/c/Cargo.toml", "core/crates/c/build.rs",
                     "core/crates/c/tests/excluded.rs", "core/Cargo.toml", "core/Cargo.lock",
                     ".cargo/config.toml", "rust-toolchain.toml",
                     "core/benchmark/fs-bench-pro/registry/cases.json",
                     "core/benchmark/fs-bench-pro/writers/work.c",
                     "core/benchmark/fs-bench-pro/fixtures/input.dat",
                     "core/benchmark/fs-bench-pro-storage-content/shared/residency.py")
            for name in names:
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(b"first")
            product, inputs = custody.inputs(root, core, harness)
            self.assertNotIn(root / "core/crates/c/tests/excluded.rs", product)
            for name in names[:5]:
                self.assertIn(root / name, product)
            for name in names[-4:]:
                self.assertIn(root / name, inputs)
            before = custody.inventory(root, product)
            sql = root / names[0]
            sql.write_bytes(b"changed SQL only")
            after = custody.inventory(root, product)
            self.assertNotEqual(before[names[0]], after[names[0]])
            self.assertEqual(before[names[1]], after[names[1]])

    def test_flags_and_sql_change_compilation_identity(self):
        with patch.dict(custody.os.environ, {}, clear=True):
            before, _ = custody.compilation_seal("old SQL")
            after, _ = custody.compilation_seal("new SQL")
            self.assertNotEqual(before, after)
            with patch.dict(custody.os.environ, {"RUSTFLAGS": "--cfg aes_armv8"}):
                flags, environment = custody.compilation_seal("old SQL")
            self.assertNotEqual(before, flags)
            self.assertEqual(environment["RUSTFLAGS"], "--cfg aes_armv8")

    def test_old_rust_only_cache_cannot_reuse_executable(self):
        # Existing cache without the complete method must fail closed, even if
        # its executable and old product hash are unchanged.
        with tempfile.TemporaryDirectory() as directory:
            results = Path(directory)
            old = results / "driver"
            old.write_bytes(b"old binary")
            binaries = {name: {"path": str(old), "sha256": hashlib.sha256(b"old binary").hexdigest()}
                        for name in runner.BINARIES}
            runner.write_json(results / "sdk-build-release.json", {
                "build_profile": "release", "product_seal": "same", "binaries": binaries})
            output = results / "out"
            output.mkdir()
            with patch.object(runner, "RESULTS", results), patch.object(
                    runner.subprocess, "run", side_effect=RuntimeError("build entered")):
                with self.assertRaisesRegex(RuntimeError, "build entered"):
                    runner.build(output, results / "target", {
                        "product_seal": "same", "compilation_seal": "SQL-inclusive"})


if __name__ == "__main__":
    unittest.main()
