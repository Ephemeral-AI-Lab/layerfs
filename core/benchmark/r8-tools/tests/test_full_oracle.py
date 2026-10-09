"""Adversarial external fixtures for the independent R8 full-byte oracle.

The native test directory is a stand-in for a public FUSE mount. Its real bytes,
names, links and descriptor traversal are exercised. Only the observations of
configured ownership, portable ctime, symlink permissions and directory link
counts are projected by an external mock. Native host ctime cannot be set, and
APFS directory link counts do not follow the public FUSE 2+subdirectories rule.
Expected inventory is authored from independent fixture definitions, not from
the mounted tree or the comparator's projected output.
"""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import stat
import tempfile
import unittest
from unittest import mock

SOURCE = Path(__file__).resolve().parents[1] / "full_oracle.py"
SPEC = importlib.util.spec_from_file_location("r8_full_oracle", SOURCE)
oracle = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(oracle)

MTIME = 1_700_000_000_123_456_789
PAYLOADS = {"a": b"original payload\0\xff", "dir/b": bytes(range(256)) * 600}


def input_fingerprint(rows):
    """Independent implementation of the documented retained R7 set seal."""
    total = xor = 0
    for row in rows:
        text = json.dumps({"path": row["path"], **row["metadata"]},
                          sort_keys=True, separators=(",", ":")).encode()
        number = int.from_bytes(hashlib.sha256(b"r7-fixture-row-v1\0" + text).digest(), "big")
        total = (total + number) % (1 << 256)
        xor ^= number
    data = (b"r7-fixture-set-v1\0" + str(len(rows)).encode() + b"\0"
            + total.to_bytes(32, "big") + xor.to_bytes(32, "big"))
    return hashlib.sha256(data).hexdigest()


class FullOracleTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="r8-oracle-unit-")
        self.addCleanup(self.temporary.cleanup)
        self.folder = Path(self.temporary.name).resolve()
        self.root = self.folder / "mount"
        self.root.mkdir()
        (self.root / "dir").mkdir()
        for name, data in PAYLOADS.items():
            (self.root / name).write_bytes(data)
            os.chmod(self.root / name, 0o640)
        os.symlink("../a", self.root / "dir/link")
        self.reset_times()
        # These are the authored fake mount's two directory identities, not
        # expectations inferred from native link counts. The fixture has one
        # root child directory and no child directories beneath dir.
        self.directory_links = {}
        for name, links in ((".", 3), ("dir", 2)):
            info = (self.root / name).lstat()
            self.directory_links[(info.st_dev, info.st_ino)] = links
        self.rows = [self.row(".", "directory", 1), self.row("dir", "directory", 2),
                     self.row("a", "file", 3), self.row("dir/b", "file", 4),
                     self.row("dir/link", "symlink", 5)]
        self.inventory = self.folder / "input.jsonl"
        self.output = self.folder / "proof.json"
        self.native_observe = oracle.observe

    def reset_times(self):
        for name in ("a", "dir/b", "dir/link", "dir", "."):
            path = self.root if name == "." else self.root / name
            if path.exists() or path.is_symlink():
                os.utime(path, ns=(MTIME, MTIME), follow_symlinks=False)
        os.chmod(self.root, 0o750)
        os.chmod(self.root / "dir", 0o750)

    def row(self, name, kind, inode, nlink=1, data=None):
        metadata = dict(kind=kind, mode=0o750 if kind == "directory" else
                        0o555 if kind == "symlink" else 0o640,
                        uid=987, gid=654, mtime_ns=MTIME)
        if kind == "file":
            data = PAYLOADS[name] if data is None else data
            metadata.update(size=len(data), content_sha256=hashlib.sha256(data).hexdigest())
        elif kind == "symlink":
            metadata["target_hex"] = b"../a".hex()
        return dict(path=name, metadata=metadata, identity=dict(device=91, inode=inode, nlink=nlink))

    def write_inventory(self, *, footer_changes=None, header_changes=None, extra_rows=()):
        header = dict(schema="r7-deployment-tree-v1", root="/independent/native/source",
                      projection="new owned input seal", read_window_bytes=65536)
        header.update(header_changes or {})
        footer = dict(event="completed", status="SEALED_SETUP_ONLY", entries=len(self.rows),
                      regular_files=sum(row["metadata"]["kind"] == "file" for row in self.rows),
                      regular_bytes=sum(row["metadata"].get("size", 0) for row in self.rows),
                      content_metadata_set_sha256=input_fingerprint(self.rows))
        footer.update(footer_changes or {})
        text = "".join(json.dumps(row, sort_keys=True) + "\n"
                       for row in (header, *self.rows, footer, *extra_rows))
        self.inventory.write_text(text)
        return hashlib.sha256(text.encode()).hexdigest()

    def fuse_observation(self, info):
        result = self.native_observe(info)
        result.update(uid=501, gid=20, ctime_ns=result["mtime_ns"])
        if result["kind"] == "symlink":
            result["mode"] = 0o777
        elif result["kind"] == "directory":
            result["nlink"] = self.directory_links[(info.st_dev, info.st_ino)]
        return result

    def run_proof(self, *, digest=None, observe=None, footer_changes=None,
                  header_changes=None, extra_rows=()):
        sealed = self.write_inventory(footer_changes=footer_changes,
                                      header_changes=header_changes, extra_rows=extra_rows)
        with mock.patch.object(oracle, "observe", side_effect=observe or self.fuse_observation):
            result = oracle.prove(self.root, self.inventory, digest or sealed, self.output)
        self.assertEqual(json.loads(self.output.read_text()), result)
        self.assertEqual(result["schema"], "r8-full-mounted-oracle-v1")
        return result

    def assert_failed(self, result, phrase, phase=None):
        self.assertEqual(result["status"], "FAIL", result)
        self.assertIn(phrase, result["exactfailure"])
        self.assertTrue(result["differences"])
        if phase:
            self.assertEqual(result["original_phase"], phase)

    def test_complete_tree_reads_every_byte_in_bounded_windows(self):
        real_read = os.read
        with mock.patch.object(oracle.os, "read", side_effect=real_read) as reads:
            result = self.run_proof()
        self.assertEqual(result["status"], "PASS", result)
        self.assertEqual((result["paths"], result["hashed_files"], result["hashed_bytes"]),
                         (5, 2, sum(map(len, PAYLOADS.values()))))
        self.assertTrue(all(call.args[1] == 65536 for call in reads.call_args_list))
        expected = Path(result["artifacts"]) / "expected.jsonl"
        projected = [json.loads(line) for line in expected.read_text().splitlines()][1:]
        values = {row["path"]: row for row in projected}
        self.assertEqual(values["."]["nlink"], 3)
        self.assertEqual(values["dir"]["nlink"], 2)
        self.assertEqual(values["dir/link"]["mode"], 0o777)
        self.assertEqual(values["a"]["uid"], 501)
        self.assertEqual(values["a"]["gid"], 20)
        self.assertEqual(result["projected_expected_sha256"], oracle.sha256(expected))

    def test_wrong_digest_reads_no_mount_payload_and_retains_failure(self):
        with mock.patch.object(oracle, "read_file") as read:
            result = self.run_proof(digest="0" * 64)
        self.assert_failed(result, "inventory SHA-256 mismatch", "input_seal")
        read.assert_not_called()
        self.assertEqual(result["paths"], 0)

    def test_missing_name(self):
        (self.root / "a").unlink()
        self.reset_times()
        self.assert_failed(self.run_proof(), "missing namespace name: a")

    def test_extra_name(self):
        (self.root / "extra").write_bytes(b"unexpected")
        self.reset_times()
        self.assert_failed(self.run_proof(), "extra namespace name: extra")

    def test_same_size_changed_bytes(self):
        (self.root / "a").write_bytes(b"x" * len(PAYLOADS["a"]))
        self.reset_times()
        self.assert_failed(self.run_proof(), "regular content mismatch: a")

    def test_changed_size(self):
        (self.root / "a").write_bytes(b"short")
        self.reset_times()
        self.assert_failed(self.run_proof(), "regular size mismatch: a")

    def test_changed_mode(self):
        os.chmod(self.root / "a", 0o600)
        self.assert_failed(self.run_proof(), "metadata mismatch mode: a")

    def test_changed_mtime(self):
        os.utime(self.root / "a", ns=(MTIME + 1, MTIME + 1))
        self.assert_failed(self.run_proof(), "metadata mismatch mtime_ns: a")

    def test_wrong_projected_ctime(self):
        def observation(info):
            value = self.fuse_observation(info)
            value["ctime_ns"] += 1
            return value
        self.assert_failed(self.run_proof(observe=observation), "metadata mismatch ctime_ns")

    def test_wrong_projected_owner(self):
        def observation(info):
            value = self.fuse_observation(info)
            value["uid"] = 0
            return value
        self.assert_failed(self.run_proof(observe=observation), "metadata mismatch uid")

    def test_wrong_symlink_target(self):
        (self.root / "dir/link").unlink()
        os.symlink("../z", self.root / "dir/link")
        self.reset_times()
        self.assert_failed(self.run_proof(), "symlink target/size mismatch: dir/link")

    def test_file_changed_to_symlink_is_not_followed(self):
        (self.root / "a").unlink()
        os.symlink(str(self.folder / "outside"), self.root / "a")
        self.reset_times()
        with mock.patch.object(oracle, "read_file") as read:
            result = self.run_proof()
        self.assert_failed(result, "metadata mismatch kind: a")
        self.assertTrue(all(call.args[1] != "a" for call in read.call_args_list))

    def test_directory_nlink_uses_expected_child_count(self):
        def observation(info):
            value = self.fuse_observation(info)
            if value["kind"] == "directory":
                value["nlink"] = 2
            return value
        self.assert_failed(self.run_proof(observe=observation), "metadata mismatch nlink: .")

    def add_alias(self, *, linked=True):
        if linked:
            os.link(self.root / "a", self.root / "alias")
        else:
            (self.root / "alias").write_bytes(PAYLOADS["a"])
            os.chmod(self.root / "alias", 0o640)
        os.utime(self.root / "alias", ns=(MTIME, MTIME))
        self.rows[2]["identity"]["nlink"] = 2
        self.rows.append(self.row("alias", "file", 3, nlink=2, data=PAYLOADS["a"]))
        self.reset_times()

    def test_regular_alias_names_are_all_hashed(self):
        self.add_alias()
        result = self.run_proof()
        self.assertEqual(result["status"], "PASS", result)
        self.assertEqual(result["regular_alias_groups"], 1)
        self.assertEqual(result["hashed_files"], 3)
        self.assertEqual(result["hashed_bytes"], sum(map(len, PAYLOADS.values())) + len(PAYLOADS["a"]))

    def test_split_alias_detected_even_with_counterfeit_nlink(self):
        self.add_alias(linked=False)
        def observation(info):
            value = self.fuse_observation(info)
            if value["kind"] == "file" and value["size"] == len(PAYLOADS["a"]):
                value["nlink"] = 2
            return value
        self.assert_failed(self.run_proof(observe=observation), "regular alias class split")

    def test_merged_distinct_alias_classes_detected(self):
        self.add_alias()
        self.rows[2]["identity"]["nlink"] = 1
        self.rows[-1]["identity"].update(inode=6, nlink=1)
        def observation(info):
            value = self.fuse_observation(info)
            if value["kind"] == "file":
                value["nlink"] = 1
            return value
        self.assert_failed(self.run_proof(observe=observation), "unrelated regular alias classes merged")

    def test_inventory_external_alias_names_refused(self):
        self.rows[2]["identity"]["nlink"] = 2
        self.assert_failed(self.run_proof(), "external names", "expected_validation")

    def test_unsafe_inventory_path(self):
        self.rows[2]["path"] = "../outside"
        self.assert_failed(self.run_proof(), "unsafe inventory path", "expected_validation")

    def test_duplicate_inventory_path(self):
        self.rows.append(self.rows[2])
        self.assert_failed(self.run_proof(), "duplicate inventory path: a", "expected_validation")

    def test_missing_inventory_parent(self):
        self.rows[3]["path"] = "absent/b"
        self.assert_failed(self.run_proof(), "missing or non-directory parent", "expected_validation")

    def test_nonportable_mode_is_not_silently_masked(self):
        self.rows[2]["metadata"]["mode"] = 0o4640
        self.assert_failed(self.run_proof(), "nonportable regular mode", "expected_validation")

    def test_wrong_footer_count(self):
        self.assert_failed(self.run_proof(footer_changes={"regular_bytes": 0}),
                           "completion count mismatch", "expected_validation")

    def test_wrong_footer_set_digest(self):
        self.assert_failed(self.run_proof(footer_changes={"content_metadata_set_sha256": "0" * 64}),
                           "set fingerprint mismatch", "expected_validation")

    def test_data_after_completion(self):
        self.assert_failed(self.run_proof(extra_rows=[self.rows[2]]),
                           "data after completion", "expected_validation")

    def test_source_u64_identity_has_no_signed_sqlite_limit(self):
        self.rows[2]["identity"]["inode"] = (1 << 64) - 1
        self.assertEqual(self.run_proof()["status"], "PASS")

    def timestamp_proof(self, nanoseconds):
        for row in self.rows:
            row["metadata"]["mtime_ns"] = nanoseconds
        def observation(info):
            value = self.fuse_observation(info)
            value.update(mtime_ns=nanoseconds, ctime_ns=nanoseconds)
            return value
        result = self.run_proof(observe=observation)
        self.assertEqual(result["status"], "PASS", result)
        rows = [json.loads(line) for line in
                (Path(result["artifacts"]) / "expected.jsonl").read_text().splitlines()][1:]
        self.assertTrue(all(type(row["mtime_ns"]) is int and row["mtime_ns"] == nanoseconds
                            for row in rows))

    def test_timestamp_outside_signed64_nanoseconds_is_preserved(self):
        # A signed64-second portable timestamp can legitimately exceed
        # SQLite INTEGER's signed64 nanosecond range. Authored independently
        # of both the native host clock and the indexed expected output.
        self.timestamp_proof((1 << 63) + 123_456_789)

    def test_negative_portable_timestamp_is_preserved(self):
        self.timestamp_proof(-123_456_789_000_000_001)

    def test_refuses_output_inside_mount(self):
        digest = self.write_inventory()
        with self.assertRaisesRegex(oracle.ProofError, "external"):
            oracle.prove(self.root, self.inventory, digest, self.root / "proof.json")
        self.assertFalse((self.root / "proof.json.artifacts").exists())

    def test_existing_output_is_never_overwritten(self):
        digest = self.write_inventory()
        self.output.write_text("retained")
        with self.assertRaisesRegex(oracle.ProofError, "fresh"):
            oracle.prove(self.root, self.inventory, digest, self.output)
        self.assertEqual(self.output.read_text(), "retained")


if __name__ == "__main__":
    unittest.main()
