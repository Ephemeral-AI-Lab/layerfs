"""Deterministic independent Git-format vectors and normal harness API checks."""
import copy
import hashlib
import io
import json
import os
from pathlib import Path
import struct
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from r7 import git_index_oracle as oracle


def entry(name=b"file", mode=0o100644, oid=b"a" * 20, *, version=2, flags=0, extra=None, cache=None):
    # Literal network-order v2/v3 layout, independent of the product parser.
    words = list(cache or (10, 20, 30, 40, 50, 60, mode, 501, 20, 7))
    words[6] = mode
    encoded_flags = flags | min(len(name), 0xFFF)
    value = struct.pack("!10I", *words) + oid + struct.pack("!H", encoded_flags)
    if extra is not None:
        value += struct.pack("!H", extra)
    value += name + b"\0"
    return value + bytes((-len(value)) % 8)


def index(rows, *, version=2, extensions=b"", count=None):
    raw = struct.pack("!4sII", b"DIRC", version, len(rows) if count is None else count) + b"".join(rows) + extensions
    return raw + hashlib.sha1(raw).digest()


def extension(name, payload):
    return name + struct.pack("!I", len(payload)) + payload


def tree_oid(body):
    return hashlib.sha1(b"tree " + str(len(body)).encode() + b"\0" + body).digest()


def valid_tree():
    leaf = tree_oid(b"100644 file\0" + b"a" * 20)
    root = tree_oid(b"40000 sub\0" + leaf)
    return b"\0" + b"1 1\n" + root + b"sub\0" + b"1 0\n" + leaf


def parse(data, **kwargs):
    return oracle.parse_index(io.BytesIO(data), len(data), **kwargs)


