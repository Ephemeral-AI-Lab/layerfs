#!/usr/bin/env python3
"""Residency, de-warm and the ordered invalidation sequence.

The Python side needs this primitive in its own right: the runner de-warms a
prepared artifact **between arms**, before the measured child starts, and the
child then attests what it sees. One implementation in each language would be two
chances to disagree, so this module and `src/support/instruments.rs` implement the
same three rules and `self_check` proves this one behaves:

    mincore -> msync(MS_INVALIDATE) only if resident_first > 0 -> mincore

**Never touch-every-page.** Reading the whole file to "flush" it is why v0.1.6
paid a measured 18.57 s for a 100k-file fixture, and it warms the very pages it
claims to evict.

`mincore` cannot distinguish a cache-served read from a device read. That is what
`disk_read_bytes` is for, and why a row claiming cold or de-warmed needs both.
"""

from __future__ import annotations

import ctypes
import os
import sys
from dataclasses import dataclass
from pathlib import Path

MS_INVALIDATE = 0x0002
PROT_READ = 0x1
MAP_SHARED = 0x0001

MAP_FAILED = ctypes.c_void_p(-1).value

_libc = ctypes.CDLL(None, use_errno=True)
_libc.mmap.argtypes = [
    ctypes.c_void_p,
    ctypes.c_size_t,
    ctypes.c_int,
    ctypes.c_int,
    ctypes.c_int,
    ctypes.c_long,
]
_libc.mmap.restype = ctypes.c_void_p
_libc.munmap.argtypes = [ctypes.c_void_p, ctypes.c_size_t]
_libc.munmap.restype = ctypes.c_int
_libc.mincore.argtypes = [ctypes.c_void_p, ctypes.c_size_t, ctypes.c_char_p]
_libc.mincore.restype = ctypes.c_int
_libc.msync.argtypes = [ctypes.c_void_p, ctypes.c_size_t, ctypes.c_int]
_libc.msync.restype = ctypes.c_int


@dataclass(frozen=True)
class Residency:
    """One residency reading, with its own page size."""

    length_bytes: int
    page_size_bytes: int
    total_pages: int
    resident_pages: int

    @property
    def is_dewarmed(self) -> bool:
        return self.resident_pages == 0

    def as_fields(self) -> dict[str, int]:
        return {
            "length_bytes": self.length_bytes,
            "page_size_bytes": self.page_size_bytes,
            "total_pages": self.total_pages,
            "resident_pages": self.resident_pages,
        }


def page_size() -> int:
    """Host page size, read rather than assumed.

    An Apple Silicon host uses 16 KiB pages; a hardcoded 4 KiB would size the
    `mincore` vector wrongly and misreport residency.
    """
    return os.sysconf("SC_PAGESIZE")


def _map(path: Path, size: int, writable: bool) -> int:
    """Maps `path` through `libc.mmap` and returns the address.

    `libc` is called directly rather than through `mmap.mmap` because
    [`ctypes`] cannot take the address of a read-only buffer: `from_buffer`
    requires a writable one, and mapping with `ACCESS_COPY` would make `mincore`
    describe a private copy instead of the file whose pages we are asking about.
    """
    flags = os.O_RDWR if writable else os.O_RDONLY
    descriptor = os.open(path, flags)
    try:
        address = _libc.mmap(
            None, size, PROT_READ, MAP_SHARED, descriptor, 0
        )
    finally:
        os.close(descriptor)
    if address in (None, MAP_FAILED):
        error = ctypes.get_errno()
        raise OSError(error, f"mmap failed for {path}")
    return address


def _unmap(address: int, size: int) -> None:
    _libc.munmap(ctypes.c_void_p(address), ctypes.c_size_t(size))


def residency(path: str | Path) -> Residency:
    """Reads residency of `path` through an `mmap` and `mincore`."""
    path = Path(path)
    size = path.stat().st_size
    page = page_size()
    if size == 0:
        return Residency(0, page, 0, 0)
    address = _map(path, size, writable=False)
    try:
        total_pages = (size + page - 1) // page
        vector = ctypes.create_string_buffer(total_pages)
        status = _libc.mincore(
            ctypes.c_void_p(address), ctypes.c_size_t(size), vector
        )
        if status != 0:
            error = ctypes.get_errno()
            raise OSError(error, f"mincore failed for {path}")
        resident = sum(1 for byte in vector.raw[:total_pages] if byte & 1)
        return Residency(size, page, total_pages, resident)
    finally:
        _unmap(address, size)


@dataclass(frozen=True)
class DeWarmReport:
    """The ordered sequence's outcome, including the case where it did nothing."""

    resident_first: int
    resident_after: int
    total_pages: int
    invalidated: bool

    @property
    def dewarmed(self) -> bool:
        return self.resident_after == 0

    def as_fields(self) -> dict[str, object]:
        return {
            "resident_first": self.resident_first,
            "resident_after": self.resident_after,
            "total_pages": self.total_pages,
            "invalidated": self.invalidated,
        }


def de_warm(path: str | Path) -> DeWarmReport:
    """Runs the ordered de-warm sequence and reports what it measured."""
    path = Path(path)
    first = residency(path)
    if first.resident_pages == 0 or first.length_bytes == 0:
        return DeWarmReport(0, 0, first.total_pages, False)
    address = _map(path, first.length_bytes, writable=True)
    try:
        status = _libc.msync(
            ctypes.c_void_p(address),
            ctypes.c_size_t(first.length_bytes),
            ctypes.c_int(MS_INVALIDATE),
        )
        if status != 0:
            error = ctypes.get_errno()
            raise OSError(error, f"msync(MS_INVALIDATE) failed for {path}")
    finally:
        _unmap(address, first.length_bytes)
    after = residency(path)
    return DeWarmReport(
        first.resident_pages, after.resident_pages, first.total_pages, True
    )


def self_check() -> list[str]:
    """Proves the primitive works on this host, and returns failures.

    A primitive that cannot demonstrate itself on a file this process just wrote
    and then read is not evidence about anything else, so the check is real:
    write a page-sized file, read it (making it resident), de-warm it and require
    zero resident pages.
    """
    import tempfile

    failures: list[str] = []
    page = page_size()
    if page <= 0:
        return ["page size is not positive"]
    with tempfile.TemporaryDirectory() as directory:
        path = Path(directory) / "residency-probe.bin"
        payload = bytes(range(256)) * (page // 256 + 1)
        path.write_bytes(payload[: page * 4])
        # Touch every page so the probe starts resident; this is the *check*, not
        # a de-warm strategy, and it is never applied to a measured artifact.
        _ = path.read_bytes()
        before = residency(path)
        if before.resident_pages == 0:
            failures.append(
                "a freshly written and read file reports zero resident pages: "
                "the instrument cannot see what it is measuring"
            )
        report = de_warm(path)
        if report.resident_first == 0:
            failures.append("de-warm saw nothing resident, so it proves nothing")
        if not report.invalidated:
            failures.append("de-warm did not issue msync(MS_INVALIDATE)")
        if report.resident_after != 0:
            failures.append(
                f"de-warm left {report.resident_after} of {report.total_pages} pages resident"
            )
    return failures


def main() -> int:
    failures = self_check()
    for failure in failures:
        print(f"residency: FAIL: {failure}", file=sys.stderr)
    if failures:
        return 1
    print(f"residency: PASS (page size {page_size()} bytes)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
