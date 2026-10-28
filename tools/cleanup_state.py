"""Inventory and ownership guards for the development cleanup CLI (stdlib only)."""

import hashlib
import json
import os
from pathlib import Path
import shutil
import stat
import subprocess
import sys


class Refused(RuntimeError):
    pass


def run(args, data=None, env=None, allowed=(0,), timeout=115):
    try:
        result = subprocess.run(args, input=data, capture_output=True, env=env, timeout=timeout)
    except subprocess.TimeoutExpired as error:
        raise Refused(f"Command exceeded {timeout} seconds; no retry: {args!r}") from error
    if result.returncode not in allowed:
        raise Refused(
            f"Command failed ({result.returncode}): {args!r}: " + result.stderr.decode(errors="replace")[-4000:]
        )
    return result


def git(repo, *args, data=None, env=None):
    return run(["git", "-C", str(repo), *args], data, env).stdout


def absolute(raw):
    path = Path(raw)
    if not path.is_absolute() or ".." in path.parts:
        raise Refused(f"Use an absolute path without '..': {raw}")
    # Resolve parents, but do not turn a symlink leaf into its target.
    return path.parent.resolve() / path.name


def overlaps(a, b):
    a, b = Path(a), Path(b)
    return a == b or a in b.parents or b in a.parents


def host_bind_path(raw):
    """Translate Docker Desktop's host aliases before comparing native paths."""
    if sys.platform == "darwin":
        for prefix in ("/host_mnt", "/run/desktop/mnt/host"):
            if raw.startswith(prefix + "/"):
                raw = raw[len(prefix) :]
                break
    return Path(raw).resolve()


def inventory(path, checkout=False):
    """Metadata snapshot; archive verification adds raw content hashes later."""
    path = Path(path)
    rows = []
    root_device = path.lstat().st_dev

    def visit(item):
        info = item.lstat()
        relative = str(item.relative_to(path))
        if "\n" in str(item) or "\r" in str(item):
            raise Refused(f"Unsupported newline in path: {item!r}")
        if not (stat.S_ISREG(info.st_mode) or stat.S_ISDIR(info.st_mode) or stat.S_ISLNK(info.st_mode)):
            raise Refused(f"Special file is not disposable: {item}")
        if info.st_dev != root_device:
            raise Refused(f"Nested filesystem: {item}")
        rows.append(
            {
                "path": relative,
                "mode": info.st_mode,
                "size": info.st_size,
                "mtime_ns": info.st_mtime_ns,
                "ctime_ns": info.st_ctime_ns,
                "inode": info.st_ino,
                "device": info.st_dev,
                "uid": info.st_uid,
                "gid": info.st_gid,
                "allocated": info.st_blocks * 512,
                "target": os.readlink(item) if stat.S_ISLNK(info.st_mode) else None,
            }
        )
        if stat.S_ISDIR(info.st_mode):
            for child in sorted(item.iterdir()):
                if child.name == ".git":
                    if checkout and item == path and child.is_file() and not child.is_symlink():
                        continue
                    if not checkout:
                        raise Refused(f"Git metadata inside selected payload: {child}")
                visit(child)

    visit(path)
    digest = hashlib.sha256(json.dumps(rows, sort_keys=True).encode()).hexdigest()
    return rows, {"fingerprint": digest, "files": len(rows), "allocated_bytes": sum(x["allocated"] for x in rows)}


def worktrees(repo):
    records = []
    for block in git(repo, "worktree", "list", "--porcelain").decode().split("\n\n"):
        record = {}
        for line in block.splitlines():
            name, _, value = line.partition(" ")
            record[name] = value
        if "worktree" in record:
            record["worktree"] = str(Path(record["worktree"]).resolve())
            records.append(record)
    return records


def docker_state():
    """All containers, including stopped ones, retain ownership of mounts."""
    if not shutil.which("docker"):
        raise Refused("Docker CLI unavailable; container ownership cannot be checked")
    ids = run(["docker", "ps", "-aq"]).stdout.decode().split()
    containers = []
    for offset in range(0, len(ids), 100):
        containers.extend(json.loads(run(["docker", "inspect", *ids[offset : offset + 100]]).stdout))
    return [
        {
            "id": c["Id"],
            "name": c["Name"],
            "running": c["State"]["Running"],
            "status": c["State"]["Status"],
            "image": c["Image"],
            "labels": c["Config"].get("Labels") or {},
            "mounts": c.get("Mounts", []),
        }
        for c in containers
    ]


def open_files():
    if not shutil.which("lsof"):
        raise Refused("lsof unavailable; process ownership cannot be checked")
    output = run(["lsof", "-nP", "-Fpn"], allowed=(0, 1))
    if output.stderr.strip():
        raise Refused("Incomplete lsof inventory: " + output.stderr.decode(errors="replace")[-2000:])
    if not output.stdout:
        raise Refused("Empty lsof inventory; cannot establish ownership")
    pid = None
    records = []
    for line in output.stdout.decode(errors="replace").splitlines():
        if line.startswith("p"):
            pid = int(line[1:])
        elif line.startswith("n/"):
            records.append((pid, line[1:]))
    return records


def ensure_idle(paths, protected, containers, opened):
    for raw in paths:
        path = absolute(raw)
        if path.is_symlink():
            raise Refused(f"Selected root is a symlink: {path}")
        for pin in protected:
            if overlaps(path, pin):
                raise Refused(f"Protected path overlaps {path}: {pin}")
        for container in containers:
            for mount in container["mounts"]:
                if mount.get("Type") == "bind" and overlaps(path, host_bind_path(mount["Source"])):
                    raise Refused(f"Container {container['name']} has a bind overlapping {path}")
        raw_path = str(path)
        prefix = raw_path + os.sep
        for pid, name in opened:
            # lsof already provides absolute paths. Avoid reparsing every open
            # filename for every candidate in a machine-wide inventory.
            if pid != os.getpid() and (name == raw_path or name.startswith(prefix)):
                raise Refused(f"PID {pid} owns an open file or cwd in {path}: {name}")


def volume_identity(name):
    return json.loads(run(["docker", "volume", "inspect", name]).stdout)[0]


def ensure_volume_idle(name, protected, containers):
    if name in protected:
        raise Refused(f"Protected volume: {name}")
    for container in containers:
        for mount in container["mounts"]:
            if mount.get("Type") == "volume" and mount.get("Name") == name:
                raise Refused(f"Volume {name} belongs to container {container['name']}")


def same_volume(before, after):
    keys = ("Name", "CreatedAt", "Driver", "Mountpoint", "Options", "Labels", "Scope")
    return {k: before.get(k) for k in keys} == {k: after.get(k) for k in keys}


def allocated(path):
    result = run(["du", "-sk", str(path)])
    return int(result.stdout.split()[0]) * 1024
