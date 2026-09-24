"""Declared cache acquisition for the two #232 data domains.

`macos-store` is the case's independent Store copy and history catalog on the
host. Its pages are invalidated with a shared read-only mapping plus
`msync(MS_INVALIDATE)`, and residency is then read with `mincore`, which reports
page residency without faulting data pages in.

`linux-fuse-backing` is the per-case sandbox's backing and spool inside the
container. No host-side or product-side invalidation exists for it, and the
contract forbids a benchmark-only eviction between Edit and Commit, so the domain
is recorded as declared-warm and the row cannot claim a cold-qualified PASS.
"""
import ctypes
import math
import os
import platform

CONTRACT = "exec-fuse-edit-cache-v1"
MACOS_METHOD = "darwin-mmap-msync-invalidate-mincore-v1"
LINUX_METHOD = "declared-warm-not-invalidatable-v1"


class Residency:
    """Darwin shared-mapping invalidation and non-faulting residency read."""

    def __init__(self):
        if platform.system() != "Darwin":
            raise OSError("Store-domain residency verification requires macOS")
        self.page_size = os.sysconf("SC_PAGE_SIZE")
        self.lib = ctypes.CDLL(None, use_errno=True)
        self.lib.mmap.restype = ctypes.c_void_p
        self.lib.mmap.argtypes = [ctypes.c_void_p, ctypes.c_size_t, ctypes.c_int,
                                  ctypes.c_int, ctypes.c_int, ctypes.c_int64]
        self.lib.msync.argtypes = [ctypes.c_void_p, ctypes.c_size_t, ctypes.c_int]
        self.lib.mincore.argtypes = [ctypes.c_void_p, ctypes.c_size_t, ctypes.c_void_p]
        self.lib.munmap.argtypes = [ctypes.c_void_p, ctypes.c_size_t]

    def check(self, fd, size, *, evict=False):
        if not size:
            return 0, 0
        # PROT_READ=1, MAP_SHARED=1, MS_SYNC=2, MS_INVALIDATE=0x10: Darwin SDK values.
        address = self.lib.mmap(None, size, 1, 1, fd, 0)
        if address == ctypes.c_void_p(-1).value:
            raise OSError(ctypes.get_errno(), "store mmap")
        pages = math.ceil(size / self.page_size)
        try:
            if evict:
                for offset in range(0, size, self.page_size):
                    ctypes.c_ubyte.from_address(address + offset).value
                if self.lib.msync(address, size, 0x10 | 0x2):
                    raise OSError(ctypes.get_errno(), "store msync")
            vector = (ctypes.c_ubyte * pages)()
            if self.lib.mincore(address, size, vector):
                raise OSError(ctypes.get_errno(), "store mincore")
            return pages, sum(bool(value & 1) for value in vector)
        finally:
            if self.lib.munmap(address, size):
                raise OSError(ctypes.get_errno(), "store munmap")


def qualify_store(paths):
    """Invalidates and checks the whole macOS Store domain for one case."""
    receipt = {"domain": "macos-store", "method": MACOS_METHOD, "contract": CONTRACT,
               "status": "UNVERIFIED", "files": {}, "pages_checked": 0,
               "resident_pages_before": 0, "resident_pages": 0}
    try:
        backend = Residency()
    except OSError as error:
        receipt["status"] = "INELIGIBLE"
        receipt["reason"] = f"residency backend unavailable: {error}"
        return receipt
    for path in paths:
        with open(path, "rb") as source:
            size = os.fstat(source.fileno()).st_size
            before, _ = backend.check(source.fileno(), size, evict=True)
            pages, resident = backend.check(source.fileno(), size)
            if pages != before:
                receipt["status"] = "INELIGIBLE"
                receipt["reason"] = "page count changed between passes"
                return receipt
            receipt["files"][str(path)] = {"bytes": size, "pages": pages,
                                           "resident_pages": resident}
            receipt["pages_checked"] += pages
            receipt["resident_pages"] += resident
    receipt["status"] = "PASS" if receipt["resident_pages"] == 0 else "INELIGIBLE"
    if receipt["status"] != "PASS":
        receipt["reason"] = "store pages remained resident after invalidation"
    return receipt


def qualify_fuse_backing():
    """Records the container domain the contract cannot invalidate or check."""
    return {
        "domain": "linux-fuse-backing",
        "method": LINUX_METHOD,
        "contract": CONTRACT,
        "status": "INELIGIBLE",
        "reason": ("the per-case sandbox creates a fresh backing directory and spool inside the "
                   "container; no host-side or product-side invalidation exists, and a "
                   "benchmark-only eviction between Edit and Commit is forbidden, so Commit may "
                   "read the bytes Edit just wrote from resident backing pages"),
    }
