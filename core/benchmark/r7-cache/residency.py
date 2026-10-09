#!/usr/bin/env python3
"""Per-file Linux content residency; an eviction hint never proves coldness.

Mappings are never dereferenced. Both address mappings and mincore vectors use
fixed windows. This observes file-content pages, not kernel metadata caches.
"""
import argparse
import ctypes
import errno
import json
import os
from pathlib import Path
import platform
import stat
import time

WINDOW_BYTES = 16 * 1024 * 1024
C_LIFETIME_NS = 60_000_000_000
PROTECTED = (
    Path('/Users/yifanxu/Ephemeral-AI-Lab/deepseek-harness'),
    Path('/var/folders/s4/xpkmz7wn6yq97w1ls_4f_dfc0000gn/T/'
         'layerfs-installed-56560-full-native-handoff'),
)


def identity(info):
    return {'device': info.st_dev, 'inode': info.st_ino,
            'logical_bytes': info.st_size, 'allocated_bytes': info.st_blocks * 512,
            'mtime_ns': info.st_mtime_ns, 'ctime_ns': info.st_ctime_ns}


def validate_path(path):
    path = Path(path).absolute()
    resolved = path.resolve()
    if any(resolved == root or root in resolved.parents for root in PROTECTED):
        raise ValueError('protected source/path must never be inspected by this helper')
    return path


