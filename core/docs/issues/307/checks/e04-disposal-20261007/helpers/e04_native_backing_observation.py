#!/usr/bin/env python3
"""Finite read-only observation of the selected E04 guest-native volume.

This source is copied and hashed before the host starts. It neither initializes
a database nor probes allocation by writing. The owning supervisor retains the
original Docker command, CID and inspect alongside this process's stdout.
"""
import ctypes
import hashlib
import json
import os
import re
import stat
import sys

ROOT = "/state"
DATABASE = "/state/overlay.sqlite"
WINDOW = 65536
MOUNTINFO_ADMISSION = 1 << 20  # Metadata-size admission, not a resident-memory claim.


def require(value, message):
    if not value:
        raise RuntimeError(message)


def root_identity(value):
    return {"device": value.st_dev, "inode": value.st_ino, "mode": value.st_mode,
            "uid": value.st_uid, "gid": value.st_gid}


def stable_identity(value):
    return (value.st_dev, value.st_ino, value.st_size, value.st_blocks,
            value.st_mode, value.st_nlink, value.st_uid, value.st_gid,
            value.st_mtime_ns, value.st_ctime_ns)


def observe(phase, receipt_id, owner_label, volume_name):
    require(sys.platform == "linux" and ctypes.sizeof(ctypes.c_long) == 8,
            "this observation requires the selected 64-bit Linux execution domain")
    require(phase in ("pre-start", "post-exit"), "unregistered observation phase")
    require(re.fullmatch(r"[0-9a-f]{64}", receipt_id), "malformed receipt identity")
    require(re.fullmatch(r"layerfs-e04-[a-z0-9-]+", owner_label), "malformed owner identity")
    require(volume_name == owner_label + "-state", "volume is not bound to the original owner")
    root_fd = database_fd = mountinfo_fd = probe_fd = None
    primary = None
    secondary = []
    result = None
    try:
        probe_path = os.path.realpath(__file__)
        require(probe_path == os.path.abspath(__file__) and probe_path.startswith("/work/"),
                "actual executing probe source must be canonical under the original repository bind")
        probe_fd = os.open(probe_path, os.O_RDONLY | os.O_NOFOLLOW | os.O_CLOEXEC)
        probe_before = os.fstat(probe_fd)
        require(stat.S_ISREG(probe_before.st_mode), "actual executing probe source is not a regular file")
        probe_digest, probe_count = hashlib.sha256(), 0
        while True:
            block = os.read(probe_fd, WINDOW)
            if not block:
                break
            probe_digest.update(block)
            probe_count += len(block)
        require(stable_identity(probe_before) == stable_identity(os.fstat(probe_fd))
                == stable_identity(os.stat(probe_path, follow_symlinks=False))
                and probe_count == probe_before.st_size, "actual probe source changed during original observation")
        probe_source = {"path": probe_path, "bytes": probe_count, "sha256": probe_digest.hexdigest()}
        root_fd = os.open(ROOT, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW | os.O_CLOEXEC)
        root_before = os.fstat(root_fd)
        require(stat.S_ISDIR(root_before.st_mode), "native root is not a directory")
        libc = ctypes.CDLL(None, use_errno=True)
        libc.fstatfs.argtypes = [ctypes.c_int, ctypes.c_void_p]
        libc.fstatfs.restype = ctypes.c_int
        filesystem = ctypes.create_string_buffer(256)
        if libc.fstatfs(root_fd, filesystem) != 0:
            error = ctypes.get_errno()
            raise OSError(error, os.strerror(error), ROOT)
        magic = ctypes.c_long.from_buffer(filesystem).value & 0xffffffffffffffff
        mountinfo_fd = os.open("/proc/self/mountinfo", os.O_RDONLY | os.O_CLOEXEC)
        raw_mountinfo = bytearray()
        while True:
            block = os.read(mountinfo_fd, min(WINDOW, MOUNTINFO_ADMISSION - len(raw_mountinfo) + 1))
            if not block:
                break  # Only an actual EOF establishes the complete inventory.
            raw_mountinfo.extend(block)
            require(len(raw_mountinfo) <= MOUNTINFO_ADMISSION, "mountinfo exceeds 1 MiB metadata admission")
        lines = raw_mountinfo.decode("utf-8").splitlines()
        candidates = []
        for line in lines:
            parts = line.split()
            require(len(parts) >= 10 and "-" in parts, "malformed observed mountinfo")
            destination = parts[4]
            if destination == ROOT:
                candidates.append((line, parts))
            elif destination.startswith(ROOT + "/"):
                raise RuntimeError("overlaid native-root submount is not the selected volume")
        require(len(candidates) == 1, "native root must have exactly one observed mount")
        mount_line, parts = candidates[0]
        separator = parts.index("-")
        require(separator + 3 < len(parts), "observed mountinfo lacks filesystem fields")
        require(parts[2] == f"{os.major(root_before.st_dev)}:{os.minor(root_before.st_dev)}",
                "native root stat device differs from actual mountinfo")
        require("ro" in parts[5].split(","), "observer native volume is not mounted read-only")
        database = {"exists": False}
        if phase == "pre-start":
            try:
                os.stat("overlay.sqlite", dir_fd=root_fd, follow_symlinks=False)
            except FileNotFoundError:
                pass
            else:
                raise RuntimeError("fresh original database already exists before product startup")
        else:
            database_fd = os.open("overlay.sqlite", os.O_RDONLY | os.O_NOFOLLOW | os.O_CLOEXEC,
                                  dir_fd=root_fd)
            before = os.fstat(database_fd)
            require(stat.S_ISREG(before.st_mode) and before.st_nlink == 1,
                    "original database is not an independently owned regular file")
            require(before.st_dev == root_before.st_dev, "database is outside original native root device")
            digest = hashlib.sha256()
            count = 0
            while True:
                block = os.read(database_fd, WINDOW)
                if not block:
                    break
                digest.update(block)
                count += len(block)
            after = os.fstat(database_fd)
            path_after = os.stat("overlay.sqlite", dir_fd=root_fd, follow_symlinks=False)
            require(stable_identity(before) == stable_identity(after) == stable_identity(path_after)
                    and count == before.st_size, "original native database changed during read-only observation")
            database = {"exists": True, "device": before.st_dev, "inode": before.st_ino,
                        "logical_bytes": before.st_size, "allocated_bytes": before.st_blocks * 512,
                        "mode": before.st_mode, "uid": before.st_uid, "gid": before.st_gid,
                        "links": before.st_nlink, "sha256": digest.hexdigest(), "hash_read_bytes": count,
                        "hash_window_bytes": WINDOW, "stable_during_observation": True}
        require(root_identity(os.fstat(root_fd)) == root_identity(root_before),
                "native volume root identity changed during observation")
        result = {"schema": "e04-native-backing-observation-v2", "phase": phase,
                  "receipt_id": receipt_id, "owner_label": owner_label, "volume_name": volume_name,
                  "root_path": ROOT, "database_path": DATABASE,
                  "probe_source": probe_source,
                  "filesystem": {"type": parts[separator + 1], "magic": magic, "mountinfo": mount_line},
                  "root_identity": root_identity(root_before), "database": database}
    except BaseException as error:
        primary = error
    finally:
        for label, descriptor in (("probe", probe_fd), ("mountinfo", mountinfo_fd),
                                  ("database", database_fd), ("root", root_fd)):
            if descriptor is not None:
                try:
                    os.close(descriptor)
                except BaseException as error:
                    if primary is None:
                        primary = error
                    else:
                        secondary.append(label + " close: " + repr(error))
    if primary is not None:
        for error in secondary:
            print("secondary observation failure: " + error, file=sys.stderr)
        raise primary
    return result


def main():
    require(len(sys.argv) == 5, "phase, receipt identity, owner label and original volume name required")
    result = observe(*sys.argv[1:])
    encoded = (json.dumps(result, sort_keys=True, separators=(",", ":")) + "\n").encode()
    require(len(encoded) <= WINDOW, "observation record exceeds fixed encoding window")
    require(os.write(sys.stdout.fileno(), encoded) == len(encoded),
            "short observation stdout write; acknowledged original prefix retained")
    return 0


if __name__ == "__main__":
    sys.exit(main())
