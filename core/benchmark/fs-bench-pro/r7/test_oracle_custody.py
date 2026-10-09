"""External selected-oracle I/O custody proofs; no product hooks or executions."""
import contextlib
import hashlib
import io
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from r7 import oracle


class Custody(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="layerfs-r7-oracle-port-test-", dir="/tmp")
        self.addCleanup(temporary.cleanup)
        self.folder = Path(temporary.name).resolve()
        self.root = self.folder / "view"
        self.root.mkdir()
        self.payload = self.root / "payload"
        self.payload.write_bytes(b"known complete payload\n")
        self.output = self.folder / "observed.jsonl"

    def test_original_payload_read_plus_close_keeps_exact_cause_and_prior_phase(self):
        original = OSError("exact original payload read")
        original.original_phase = "prior selected payload phase"
        original.independent_close_failures = ["prior independent close"]
        reader = mock.Mock()
        reader.read.side_effect = original
        reader.close.side_effect = OSError("independent payload reader close")
        with mock.patch.object(Path, "open", return_value=reader):
            with self.assertRaises(OSError) as caught:
                oracle.digest_file(self.payload)
        self.assertIs(caught.exception, original)
        self.assertEqual(original.original_phase, "prior selected payload phase")
        self.assertEqual(original.independent_close_failures, ["prior independent close", "independent payload reader close"])
        reader.read.assert_called_once_with(65536)
        reader.close.assert_called_once_with()

    def test_payload_failure_and_both_closes_do_not_attempt_a_manifest_row(self):
        original = OSError("exact original selected read")
        reader, writer = mock.Mock(), mock.Mock()
        reader.read.side_effect = original
        reader.close.side_effect = OSError("independent payload close")
        writer.close.side_effect = OSError("independent manifest close")
        def opened(path, *args, **kwargs):
            self.assertEqual(kwargs.get("buffering"), 0)
            self.assertIn(path, {self.payload, self.output})
            return reader if path == self.payload else writer
        with mock.patch.object(Path, "open", opened), \
                mock.patch.object(oracle, "entries", return_value=iter([("payload", self.payload)])):
            with self.assertRaises(OSError) as caught:
                oracle.observe(self.root, "C01", self.output)
        self.assertIs(caught.exception, original)
        self.assertEqual(original.original_phase, "payload_read")
        self.assertEqual(original.independent_close_failures, ["independent payload close", "independent manifest close"])
        reader.read.assert_called_once_with(65536)
        writer.write.assert_not_called()
        writer.close.assert_called_once_with()

    def manifest_failure(self, short=False):
        original = OSError("exact original selected manifest write")
        writer = mock.Mock()
        if short:
            writer.write.return_value = 1
        else:
            writer.write.side_effect = original
        writer.close.side_effect = OSError("independent manifest close")
        actual_open = Path.open
        def opened(path, *args, **kwargs):
            return writer if path == self.output else actual_open(path, *args, **kwargs)
        with mock.patch.object(Path, "open", opened), \
                mock.patch.object(oracle, "entries", return_value=iter([("payload", self.payload)])):
            with self.assertRaises(ValueError if short else OSError) as caught:
                oracle.observe(self.root, "C01", self.output)
        if not short:
            self.assertIs(caught.exception, original)
        self.assertEqual(caught.exception.original_phase, "manifest_write")
        self.assertEqual(caught.exception.independent_close_failures, ["independent manifest close"])
        writer.write.assert_called_once()
        writer.close.assert_called_once_with()
        raw = writer.write.call_args.args[0]
        self.assertIsInstance(raw, bytes)
        row = json.loads(raw)
        self.assertEqual(row["path"], "payload")
        self.assertEqual(row["sha256"], hashlib.sha256(b"known complete payload\n").hexdigest())
        return caught.exception

    def test_original_manifest_write_plus_close_preserves_original_without_resend(self):
        self.manifest_failure()

    def test_short_manifest_write_plus_close_is_failure_without_tail_resend(self):
        self.assertIn("short original", str(self.manifest_failure(short=True)))

    def test_manifest_parse_failure_is_not_replaced_by_manifest_reader_close(self):
        reader = mock.Mock()
        reader.__iter__ = mock.Mock(return_value=iter(["{malformed original manifest\n"]))
        reader.close.side_effect = OSError("independent comparison input close")
        with mock.patch.object(Path, "open", return_value=reader):
            with self.assertRaises(json.JSONDecodeError) as caught:
                oracle.compare(self.output, self.output, "C01")
        self.assertEqual(caught.exception.original_phase, "manifest_read")
        self.assertEqual(caught.exception.independent_close_failures, ["independent comparison input close"])
        reader.close.assert_called_once_with()

    def test_close_only_failure_is_the_original_payload_failure(self):
        original = OSError("original payload close failed")
        reader = mock.Mock()
        reader.read.return_value = b""
        reader.close.side_effect = original
        with mock.patch.object(Path, "open", return_value=reader):
            with self.assertRaises(OSError) as caught:
                oracle.digest_file(self.payload)
        self.assertIs(caught.exception, original)
        self.assertEqual(original.original_phase, "payload_read_close")
        reader.close.assert_called_once_with()

    def test_cli_failure_envelope_retains_original_phase_and_independent_close(self):
        original = OSError("exact original port read")
        original.original_phase = "payload_read"
        original.independent_close_failures = ["independent port close"]
        output = io.StringIO()
        args = ["oracle.py", "observe", "--root", str(self.root), "--case", "C01", "--output", str(self.output)]
        with mock.patch.object(sys, "argv", args), mock.patch.object(oracle, "observe", side_effect=original), \
                contextlib.redirect_stdout(output):
            with self.assertRaises(SystemExit) as caught:
                oracle.main()
        self.assertEqual(caught.exception.code, 1)
        result = json.loads(output.getvalue())
        self.assertEqual(result["status"], "INCOMPLETE")
        self.assertEqual(result["original_failure"], "exact original port read")
        self.assertEqual(result["original_phase"], "payload_read")
        self.assertEqual(result["independent_close_failures"], ["independent port close"])

    def test_namespace_walk_failure_cannot_be_silently_skipped(self):
        original = OSError("exact original selected directory enumeration failure")
        original.independent_close_failures = ["prior directory ownership failure"]
        writer = mock.Mock()
        writer.write.side_effect = lambda raw: len(raw)
        writer.close.side_effect = OSError("independent partial manifest close")
        def failing_walk(root, *, followlinks, onerror):
            self.assertFalse(followlinks)
            onerror(original)
            yield None
        with mock.patch.object(oracle.os, "walk", side_effect=failing_walk), \
                mock.patch.object(Path, "open", return_value=writer):
            with self.assertRaises(OSError) as caught:
                oracle.observe(self.root, "C01", self.output)
        self.assertIs(caught.exception, original)
        self.assertEqual(original.original_phase, "namespace_walk")
        self.assertEqual(original.independent_close_failures, ["prior directory ownership failure", "independent partial manifest close"])
        self.assertEqual(writer.write.call_count, 1)
        self.assertEqual(json.loads(writer.write.call_args.args[0])["path"], ".")
        writer.close.assert_called_once_with()


