"""External scope/byte/untracked checks; no Git, build or product execution."""
from pathlib import Path
import tempfile
import unittest
from unittest import mock
import sys

sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from r7 import source_inventory


class SourceInventoryContract(unittest.TestCase):
    def test_control_source_scope_excludes_unrelated_product(self):
        product = "core/crates/layerfs-storage/src/object.rs"
        self.assertTrue(source_inventory.relevant(product,"L"))
        self.assertFalse(source_inventory.relevant(product,"N"))
        self.assertFalse(source_inventory.relevant(product,"P"))
        callback = "core/benchmark/r7-passthrough/src/callbacks.rs"
        self.assertTrue(source_inventory.relevant(callback,"P"))
        self.assertFalse(source_inventory.relevant(callback,"N"))
        for arm in ("L","N","P"):
            self.assertTrue(source_inventory.relevant("core/benchmark/fs-bench-pro/r7/deployment.py",arm))

    def test_outside_src_shipped_input_not_dropped_by_extension(self):
        self.assertTrue(source_inventory.relevant("core/crates/layerfs-content/runtime/format.codec","L"))
        self.assertTrue(source_inventory.relevant("core/crates/layerfs-content/src/embedded.bin","L"))
        self.assertFalse(source_inventory.relevant("core/crates/layerfs-content/tests/fixture.rs","L"))

    def test_same_length_byte_edit_changes_source_set(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            path = root/"source.py"
            path.write_bytes(b"first")
            first = source_inventory.file_record(root,"source.py")
            path.write_bytes(b"other")
            second = source_inventory.file_record(root,"source.py")
            self.assertEqual(first["bytes"],second["bytes"])
            self.assertNotEqual(first["sha256"],second["sha256"])
            self.assertNotEqual(source_inventory.set_sha([first]),source_inventory.set_sha([second]))

    def test_relevant_untracked_source_rejected_even_if_inventory_would_include_it(self):
        with mock.patch.object(source_inventory,"git",return_value="tracked.py\0"):
            with self.assertRaisesRegex(ValueError,"untracked"):
                source_inventory.assert_clean(Path("/unused"),"N",["tracked.py","untracked.py"])

    def test_tracked_dirty_relevant_source_rejected(self):
        path = "core/benchmark/fs-bench-pro/r7/runner.py"
        with mock.patch.object(source_inventory,"git",side_effect=[path+"\0",path+"\0"]):
            with self.assertRaisesRegex(ValueError,"dirty"):
                source_inventory.assert_clean(Path("/unused"),"N",[path])

    def test_owned_tests_and_generated_cache_not_treatment_sources(self):
        self.assertFalse(source_inventory.relevant("core/benchmark/fs-bench-pro/r7/test_runner.py","N"))
        self.assertFalse(source_inventory.relevant("core/benchmark/fs-bench-pro/r7/__pycache__/runner.pyc","N"))
        self.assertFalse(source_inventory.relevant("core/benchmark/r7-runtime/target/release/runtime","N"))


if __name__ == "__main__":
    unittest.main()
