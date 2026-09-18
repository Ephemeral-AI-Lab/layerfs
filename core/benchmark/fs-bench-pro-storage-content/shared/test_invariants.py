#!/usr/bin/env python3
"""`unittest` entry points for `invariants.py`, wrapping its `self_check`.

There is no counter for an absent route, so the no-retry / no-fsync / no-WAL
contract is carried by two things that *can* fail: a sealed scan over the product
source, and runtime tripwires read off the Stores a run produced. Both are tested
here in the failing direction as well as the passing one — a tripwire that cannot
report a sidecar is not a tripwire.

The comment stripper gets its own tests because it decides what the scan sees: a
scanner that counted the product's own prose about `fsync` could never be kept
green, and one that stripped string literals could never catch a WAL literal.
"""

from __future__ import annotations

import sqlite3
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import invariants


def make_store(directory: str, name: str, statements: list[str]) -> Path:
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
        self.assertEqual(invariants.self_check(), [])


class CommentStrippingTest(unittest.TestCase):
    def test_line_and_block_comments_are_removed(self) -> None:
        stripped = invariants.strip_rust_comments("let a = 1; // fsync\n/* sync_all( */\nlet b = 2;")
        self.assertNotIn("fsync", stripped)
        self.assertNotIn("sync_all", stripped)
        self.assertIn("let a = 1;", stripped)
        self.assertIn("let b = 2;", stripped)

    def test_a_string_literal_is_kept(self) -> None:
        # A WAL mode spelled as a literal is exactly what the scan must catch, so
        # it must not be stripped as if it were prose.
        self.assertIn("fsync", invariants.strip_rust_comments('let x = "fsync";'))
        self.assertIn("WAL", invariants.strip_rust_comments('execute("PRAGMA journal_mode = WAL")'))

    def test_a_block_comment_is_removed_even_when_it_spans_lines(self) -> None:
        stripped = invariants.strip_rust_comments("a /* fdatasync\n sync_data( */ b")
        self.assertNotIn("fdatasync", stripped)
        self.assertNotIn("sync_data", stripped)
        self.assertIn("a", stripped)
        self.assertIn("b", stripped)

    def test_an_escaped_quote_does_not_end_the_literal_early(self) -> None:
        stripped = invariants.strip_rust_comments('let x = "a\\"fsync"; let y = 1;')
        self.assertIn("fsync", stripped)
        self.assertIn("let y = 1;", stripped)

    def test_the_patterns_detect_what_they_name(self) -> None:
        import re

        self.assertTrue(re.search(invariants.FORBIDDEN_CODE["fsync"], "std::fsync(fd)?;"))
        self.assertTrue(re.search(invariants.FORBIDDEN_CODE["sync_all"], "file.sync_all()?;"))
        self.assertTrue(
            re.search(invariants.WAL_PATTERNS["wal_mode_literal"], 'PRAGMA journal_mode = WAL')
        )
        self.assertTrue(re.search(invariants.RETRY_PATTERNS["retry_call"], "retry(|| go())"))


class ScanTest(unittest.TestCase):
    def test_the_scan_covers_the_product_source(self) -> None:
        status = invariants.scan()
        self.assertGreater(status["files_scanned"], 0, "the scan found no product source")
        self.assertEqual(status["status"], "PASS", status["findings"])
        self.assertEqual(status["findings"], {})
        for token in ("fsync", "fdatasync", "sync_all", "sync_data", "non_zero_busy_timeout"):
            self.assertIn(token, status["checked"])

    def test_the_scan_reads_the_product_tree_not_the_harness(self) -> None:
        for path in invariants.production_sources():
            relative = str(path.relative_to(invariants.REPO_ROOT))
            self.assertTrue(relative.startswith("core/crates/"), relative)
            self.assertTrue(relative.endswith((".rs", ".sql")), relative)


class TripwireTest(unittest.TestCase):
    def test_no_stores_is_a_vacuous_pass_with_nothing_checked(self) -> None:
        result = invariants.tripwires([])
        self.assertEqual(result["stores_checked"], 0)
        self.assertEqual(result["status"], "PASS")
        self.assertEqual(result["findings"], [])

    def test_a_missing_store_is_skipped_rather_than_failing(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            result = invariants.tripwires([Path(directory) / "absent.sqlite"])
            self.assertEqual(result["stores_checked"], 0)
            self.assertEqual(result["status"], "PASS")

    def test_a_clean_store_passes_every_tripwire(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = make_store(
                directory,
                "clean.sqlite",
                [
                    "CREATE TABLE store_policy (watermark INTEGER)",
                    "INSERT INTO store_policy (watermark) VALUES (1)",
                ],
            )
            result = invariants.tripwires([path])
            self.assertEqual(result["stores_checked"], 1)
            self.assertEqual(result["status"], "PASS", result["findings"])
            self.assertEqual(result["stores"][0]["sidecars"], [])
            self.assertEqual(result["stores"][0]["store_policy_rows"], 1)
            self.assertNotEqual(
                str(result["stores"][0].get("journal_mode_persisted", "")).lower(), "wal"
            )

    def test_a_sidecar_is_caught(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = make_store(
                directory,
                "wal.sqlite",
                [
                    "CREATE TABLE store_policy (watermark INTEGER)",
                    "INSERT INTO store_policy (watermark) VALUES (1)",
                ],
            )
            Path(f"{path}-wal").write_bytes(b"")
            result = invariants.tripwires([path])
            self.assertEqual(result["status"], "FAIL")
            self.assertEqual(result["stores"][0]["sidecars"], ["-wal"])
            self.assertTrue(any("sidecar" in finding for finding in result["findings"]))

    def test_a_second_watermark_row_is_caught(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = make_store(
                directory,
                "twowatermarks.sqlite",
                [
                    "CREATE TABLE store_policy (watermark INTEGER)",
                    "INSERT INTO store_policy (watermark) VALUES (1)",
                    "INSERT INTO store_policy (watermark) VALUES (2)",
                ],
            )
            result = invariants.tripwires([path])
            self.assertEqual(result["status"], "FAIL")
            self.assertEqual(result["stores"][0]["store_policy_rows"], 2)
            self.assertTrue(any("expected one" in finding for finding in result["findings"]))

    def test_a_store_without_store_policy_is_caught(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = make_store(directory, "no-policy.sqlite", ["CREATE TABLE other (a INTEGER)"])
            result = invariants.tripwires([path])
            self.assertEqual(result["status"], "FAIL")
            self.assertIn("sqlite_error", result["stores"][0])


if __name__ == "__main__":
    unittest.main()