class IndexGrammar(unittest.TestCase):
    def refuses(self, raw, text):
        with self.assertRaisesRegex(oracle.Refusal, text):
            parse(raw)

    def test_literal_v2_entry_keeps_all_cache_observations(self):
        raw = index([entry()])
        result = parse(raw)
        self.assertEqual(result["version"], 2)
        self.assertEqual(result["entries"], [dict(path_hex=b"file".hex(), mode=0o100644,
                                              object_id=(b"a" * 20).hex(), stage=0, assume_valid=False,
                                              extended=False, skip_worktree=False, intent_to_add=False)])
        self.assertEqual(result["stat_cache"][0], dict(path_hex=b"file".hex(), stage=0,
            ctime_seconds=10, ctime_nanoseconds=20, mtime_seconds=30, mtime_nanoseconds=40,
            device=50, inode=60, uid=501, gid=20, file_size=7))
        self.assertEqual(result["raw_index_sha256"], hashlib.sha256(raw).hexdigest())
        self.assertEqual(result["checksum"]["value"], raw[-20:].hex())
        self.assertTrue(result["checksum"]["verified"])

    def test_v3_persisted_flags_and_non_utf8_path(self):
        result = parse(index([entry(b"raw\xff", version=3, flags=0xC000, extra=0x6000)], version=3))
        row = result["entries"][0]
        self.assertEqual(row["path_hex"], b"raw\xff".hex())
        self.assertTrue(row["assume_valid"] and row["extended"] and row["skip_worktree"] and row["intent_to_add"])

    def test_conflict_stages_retained_in_order(self):
        result = parse(index([entry(flags=0x1000), entry(flags=0x2000)]))
        self.assertEqual([row["stage"] for row in result["entries"]], [1, 2])
        self.refuses(index([entry(), entry(flags=0x1000)]), "mixed stage-zero")

    def test_header_version_and_count_are_qualified(self):
        self.refuses(index([], version=4), "version")
        self.refuses(index([], count=1), "cannot fit")
        self.refuses(b"BAD!" + index([])[4:], "signature")
        with self.assertRaisesRegex(oracle.Refusal, "object format"):
            parse(index([]), object_format="sha256")
        with self.assertRaisesRegex(oracle.Refusal, "version"):
            parse(index([entry()]), versions=(3,))

    def test_checksum_covers_actual_cache_bytes(self):
        raw = bytearray(index([entry()]))
        raw[15] ^= 1  # Structurally legal cached ctime corruption, checksum unchanged.
        self.refuses(bytes(raw), "checksum mismatch")
        self.refuses(index([entry()])[:-20] + bytes(20), "zero/absent")
        self.refuses(index([entry()]) + b"extra", "bounds|checksum|extension")

    def test_paths_modes_flags_padding_and_order_refuse(self):
        for name in (b"", b"/absolute", b"a//b", b"a/../b", b".git/file", b"a/"):
            self.refuses(index([entry(name)]), "pathname")
        self.refuses(index([entry(mode=0o120755)]), "object mode")
        self.refuses(index([entry(flags=0x4000, extra=0)]), "extended flags")
        self.refuses(index([entry(version=3, flags=0x4000, extra=0x8000)], version=3), "reserved")
        self.refuses(index([entry(b"z"), entry(b"a")]), "unordered")
        self.refuses(index([entry(), entry()]), "duplicate")
        raw_entry = bytearray(entry())
        raw_entry[61] = 3
        self.refuses(index([bytes(raw_entry)]), "length flag")
        raw_entry = bytearray(entry())
        raw_entry[-1] = 1
        self.refuses(index([bytes(raw_entry)]), "padding")
        cache = (10, 1_000_000_000, 30, 40, 50, 60, 0, 501, 20, 7)
        self.refuses(index([entry(cache=cache)]), "nanosecond")

    def test_tree_object_ids_counts_and_hierarchy_are_validated(self):
        raw = index([entry(b"sub/file")], extensions=extension(b"TREE", valid_tree()))
        parsed = parse(raw)
        self.assertEqual([row["path_hex"] for row in parsed["extensions"][0]["nodes"]], ["", b"sub".hex()])
        wrong_oid = bytearray(valid_tree())
        wrong_oid[6] ^= 1
        self.refuses(index([entry(b"sub/file")], extensions=extension(b"TREE", bytes(wrong_oid))), "object ID")
        bad = b"\0" + b"2 0\n" + b"a" * 20
        self.refuses(index([entry()], extensions=extension(b"TREE", bad)), "count/object ID")
        self.refuses(index([entry()], extensions=extension(b"TREE", b"\0-1 1\n")), "incomplete")
        self.refuses(index([entry()], extensions=extension(b"TREE", b"file\0-1 0\n")), "root name")
        self.refuses(index([entry()], extensions=extension(b"TREE", b"\0-2 0\n")), "malformed counts")

    def test_invalid_tree_cache_is_observed_without_inventing_oid(self):
        result = parse(index([entry()], extensions=extension(b"TREE", b"\0-1 0\n")))
        self.assertEqual(result["extensions"][0]["nodes"][0]["object_id"], None)

    def test_valid_tree_cannot_include_intent_to_add_or_omit_directory_child(self):
        oid = tree_oid(b"100644 file\0" + b"a" * 20)
        ita = entry(version=3, flags=0x4000, extra=0x2000)
        self.refuses(index([ita], version=3, extensions=extension(b"TREE", b"\0" + b"1 0\n" + oid)), "object ID")
        leaf = tree_oid(b"100644 file\0" + b"a" * 20)
        root = tree_oid(b"40000 sub\0" + leaf)
        self.refuses(index([entry(b"sub/file")], extensions=extension(b"TREE", b"\0" + b"1 0\n" + root)), "complete directory children")

    def test_caller_root_leaf_symlink_is_refused_before_resolution(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "real"
            root.mkdir()
            alias = Path(directory) / "alias"
            alias.symlink_to(root, target_is_directory=True)
            with self.assertRaisesRegex(oracle.Refusal, "symlink"):
                oracle.checked_root(alias)

    def test_split_sparse_cache_and_unknown_extensions_refuse(self):
        for signature in (b"link", b"sdir", b"UNTR", b"FSMN", b"REUC", b"ABCD"):
            self.refuses(index([entry()], extensions=extension(signature, b"")), "unsupported")
        self.refuses(index([entry()], extensions=extension(b"TREE", b"\0-1 0\n") * 2), "duplicate")
        self.refuses(index([entry()], extensions=b"TREE" + struct.pack("!I", 1000)), "bounds")

    def test_short_original_read_has_no_retry(self):
        class Short(io.BytesIO):
            def read(self, count=-1):
                return super().read(max(0, count - 1))
        raw = index([entry()])
        with self.assertRaisesRegex(oracle.Refusal, "short original"):
            oracle.parse_index(Short(raw), len(raw))


class ObserverContract(unittest.TestCase):
    def setUp(self):
        self.folder = tempfile.TemporaryDirectory(prefix="layerfs-r7-index-oracle-")
        self.base = Path(self.folder.name)
        self.root = self.base / "worktree"
        self.root.mkdir()
        self.git = self.root / ".git"
        self.git.mkdir()
        (self.git / "config").write_bytes(b"[core]\nrepositoryformatversion = 1\n")
        self.binary = self.root / "git-binary-evidence"
        self.binary.write_bytes(b"external synthetic parser fixture; never executed")
        (self.git / "index").write_bytes(index([entry()]))
        effective = {key: "false" for key in oracle.EFFECTIVE_KEYS}
        effective.update({"core.repositoryformatversion": "1", "extensions.objectformat": "sha1",
                          "core.filemode": "true", "core.ignorecase": "true"})
        self.pin = dict(schema=oracle.PIN_SCHEMA, object_format="sha1", index_versions=[2, 3],
                        git=dict(binary=str(self.binary), sha256=oracle.file_digest(self.binary),
                                 version="external fixture, no execution", version_receipt_sha256="1" * 64),
                        config_sha256=oracle.file_digest(self.git / "config"), worktree_config_sha256=None,
                        effective_config=effective, effective_config_sha256=oracle.sha256(oracle.canonical(effective)),
                        effective_config_receipt_sha256="2" * 64)

    def tearDown(self):
        self.folder.cleanup()

    def test_observe_does_not_mutate_index_and_records_scoped_limits(self):
        before = (self.git / "index").read_bytes()
        actual = oracle.observe(self.root, self.pin)
        self.assertEqual(actual["status"], "OBSERVED")
        self.assertEqual((self.git / "index").read_bytes(), before)
        self.assertEqual(actual["excluded_cross_filesystem_fields"], list(oracle.STAT_FIELDS))
        self.assertIn("NOT_CLAIMED", actual["limitations"])
        self.assertEqual(oracle.compare(actual, actual)["status"], "PASS")

    def test_stat_only_difference_passes_but_semantic_change_fails(self):
        expected = oracle.observe(self.root, self.pin)
        words = (100, 200, 300, 400, 500, 600, 0, 502, 21, 8)
        (self.git / "index").write_bytes(index([entry(cache=words)]))
        actual = oracle.observe(self.root, self.pin)
        self.assertNotEqual(expected["raw_index_sha256"], actual["raw_index_sha256"])
        self.assertEqual(oracle.compare(expected, actual)["status"], "PASS")
        for change in (dict(mode=0o100755), dict(oid=b"b" * 20), dict(flags=0x8000)):
            (self.git / "index").write_bytes(index([entry(**change)]))
            self.assertEqual(oracle.compare(expected, oracle.observe(self.root, self.pin))["status"], "FAIL")

    def test_file_metadata_is_retained_in_comparison(self):
        expected = oracle.observe(self.root, self.pin)
        os.chmod(self.git / "index", 0o600)
        actual = oracle.observe(self.root, self.pin)
        self.assertIn("filesystem_metadata.mode", oracle.compare(expected, actual)["differences"])

    def test_config_binary_and_worktree_presence_require_actual_seals(self):
        for key, bad in (("config_sha256", "0" * 64), ("worktree_config_sha256", "0" * 64)):
            pin = dict(self.pin, **{key: bad})
            with self.assertRaisesRegex(oracle.Refusal, "config"):
                oracle.observe(self.root, pin)
        self.binary.write_bytes(b"changed actual binary")
        with self.assertRaisesRegex(oracle.Refusal, "binary"):
            oracle.observe(self.root, self.pin)

    def test_actual_default_keep_is_retained_and_does_not_admit_untr(self):
        pin = copy.deepcopy(self.pin)
        pin["effective_config"]["core.untrackedcache"] = "keep"
        pin["effective_config_sha256"] = oracle.sha256(oracle.canonical(pin["effective_config"]))
        self.assertEqual(oracle.validate_pin(pin)["effective_config"]["core.untrackedcache"], "keep")
        self.assertEqual(oracle.observe(self.root, pin)["status"], "OBSERVED")
        (self.git / "index").write_bytes(index([entry()], extensions=extension(b"UNTR", b"cache")))
        with self.assertRaisesRegex(oracle.Refusal, "unsupported"):
            oracle.observe(self.root, pin)

    def test_unknown_effective_forms_and_missing_pin_data_refuse(self):
        for key in ("index.skiphash", "core.splitindex", "core.sparsecheckout", "core.untrackedcache", "core.fsmonitor"):
            pin = copy.deepcopy(self.pin)
            pin["effective_config"][key] = "true"
            pin["effective_config_sha256"] = oracle.sha256(oracle.canonical(pin["effective_config"]))
            with self.assertRaisesRegex(oracle.Refusal, "unsupported"):
                oracle.validate_pin(pin)
        pin = copy.deepcopy(self.pin)
        del pin["git"]["version_receipt_sha256"]
        with self.assertRaisesRegex(oracle.Refusal, "version receipt"):
            oracle.validate_pin(pin)

    def test_symlink_index_and_external_git_directory_refuse(self):
        external = self.root / "external-index"
        external.write_bytes(index([entry()]))
        (self.git / "index").unlink()
        (self.git / "index").symlink_to(external)
        with self.assertRaises(OSError):
            oracle.observe(self.root, self.pin)
        (self.git / "index").unlink()
        self.git.rename(self.root / "moved-git")
        self.git.symlink_to(self.root / "moved-git", target_is_directory=True)
        with self.assertRaises(OSError):
            oracle.observe(self.root, self.pin)

    def test_tree_wrapper_changes_only_declared_index_payload_hash(self):
        expected = oracle.observe(self.root, self.pin)
        words = (100, 200, 300, 400, 500, 600, 0, 502, 21, 8)
        (self.git / "index").write_bytes(index([entry(cache=words)]))
        actual = oracle.observe(self.root, self.pin)
        base = dict(path=".git/index", **expected["filesystem_metadata"], sha256=expected["raw_index_sha256"])
        left, right = self.base / "expected.jsonl", self.base / "actual.jsonl"
        left.write_text(json.dumps(base) + "\n")
        right.write_text(json.dumps({**base, "sha256": actual["raw_index_sha256"]}) + "\n")
        self.assertEqual(oracle.compare_tree(left, right, "E04", expected, actual)["status"], "PASS")
        right.write_text(json.dumps({**base, "sha256": actual["raw_index_sha256"], "mode": 0o600}) + "\n")
        self.assertEqual(oracle.compare_tree(left, right, "E04", expected, actual)["status"], "FAIL")
        right.write_text(json.dumps({**base, "path": ".git/other", "sha256": "b"}) + "\n")
        self.assertEqual(oracle.compare_tree(left, right, "E04", expected, actual)["status"], "FAIL")
        right.write_text(json.dumps({**base, "sha256": "unrelated index"}) + "\n")
        self.assertEqual(oracle.compare_tree(left, right, "E04", expected, actual)["status"], "FAIL")

    def test_exclusive_output_and_sealed_json_inputs(self):
        path = self.base / "observation.json"
        value = oracle.observe(self.root, self.pin)
        oracle.write_new(path, value)
        self.assertEqual(oracle.load_sealed(path, oracle.file_digest(path)), value)
        with self.assertRaises(FileExistsError):
            oracle.write_new(path, value)
        with self.assertRaisesRegex(oracle.Refusal, "SHA-256"):
            oracle.load_sealed(path, "0" * 64)
        with self.assertRaisesRegex(oracle.Refusal, "outside verification root"):
            oracle.write_new(self.root / "would-mutate-worktree", value)


if __name__ == "__main__":
    unittest.main()
