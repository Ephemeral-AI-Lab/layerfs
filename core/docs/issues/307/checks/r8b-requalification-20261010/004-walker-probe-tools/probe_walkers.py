#!/usr/bin/env python3
"""Bounded read-only probe: serial against concurrent directory walkers.

Exploratory and ineligible, same rules as probe.py. Every walker uses the
registered comparator's per-entry system-call pattern. A walker opens a child
directory by name under its parent descriptor (never following symlinks) and
hands it to an idle walker through a small bounded queue, or descends itself
when the queue is full. Top-level package names are taken from the END of the
sorted listing so that no name read by an earlier probe phase is read again.
"""
import hashlib
import json
import os
import queue
import stat
import sys
import threading
import time

WINDOW = 65536
LARGE = 4 << 20
DIRECTORY = os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW | os.O_CLOEXEC
REGULAR = os.O_RDONLY | os.O_NOFOLLOW | os.O_CLOEXEC | os.O_NONBLOCK


class Walk:
    def __init__(self, workers, deadline):
        self.workers, self.deadline = workers, deadline
        self.pending = queue.Queue(maxsize=2 * workers)
        self.lock = threading.Lock()
        self.outstanding = 0
        self.failure = None
        self.counts = dict(entries=0, directories=0, symlinks=0, files_complete=0,
                           bytes=0, large_skipped=0, handoffs=0, abandoned_directories=0)

    def add(self, local):
        with self.lock:
            for key, amount in local.items():
                self.counts[key] += amount

    def offer(self, descriptor):
        with self.lock:
            self.outstanding += 1
        try:
            self.pending.put_nowait(descriptor)
            return True
        except queue.Full:
            with self.lock:
                self.outstanding -= 1
            return False

    def directory(self, descriptor, local):
        stack = [(descriptor, os.scandir(descriptor))]
        try:
            while stack:
                parent_fd, iterator = stack[-1]
                entry = None if time.monotonic() >= self.deadline else next(iterator, None)
                if entry is None:
                    iterator.close()
                    os.close(parent_fd)
                    stack.pop()
                    continue
                info = os.stat(entry.name, dir_fd=parent_fd, follow_symlinks=False)
                local["entries"] += 1
                if stat.S_ISDIR(info.st_mode):
                    local["directories"] += 1
                    child = os.open(entry.name, DIRECTORY, dir_fd=parent_fd)
                    os.fstat(child)
                    if self.workers and self.offer(child):
                        local["handoffs"] += 1
                    else:
                        stack.append((child, os.scandir(child)))
                elif stat.S_ISLNK(info.st_mode):
                    os.readlink(entry.name, dir_fd=parent_fd)
                    os.stat(entry.name, dir_fd=parent_fd, follow_symlinks=False)
                    local["symlinks"] += 1
                elif info.st_size > LARGE:
                    local["large_skipped"] += 1
                else:
                    file_fd = os.open(entry.name, REGULAR, dir_fd=parent_fd)
                    try:
                        os.fstat(file_fd)
                        digest = hashlib.sha256()
                        while True:
                            block = os.read(file_fd, WINDOW)
                            if not block:
                                break
                            local["bytes"] += len(block)
                            digest.update(block)
                        os.fstat(file_fd)
                        os.stat(entry.name, dir_fd=parent_fd, follow_symlinks=False)
                    finally:
                        os.close(file_fd)
                    local["files_complete"] += 1
        finally:
            for parent_fd, iterator in stack:
                iterator.close()
                os.close(parent_fd)

    def worker(self):
        local = dict.fromkeys(self.counts, 0)
        try:
            while True:
                descriptor = self.pending.get()
                if descriptor is None:
                    return
                try:
                    if time.monotonic() >= self.deadline or self.failure is not None:
                        os.close(descriptor)
                        local["abandoned_directories"] += 1
                    else:
                        self.directory(descriptor, local)
                except BaseException as error:
                    with self.lock:
                        self.failure = self.failure or error
                finally:
                    with self.lock:
                        self.outstanding -= 1
                        finished = self.outstanding == 0
                    if finished:
                        for _ in range(self.workers):
                            self.pending.put(None)
        finally:
            self.add(local)


def main():
    tree, modulus, residue, workers, seconds = (sys.argv[1], int(sys.argv[2]), int(sys.argv[3]),
                                                int(sys.argv[4]), float(sys.argv[5]))
    started = time.monotonic_ns()
    walk = Walk(workers, time.monotonic() + seconds)
    root = os.open(tree, DIRECTORY)
    names = sorted((entry.name for entry in os.scandir(root)), reverse=True)
    selected = [name for index, name in enumerate(names) if index % modulus == residue]
    local = dict.fromkeys(walk.counts, 0)
    if workers == 0:
        for name in selected:
            if time.monotonic() >= walk.deadline:
                break
            info = os.stat(name, dir_fd=root, follow_symlinks=False)
            local["entries"] += 1
            if stat.S_ISDIR(info.st_mode):
                local["directories"] += 1
                walk.directory(os.open(name, DIRECTORY, dir_fd=root), local)
        walk.add(local)
    else:
        threads = [threading.Thread(target=walk.worker) for _ in range(workers)]
        for thread in threads:
            thread.start()
        with walk.lock:
            walk.outstanding += 1
        for name in selected:
            if time.monotonic() >= walk.deadline or walk.failure is not None:
                break
            info = os.stat(name, dir_fd=root, follow_symlinks=False)
            local["entries"] += 1
            if stat.S_ISDIR(info.st_mode):
                local["directories"] += 1
                with walk.lock:
                    walk.outstanding += 1
                walk.pending.put(os.open(name, DIRECTORY, dir_fd=root))
        with walk.lock:
            walk.outstanding -= 1
            finished = walk.outstanding == 0
        if finished:
            for _ in range(workers):
                walk.pending.put(None)
        for thread in threads:
            thread.join()
        walk.add(local)
    os.close(root)
    if walk.failure is not None:
        raise walk.failure
    print(json.dumps(dict(schema="r8b-full-mount-walker-probe-v1", eligible=False, tree=tree,
                          modulus=modulus, residue=residue, workers=workers, budget_seconds=seconds,
                          elapsed_ns=time.monotonic_ns() - started,
                          clock="in-container CLOCK_MONOTONIC", **walk.counts), sort_keys=True))


if __name__ == "__main__":
    main()
