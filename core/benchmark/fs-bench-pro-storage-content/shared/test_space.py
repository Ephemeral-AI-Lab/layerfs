#!/usr/bin/env python3
"""`unittest` entry points for `space.py`, wrapping its `self_check`.

The module's own `self_check` proves the two rules it exists for; these tests run
it *and* pin the individual behaviours so a failure names the rule that broke
rather than the whole module. The fabricated zero gets the most attention, because
`0 <= database` passes the O6 gate and the only thing standing between a renamed
table and a green row is that the sum is refused instead of defaulted.
"""

from __future__ import annotations

import os
import sqlite3
import struct
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import space


def make_store(directory: str, name: str, statements: list[str]) -> Path:
    """Creates a SQLite file and runs `statements` against it."""
    path = Path(directory) / name
    connection = sqlite3.connect(path)
    try:
        for statement in statements:
            connection.execute(statement)
        connection.commit()
    finally:
        connection.close()
    return path


class SelfCheckTest(unittest.TestCase):
    def test_the_module_self_check_is_clean(self) -> None:
        self.assertEqual(space.self_check(), [])


class FabricatedZeroTest(unittest.TestCase):
    def test_an_absent_object_packs_table_is_unknown_not_zero(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = make_store(directory, "absent.sqlite", ["CREATE TABLE something_else (data BLOB)"])
            self.assertTrue(space.table_exists(path, "something_else"))
            self.assertFalse(space.table_exists(path, "object_packs"))
            with self.assertRaises(space.Incomplete):
                space.pack_bodies(path)

    def test_an_object_packs_table_with_no_rows_is_unknown_not_zero(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = make_store(directory, "empty.sqlite", ["CREATE TABLE object_packs (data BLOB)"])
            self.assertTrue(space.table_exists(path, "object_packs"))
            with self.assertRaises(space.Incomplete):
                space.pack_bodies(path)

    def test_an_absent_objects_table_is_unknown_not_zero(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = make_store(directory, "no-objects.sqlite", ["CREATE TABLE object_packs (data BLOB)"])
            with self.assertRaises(space.Incomplete):
                space.object_rows(path)

    def test_the_executed_pack_sql_carries_no_default(self) -> None:
        executed = (space.PACK_SUM_SQL + " " + space.TABLE_EXISTS_SQL + " " + space.OBJECT_ROWS_SQL).upper()
        self.assertIn("SUM", space.PACK_SUM_SQL.upper())
        self.assertNotIn("COALESCE", executed)
        self.assertNotIn("IFNULL", executed)
        self.assertIn("OBJECT_PACKS", executed)

    def test_pack_bodies_sums_the_rows_it_finds(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = make_store(
                directory,
                "real.sqlite",
                [
                    "CREATE TABLE object_packs (data BLOB)",
                    "INSERT INTO object_packs (data) VALUES (zeroblob(4096))",
                    "INSERT INTO object_packs (data) VALUES (zeroblob(1024))",
                ],
            )
            self.assertEqual(space.pack_bodies(path), 5120)

    def test_a_zero_pack_sum_over_a_non_empty_store_is_incomplete(self) -> None:
        # A row whose `data` has length zero makes the sum a *real* 0 rather than
        # an absent one, which is the only way to reach the zero-sum branch: an
        # empty table raises `Incomplete` first, and a NULL row sums to NULL.
        with tempfile.TemporaryDirectory() as directory:
            path = make_store(
                directory,
                "zero.sqlite",
                [
                    "CREATE TABLE object_packs (data BLOB)",
                    "CREATE TABLE objects (id INTEGER)",
                    "INSERT INTO object_packs (data) VALUES (x'')",
                    "INSERT INTO objects (id) VALUES (1)",
                ],
            )
            self.assertEqual(space.pack_bodies(path), 0, "the sum is a real zero here")
            reading = space.footprint(path)
            self.assertEqual(reading.pack_accounting()["status"], "INCOMPLETE")
            self.assertIn("non-empty store", reading.pack_accounting()["detail"])

    def test_an_unreadable_pack_table_is_recorded_rather_than_defaulted(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = make_store(
                directory,
                "renamed.sqlite",
                [
                    "CREATE TABLE object_packs_v2 (data BLOB)",
                    "INSERT INTO object_packs_v2 (data) VALUES (zeroblob(4096))",
                    "CREATE TABLE objects (id INTEGER)",
                    "INSERT INTO objects (id) VALUES (1)",
                ],
            )
            reading = space.footprint(path)
            self.assertIsNone(reading.pack_bodies, "a renamed table must not read as zero")
            self.assertTrue(any("pack bodies" in entry for entry in reading.incomplete))
            self.assertEqual(reading.pack_accounting()["status"], "INCOMPLETE")


class PackAccountingTest(unittest.TestCase):
    """The O6 cell as a status, built from explicit values so each branch is hit."""

    def reading(self, pack_bodies: int | None, database_bytes: int, objects: int) -> space.Footprint:
        return space.Footprint(
            store_path="synthetic",
            sqlite=space.SqliteSpace(4096, 1, 0, database_bytes, 0, 0),
            pack_bodies=pack_bodies,
            objects=objects,
        )

    def test_a_missing_pack_reading_is_incomplete(self) -> None:
        self.assertEqual(self.reading(None, 4096, 3).pack_accounting()["status"], "INCOMPLETE")

    def test_pack_bytes_within_the_database_pass(self) -> None:
        self.assertEqual(self.reading(4096, 4096, 3).pack_accounting()["status"], "PASS")
        self.assertEqual(self.reading(1, 4096, 3).pack_accounting()["status"], "PASS")

    def test_pack_bytes_over_the_database_fail(self) -> None:
        self.assertEqual(self.reading(4097, 4096, 3).pack_accounting()["status"], "FAIL")

    def test_an_empty_store_may_legitimately_hold_nothing(self) -> None:
        self.assertEqual(self.reading(0, 4096, 0).pack_accounting()["status"], "PASS")


class ReadingTest(unittest.TestCase):
    def test_stat_space_reports_apparent_and_allocated_separately(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "payload.bin"
            path.write_bytes(b"x" * 8192)
            reading = space.stat_space(path)
            self.assertEqual(reading.apparent_bytes, 8192)
            self.assertGreaterEqual(reading.allocated_bytes, 8192)
            self.assertEqual(reading.as_fields()["apparent_bytes"], 8192)

    def test_sqlite_space_reads_the_pragmas_it_names(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = make_store(directory, "pragma.sqlite", ["CREATE TABLE t (a INTEGER)"])
            reading = space.sqlite_space(path)
            self.assertGreater(reading.page_size, 0)
            self.assertGreaterEqual(reading.page_count, 1)
            self.assertEqual(reading.database_bytes, reading.page_size * reading.page_count)
            self.assertEqual(space.quick_check(path), "ok")
            self.assertIn("t", space.schema_shape(path)["tables"])

    def test_sidecars_are_reported_and_never_hidden(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "store.sqlite"
            path.write_bytes(b"")
            self.assertEqual(space.sidecars(path), [])
            for suffix in ("-wal", "-shm", "-journal"):
                Path(f"{path}{suffix}").write_bytes(b"")
            self.assertEqual(
                space.sidecars(path),
                [f"{path}-wal", f"{path}-shm", f"{path}-journal"],
            )

    def test_store_files_lists_only_sqlite_looking_files(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            for name in ("a.sqlite", "b.db", "c.sqlite3", "notes.txt"):
                (Path(directory) / name).write_bytes(b"")
            names = [Path(entry).name for entry in space.store_files(directory)]
            self.assertEqual(names, ["a.sqlite", "b.db", "c.sqlite3"])

    def test_a_missing_store_is_recorded_incomplete_rather_than_zeroed(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            reading = space.footprint(Path(directory) / "absent.sqlite")
            self.assertIsNone(reading.stat)
            self.assertIsNone(reading.sqlite)
            self.assertIn("store file does not exist", reading.incomplete)
            self.assertEqual(reading.pack_accounting()["status"], "INCOMPLETE")


class PackDirectoryTests(unittest.TestCase):
    """The pack decoder, on packs built here rather than on a Store."""

    def _store(self, directory: str) -> str:
        path = os.path.join(directory, "sample.sqlite")
        connection = sqlite3.connect(path)
        connection.execute("CREATE TABLE object_packs (pack_id INTEGER PRIMARY KEY, data BLOB)")
        connection.execute(
            "CREATE TABLE objects (object_id BLOB, object_role INTEGER, pack_id INTEGER,"
            " group_number INTEGER)"
        )
        return path

    def _whole_file_pack(self, bodies: list[bytes]) -> bytes:
        # HEADER_LEN + 4 * groups, then each body: a tag byte and a frame. The
        # compact lane drops the two length fields at assembly time.
        count = len(bodies)
        base = 16 + 4 * count
        out = bytearray(b"LFPACK\x00\x00")
        out += struct.pack("<II", 4, count)
        offset = base
        for body in bodies:
            out += struct.pack("<I", offset)
            offset += len(body)
        for body in bodies:
            out += body
        return bytes(out)

    def _ordinary_pack(self, bodies: list[bytes], version: int = 1) -> bytes:
        count = len(bodies)
        base = 16 + 16 * count
        out = bytearray(b"LFPACK\x00\x00")
        out += struct.pack("<II", version, count)
        offset = base
        for body in bodies:
            out += struct.pack("<IIIB3x", offset, len(body), len(body), 0)
            offset += len(body)
        for body in bodies:
            out += body
        return bytes(out)

    def test_a_whole_file_pack_splits_into_framing_and_bodies(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = self._store(directory)
            bodies = [b"\x00" + b"a" * 9, b"\x01" + b"b" * 19]
            blob = self._whole_file_pack(bodies)
            connection = sqlite3.connect(path)
            connection.execute("INSERT INTO object_packs VALUES (1, ?)", (blob,))
            connection.commit()
            connection.close()
            reading = space.pack_directory(path)
            self.assertEqual(reading.packs, 1)
            self.assertEqual(reading.groups, 2)
            self.assertEqual(reading.blob_bytes, len(blob))
            self.assertEqual(reading.header_bytes, 16)
            self.assertEqual(reading.directory_bytes, 8)
            self.assertEqual(reading.framing_bytes, 24)
            self.assertEqual(reading.body_bytes, len(blob) - 24)
            self.assertEqual(reading.by_lane, {"whole-file": len(blob) - 24})

    def test_the_blob_sum_is_bodies_plus_framing(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = self._store(directory)
            blob = self._whole_file_pack([b"\x00" + b"x" * 5])
            connection = sqlite3.connect(path)
            connection.execute("INSERT INTO object_packs VALUES (1, ?)", (blob,))
            connection.commit()
            connection.close()
            reading = space.pack_directory(path)
            self.assertEqual(
                reading.blob_bytes,
                reading.body_bytes + reading.framing_bytes,
                "the pack blob is not bodies plus framing",
            )
            self.assertEqual(space.pack_bodies(path), reading.blob_bytes)

    def test_an_unimplemented_framing_version_is_incomplete(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = self._store(directory)
            blob = bytearray(self._whole_file_pack([b"\x00" + b"x" * 5]))
            blob[8:12] = struct.pack("<I", 3)
            connection = sqlite3.connect(path)
            connection.execute("INSERT INTO object_packs VALUES (1, ?)", (bytes(blob),))
            connection.commit()
            connection.close()
            with self.assertRaises(space.Incomplete):
                space.pack_directory(path)

    def test_a_wrong_magic_is_incomplete(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = self._store(directory)
            connection = sqlite3.connect(path)
            connection.execute("INSERT INTO object_packs VALUES (1, ?)", (b"NOTPACK!" + b"\x00" * 24,))
            connection.commit()
            connection.close()
            with self.assertRaises(space.Incomplete):
                space.pack_directory(path)

    def test_an_absent_pack_table_is_incomplete_not_empty(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = os.path.join(directory, "sample.sqlite")
            connection = sqlite3.connect(path)
            connection.execute("CREATE TABLE objects (object_id BLOB)")
            connection.commit()
            connection.close()
            with self.assertRaises(space.Incomplete):
                space.pack_directory(path)

    def test_whole_file_records_attributes_one_record_per_group(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = self._store(directory)
            bodies = [b"\x00" + b"a" * 9, b"\x01" + b"b" * 19]
            blob = self._whole_file_pack(bodies)
            connection = sqlite3.connect(path)
            connection.execute("INSERT INTO object_packs VALUES (1, ?)", (blob,))
            connection.execute("INSERT INTO objects VALUES (?, 1, 1, 0)", (b"first",))
            connection.execute("INSERT INTO objects VALUES (?, 1, 1, 1)", (b"second",))
            connection.commit()
            connection.close()
            sizes = space.whole_file_records(path)
            self.assertEqual(sizes, {b"first": 10, b"second": 20})

    def test_a_multi_record_lane_is_never_attributed_per_object(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = self._store(directory)
            # One native group holding two chunk records: a per-object join would
            # charge each of them the whole group, so the reader returns nothing for
            # them and the lane is reported as an aggregate instead.
            blob = self._ordinary_pack([b"two records in one group"], version=2)
            connection = sqlite3.connect(path)
            connection.execute("INSERT INTO object_packs VALUES (1, ?)", (blob,))
            connection.execute("INSERT INTO objects VALUES (?, 2, 1, 0)", (b"a",))
            connection.execute("INSERT INTO objects VALUES (?, 2, 1, 0)", (b"b",))
            connection.commit()
            connection.close()
            self.assertEqual(space.whole_file_records(path), {})
            reading = space.pack_directory(path)
            self.assertEqual(reading.by_lane, {"native": len(blob) - 16 - 16})

    def test_a_whole_file_row_in_a_multi_record_pack_is_refused(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = self._store(directory)
            # A whole-file role code inside a lane that is not one-record-per-group
            # is a disagreement between the row and the pack. Charging the row the
            # whole group is exactly the multiply-count this reader exists to refuse.
            blob = self._ordinary_pack([b"two records in one group"], version=1)
            connection = sqlite3.connect(path)
            connection.execute("INSERT INTO object_packs VALUES (1, ?)", (blob,))
            connection.execute("INSERT INTO objects VALUES (?, 1, 1, 0)", (b"a",))
            connection.commit()
            connection.close()
            with self.assertRaises(space.Incomplete):
                space.whole_file_records(path)

    def test_a_whole_file_locator_with_no_group_is_incomplete(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = self._store(directory)
            blob = self._whole_file_pack([b"\x00" + b"a" * 9])
            connection = sqlite3.connect(path)
            connection.execute("INSERT INTO object_packs VALUES (1, ?)", (blob,))
            connection.execute("INSERT INTO objects VALUES (?, 1, 1, 7)", (b"dangling",))
            connection.commit()
            connection.close()
            with self.assertRaises(space.Incomplete):
                space.whole_file_records(path)


if __name__ == "__main__":
    unittest.main()



class CatalogueTest(unittest.TestCase):
    """The pooled catalogue is read, never fabricated."""

    def test_an_absent_catalogue_is_incomplete_not_zero(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "store.sqlite"
            connection = sqlite3.connect(path)
            connection.execute("CREATE TABLE objects (id INTEGER)")
            connection.commit()
            connection.close()
            with self.assertRaises(space.Incomplete):
                space.catalogue(path)

    def test_an_empty_catalogue_is_a_legitimate_zero(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "store.sqlite"
            connection = sqlite3.connect(path)
            connection.execute(
                "CREATE TABLE metadata_value_groups (first_ordinal INTEGER, count INTEGER)"
            )
            connection.commit()
            connection.close()
            self.assertEqual(space.catalogue(path), (0, 0))

    def test_the_catalogue_reports_rows_and_values_separately(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "store.sqlite"
            connection = sqlite3.connect(path)
            connection.execute(
                "CREATE TABLE metadata_value_groups (first_ordinal INTEGER, count INTEGER)"
            )
            connection.execute("INSERT INTO metadata_value_groups VALUES (1, 165), (166, 100)")
            connection.commit()
            connection.close()
            self.assertEqual(space.catalogue(path), (2, 265))

    def test_the_executed_catalogue_sql_has_no_default(self) -> None:
        executed = space.CATALOGUE_SQL.upper()
        self.assertNotIn("COALESCE", executed)
        self.assertNotIn("IFNULL", executed)
