"""Values across the native module (#35, F7): what a Python value may be on
its way into a call, before any call is made, and how bytes, integers and
scope settings cross. Build first, as for test_calls.py. Temp homes need a
filesystem with SQLite locks (ext4, xfs, btrfs): point TMPDIR at one.
"""

import shutil
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent / "src"))

import layerfs  # noqa: E402
from layerfs import _client  # noqa: E402
from layerfs._values import decode, encode  # noqa: E402

P, B = "shop/api", "agent"


class Fixture(unittest.TestCase):
    """A Disposable Store with one project and a Branch, in a temp home."""

    @classmethod
    def setUpClass(cls):
        cls.home = tempfile.mkdtemp(prefix="layerfs-py-values-")
        cls.folder = tempfile.mkdtemp(prefix="layerfs-py-values-folder-")
        Path(cls.folder, "README.md").write_text("hello\n")
        cls.lfs = layerfs.client(home=cls.home)
        cls.lfs.store.create("shop")
        cls.lfs.project.init(P, cls.folder)
        cls.lfs.branch.fork(P, B, "main")

    @classmethod
    def tearDownClass(cls):
        layerfs.close()
        shutil.rmtree(cls.home)
        shutil.rmtree(cls.folder)

    def invalid(self, call, pattern, *args, **keywords):
        """The call is refused as InvalidInput, and never reaches the library."""
        with mock.patch.object(_client._native, "call", side_effect=AssertionError("reached the library")):
            with self.assertRaisesRegex(layerfs.LayerFSError, pattern) as caught:
                call(*args, **keywords)
        self.assertEqual(caught.exception.code, "InvalidInput")


class Encoding(Fixture):
    def test_a_lone_surrogate_is_invalid_input_in_every_place_text_goes(self):
        lone = "ab\ud800cd"
        pattern = "lone surrogate"
        self.invalid(self.lfs.file.write, pattern, P, B, "/s", lone)
        self.invalid(self.lfs.file.write, pattern, P, B, lone, "x")
        self.invalid(self.lfs.file.write, pattern, P, B, "/s", "x", message=lone)
        self.invalid(self.lfs.file.apply, pattern, P, B, [{"op": "write", "path": "/s", "content": lone}])
        self.invalid(layerfs.client(home=self.home, scope={"project": P, "folder": lone}).store.list, pattern)
        with self.assertRaisesRegex(layerfs.LayerFSError, pattern) as caught:
            layerfs.client(home=lone)
        self.assertEqual(caught.exception.code, "InvalidInput")
        with self.assertRaisesRegex(layerfs.LayerFSError, pattern):
            layerfs.doctor(lone)

    def test_a_paired_surrogate_written_as_one_character_is_fine(self):
        text = "\U0001f600 pair\n"
        self.lfs.file.write(P, B, "/emoji", text)
        self.assertEqual(self.lfs.file.read(P, B, "/emoji")["text"], text)

    def test_nan_and_infinity_are_invalid_input(self):
        for number in (float("nan"), float("inf"), float("-inf")):
            self.invalid(self.lfs.branch.list, "JSON", P, page_size=number)
            self.invalid(self.lfs.file.apply, "JSON", P, B, [{"op": "write", "path": "/n", "content": [number]}])
            self.invalid(
                layerfs.client(home=self.home, scope={"project": P, "folder": "/", "x": number}).store.list, "JSON"
            )

    def test_values_json_has_no_form_for_are_invalid_input(self):
        circular = []
        circular.append(circular)
        for value in (object(), {1, 2}, 1 + 2j, print, circular, {(1, 2): "tuple key"}):
            self.invalid(self.lfs.file.apply, "", P, B, [{"op": "write", "path": "/n", "content": value}])
        self.invalid(self.lfs.file.apply, "keys must be text, not int", P, B, [{1: "int key"}])
        self.invalid(self.lfs.file.write, "", P, B, "/n", object())

    def test_a_fraction_is_refused_by_the_library_as_invalid_input(self):
        with self.assertRaisesRegex(layerfs.LayerFSError, "whole numbers") as caught:
            self.lfs.branch.list(P, page_size=1.5)
        self.assertEqual(caught.exception.code, "InvalidInput")


class Integers(Fixture):
    def refused(self, value):
        with self.assertRaises(layerfs.LayerFSError) as caught:
            self.lfs.file.read(P, B, "/README.md", start_line=value)
        self.assertEqual(caught.exception.code, "InvalidInput")
        return caught.exception.message

    def test_past_64_bits_is_refused_as_out_of_range(self):
        for value in (2**63, -(2**63) - 1, 2**64, 10**30):
            self.assertIn("number out of range", self.refused(value), value)

    def test_more_digits_than_python_will_print_is_invalid_input(self):
        self.invalid(self.lfs.file.read, "not a JSON value", P, B, "/README.md", start_line=10**5000)

    def test_the_whole_signed_64_bit_range_reaches_the_call_exactly(self):
        # the text is parsed as an i64 (not out of range); the call itself
        # then refuses what is not a positive safe integer (2^53 - 1 at most)
        for value in (2**63 - 1, -(2**63), 2**53, 2**53 + 1):
            self.assertIn("positive safe integer", self.refused(value), value)
        self.assertEqual(self.lfs.file.read(P, B, "/README.md", start_line=2**53 - 1)["start_line"], 2**53 - 1)

    def test_a_bool_is_not_a_whole_number(self):
        self.assertIn("whole number", self.refused(True))

    def test_results_decode_exactly_to_python_ints(self):
        for value in (2**63 - 1, -(2**63), 2**53 + 1):
            self.assertEqual(decode(f'{{"n":{value}}}'), {"n": value})
            self.assertEqual(encode({"n": value}), f'{{"n": {value}}}')


