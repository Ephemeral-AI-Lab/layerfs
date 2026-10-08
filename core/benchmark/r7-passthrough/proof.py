#!/usr/bin/env python3
"""Bounded external filesystem oracle; lead owns mount, unmount and receipts."""
import argparse
import json
import os
from pathlib import Path
import stat


def facts(path):
    info = path.stat()
    return {'inode': info.st_ino, 'mode': stat.S_IMODE(info.st_mode),
            'uid': info.st_uid, 'gid': info.st_gid, 'links': info.st_nlink,
            'size': info.st_size, 'mtime_ns': info.st_mtime_ns, 'ctime_ns': info.st_ctime_ns}


def mutation(mount):
    root = mount / 'r7-passthrough-proof'
    root.mkdir()
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
        assert os.pwrite(fd, b'changed', 17) == 7
        os.ftruncate(fd, 100)
        os.ftruncate(fd, 200)
        expected = payload[:17] + b'changed' + payload[24:100] + b'\0' * 100
        assert os.pread(fd, 200, 0) == expected
    finally:
        os.close(fd)
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


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--mount', required=True, type=Path)
    parser.add_argument('--stage', choices=['mutation', 'remount'], required=True)
    parser.add_argument('--original', type=Path)
    args = parser.parse_args()
    mount = args.mount.resolve()
    if args.stage == 'mutation':
        row = mutation(mount)
    else:
        if args.original is None:
            parser.error('--original mutation JSON required for remount')
        row = remount(mount, json.loads(args.original.read_text()))
    print(json.dumps({'status': 'PASS', 'scope': 'scoped external filesystem oracle',
                      'stage': args.stage, **row}, sort_keys=True))


if __name__ == '__main__':
    main()
