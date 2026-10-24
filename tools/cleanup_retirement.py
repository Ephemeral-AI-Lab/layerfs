"""Distinguish idle session directory handles from commands using a checkout."""

import json
import os
from pathlib import Path

from cleanup_state import Refused, run


def retirement_open_files(path, opened, enabled):
    if not enabled:
        return opened
    processes = {}
    children = {}
    for line in run(["ps", "-axo", "pid=,ppid=,stat=,comm="]).stdout.decode().splitlines():
        fields = line.strip().split(None, 3)
        if len(fields) == 4 and not fields[2].startswith("Z"):
            pid, parent = int(fields[0]), int(fields[1])
            processes[pid] = fields[3]
            children.setdefault(parent, []).append(pid)
    idle = set()
    for file in (Path.home() / ".claude/sessions").glob("*.json"):
        try:
            session = json.loads(file.read_text())
        except (ValueError, OSError) as error:
            raise Refused("Session ownership inspection incomplete: " + str(error)) from error
        pid = session.get("pid")
        if session.get("status") == "idle" and Path(processes.get(pid, "")).name == "claude":
            idle.add(pid)
    # The desktop wrapper holds the same cwd while its sole idle child waits.
    for parent, pids in children.items():
        if Path(processes.get(parent, "")).name == "disclaimer" and all(pid in idle for pid in pids):
            idle.add(parent)
    ignored = set()
    root = str(path)
    docker_disks = Path.home() / "Library/Containers/com.docker.docker/Data/vms"
    docker_vms = {pid for pid, name in opened if Path(name).name == "Docker.raw" and docker_disks in Path(name).parents}
    relevant = {}
    for pid, name in opened:
        if name == root or name.startswith(root + os.sep):
            relevant.setdefault(pid, []).append(name)
    for pid, names in relevant.items():
        if pid not in processes:
            # lsof and ps are sequential observations. A completed inspector
            # must not remain an owner forever in the earlier lsof snapshot.
            ignored.add(pid)
            continue
        executable = Path(processes.get(pid, "")).name
        # A shell with no children, or an idle agent, may retain only the root
        # directory. Open files below it remain protected. This never kills an
        # agent and is re-evaluated before unlinking the backed-up checkout.
        if all(name == root for name in names) and (pid in idle or executable == "zsh" and not children.get(pid)):
            ignored.add(pid)
        # Docker Desktop may retain cached host file descriptors after a bind
        # is gone. The caller independently checks every current container bind.
        if executable == "com.apple.Virtualization.VirtualMachine" and pid in docker_vms:
            ignored.add(pid)
    return [(pid, name) for pid, name in opened if pid not in ignored]