class Bytes(Fixture):
    def size(self, path):
        return self.lfs.file.stat(P, B, path)["size"]

    def test_bytes_round_trip_by_size_and_text(self):
        cases = {
            "empty": b"",
            "binary": b"\x00\xff\x01\xfe",
            "unicode": "héllo 你好 \U0001f600\n".encode(),
            "bytearray": bytearray(b"ab"),
            "memoryview": memoryview(b"cd"),
        }
        for name, data in cases.items():
            self.lfs.file.write(P, B, f"/{name}", data)
            self.assertEqual(self.size(f"/{name}"), len(data), name)
        self.assertEqual(self.lfs.file.read(P, B, "/empty")["text"], "")
        self.assertEqual(self.lfs.file.read(P, B, "/unicode")["text"], "héllo 你好 \U0001f600\n")
        self.assertEqual(self.lfs.file.read(P, B, "/memoryview")["text"], "cd")

    def test_binary_content_is_stored_but_read_refuses_it(self):
        # `file.read` returns text only (the baseline has no binary read, D2)
        with self.assertRaisesRegex(layerfs.LayerFSError, "NUL or invalid UTF-8"):
            self.lfs.file.read(P, B, "/binary") if self.lfs.file.write(P, B, "/binary", b"\x00\xff") else None

    def test_a_one_key_base64_map_is_bytes_when_it_is_base64(self):
        # `{"base64": "AP8B"}` is the wire form of b"\x00\xff\x01": a map a
        # caller builds by hand is read as those bytes, not as a map
        self.lfs.file.write(P, B, "/wire", {"base64": "AP8B"})
        self.assertEqual(self.size("/wire"), 3)
        # the text "AP8B" stays text
        self.lfs.file.write(P, B, "/plain", "AP8B")
        self.assertEqual(self.size("/plain"), 4)
        self.assertEqual(self.lfs.file.read(P, B, "/plain")["text"], "AP8B")

    def test_a_one_key_base64_map_that_is_not_base64_stays_a_map(self):
        with self.assertRaisesRegex(layerfs.LayerFSError, "text or bytes") as caught:
            self.lfs.file.write(P, B, "/wire", {"base64": "!!"})
        self.assertEqual(caught.exception.code, "InvalidInput")

    def test_decode_reads_only_a_one_key_base64_text_map_as_bytes(self):
        self.assertEqual(decode('{"base64":"AP8B"}'), b"\x00\xff\x01")
        self.assertEqual(decode('{"base64":""}'), b"")
        self.assertEqual(decode('"AP8B"'), "AP8B")
        for text in ('{"base64":"AP8B","x":1}', '{"base64":5}', '{"base64":"!!"}', '{"base64":"é"}', "{}"):
            self.assertIsInstance(decode(text), dict, text)


class Scopes(Fixture):
    def test_a_scope_that_is_not_project_folder_branch_is_invalid_input(self):
        for scope, pattern in [
            ("x", "scope must be"),
            ([], "scope must be"),
            ({"folder": "/web"}, "scope.project"),
            ({"project": "nosuch"}, "scope.project"),
            ({"project": P}, "scope.folder"),
            ({"project": P, "folder": "web"}, "scope.folder"),
            ({"project": P, "folder": "/web/"}, "scope.folder"),
            ({"project": P, "folder": "/web", "bogus": 1}, "unknown scope option"),
            ({"project": P, "folder": "/web", "branch": "a/b"}, "scope.branch"),
            ({"project": P, "folder": "/web", "branch": 7}, "scope.branch"),
        ]:
            with self.assertRaisesRegex(layerfs.LayerFSError, pattern, msg=repr(scope)) as caught:
                layerfs.client(home=self.home, scope=scope).store.list()
            self.assertEqual(caught.exception.code, "InvalidInput", scope)

    def test_a_scope_without_a_branch_is_accepted_and_confines_paths(self):
        scoped = layerfs.client(home=self.home, scope={"project": P, "folder": "/web", "branch": None})
        with self.assertRaisesRegex(layerfs.LayerFSError, "out of scope: path"):
            scoped.file.stat(P, "main", "/README.md")

    def test_the_scope_of_a_client_is_not_shared_with_the_default_client(self):
        scoped = layerfs.client(home=self.home, scope={"project": P, "folder": "/web"})
        self.assertEqual(scoped.scope, {"project": P, "folder": "/web"})
        self.assertIsNone(self.lfs.scope)
        self.assertIsNone(layerfs.client().scope)


if __name__ == "__main__":
    unittest.main()
