#!/usr/bin/env python3
"""A descendant keeps its working directory in a private copy of the mount.

Run as the ordinary command identity with the Workspace as working directory.
The descendant detaches from the command's streams, asks for a private user
and mount namespace, and sleeps a finite time; the command itself exits 0 at
once and reports what the descendant achieved. Nothing is written anywhere.
"""
import ctypes
import json
import os
import sys
import time

CLONE_NEWNS, CLONE_NEWUSER = 0x00020000, 0x10000000


def main():
    seconds, private = float(sys.argv[1]), sys.argv[2] == "private"
    reader, writer = os.pipe()
    child = os.fork()
    if child == 0:
        os.close(reader)
        os.setsid()
        null = os.open("/dev/null", os.O_RDWR)
        for descriptor in (0, 1, 2):
            os.dup2(null, descriptor)
        os.close(null)
        report = dict(pid=os.getpid(), cwd=os.getcwd(), private=private)
        if private:
            libc = ctypes.CDLL(None, use_errno=True)
            ctypes.set_errno(0)
            report["unshare"] = ("ALLOWED" if libc.unshare(CLONE_NEWUSER | CLONE_NEWNS) == 0
                                 else os.strerror(ctypes.get_errno()))
        report["mount_namespace"] = os.readlink("/proc/self/ns/mnt")
        os.write(writer, json.dumps(report).encode())
        os.close(writer)
        time.sleep(seconds)
        os._exit(0)
    os.close(writer)
    data = os.read(reader, 65536)
    os.close(reader)
    report = json.loads(data)
    report.update(schema="r8b-namespace-holder-v1", hold_seconds=seconds,
                  command_mount_namespace=os.readlink("/proc/self/ns/mnt"))
    print(json.dumps(report, sort_keys=True))


if __name__ == "__main__":
    main()
