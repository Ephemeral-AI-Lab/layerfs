"""External build-custody and source-classification regression proofs."""
import hashlib
import json
from types import SimpleNamespace
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

    def test_sealed_preparation_reuses_only_matching_binaries_and_receipt(self):
        from families import workspace_write as write
        with tempfile.TemporaryDirectory() as directory:
            results = Path(directory)
            original, output = results / "original", results / "new"
            original.mkdir(); output.mkdir()
            binary = results / "binary"; binary.write_bytes(b"release")
            identity = {"identity_method": custody.METHOD, "compilation_seal": "sql-v1", "harness_seal": "harness"}
            common = SimpleNamespace(RESULTS=results)
            cache, fields = write.preparation_cache(common, identity, True)
            prepared = {"identity": identity, "artifacts": {"benchmark_shell": {
                "path": str(binary), "sha256": write.sha256(binary)}}, "image_id": "sha256:image"}
            write.save(original / "prepared-artifacts.json", prepared)
            write.publish_preparation(cache, fields, original, prepared, True)
            with patch.object(write.subprocess, "check_output", return_value="sha256:image\n"):
                result = write.reuse_preparation(cache, fields, output, common, identity)
                self.assertIn("no rebuild", result["build_mode"])
                self.assertEqual(result["dependency_reuse"]["original_producer"], identity)
                binary.write_bytes(b"altered")
                with self.assertRaisesRegex(ValueError, "binary custody"):
                    write.reuse_preparation(cache, fields, output, common, identity)
            sql_change = {**identity, "compilation_seal": "sql-v2"}
            self.assertNotEqual(write.preparation_cache(common, sql_change, True)[0], cache)
            self.assertNotEqual(write.preparation_cache(common, identity, False)[0], cache)

    def test_corrupt_preparation_receipt_never_triggers_regeneration(self):
        from families import workspace_write as write
        with tempfile.TemporaryDirectory() as directory:
            results = Path(directory)
            identity = {"identity_method": custody.METHOD, "compilation_seal": "SQL", "harness_seal": "harness"}
            common = SimpleNamespace(RESULTS=results)
            cache, _ = write.preparation_cache(common, identity, False)
            cache.parent.mkdir(); cache.write_text("unfinished publication")
            with patch.object(write, "execute") as execute:
                with self.assertRaises(json.JSONDecodeError):
                    write.build(results, common, identity)
                execute.assert_not_called()

    def test_build_input_change_refuses_before_archive_publication(self):
        with tempfile.TemporaryDirectory() as directory:
            results = Path(directory); output = results / "out"; output.mkdir()
            identity = {"identity_method": custody.METHOD, "product_seal": "product",
                        "compilation_seal": "before"}
            with patch.object(runner, "RESULTS", results), patch.object(
                    runner.subprocess, "run", return_value=SimpleNamespace(returncode=0)), patch.object(
                    runner, "identities", return_value={"compilation_seal": "changed SQL"}):
                with self.assertRaisesRegex(ValueError, "inputs changed"):
                    runner.build(output, results / "target", identity)
            self.assertFalse((results / "binary-archive").exists())
            self.assertFalse((results / "sdk-build-release.json").exists())


if __name__ == "__main__":
    unittest.main()
