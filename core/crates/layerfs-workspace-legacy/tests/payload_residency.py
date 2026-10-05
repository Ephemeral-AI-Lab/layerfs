#!/usr/bin/env python3
"""Inspect payload-inode residency without reading or touching their data pages.

This is an external Linux functional oracle, not an RSS/cgroup or storage-speed
measurement. It maps PROT_NONE, calls mincore and immediately unmaps each file.
"""
import ctypes
import json
import os
from pathlib import Path
import sys

libc = ctypes.CDLL(None, use_errno=True)
libc.mmap.argtypes = [ctypes.c_void_p, ctypes.c_size_t, ctypes.c_int,
                      ctypes.c_int, ctypes.c_int, ctypes.c_long]
libc.mmap.restype = ctypes.c_void_p
libc.mincore.argtypes = [ctypes.c_void_p, ctypes.c_size_t, ctypes.c_void_p]
libc.munmap.argtypes = [ctypes.c_void_p, ctypes.c_size_t]
page_size = os.sysconf('SC_PAGE_SIZE')
files = pages = resident = allocated = 0
for path in sorted(Path(sys.argv[1]).iterdir()):
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW)
    try:
        stat = os.fstat(fd)
        assert stat.st_size > 0 and stat.st_nlink == 1
        count = (stat.st_size + page_size - 1) // page_size
        address = libc.mmap(None, stat.st_size, 0, 1, fd, 0)
        if address == ctypes.c_void_p(-1).value:
            raise OSError(ctypes.get_errno(), 'PROT_NONE mmap')
        try:
            vector = (ctypes.c_ubyte * count)()
            if libc.mincore(address, stat.st_size, vector):
                raise OSError(ctypes.get_errno(), 'mincore')
            resident += sum(byte & 1 for byte in vector)
            pages += count
            allocated += stat.st_blocks * 512
            files += 1
        finally:
            if libc.munmap(address, stat.st_size):
                raise OSError(ctypes.get_errno(), 'munmap')
    finally:
        os.close(fd)
print(json.dumps({'files': files, 'pages': pages, 'resident_pages': resident,
                  'allocated_bytes': allocated, 'page_size': page_size,
                  'phase': sys.argv[2] if len(sys.argv) > 2 else 'observation',
                  'scope': 'private payload inodes; not cgroup/RSS/global filesystem metadata'}))
assert files and resident == 0, 'direct-I/O payload pages unexpectedly resident'
