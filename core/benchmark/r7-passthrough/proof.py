#!/usr/bin/env python3
"""Bounded external filesystem oracle; lead owns mount, unmount and receipts."""
import argparse
import ctypes
import json
import os
from pathlib import Path
import stat


def directory_inodes(path):
    """Observe native directory entries including dots, without guessing IDs."""
    if ctypes.sizeof(ctypes.c_long) != 8:
        raise RuntimeError('the registered Linux 64-bit dirent ABI is required')

    class Dirent(ctypes.Structure):
        _fields_ = [('inode', ctypes.c_ulonglong), ('offset', ctypes.c_longlong),
                    ('record_bytes', ctypes.c_ushort), ('kind', ctypes.c_ubyte),
                    ('name', ctypes.c_char * 256)]

    libc = ctypes.CDLL(None, use_errno=True)
    libc.opendir.argtypes = [ctypes.c_char_p]
    libc.opendir.restype = ctypes.c_void_p
    libc.readdir.argtypes = [ctypes.c_void_p]
    libc.readdir.restype = ctypes.POINTER(Dirent)
    libc.closedir.argtypes = [ctypes.c_void_p]
    libc.closedir.restype = ctypes.c_int
    owner = libc.opendir(os.fsencode(path))
    if not owner:
        code = ctypes.get_errno()
        raise OSError(code, os.strerror(code), path)
    try:
        result = {}
        while True:
            ctypes.set_errno(0)
            item = libc.readdir(owner)
            if not item:
                code = ctypes.get_errno()
                if code:
                    raise OSError(code, os.strerror(code), path)
                return result
            row = item.contents
            result[os.fsdecode(row.name)] = row.inode
    finally:
        if libc.closedir(owner):
            code = ctypes.get_errno()
            raise OSError(code, os.strerror(code), path)


def facts(path):
    info = path.stat()
    return {'inode': info.st_ino, 'mode': stat.S_IMODE(info.st_mode),
            'uid': info.st_uid, 'gid': info.st_gid, 'links': info.st_nlink,
            'size': info.st_size, 'mtime_ns': info.st_mtime_ns, 'ctime_ns': info.st_ctime_ns}


def mutation(mount):
    root = mount / 'r7-passthrough-proof'
    root.mkdir()
    dots = directory_inodes(root)
    assert dots['.'] == root.stat().st_ino
    assert dots['..'] == mount.stat().st_ino
    source = root / 'one'
    source.mkdir()
    destination = root / 'two'
    destination.mkdir()
    path = source / 'bytes'
    payload = bytes(range(256)) * 513
    with path.open('xb') as stream:
        stream.write(payload)
    assert path.read_bytes() == payload
    fd = os.open(path, os.O_RDWR)
    try:
        # Requests through the mount; P acknowledges without a backend sync.
        os.fsync(fd)
        assert os.pwrite(fd, b'changed', 17) == 7
        os.ftruncate(fd, 100)
        os.ftruncate(fd, 200)
        expected = payload[:17] + b'changed' + payload[24:100] + b'\0' * 100
        assert os.pread(fd, 200, 0) == expected
    finally:
        os.close(fd)
    directory = os.open(source, os.O_RDONLY | os.O_DIRECTORY)
    try:
        os.fsync(directory)
    finally:
        os.close(directory)
    hard = source / 'hard'
    os.link(path, hard)
    assert path.stat().st_ino == hard.stat().st_ino
    assert path.stat().st_nlink == 2
    renamed = source / 'renamed'
    path.rename(renamed)
    assert not path.exists()
    symlink = source / 'link'
    symlink.symlink_to('renamed')
    assert os.readlink(symlink) == 'renamed'
    assert symlink.read_bytes() == expected
    source.rename(destination / 'moved')
    moved = destination / 'moved'
    assert sorted(p.name for p in moved.iterdir()) == ['hard', 'link', 'renamed']
    orphan = root / 'orphan'
    orphan.write_bytes(b'held after unlink')
    fd = os.open(orphan, os.O_RDONLY)
    try:
        orphan.unlink()
        assert os.pread(fd, 64, 0) == b'held after unlink'
        assert not orphan.exists()
    finally:
        os.close(fd)
    (moved / 'renamed').chmod(0o640)
    info = facts(moved / 'renamed')
    assert info['mode'] == 0o640
    assert info['uid'] == os.geteuid()
    assert info['links'] == 2
    assert (moved / 'hard').read_bytes() == expected
    return {'facts': info, 'expected_hex': expected.hex(),
            'relative': 'r7-passthrough-proof/two/moved/renamed'}


def remount(mount, original):
    path = mount / original['relative']
    assert path.read_bytes() == bytes.fromhex(original['expected_hex'])
    assert facts(path) == original['facts']
    assert path.stat().st_ino == (path.parent / 'hard').stat().st_ino
    assert (path.parent / 'link').read_bytes() == path.read_bytes()
    assert not (mount / 'r7-passthrough-proof/orphan').exists()
    return {'remount_facts': facts(path), 'stable_identity': True}


def wide(mount):
    """Counts/functional diagnostic at three sizes; no timing claim."""
    root = mount / 'r7-passthrough-wide-proof'
    root.mkdir()
    rows = []
    for count in (64, 128, 256):
        directory = root / str(count)
        directory.mkdir()
        names = {f'entry-{index:04d}-' + 'x' * 200 for index in range(count)}
        for name in sorted(names):
            (directory / name).touch(exist_ok=False)
        opened = directory_inodes(directory)
        assert set(opened) == names | {'.', '..'}
        assert opened['.'] == directory.stat().st_ino
        assert opened['..'] == root.stat().st_ino
        assert len(set(opened[name] for name in names)) == count
        rows.append({'files': count, 'returned_entries': len(opened),
                     'name_bytes': sum(len(name) for name in names)})
    return {'sizes': rows, 'shape': 'wide directory, 211-byte names',
            'timing': 'NOT_MEASURED', 'backend_buffer': 'bounded libc DIR buffer'}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--mount', required=True, type=Path)
    parser.add_argument('--stage', choices=['mutation', 'remount', 'wide'], required=True)
    parser.add_argument('--original', type=Path)
    args = parser.parse_args()
    mount = args.mount.resolve()
    if args.stage == 'mutation':
        row = mutation(mount)
    elif args.stage == 'remount':
        if args.original is None:
            parser.error('--original mutation JSON required for remount')
        row = remount(mount, json.loads(args.original.read_text()))
    else:
        row = wide(mount)
    print(json.dumps({'status': 'PASS', 'scope': 'scoped external filesystem oracle',
                      'stage': args.stage, **row}, sort_keys=True))


if __name__ == '__main__':
    main()
