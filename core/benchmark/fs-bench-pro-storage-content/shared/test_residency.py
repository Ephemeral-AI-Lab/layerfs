#!/usr/bin/env python3
"""`unittest` entry points for `residency.py`, wrapping its `self_check`.

A residency instrument that cannot see a page this process just made resident is
reporting a property of the instrument, not of the row. So every check here makes
the condition real first — write a file, read it, then ask — and the de-warm is
required to *say* whether it invalidated anything, so "nothing was resident" can
never be mistaken for "the eviction worked".
"""

from __future__ import annotations

import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import residency


def probe(directory: str, pages: int = 4, name: str = "probe.bin") -> tuple[Path, bytes]:
    """Writes a file of `pages` pages and reads it back, making it resident."""
    page = residency.page_size()
    total = page * pages
    # A repeating 256-byte pattern, cut to exactly `pages` pages. The repeat count
    # has to scale with the request, or the slice below silently returns less.
    payload = (bytes(range(256)) * (total // 256 + 1))[:total]
    assert len(payload) == total
    path = Path(directory) / name
    path.write_bytes(payload)
    # The read is the *check*, not a de-warm strategy, and is never applied to a
    # measured artifact.
    _ = path.read_bytes()
    return path, payload


class SelfCheckTest(unittest.TestCase):
    def test_the_module_self_check_is_clean(self) -> None:
        self.assertEqual(residency.self_check(), [])


class PageSizeTest(unittest.TestCase):
    def test_the_page_size_is_read_rather_than_assumed(self) -> None:
        page = residency.page_size()
        self.assertGreater(page, 0)
        self.assertEqual(page & (page - 1), 0, f"{page} is not a power of two")
        self.assertGreaterEqual(page, 4096)
        self.assertLessEqual(page, 65536)


class ReadingTest(unittest.TestCase):
    def test_a_freshly_read_file_shows_resident_pages(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path, payload = probe(directory)
            reading = residency.residency(path)
            self.assertEqual(reading.length_bytes, len(payload))
            self.assertEqual(reading.page_size_bytes, residency.page_size())
            self.assertEqual(reading.total_pages, 4)
            self.assertGreater(
                reading.resident_pages,
                0,
                "the instrument cannot see a file this process just read",
            )
            self.assertFalse(reading.is_dewarmed)

    def test_the_fields_are_reported_separately(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path, _ = probe(directory)
            fields = residency.residency(path).as_fields()
            self.assertEqual(
                sorted(fields),
                ["length_bytes", "page_size_bytes", "resident_pages", "total_pages"],
            )

    def test_a_zero_length_file_is_resident_by_definition(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "empty.bin"
            path.write_bytes(b"")
            reading = residency.residency(path)
            self.assertEqual(reading.length_bytes, 0)
            self.assertEqual(reading.total_pages, 0)
            self.assertEqual(reading.resident_pages, 0)
            self.assertTrue(reading.is_dewarmed)

    def test_a_missing_file_raises_rather_than_reporting_zero(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaises(OSError):
                residency.residency(Path(directory) / "absent.bin")


class DeWarmTest(unittest.TestCase):
    def test_de_warm_clears_what_it_found_and_says_so(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path, _ = probe(directory)
            report = residency.de_warm(path)
            self.assertGreater(report.resident_first, 0, "de-warm saw nothing resident")
            self.assertTrue(report.invalidated, "de-warm did not issue msync(MS_INVALIDATE)")
            self.assertEqual(report.resident_after, 0)
            self.assertTrue(report.dewarmed)
            self.assertTrue(residency.residency(path).is_dewarmed)

    def test_de_warm_on_a_zero_length_file_claims_nothing(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "empty.bin"
            path.write_bytes(b"")
            report = residency.de_warm(path)
            self.assertFalse(report.invalidated, "there was nothing to invalidate")
            self.assertEqual(report.resident_first, 0)
            self.assertEqual(report.resident_after, 0)
            self.assertTrue(report.dewarmed)

    def test_the_report_renders_the_fields_a_receipt_needs(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path, _ = probe(directory)
            fields = residency.de_warm(path).as_fields()
            self.assertEqual(
                sorted(fields),
                ["invalidated", "resident_after", "resident_first", "total_pages"],
            )

    def test_de_warm_is_not_a_touch_every_page(self) -> None:
        # The module's stated rule: reading the whole file to "flush" it warms the
        # very pages it claims to evict. A second de-warm must therefore see
        # nothing resident and issue no msync, because the first one left nothing.
        with tempfile.TemporaryDirectory() as directory:
            path, _ = probe(directory)
            residency.de_warm(path)
            second = residency.de_warm(path)
            self.assertFalse(second.invalidated)
            self.assertEqual(second.resident_first, 0)
            self.assertEqual(second.resident_after, 0)


if __name__ == "__main__":
    unittest.main()