class Semantics(unittest.TestCase):
    def test_c_and_dependency_observations_keep_payload_and_alias_contract(self):
        with tempfile.TemporaryDirectory(prefix="layerfs-r7-oracle-port-test-", dir="/tmp") as directory:
            folder = Path(directory).resolve()
            root = folder / "view"
            (root / "node_modules/pkg").mkdir(parents=True)
            (root / ".experiment-store").mkdir()
            (root / "unselected").write_bytes(b"unrelated bytes")
            source = root / ".experiment-store/0"
            source.write_bytes(b"dependency payload\n")
            os.link(source, root / "node_modules/pkg/file")
            for case in ("C01", "E12", "E13"):
                target = folder / (case + ".jsonl")
                result = oracle.observe(root, case, target)
                rows = [json.loads(line) for line in target.read_text().splitlines()]
                names = {row["path"] for row in rows}
                self.assertEqual("unselected" in names, case.startswith("C"))
                self.assertIn("node_modules/pkg/file", names)
                self.assertIn(".experiment-store/0", names)
                aliases = [row for row in rows if row["kind"] == "regular" and row["path"] != "unselected"]
                self.assertEqual(len(aliases), 2)
                self.assertEqual(aliases[0]["inode"], aliases[1]["inode"])
                self.assertTrue(all(row["sha256"] == hashlib.sha256(b"dependency payload\n").hexdigest() for row in aliases))
                self.assertEqual(result["manifest_sha256"], hashlib.sha256(target.read_bytes()).hexdigest())
                changed = folder / (case + ".changed.jsonl")
                for row in aliases:
                    row["device"] += 1
                    row["inode"] += 100
                changed.write_text("".join(json.dumps(row, sort_keys=True) + "\n" for row in rows))
                self.assertEqual(oracle.compare(target, changed, case)["status"], "PASS")
                aliases[0]["sha256"] = "0" * 64
                changed.write_text("".join(json.dumps(row, sort_keys=True) + "\n" for row in rows))
                self.assertEqual(oracle.compare(target, changed, case)["status"], "FAIL")

    def test_e13_alias_relation_remains_required_after_custody_change(self):
        with tempfile.TemporaryDirectory(prefix="layerfs-r7-oracle-port-test-", dir="/tmp") as directory:
            folder = Path(directory)
            expected, actual = folder / "expected.jsonl", folder / "actual.jsonl"
            rows = [dict(path=path, kind="regular", mode=0o644, uid=501, gid=20, size=4,
                         nlink=2, device=1, inode=2, sha256="a" * 64)
                    for path in ("node_modules/file", ".experiment-store/0")]
            expected.write_text("".join(json.dumps(row) + "\n" for row in rows))
            rows[1]["inode"] = 3
            actual.write_text("".join(json.dumps(row) + "\n" for row in rows))
            result = oracle.compare(expected, actual, "E13")
            self.assertEqual(result["status"], "FAIL")
            self.assertIn({"reason": "hard-link equivalence classes"}, result["differences"])


if __name__ == "__main__":
    unittest.main()
