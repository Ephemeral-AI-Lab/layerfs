#!/usr/bin/env python3
"""A descendant of an ordinary command keeps one reference into the mount.

Run as the command identity with the Workspace as working directory. The
descendant detaches from the command's streams, takes exactly the requested
reference, and sleeps a finite time; the command exits 0 at once and reports
what the descendant holds. Nothing is written anywhere.

  cwd         working directory in the mount, daemon's own mount namespace
  descriptor  one open regular file, working directory moved out of the mount
  mapping     one shared read-only mapping, descriptor closed, cwd moved out
  private     working directory in a private user and mount namespace copy
"""
import ctypes
import json
import mmap
import os
import stat
import sys
import time

CLONE_NEWNS, CLONE_NEWUSER = 0x00020000, 0x10000000


def regular_file():
    """The first nonempty regular file by name order, bounded breadth-first."""
    pending = ["."]
    for _ in range(64):
        if not pending:
            break
        directory = pending.pop(0)
        for name in sorted(os.listdir(directory)):
            path = os.path.join(directory, name)
            info = os.lstat(path)
            if stat.S_ISREG(info.st_mode) and info.st_size:
                return path
            if stat.S_ISDIR(info.st_mode):
                pending.append(path)
    raise RuntimeError("no nonempty regular file within the bounded search")


def main():
    seconds, arrangement = float(sys.argv[1]), sys.argv[2]
    reader, writer = os.pipe()
    child = os.fork()
    if child == 0:
        os.close(reader)
        os.setsid()
        null = os.open("/dev/null", os.O_RDWR)
        for descriptor in (0, 1, 2):
            os.dup2(null, descriptor)
        os.close(null)
        report = dict(pid=os.getpid(), mount=os.getcwd(), arrangement=arrangement)
        held = None
        if arrangement in {"descriptor", "mapping"}:
            report["file"] = regular_file()
            descriptor = os.open(report["file"], os.O_RDONLY | os.O_CLOEXEC)
            if arrangement == "mapping":
                held = mmap.mmap(descriptor, 1, mmap.MAP_SHARED, mmap.PROT_READ)
                report["first_byte"] = held[0]
                os.close(descriptor)
            else:
                held = descriptor
            os.chdir("/")
        elif arrangement == "private":
            libc = ctypes.CDLL(None, use_errno=True)
            ctypes.set_errno(0)
            report["unshare"] = ("ALLOWED" if libc.unshare(CLONE_NEWUSER | CLONE_NEWNS) == 0
                                 else os.strerror(ctypes.get_errno()))
        elif arrangement != "cwd":
            raise SystemExit("unknown arrangement")
        # Asked by path, so the enumeration holds no descriptor of its own.
        descriptors = {}
        for number in range(64):
            try:
                descriptors[str(number)] = os.readlink("/proc/self/fd/%d" % number)
            except OSError:
                pass
        report.update(cwd=os.getcwd(), mount_namespace=os.readlink("/proc/self/ns/mnt"),
                      descriptors=descriptors)
        os.write(writer, json.dumps(report).encode())
        os.close(writer)
        time.sleep(seconds)
        os._exit(0)
    os.close(writer)
    data = os.read(reader, 65536)
    os.close(reader)
    report = json.loads(data)
    report.update(schema="r8b-mount-holder-v1", hold_seconds=seconds,
                  command_mount_namespace=os.readlink("/proc/self/ns/mnt"))
    print(json.dumps(report, sort_keys=True))


if __name__ == "__main__":
    main()
