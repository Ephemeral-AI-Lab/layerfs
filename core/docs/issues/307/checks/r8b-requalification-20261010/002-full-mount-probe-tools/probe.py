#!/usr/bin/env python3
"""Bounded read-only count probe of an installed full-fixture mount.

Exploratory and ineligible: no verdict, no latency admission, no oracle. It
separates per-entry from per-byte through-mount cost and serial from concurrent
reader service, using the registered comparator's own system-call pattern
(stat, open, fstat, 65536-byte reads to EOF, fstat, stat, close; readlink;
one directory stream per directory). Each phase has its own wall budget and
stops at a directory or file boundary; it reads disjoint names, so no phase
credits another phase's page or daemon-cache warmth for file payloads.
"""
import hashlib
import json
import os
import stat
import sys
import threading
import time
from concurrent.futures import ThreadPoolExecutor

WINDOW = 65536
LARGE = 4 << 20
TREE = "node_modules/.pnpm"
DIRECTORY = os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW | os.O_CLOEXEC
REGULAR = os.O_RDONLY | os.O_NOFOLLOW | os.O_CLOEXEC | os.O_NONBLOCK


def read_file(parent_fd, name, deadline, own_parent):
    total = 0
    try:
        descriptor = os.open(name, REGULAR, dir_fd=parent_fd)
        try:
            os.fstat(descriptor)
            digest = hashlib.sha256()
            while time.monotonic() < deadline:
                block = os.read(descriptor, WINDOW)
                if not block:
                    os.fstat(descriptor)
                    os.stat(name, dir_fd=parent_fd, follow_symlinks=False)
                    return total, True
                total += len(block)
                digest.update(block)
            return total, False
        finally:
            os.close(descriptor)
    finally:
        if own_parent:
            os.close(parent_fd)


class Counts:
    def __init__(self):
        self.lock = threading.Lock()
        self.value = dict(entries=0, directories=0, symlinks=0, files_complete=0,
                          files_partial=0, bytes=0, large_skipped=0, read_calls_min=0)

    def add(self, **fields):
        with self.lock:
            for key, amount in fields.items():
                self.value[key] += amount

    def file(self, outcome):
        total, complete = outcome
        self.add(bytes=total, read_calls_min=total // WINDOW + 1,
                 files_complete=int(complete), files_partial=int(not complete))


def tree(selected, deadline, counts, pool, slots):
    futures = []
    root = os.open(TREE, DIRECTORY)
    names = sorted(entry.name for entry in os.scandir(root))
    stack = []
    for index, name in enumerate(names):
        if index % 2 != selected or time.monotonic() >= deadline:
            continue
        info = os.stat(name, dir_fd=root, follow_symlinks=False)
        counts.add(entries=1)
        if not stat.S_ISDIR(info.st_mode):
            continue
        counts.add(directories=1)
        descriptor = os.open(name, DIRECTORY, dir_fd=root)
        stack.append((descriptor, os.scandir(descriptor)))
        while stack and time.monotonic() < deadline:
            parent_fd, iterator = stack[-1]
            entry = next(iterator, None)
            if entry is None:
                iterator.close()
                os.close(parent_fd)
                stack.pop()
                continue
            info = os.stat(entry.name, dir_fd=parent_fd, follow_symlinks=False)
            counts.add(entries=1)
            if stat.S_ISDIR(info.st_mode):
                counts.add(directories=1)
                child = os.open(entry.name, DIRECTORY, dir_fd=parent_fd)
                os.fstat(child)
                stack.append((child, os.scandir(child)))
            elif stat.S_ISLNK(info.st_mode):
                os.readlink(entry.name, dir_fd=parent_fd)
                os.stat(entry.name, dir_fd=parent_fd, follow_symlinks=False)
                counts.add(symlinks=1)
            elif info.st_size > LARGE:
                counts.add(large_skipped=1)
            elif pool is None:
                counts.file(read_file(parent_fd, entry.name, deadline, False))
            else:
                slots.acquire()
                future = pool.submit(read_file, os.dup(parent_fd), entry.name, deadline, True)
                future.add_done_callback(lambda done: slots.release())
                futures.append(future)
        while stack:
            parent_fd, iterator = stack.pop()
            iterator.close()
            os.close(parent_fd)
    os.close(root)
    for future in futures:
        counts.file(future.result())


def large(paths, deadline, counts, pool):
    def one(path):
        parent = os.open(os.path.dirname(path), DIRECTORY)
        return read_file(parent, os.path.basename(path), deadline, True)
    if pool is None:
        for path in paths:
            counts.file(one(path))
    else:
        for outcome in pool.map(one, paths):
            counts.file(outcome)


def main():
    phase, seconds = sys.argv[1], float(sys.argv[2])
    counts = Counts()
    started = time.monotonic_ns()
    deadline = time.monotonic() + seconds
    workers = 0
    if phase in {"tree-serial", "tree-parallel"}:
        if phase == "tree-serial":
            tree(0, deadline, counts, None, None)
        else:
            workers = int(sys.argv[3])
            with ThreadPoolExecutor(workers) as pool:
                tree(1, deadline, counts, pool, threading.BoundedSemaphore(4 * workers))
    elif phase == "large-serial":
        large(sys.argv[3:], deadline, counts, None)
    elif phase == "large-parallel":
        workers = len(sys.argv) - 3
        with ThreadPoolExecutor(workers) as pool:
            large(sys.argv[3:], deadline, counts, pool)
    else:
        raise SystemExit("unknown phase")
    elapsed = time.monotonic_ns() - started
    print(json.dumps(dict(schema="r8b-full-mount-count-probe-v1", eligible=False, phase=phase,
                          budget_seconds=seconds, workers=workers, elapsed_ns=elapsed,
                          clock="in-container CLOCK_MONOTONIC", **counts.value), sort_keys=True))


if __name__ == "__main__":
    main()