def inspect_file(path, evict=False, *, open_scope=None, expected_identity=None):
    """One eviction attempt and one bounded mincore pass, with stable identity."""
    if platform.system() != 'Linux':
        raise RuntimeError('Linux posix_fadvise/mincore observer unavailable on this platform')
    path = validate_path(path)
    parent_fd = leaf = None
    if open_scope is None:
        fd = os.open(path, os.O_RDONLY | os.O_CLOEXEC | os.O_NOFOLLOW | os.O_NONBLOCK)
    else:
        fd, parent_fd, leaf = open_scope.open(path)
    try:
        before = os.fstat(fd)
        if not stat.S_ISREG(before.st_mode):
            raise ValueError('regular file required')
        if expected_identity is not None and identity(before) != expected_identity:
            raise ValueError(f'sealed file identity differs before eviction: actual={identity(before)} expected={expected_identity}')
        if evict:
            # A dirty page may remain resident. No fsync and no global drop.
            os.posix_fadvise(fd, 0, 0, os.POSIX_FADV_DONTNEED)
        page_size = os.sysconf('SC_PAGE_SIZE')
        window = WINDOW_BYTES // page_size * page_size
        if window <= 0:
            raise ValueError('page size exceeds bounded observer window')
        libc = ctypes.CDLL(None, use_errno=True)
        libc.mmap.argtypes = (ctypes.c_void_p, ctypes.c_size_t, ctypes.c_int,
                              ctypes.c_int, ctypes.c_int, ctypes.c_longlong)
        libc.mmap.restype = ctypes.c_void_p
        libc.mincore.argtypes = (ctypes.c_void_p, ctypes.c_size_t, ctypes.c_void_p)
        libc.mincore.restype = ctypes.c_int
        libc.munmap.argtypes = (ctypes.c_void_p, ctypes.c_size_t)
        libc.munmap.restype = ctypes.c_int
        resident = pages = maps = 0
        for offset in range(0, before.st_size, window):
            length = min(window, before.st_size - offset)
            count = (length + page_size - 1) // page_size
            vector = (ctypes.c_ubyte * count)()
            address = libc.mmap(None, length, 0, 1, fd, offset)  # PROT_NONE, MAP_SHARED
            if address == ctypes.c_void_p(-1).value:
                code = ctypes.get_errno()
                raise OSError(code, os.strerror(code), path)
            try:
                if libc.mincore(address, length, vector) != 0:
                    code = ctypes.get_errno()
                    raise OSError(code, os.strerror(code), path)
                resident += sum(byte & 1 for byte in vector)
                pages += count
                maps += 1
            finally:
                if libc.munmap(address, length) != 0:
                    code = ctypes.get_errno()
                    raise OSError(code, os.strerror(code), path)
            # Release this fixed vector before allocating the next window.
            # Assignment otherwise allocates a replacement while the old
            # vector is still live, doubling its stated peak.
            del vector
        after = os.fstat(fd)
        current = (os.stat(path, follow_symlinks=False) if parent_fd is None
                   else os.stat(leaf, dir_fd=parent_fd, follow_symlinks=False))
        if identity(before) != identity(after) or identity(after) != identity(current):
            raise ValueError('file identity changed during residency attestation')
        return {'path': str(path), 'present': True, **identity(after),
                'total_pages': pages, 'resident_pages': resident,
                'page_size_bytes': page_size, 'mmap_calls': maps,
                'mincore_calls': maps, 'munmap_calls': maps,
                'eviction_hint_attempts': int(evict),
                'observer_window_bytes': window,
                'vector_peak_bytes': min(pages, window // page_size)}
    finally:
        os.close(fd)
        if parent_fd is not None:
            os.close(parent_fd)


def attest(paths, *, evict=False, optional=()):
    """Observe explicit files and absent optional SQLite sidecars, without reads."""
    optional = {str(validate_path(path)) for path in optional}
    rows = []
    seen = set()
    for path in paths:
        path = validate_path(path)
        try:
            row = inspect_file(path, evict)
        except FileNotFoundError:
            if str(path) not in optional:
                raise
            row = {'path': str(path), 'present': False, 'resident_pages': 0}
        if row['present']:
            key = row['device'], row['inode']
            if key in seen:
                raise ValueError('duplicate inode in residency scope')
            seen.add(key)
        rows.append(row)
    return {'files': rows, 'resident_pages': sum(row['resident_pages'] for row in rows),
            'method': 'one per-file DONTNEED hint then bounded mincore' if evict
                      else 'bounded per-file mincore without eviction',
            'scope': 'content pages only; metadata, SQLite and daemon caches separately declared',
            'observed_monotonic_ns': time.monotonic_ns(), 'payload_bytes_read': 0}


def class_status(cache_class, receipt, *, object_demands=None, warm_interval_ns=None):
    """Post-phase class validation; B demands must come from the actual phase."""
    if cache_class == 'A':
        return 'ELIGIBLE' if receipt['resident_pages'] == 0 else 'INELIGIBLE'
    if cache_class == 'B':
        if object_demands is None:
            return 'PENDING_PHASE_COUNTERS'
        return 'ELIGIBLE' if object_demands == 0 else 'INELIGIBLE'
    if cache_class == 'C':
        if warm_interval_ns is None:
            return 'PENDING_WARMUP_BOUNDARY'
        return 'ELIGIBLE' if 0 <= warm_interval_ns < C_LIFETIME_NS else 'INELIGIBLE'
    raise ValueError('cache class must be A, B or C')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--class', choices=['A', 'B', 'C'], dest='cache_class', required=True)
    parser.add_argument('--store', type=Path)
    parser.add_argument('--overlay', type=Path)
    parser.add_argument('--file', type=Path, action='append', default=[])
    parser.add_argument('--file-manifest', type=Path)
    parser.add_argument('--file-manifest-sha256')
    parser.add_argument('--root', type=Path)
    parser.add_argument('--inventory-output', type=Path)
    parser.add_argument('--allow-declared-aliases', action='store_true')
    parser.add_argument('--object-demands', type=int)
    parser.add_argument('--warmup-ended-monotonic-ns', type=int)
    args = parser.parse_args()
    paths = list(args.file)
    optional = []
    for database in (args.store, args.overlay):
        if database is not None:
            paths.append(database)
            sidecars = [Path(str(database) + suffix) for suffix in ('-wal', '-shm', '-journal')]
            paths.extend(sidecars)
            optional.extend(sidecars)
    if args.file_manifest:
        if paths or not all((args.file_manifest_sha256, args.root, args.inventory_output)):
            parser.error('manifest mode requires sha256/root/inventory-output and no scalar input paths')
    elif any((args.file_manifest_sha256, args.root, args.inventory_output, args.allow_declared_aliases)):
        parser.error('manifest-only options require --file-manifest')
    elif not paths:
        parser.error('at least one --store, --overlay or --file is required')
    try:
        if args.file_manifest:
            from stream_manifest import attest_manifest
            receipt = attest_manifest(args.file_manifest, args.file_manifest_sha256,
                                      args.root, args.inventory_output,
                                      inspect_file=inspect_file, validate_path=validate_path,
                                      evict=args.cache_class == 'A',
                                      allow_aliases=args.allow_declared_aliases)
        else:
            receipt = attest(paths, evict=args.cache_class == 'A', optional=optional)
        interval = (time.monotonic_ns() - args.warmup_ended_monotonic_ns
                    if args.warmup_ended_monotonic_ns is not None else None)
        status = class_status(args.cache_class, receipt,
                              object_demands=args.object_demands, warm_interval_ns=interval)
        print(json.dumps({'cache_class': args.cache_class, 'status': status,
                          'attempts': 0, 'warm_interval_ns': interval,
                          'exploratory': True, 'admission_eligible': False, **receipt}, sort_keys=True))
        return 3 if status == 'INELIGIBLE' else 0
    except (OSError, ValueError, RuntimeError, AttributeError, TypeError) as error:
        print(json.dumps({'cache_class': args.cache_class, 'status': 'UNAVAILABLE',
                          'attempts': 0, 'error': str(error),
                          **getattr(error, 'receipt', {}),
                          'errno': getattr(error, 'errno', errno.ENOSYS)}, sort_keys=True))
        return 4


if __name__ == '__main__':
    raise SystemExit(main())
