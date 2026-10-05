#!/usr/bin/env python3
"""Check the read-only native lock observer against a separate lock-owning process.

Usage: check_store_lock.py <compiled store_lock.c library> <fresh output.json>
This validates the observer only; it is not evidence of a C2 save.
"""
import ctypes
import fcntl
import hashlib
import json
import multiprocessing
import os
from pathlib import Path
import sys
import tempfile


def hold(path, pipe):
    with path.open('r+b') as stream:
        fcntl.lockf(stream, fcntl.LOCK_EX, 1, (1 << 30) + 1)
        pipe.send(os.getpid()); pipe.recv()
        fcntl.lockf(stream, fcntl.LOCK_UN, 1, (1 << 30) + 1)
        pipe.send('released')


def main():
    library, output = map(Path, sys.argv[1:])
    assert not output.exists()
    observer = ctypes.CDLL(str(library.resolve())).layerfs_reserved_lock_owner
    observer.argtypes = [ctypes.c_char_p]; observer.restype = ctypes.c_int
    with tempfile.TemporaryDirectory(dir=output.parent) as directory:
        path = Path(directory) / 'lock'; path.touch()
        parent, child = multiprocessing.Pipe()
        process = multiprocessing.get_context('fork').Process(target=hold, args=(path, child))
        process.start()
        try:
            assert parent.poll(5); pid = parent.recv()
            observed = observer(os.fsencode(path)); assert observed == pid
            parent.send('release'); assert parent.poll(5) and parent.recv() == 'released'
            released = observer(os.fsencode(path)); assert released == 0
            process.join(5); assert process.exitcode == 0
        finally:
            if process.is_alive(): process.kill(); process.join(5)
    output.write_text(json.dumps({'status': 'PASS', 'scope': 'observer only; not a C2 proof',
        'library_sha256': hashlib.sha256(library.read_bytes()).hexdigest(),
        'observer_pid': observed, 'holder_pid': pid, 'after_release': released}, indent=2) + '\n')


if __name__ == '__main__': main()
