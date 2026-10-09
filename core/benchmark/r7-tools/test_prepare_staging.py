"""Owned synthetic inventory and first-original I/O checks; no product run."""
import copy
import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
sys.dont_write_bytecode = True
SPEC = importlib.util.spec_from_file_location("r7_test_prepare_staging", HERE / "prepare_staging.py")
staging = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(staging)


class InventoryContract(unittest.TestCase):
    def setUp(self):
        self.folder = tempfile.TemporaryDirectory(prefix="layerfs-r7-staging-test-", dir="/tmp")
        self.addCleanup(self.folder.cleanup)
        self.base = Path(self.folder.name).resolve()
        self.root = self.base / "tree"
        self.root.mkdir()
        (self.root / "directory").mkdir()
        (self.root / "directory/payload").write_bytes(b"unchanged payload")
        self.inventory = self.base / "inventory.jsonl"
        self.record = staging.deployment.seal_tree(self.root, self.inventory)
        self.rows = [json.loads(line) for line in self.inventory.read_text().splitlines()]

    def rewrite(self, rows, trailing=b""):
        content = ("".join(json.dumps(row) + "\n" for row in rows)).encode() + trailing
        self.inventory.write_bytes(content)
        result = dict(self.record, sha256=staging.input_digest(self.inventory))
        return result

    def test_complete_closed_names_pass_without_payload_reread(self):
        with mock.patch.object(staging, "copy_once", side_effect=AssertionError("no payload copy")):
            result = staging.sealed_tree(self.record)
        self.assertEqual(result["root"], self.root)

    def test_duplicate_general_path_and_duplicate_root_are_refused(self):
        for name in (".", "directory/payload"):
            rows = copy.deepcopy(self.rows)
            duplicate = next(row for row in rows if row.get("path") == name)
            rows.insert(-1, duplicate)
            with self.assertRaisesRegex(ValueError, "duplicate inventory path"):
                staging.sealed_tree(self.rewrite(rows))

    def test_missing_root_and_non_directory_root_refuse(self):
        rows = [row for row in copy.deepcopy(self.rows) if row.get("path") != "."]
        with self.assertRaisesRegex(ValueError, "root row"):
            staging.sealed_tree(self.rewrite(rows))
        rows = copy.deepcopy(self.rows)
        next(row for row in rows if row.get("path") == ".")["metadata"]["kind"] = "file"
        with self.assertRaisesRegex(ValueError, "root row"):
            staging.sealed_tree(self.rewrite(rows))

    def test_unknown_kind_and_unsealed_trailing_footer_refuse(self):
        rows = copy.deepcopy(self.rows)
        next(row for row in rows if row.get("path") == "directory/payload")["metadata"]["kind"] = "socket"
        with self.assertRaisesRegex(ValueError, "unsupported closed input kind"):
            staging.sealed_tree(self.rewrite(rows))
        for trailing in (b"{}\n", json.dumps(self.rows[-1]).encode() + b"\n"):
            with self.assertRaisesRegex(ValueError, "trailing inventory"):
                staging.sealed_tree(self.rewrite(self.rows, trailing))

    def test_actual_namespace_must_biject_closed_paths(self):
        for last in ("different", "directory"):
            with mock.patch.object(staging.deployment, "entries", return_value=iter([
                    (".", self.root), ("directory", self.root / "directory"),
                    (last, self.root / "directory/payload")])):
                with self.assertRaisesRegex(ValueError, "outside closed inventory"):
                    staging.sealed_tree(self.record)

    def test_inventory_record_read_is_bounded(self):
        self.inventory.write_bytes(b"x" * (staging.WINDOW + 1) + b"\n")
        changed = dict(self.record, sha256=staging.input_digest(self.inventory))
        with self.assertRaisesRegex(ValueError, "64 KiB"):
            staging.sealed_tree(changed)


class OriginalInputCustody(unittest.TestCase):
    def reader(self, original=None):
        instance = staging.InputFile.__new__(staging.InputFile)
        instance.stream = mock.Mock()
        instance.stream.close.side_effect = OSError("independent input close")
        instance.before = None
        return instance

    def test_read_error_and_independent_close_keep_first_original(self):
        original = OSError("original read failed")
        original.independent_close_failures = ["prior source close"]
        reader = self.reader()
        with self.assertRaises(OSError) as caught:
            with reader as stream:
                stream.read.side_effect = original
                stream.read(staging.WINDOW)
        self.assertIs(caught.exception, original)
        self.assertEqual(original.independent_close_failures, ["prior source close", "independent input close"])
        reader.stream.close.assert_called_once_with()
        reader.stream.read.assert_called_once_with(staging.WINDOW)

    def test_json_decode_error_survives_independent_close(self):
        reader = self.reader()
        reader.stream.read.return_value = b"invalid-json"
        with tempfile.TemporaryDirectory(prefix="layerfs-r7-staging-test-", dir="/tmp") as folder:
            path = Path(folder) / "original.json"
            path.write_bytes(b"invalid-json")
            with mock.patch.object(staging, "InputFile", return_value=reader):
                with self.assertRaises(json.JSONDecodeError) as caught:
                    staging.json_input(path)
        self.assertEqual(caught.exception.independent_close_failures, ["independent input close"])
        reader.stream.read.assert_called_once_with()

    def test_close_only_failure_remains_failure(self):
        reader = self.reader()
        with self.assertRaisesRegex(OSError, "independent input close"):
            with reader:
                pass

    def test_write_and_independent_close_keep_original_no_resend(self):
        original = OSError("original receipt write failed")
        stream = mock.Mock()
        stream.write.side_effect = original
        stream.close.side_effect = OSError("independent receipt close")
        with mock.patch.object(Path, "open", return_value=stream):
            with self.assertRaises(OSError) as caught:
                staging.write_record(Path("unused"), {"retained": "once"})
        self.assertIs(caught.exception, original)
        self.assertEqual(original.independent_close_failures, ["independent receipt close"])
        stream.write.assert_called_once()
        stream.close.assert_called_once_with()

    def test_short_write_and_close_keep_short_write_without_resend(self):
        stream = mock.Mock()
        stream.write.return_value = 1
        stream.close.side_effect = OSError("independent receipt close")
        with mock.patch.object(Path, "open", return_value=stream):
            with self.assertRaisesRegex(ValueError, "short original") as caught:
                staging.write_record(Path("unused"), {"retained": "once"})
        self.assertEqual(caught.exception.independent_close_failures, ["independent receipt close"])
        stream.write.assert_called_once()

    def test_json_reader_returns_digest_of_exact_decoded_bytes(self):
        with tempfile.TemporaryDirectory(prefix="layerfs-r7-staging-test-", dir="/tmp") as folder:
            path = Path(folder) / "declared.json"
            path.write_text('{"original":true}\n')
            actual, value, digest = staging.json_input(path)
            self.assertEqual(actual, path.resolve())
            self.assertEqual(value, {"original": True})
            self.assertEqual(digest, staging.input_digest(path))


if __name__ == "__main__":
    unittest.main()
