#!/usr/bin/env python3
"""Crash/reopen durability measurements for the frozen Stage 2 FUSE image.

The host owns Docker orchestration.  Each warmup or measured sample gets a
fresh Store volume, proves its prepared root through SIGKILL/reopen, times the
unchanged upstream command, checkpoints with a whole-workspace fsyncdir, and
then proves the acknowledged snapshot and ref root through another crash.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import statistics
import subprocess
import sys
import time


IMAGE = "layerfs-fuse:frozen-7e82abc"
IMAGE_ID = "sha256:f8647b84580c75d4688a18665e4c60cd6dcf5b2d3092cf22bce34dfbd86b59b0"
SOURCE_COMMIT = "7e82abcd7320f6a214be336d82488ba0527b6025"
SOURCE_TREE = "df13d88eb7e7d2471971b0c58ca6425bb81b0b03"
FS_BENCH_SHA256 = "0e2004339a9269b88c2de451d56575957477bde16b96a10cf1837519e37230ef"
SCENARIOS_SHA256 = "ee96f7ce161f09e7a60a39d3893458fef9f6449827c08ce4db9b430fb986b61b"
WARMUP = 1
REPS = 3
PROBE_SCENARIOS = {
    "stat 1000 files",
    "mkdir tree (10x10x10)",
    "copy 64 MiB",
    "overwrite 64 MiB",
}
OWNED = "/var/tmp/layerfs-owned"
STORE = "/var/lib/layerfs/store.sqlite"
MOUNT = "/workspace"
DURABLE_ROOT = Path(__file__).resolve().parents[1] / "durable"

# These are the exact twelve command/prep strings already frozen in the
# candidate-015 harness for the upstream fs-bench.sh scenarios.
SCENARIOS = (
    (
        "create 1000 files",
        "for i in $(seq 1 1000); do echo $i > f$i; done",
        "",
    ),
    (
        "stat 1000 files",
        "for i in $(seq 1 1000); do echo $i > f$i; done; "
        "for i in $(seq 1 1000); do stat f$i; done",
        "",
    ),
    (
        "rm 1000 files",
        "for i in $(seq 1 1000); do echo $i > f$i; done; rm f*",
        "",
    ),
    (
        "mkdir tree (10x10x10)",
        "for a in $(seq 1 10); do for b in $(seq 1 10); do mkdir -p $a/$b; "
        "for c in $(seq 1 10); do touch $a/$b/$c; done; done; done",
        "",
    ),
    (
        "find tree",
        "for a in $(seq 1 10); do for b in $(seq 1 10); do mkdir -p $a/$b; "
        "for c in $(seq 1 10); do touch $a/$b/$c; done; done; done; "
        "find . -type f | wc -l",
        "",
    ),
    (
        "write 64 MiB",
        "dd if=/dev/zero of=big bs=1M count=64 status=none",
        "",
    ),
    (
        "copy 64 MiB",
        "dd if=/dev/zero of=big bs=1M count=64 status=none; cp big big2",
        "",
    ),
    (
        "read 64 MiB",
        "dd if=/dev/zero of=big bs=1M count=64 status=none; cat big > /dev/null",
        "",
    ),
    (
        "pure read 64 MiB",
        "cat big > /dev/null",
        "dd if=/dev/zero of=big bs=1M count=64 status=none",
    ),
    (
        "pure copy 64 MiB",
        "cp big big2",
        "dd if=/dev/zero of=big bs=1M count=64 status=none",
    ),
    (
        "overwrite 64 MiB",
        "dd if=/dev/zero of=big bs=1M count=64 status=none conv=notrunc",
        "dd if=/dev/zero of=big bs=1M count=64 status=none",
    ),
    (
        "git init + commit 100 files",
        "git init -q; for i in $(seq 1 100); do echo $i > f$i; done; "
        "git add -A; git -c user.email=a@b -c user.name=a commit -qm init",
        "",
    ),
)

ENVELOPE = (
    "--platform",
    "linux/arm64",
    "--init",
    "--stop-timeout",
    "1",
    "--cpus",
    "1",
    "--memory",
    "3g",
    "--pids-limit",
    "512",
    "--device",
    "/dev/fuse:rwm",
    "--cap-add",
    "SYS_ADMIN",
    "--network",
    "none",
    "--tmpfs",
    "/tmp:rw,nosuid,nodev,size=1g,mode=1777",
)
DAEMON_ARGS = (
    "--store",
    STORE,
    "--mount",
    MOUNT,
    "--spool",
    f"{OWNED}/spool",
    "--receipt",
    f"{OWNED}/terminal.json",
    "--ref",
    "main",
    "--integrity",
    "verified",
    "--uid",
    "0",
    "--gid",
    "0",
)

ROOT_SCRIPT = r'''
import json, sqlite3
connection = sqlite3.connect("file:/var/lib/layerfs/store.sqlite?mode=ro", uri=True)
row = connection.execute(
    "SELECT generation, root_id FROM layerfs_refs WHERE name='main'"
).fetchone()
connection.close()
if row is None:
    raise SystemExit("main ref missing")
print(json.dumps({"generation": row[0], "root": bytes(row[1]).hex()}, sort_keys=True))
'''

SNAPSHOT_SCRIPT = r'''
import hashlib, json, os, stat, sys

root = os.path.abspath(sys.argv[1])
entries = []

def add(path, relative):
    value = os.lstat(path)
    mode = stat.S_IMODE(value.st_mode)
    item = {
        "path_hex": os.fsencode(relative).hex(),
        "mode": mode,
        "mtime_ns": value.st_mtime_ns,
        "nlink": value.st_nlink,
    }
    if stat.S_ISDIR(value.st_mode):
        item["type"] = "directory"
    elif stat.S_ISREG(value.st_mode):
        digest = hashlib.sha256()
        with open(path, "rb", buffering=0) as source:
            while True:
                block = source.read(1024 * 1024)
                if not block:
                    break
                digest.update(block)
        item.update(type="file", size=value.st_size, sha256=digest.hexdigest())
    elif stat.S_ISLNK(value.st_mode):
        item.update(type="symlink", target_hex=os.fsencode(os.readlink(path)).hex())
    else:
        item["type"] = "other"
    entries.append(item)
    if item["type"] == "directory":
        with os.scandir(path) as children:
            ordered = sorted(children, key=lambda child: os.fsencode(child.name))
        for child in ordered:
            child_relative = child.name if relative == "." else relative + "/" + child.name
            add(child.path, child_relative)

add(root, ".")
encoded = json.dumps(entries, separators=(",", ":"), sort_keys=True).encode()
print(json.dumps({
    "entries": entries,
    "entries_sha256": hashlib.sha256(encoded).hexdigest(),
    "descendant_count": len(entries) - 1,
}, separators=(",", ":"), sort_keys=True))
'''

BARRIER_SCRIPT = r'''
import json, os, sys, time
descriptor = os.open(sys.argv[1], os.O_RDONLY | os.O_DIRECTORY)
started = time.perf_counter_ns()
os.fsync(descriptor)
ended = time.perf_counter_ns()
os.close(descriptor)
print(json.dumps({"fsyncdir_ns": ended - started}, sort_keys=True))
'''

TIMED_SCRIPT = r'''
import json, os, subprocess, sys, time

directory, command = sys.argv[1:]

def daemon_pid():
    matches = []
    for name in os.listdir("/proc"):
        if not name.isdigit():
            continue
        try:
            if os.path.realpath(f"/proc/{name}/exe") == "/usr/local/bin/layerfs-fuse":
                matches.append(int(name))
        except (FileNotFoundError, PermissionError):
            pass
    if len(matches) != 1:
        raise RuntimeError(f"expected one LayerFS daemon, found {matches}")
    return matches[0]

def status(pid):
    wanted = {"Threads", "VmHWM", "VmRSS"}
    result = {}
    with open(f"/proc/{pid}/status") as source:
        for line in source:
            fields = line.split()
            if fields and fields[0].rstrip(":") in wanted:
                key = fields[0].rstrip(":")
                result[key] = int(fields[1]) * (1024 if key.startswith("Vm") else 1)
    if set(result) != wanted:
        raise RuntimeError(f"incomplete daemon status: {result}")
    return result

def schedstat(pid):
    result = {}
    for task in sorted(os.listdir(f"/proc/{pid}/task"), key=int):
        with open(f"/proc/{pid}/task/{task}/schedstat") as source:
            fields = source.read().split()
        if len(fields) < 3:
            raise RuntimeError(f"short schedstat for task {task}")
        result[task] = int(fields[0])
    if not result:
        raise RuntimeError("daemon has no schedstat tasks")
    return result

def cgroup_values(name):
    result = {}
    with open(f"/sys/fs/cgroup/{name}") as source:
        for line in source:
            key, value = line.split()
            result[key] = int(value)
    return result

def cgroup_scalar(name):
    with open(f"/sys/fs/cgroup/{name}") as source:
        return int(source.read().strip())

pid = daemon_pid()
period_before_alignment = cgroup_values("cpu.stat")["nr_periods"]
while cgroup_values("cpu.stat")["nr_periods"] == period_before_alignment:
    time.sleep(0.001)
period_after_alignment = cgroup_values("cpu.stat")["nr_periods"]
before = {
    "daemon_status": status(pid),
    "schedstat_runtime_ns": schedstat(pid),
    "cpu_stat": cgroup_values("cpu.stat"),
    "memory_events": cgroup_values("memory.events"),
    "memory_current_bytes": cgroup_scalar("memory.current"),
    "memory_peak_bytes": cgroup_scalar("memory.peak"),
}
descriptor = os.open("/workspace", os.O_RDONLY | os.O_DIRECTORY)
started = time.perf_counter_ns()
completed = subprocess.run(
    ["bash", "-c", command],
    cwd=directory,
    stdout=subprocess.DEVNULL,
    stderr=subprocess.DEVNULL,
)
live_done = time.perf_counter_ns()
if completed.returncode:
    os.close(descriptor)
    print(json.dumps({"command_returncode": completed.returncode}, sort_keys=True))
    raise SystemExit(completed.returncode)
os.fsync(descriptor)
durable_done = time.perf_counter_ns()
os.close(descriptor)
after = {
    "daemon_status": status(pid),
    "schedstat_runtime_ns": schedstat(pid),
    "cpu_stat": cgroup_values("cpu.stat"),
    "memory_events": cgroup_values("memory.events"),
    "memory_current_bytes": cgroup_scalar("memory.current"),
    "memory_peak_bytes": cgroup_scalar("memory.peak"),
}
resource = {
    "daemon_pid": pid,
    "quota_period_before_alignment": period_before_alignment,
    "quota_period_after_alignment": period_after_alignment,
    "command_started_ns": started,
    "command_returned_ns": live_done,
    "durability_acknowledged_ns": durable_done,
    "before": before,
    "after": after,
}
print(json.dumps({
    "command_returncode": 0,
    "T_live_ns": live_done - started,
    "T_checkpoint_ns": durable_done - live_done,
    "T_to_durable_ns": durable_done - started,
    "resources": resource,
}, sort_keys=True))
'''

SAMPLER_SCRIPT = r'''
import json, os, sys, time

ready_path, stop_path, output_path = sys.argv[1:]

def daemon_pid():
    matches = []
    for name in os.listdir("/proc"):
        if not name.isdigit():
            continue
        try:
            if os.path.realpath(f"/proc/{name}/exe") == "/usr/local/bin/layerfs-fuse":
                matches.append(int(name))
        except (FileNotFoundError, PermissionError):
            pass
    if len(matches) != 1:
        raise RuntimeError(f"expected one LayerFS daemon, found {matches}")
    return matches[0]

def status(pid):
    wanted = {"Threads", "VmHWM", "VmRSS"}
    result = {}
    with open(f"/proc/{pid}/status") as source:
        for line in source:
            fields = line.split()
            if fields and fields[0].rstrip(":") in wanted:
                key = fields[0].rstrip(":")
                result[key] = int(fields[1]) * (1024 if key.startswith("Vm") else 1)
    return result

pid = daemon_pid()
baseline = status(pid)
result = {
    "daemon_pid": pid,
    "fd_baseline": len(os.listdir(f"/proc/{pid}/fd")),
    "fd_high_water": 0,
    "threads_high_water": 0,
    "daemon_rss_high_water_bytes": 0,
    "samples": 0,
    "sampler_started_ns": time.perf_counter_ns(),
}
with open(ready_path, "x") as ready:
    ready.write("ready\n")
while not os.path.exists(stop_path):
    current = status(pid)
    result["fd_high_water"] = max(
        result["fd_high_water"], len(os.listdir(f"/proc/{pid}/fd"))
    )
    result["threads_high_water"] = max(result["threads_high_water"], current["Threads"])
    result["daemon_rss_high_water_bytes"] = max(
        result["daemon_rss_high_water_bytes"], current["VmRSS"], current["VmHWM"]
    )
    result["samples"] += 1
    time.sleep(0.005)
result["sampler_ended_ns"] = time.perf_counter_ns()
result["daemon_status_at_sampler_start"] = baseline
with open(output_path, "x") as output:
    json.dump(result, output, sort_keys=True)
    output.write("\n")
'''

RESOURCE_FIELDS = (
    "dirty_nodes",
    "dirty_ranges",
    "pending_nodes",
    "directory_changes",
    "open_handles",
    "logical_workspace_bytes",
    "spool_live_bytes",
    "spool_dead_bytes",
    "spool_physical_bytes",
    "operation_q_terminal_bytes",
)


class HarnessError(RuntimeError):
    pass


def slug(value: str) -> str:
    return re.sub(r"[^a-z0-9]+", "-", value.lower()).strip("-")


def validate_token(value: str) -> str:
    if not re.fullmatch(r"[a-z0-9][a-z0-9-]{0,31}", value):
        raise argparse.ArgumentTypeError("value must match [a-z0-9][a-z0-9-]{0,31}")
    return value


def write_json(path: Path, value: object) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("x") as output:
        json.dump(value, output, indent=2, sort_keys=True)
        output.write("\n")


def write_text(path: Path, value: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("x") as output:
        output.write(value)


def append_json(path: Path, value: object) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("a") as output:
        output.write(json.dumps(value, separators=(",", ":"), sort_keys=True) + "\n")


def run(
    argv: list[str],
    *,
    check: bool = True,
    timeout: int = 180,
) -> subprocess.CompletedProcess[bytes]:
    completed = subprocess.run(
        argv,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=timeout,
    )
    if check and completed.returncode:
        stderr = completed.stderr.decode(errors="replace").strip()
        raise HarnessError(f"command exited {completed.returncode}: {argv!r}: {stderr}")
    return completed


def docker_absent(kind: str, name: str) -> bool:
    return run(["docker", kind, "inspect", name], check=False).returncode != 0


def exec_json(container: str, script: str, *arguments: str) -> dict[str, object]:
    completed = run(["docker", "exec", container, "python3", "-c", script, *arguments])
    try:
        return json.loads(completed.stdout)
    except json.JSONDecodeError as error:
        raise HarnessError(f"invalid JSON from container helper: {completed.stdout!r}") from error


def image_binding() -> dict[str, object]:
    inspected = json.loads(run(["docker", "image", "inspect", IMAGE_ID]).stdout)[0]
    labels = inspected["Config"].get("Labels") or {}
    environment = dict(
        field.split("=", 1) for field in inspected["Config"].get("Env", []) if "=" in field
    )
    checks = {
        "architecture_arm64": inspected.get("Architecture") == "arm64",
        "fs_bench_environment_exact": environment.get("LAYERFS_FS_BENCH_SHA256")
        == FS_BENCH_SHA256,
        "fs_bench_label_exact": labels.get("org.opencontainers.image.layerfs.fs-bench-sha256")
        == FS_BENCH_SHA256,
        "image_id_exact": inspected.get("Id") == IMAGE_ID,
        "source_commit_environment_exact": environment.get("LAYERFS_SOURCE_COMMIT")
        == SOURCE_COMMIT,
        "source_commit_label_exact": labels.get("org.opencontainers.image.layerfs.source-commit")
        == SOURCE_COMMIT,
        "source_tree_environment_exact": environment.get("LAYERFS_SOURCE_TREE") == SOURCE_TREE,
        "source_tree_label_exact": labels.get("org.opencontainers.image.layerfs.source-tree")
        == SOURCE_TREE,
    }
    receipt = {
        "schema": "layerfs-stage2-015-durable-binding-v1",
        "status": "PASS" if all(checks.values()) else "FAIL",
        "checks": checks,
        "image": IMAGE,
        "image_id": inspected.get("Id"),
        "source_commit": SOURCE_COMMIT,
        "source_tree": SOURCE_TREE,
        "fs_bench_sha256": FS_BENCH_SHA256,
    }
    return receipt


class Runtime:
    def __init__(self, output: Path, volume: str, prefix: str):
        self.output = output
        self.volume = volume
        self.prefix = prefix
        self.container: str | None = None
        self.launch_index = 0

    def create_volume(self) -> None:
        if not docker_absent("volume", self.volume):
            raise HarnessError(f"Store volume already exists: {self.volume}")
        completed = run(["docker", "volume", "create", self.volume])
        write_text(self.output / "volume-create.stdout", completed.stdout.decode())

    def launch(self, phase: str) -> str:
        if self.container is not None:
            raise HarnessError(f"container still active: {self.container}")
        container = f"{self.prefix}-{self.launch_index:02d}-{phase}"
        self.launch_index += 1
        if not docker_absent("container", container):
            raise HarnessError(f"container already exists: {container}")
        argv = [
            "docker",
            "run",
            "-d",
            "--name",
            container,
            *ENVELOPE,
            "-v",
            f"{self.volume}:/var/lib/layerfs",
            IMAGE,
            *DAEMON_ARGS,
        ]
        launch_path = self.output / f"launch-{self.launch_index - 1:02d}-{phase}"
        write_json(
            launch_path.with_suffix(".plan.json"),
            {"container": container, "daemon_argv": argv, "envelope": list(ENVELOPE)},
        )
        completed = run(argv, check=False)
        write_text(launch_path.with_suffix(".stdout"), completed.stdout.decode())
        write_text(launch_path.with_suffix(".stderr"), completed.stderr.decode())
        if completed.returncode:
            raise HarnessError(f"docker run failed with {completed.returncode}")
        self.container = container
        startup = self._wait_for_startup()
        inspected = json.loads(run(["docker", "inspect", container]).stdout)[0]
        write_json(launch_path.with_suffix(".inspect.json"), inspected)
        write_text(
            launch_path.with_suffix(".cpu.max.txt"),
            run(["docker", "exec", container, "cat", "/sys/fs/cgroup/cpu.max"])
            .stdout.decode(),
        )
        mountinfo = run(["docker", "exec", container, "cat", "/proc/1/mountinfo"]).stdout.decode()
        write_text(launch_path.with_suffix(".mountinfo.txt"), mountinfo)
        if " /workspace " not in mountinfo or " - fuse layerfs " not in mountinfo:
            raise HarnessError("native LayerFS FUSE mount missing from PID 1 mountinfo")
        write_json(launch_path.with_suffix(".startup.json"), startup)
        script_hash = run(
            ["docker", "exec", container, "sha256sum", "/usr/local/bin/fs-bench.sh"]
        ).stdout.decode()
        write_text(launch_path.with_suffix(".fs-bench.sha256"), script_hash)
        if script_hash.split()[0] != FS_BENCH_SHA256:
            raise HarnessError("runtime fs-bench hash mismatch")
        return container

    def _wait_for_startup(self) -> dict[str, object]:
        assert self.container is not None
        for _ in range(200):
            lines = run(["docker", "logs", self.container], check=False).stdout.decode().splitlines()
            for line in lines:
                try:
                    value = json.loads(line)
                except json.JSONDecodeError:
                    continue
                if value.get("backend") != "layerfs-fuse":
                    continue
                if (
                    value.get("source_commit") != SOURCE_COMMIT
                    or value.get("source_tree") != SOURCE_TREE
                    or value.get("fs_bench_sha256") != FS_BENCH_SHA256
                    or value.get("integrity") != "Verified"
                ):
                    raise HarnessError(f"runtime startup binding mismatch: {value}")
                return value
            time.sleep(0.05)
        raise HarnessError("LayerFS startup receipt missing")

    def state(self, path: str, artifact: str) -> dict[str, object]:
        assert self.container is not None
        value = {
            "ref": exec_json(self.container, ROOT_SCRIPT),
            "snapshot": exec_json(self.container, SNAPSHOT_SCRIPT, path),
        }
        write_json(self.output / f"{artifact}.json", value)
        return value

    def ref(self, artifact: str) -> dict[str, object]:
        assert self.container is not None
        value = exec_json(self.container, ROOT_SCRIPT)
        write_json(self.output / f"{artifact}.json", value)
        return value

    def barrier(self, artifact: str) -> dict[str, object]:
        assert self.container is not None
        value = exec_json(self.container, BARRIER_SCRIPT, MOUNT)
        write_json(self.output / f"{artifact}.json", value)
        return value

    def sigkill(self, artifact: str) -> None:
        assert self.container is not None
        container = self.container
        killed = run(["docker", "kill", "--signal", "KILL", container], check=False)
        waited = run(["docker", "wait", container], check=False)
        write_text(self.output / f"{artifact}.kill.stdout", killed.stdout.decode())
        write_text(self.output / f"{artifact}.kill.stderr", killed.stderr.decode())
        write_text(self.output / f"{artifact}.exit", waited.stdout.decode())
        write_text(
            self.output / f"{artifact}.logs",
            run(["docker", "logs", container], check=False).stdout.decode(),
        )
        stopped = json.loads(run(["docker", "inspect", container]).stdout)[0]
        write_json(self.output / f"{artifact}.stopped-inspect.json", stopped)
        unexpected_path = self.output / f"{artifact}.unexpected-terminal.json"
        unexpected = run(
            [
                "docker",
                "cp",
                f"{container}:{OWNED}/terminal.json",
                str(unexpected_path),
            ],
            check=False,
        )
        write_text(self.output / f"{artifact}.terminal-copy.stdout", unexpected.stdout.decode())
        write_text(self.output / f"{artifact}.terminal-copy.stderr", unexpected.stderr.decode())
        if killed.returncode or waited.returncode or waited.stdout.strip() != b"137":
            raise HarnessError(f"SIGKILL did not produce exit 137 for {container}")
        if stopped["State"]["OOMKilled"] is not False:
            raise HarnessError(f"SIGKILL container was OOM-killed: {container}")
        if unexpected.returncode == 0:
            raise HarnessError(f"SIGKILL unexpectedly produced a terminal receipt in {container}")
        removed = run(["docker", "rm", container], check=False)
        write_text(self.output / f"{artifact}.rm.stdout", removed.stdout.decode())
        write_text(self.output / f"{artifact}.rm.stderr", removed.stderr.decode())
        if removed.returncode:
            raise HarnessError(f"could not remove acknowledged stopped container {container}")
        self.container = None

    def graceful_terminal(self, artifact: str) -> dict[str, object]:
        assert self.container is not None
        container = self.container
        killed = run(["docker", "kill", "--signal", "TERM", container], check=False)
        waited = run(["docker", "wait", container], check=False)
        stopped = json.loads(run(["docker", "inspect", container]).stdout)[0]
        write_json(self.output / f"{artifact}.stopped-inspect.json", stopped)
        write_text(self.output / f"{artifact}.kill.stdout", killed.stdout.decode())
        write_text(self.output / f"{artifact}.kill.stderr", killed.stderr.decode())
        write_text(self.output / f"{artifact}.exit", waited.stdout.decode())
        receipt_path = self.output / f"{artifact}.json"
        copied = run(
            ["docker", "cp", f"{container}:{OWNED}/terminal.json", str(receipt_path)],
            check=False,
        )
        write_text(self.output / f"{artifact}.copy.stdout", copied.stdout.decode())
        write_text(self.output / f"{artifact}.copy.stderr", copied.stderr.decode())
        if copied.returncode:
            raise HarnessError(f"terminal receipt missing for {container}")
        terminal = json.loads(receipt_path.read_text())
        if killed.returncode or waited.returncode or waited.stdout.strip() != b"0":
            raise HarnessError(f"graceful terminal failed for {container}")
        if stopped["State"]["OOMKilled"] is not False:
            raise HarnessError(f"graceful container was OOM-killed: {container}")
        if terminal.get("status") != "PASS":
            raise HarnessError(f"terminal receipt failed for {container}")
        if (
            terminal.get("source_commit") != SOURCE_COMMIT
            or terminal.get("source_tree") != SOURCE_TREE
            or terminal.get("fs_bench_sha256") != FS_BENCH_SHA256
        ):
            raise HarnessError(f"terminal source binding mismatch for {container}")
        return terminal

    def remove_success(self) -> None:
        assert self.container is not None
        container = self.container
        removed = run(["docker", "rm", container], check=False)
        volume_removed = run(["docker", "volume", "rm", self.volume], check=False)
        cleanup = {
            "container": container,
            "container_remove_exit": removed.returncode,
            "volume": self.volume,
            "volume_remove_exit": volume_removed.returncode,
        }
        cleanup["status"] = (
            "PASS"
            if removed.returncode == volume_removed.returncode == 0
            and docker_absent("container", container)
            and docker_absent("volume", self.volume)
            else "FAIL"
        )
        write_json(self.output / "resource-cleanup.json", cleanup)
        if cleanup["status"] != "PASS":
            raise HarnessError(f"successful runtime cleanup failed: {cleanup}")
        self.container = None

    def remove_container(self, artifact: str) -> None:
        assert self.container is not None
        container = self.container
        removed = run(["docker", "rm", container], check=False)
        write_text(self.output / f"{artifact}.stdout", removed.stdout.decode())
        write_text(self.output / f"{artifact}.stderr", removed.stderr.decode())
        if removed.returncode or not docker_absent("container", container):
            raise HarnessError(f"could not remove stopped container {container}")
        self.container = None

    def preserve_failure(self, error: BaseException) -> None:
        value = {
            "schema": "layerfs-stage2-015-durable-preserved-failure-v1",
            "status": "PRESERVED_FAILURE",
            "error": f"{type(error).__name__}: {error}",
            "container": self.container,
            "volume": self.volume,
        }
        path = self.output / "failure.json"
        if not path.exists():
            write_json(path, value)


def same_state(expected: dict[str, object], actual: dict[str, object], label: str) -> None:
    if expected != actual:
        raise HarnessError(f"{label} snapshot/ref mismatch after reopen")


def terminal_is_clean(terminal: dict[str, object]) -> bool:
    mounted = terminal.get("mounted")
    engine = terminal.get("engine")
    return (
        terminal.get("status") == "PASS"
        and terminal.get("session_terminated") is True
        and terminal.get("kernel_cache_released") is True
        and isinstance(mounted, dict)
        and isinstance(engine, dict)
        and all(mounted.get(name) == 0 for name in RESOURCE_FIELDS)
        and mounted.get("lookup_refs")
        == mounted.get("live_nodes")
        == mounted.get("inode_mappings")
        == 1
        and engine.get("connections_terminal") == 0
        and terminal["callbacks"].get("init") == 1
        and terminal["callbacks"].get("destroy") == 1
    )


def timed_terminal_is_clean(terminal: dict[str, object]) -> bool:
    mounted = terminal.get("mounted")
    engine = terminal.get("engine")
    return (
        terminal.get("status") == "PASS"
        and terminal.get("session_terminated") is True
        and terminal.get("kernel_cache_released") is True
        and isinstance(mounted, dict)
        and isinstance(engine, dict)
        and all(
            mounted.get(name) == 0
            for name in RESOURCE_FIELDS
            if name != "logical_workspace_bytes"
        )
        and mounted.get("lookup_refs")
        == mounted.get("live_nodes")
        == mounted.get("inode_mappings")
        == 1
        and engine.get("connections_terminal") == 0
        and terminal["callbacks"].get("init") == 1
        and terminal["callbacks"].get("destroy") == 1
    )


def resource_facts(timing: dict[str, object], terminal: dict[str, object]) -> dict[str, object]:
    resources = timing["resources"]
    before = resources["before"]
    after = resources["after"]
    sched_before = before["schedstat_runtime_ns"]
    sched_after = after["schedstat_runtime_ns"]
    stable_tasks = set(sched_before) == set(sched_after) and bool(sched_before)
    sched_monotonic = stable_tasks and all(
        sched_after[task] >= sched_before[task] for task in sched_before
    )
    daemon_cpu_ns = (
        sum(sched_after.values()) - sum(sched_before.values()) if sched_monotonic else -1
    )
    durable_ns = int(timing["T_to_durable_ns"])
    cpu_limit_ns = durable_ns * 105 // 100 + 5_000_000
    throttled_usec = (
        after["cpu_stat"].get("throttled_usec", 0)
        - before["cpu_stat"].get("throttled_usec", 0)
    )
    oom_delta = after["memory_events"].get("oom", 0) - before["memory_events"].get("oom", 0)
    oom_kill_delta = (
        after["memory_events"].get("oom_kill", 0)
        - before["memory_events"].get("oom_kill", 0)
    )
    rss_upper_bound = max(
        0,
        after["daemon_status"]["VmHWM"] - before["daemon_status"]["VmRSS"],
    )
    mounted = terminal["mounted"]
    engine = terminal["engine"]
    facts = {
        "quota_period_aligned": resources["quota_period_after_alignment"]
        > resources["quota_period_before_alignment"],
        "sampler_covers_timer": resources["sampler_started_ns"]
        <= resources["command_started_ns"]
        and resources["sampler_ended_ns"] >= resources["durability_acknowledged_ns"],
        "samples_nonzero": resources["samples"] > 0,
        "task_set_stable": stable_tasks,
        "schedstat_monotonic": sched_monotonic,
        "daemon_cpu_ns": daemon_cpu_ns,
        "daemon_cpu_limit_ns": cpu_limit_ns,
        "daemon_cpu_bounded": sched_monotonic and daemon_cpu_ns <= cpu_limit_ns,
        "throttled_usec_delta": throttled_usec,
        "throttle_ratio_bounded": throttled_usec >= 0
        and 20 * throttled_usec * 1000 <= durable_ns,
        "oom_delta": oom_delta,
        "oom_kill_delta": oom_kill_delta,
        "oom_zero": oom_delta == oom_kill_delta == 0,
        "daemon_rss_upper_bound_bytes": rss_upper_bound,
        "daemon_rss_delta_bounded": rss_upper_bound <= 64 * 1024 * 1024,
        "cgroup_memory_peak_bytes": after["memory_peak_bytes"],
        "cgroup_memory_peak_bounded": after["memory_peak_bytes"] <= 512 * 1024 * 1024,
        "threads_high_water": resources["threads_high_water"],
        "threads_bounded": resources["threads_high_water"] <= 8,
        "fd_baseline": resources["fd_baseline"],
        "fd_high_water": resources["fd_high_water"],
        "fd_bounded": resources["fd_high_water"] <= resources["fd_baseline"] + 64,
        "connections_bounded": engine["connections_high_water"] <= 2
        and engine["connections_terminal"] == 0,
        "operation_q_bounded": mounted["operation_q_high_water_bytes"] <= 8_388_607
        and mounted["operation_q_terminal_bytes"] == 0,
        "request_buffer_bounded": mounted["largest_request_bytes"] <= 1024 * 1024,
        "request_buffer_scope": "largest admitted FUSE request; not a product-wide allocation claim",
        "callback_wall_ns": terminal["callbacks"]["callback_wall_ns"],
        "mount_lock_wait_ns": terminal["callbacks"]["mount_lock_wait_ns"],
        "callback_wall_positive": terminal["callbacks"]["callback_wall_ns"] > 0,
        "mount_lock_ratio_bounded": 10 * terminal["callbacks"]["mount_lock_wait_ns"]
        <= terminal["callbacks"]["callback_wall_ns"],
        "sqlite_contention_zero": engine["busy_events"] == engine["locked_events"] == 0,
        "spool_terminal_zero": all(
            mounted[name] == 0
            for name in ("spool_live_bytes", "spool_dead_bytes", "spool_physical_bytes")
        ),
        "spool_steady_bound": mounted["spool_physical_high_water_bytes"]
        <= 2 * mounted["spool_live_high_water_bytes"] + 64 * 1024 * 1024,
        "timed_terminal_clean": timed_terminal_is_clean(terminal),
    }
    required = (
        "quota_period_aligned",
        "sampler_covers_timer",
        "samples_nonzero",
        "task_set_stable",
        "schedstat_monotonic",
        "daemon_cpu_bounded",
        "oom_zero",
        "daemon_rss_delta_bounded",
        "cgroup_memory_peak_bounded",
        "threads_bounded",
        "fd_bounded",
        "connections_bounded",
        "operation_q_bounded",
        "request_buffer_bounded",
        "callback_wall_positive",
        "sqlite_contention_zero",
        "spool_terminal_zero",
        "spool_steady_bound",
        "timed_terminal_clean",
    )
    facts["status"] = "PASS" if all(facts[name] is True for name in required) else "FAIL"
    return facts


def shell(container: str, directory: str, command: str) -> None:
    completed = run(
        ["docker", "exec", "-w", directory, container, "bash", "-c", command],
        check=False,
    )
    if completed.returncode:
        raise HarnessError(f"scenario prep exited {completed.returncode}: {command!r}")


def make_directory(container: str, path: str) -> None:
    run(
        [
            "docker",
            "exec",
            container,
            "python3",
            "-c",
            "import os,sys; os.mkdir(sys.argv[1], 0o700)",
            path,
        ]
    )


def remove_tree(container: str, path: str) -> dict[str, object]:
    started = time.time_ns()
    run(
        [
            "docker",
            "exec",
            container,
            "python3",
            "-c",
            "import shutil,sys; shutil.rmtree(sys.argv[1])",
            path,
        ]
    )
    return {
        "classification": "ACKNOWLEDGED_CLEANUP_OUTSIDE_TIMER",
        "host_wall_ns": time.time_ns() - started,
        "path": path,
    }


def measured_command(
    container: str,
    directory: str,
    command: str,
    output: Path,
) -> dict[str, object]:
    sidecar = f"{container}-sampler"
    ready = output / "sampler-ready"
    stop = output / "sampler-stop"
    sampler_output = output / "sampler.json"
    sidecar_argv = [
        "docker",
        "run",
        "-d",
        "--name",
        sidecar,
        "--platform",
        "linux/arm64",
        "--pid",
        f"container:{container}",
        "--cap-add",
        "SYS_PTRACE",
        "--cgroupns",
        "host",
        "--network",
        "none",
        "--read-only",
        "--cpus",
        "0.25",
        "--memory",
        "128m",
        "--pids-limit",
        "32",
        "--mount",
        f"type=bind,src={output.resolve()},dst=/evidence",
        "--entrypoint",
        "python3",
        IMAGE,
        "-c",
        SAMPLER_SCRIPT,
        "/evidence/sampler-ready",
        "/evidence/sampler-stop",
        "/evidence/sampler.json",
    ]
    write_json(output / "sampler-plan.json", {"argv": sidecar_argv, "sidecar": sidecar})
    launched = run(sidecar_argv, check=False)
    write_text(output / "sampler-launch.stdout", launched.stdout.decode())
    write_text(output / "sampler-launch.stderr", launched.stderr.decode())
    if launched.returncode:
        raise HarnessError(f"resource sampler did not launch: {sidecar}")
    for _ in range(500):
        if ready.exists():
            break
        time.sleep(0.01)
    else:
        write_text(
            output / "sampler-logs",
            run(["docker", "logs", sidecar], check=False).stdout.decode(),
        )
        run(["docker", "rm", "--force", sidecar], check=False)
        raise HarnessError(f"resource sampler did not become ready: {sidecar}")
    completed = None
    try:
        completed = run(
            ["docker", "exec", container, "python3", "-c", TIMED_SCRIPT, directory, command],
            check=False,
            timeout=300,
        )
    finally:
        if not stop.exists():
            write_text(stop, "stop\n")
        waited = run(["docker", "wait", sidecar], check=False)
        write_text(output / "sampler-exit", waited.stdout.decode())
        write_text(
            output / "sampler-logs",
            run(["docker", "logs", sidecar], check=False).stdout.decode(),
        )
        inspected = run(["docker", "inspect", sidecar], check=False)
        if inspected.returncode == 0:
            write_json(output / "sampler-inspect.json", json.loads(inspected.stdout)[0])
        removed = run(["docker", "rm", sidecar], check=False)
        write_text(output / "sampler-rm.stdout", removed.stdout.decode())
        write_text(output / "sampler-rm.stderr", removed.stderr.decode())
        if waited.returncode or waited.stdout.strip() != b"0" or removed.returncode:
            raise HarnessError(f"resource sampler failed: {sidecar}")
    if completed is None:
        raise HarnessError("timed command did not return a result")
    try:
        value = json.loads(completed.stdout)
    except json.JSONDecodeError as error:
        raise HarnessError(f"timed helper returned invalid JSON: {completed.stdout!r}") from error
    if completed.returncode or value.get("command_returncode") != 0:
        raise HarnessError(f"exact scenario command failed: {command!r}: {value}")
    live = value.get("T_live_ns")
    checkpoint = value.get("T_checkpoint_ns")
    durable = value.get("T_to_durable_ns")
    if not all(isinstance(number, int) and number > 0 for number in (live, checkpoint, durable)):
        raise HarnessError(f"invalid timing tuple: {value}")
    if durable != live + checkpoint:
        raise HarnessError(f"T_to_durable equation is not exact: {value}")
    sampler = json.loads(sampler_output.read_text())
    if sampler["daemon_pid"] != value["resources"]["daemon_pid"]:
        raise HarnessError("resource sampler observed a different daemon")
    value["resources"].update(sampler)
    return value


def sample_names(owner: str, run_id: str, ordinal: int) -> tuple[str, str]:
    base = f"layerfs-stage2-final015-dur-{owner}-{run_id}-s{ordinal:02d}"
    return base, base.replace("-", "_") + "_store"


def run_sample(
    campaign: Path,
    owner: str,
    run_id: str,
    ordinal: int,
    scenario: tuple[str, str, str],
    repetition: int,
    warmup: bool,
) -> dict[str, object]:
    name, command, prep = scenario
    sample_id = f"{ordinal:02d}-{slug(name)}-{'warmup' if warmup else f'rep-{repetition}'}"
    output = campaign / "samples" / sample_id
    output.mkdir(parents=True)
    prefix, volume = sample_names(owner, run_id, ordinal)
    runtime = Runtime(output, volume, prefix)
    scenario_path = f"{MOUNT}/scenario"
    write_json(
        output / "plan.json",
        {
            "schema": "layerfs-stage2-015-durable-sample-plan-v1",
            "image": IMAGE,
            "source_commit": SOURCE_COMMIT,
            "source_tree": SOURCE_TREE,
            "fs_bench_sha256": FS_BENCH_SHA256,
            "scenario": name,
            "command": command,
            "prep": prep,
            "warmup": warmup,
            "repetition": repetition,
            "fresh_store": True,
            "volume": volume,
        },
    )
    try:
        runtime.create_volume()
        container = runtime.launch("prepare")
        pristine = runtime.state(MOUNT, "state-00-pristine")
        make_directory(container, scenario_path)
        if prep:
            shell(container, scenario_path, prep)
        prep_barrier = runtime.barrier("barrier-01-prepared")
        prepared = runtime.state(MOUNT, "state-01-prepared")
        runtime.sigkill("crash-01-prepared")

        container = runtime.launch("timed")
        prepared_ref_reopen = runtime.ref("state-02-prepared-ref-reopen")
        if prepared["ref"] != prepared_ref_reopen:
            raise HarnessError("prepared Store ref mismatch before timing")
        timing = measured_command(container, scenario_path, command, output)
        write_json(output / "timing.json", timing)
        acknowledged_ref = runtime.ref("state-03-acknowledged-ref")
        timed_terminal = runtime.graceful_terminal("timed-terminal")
        resources = resource_facts(timing, timed_terminal)
        write_json(output / "timed-resources.json", resources)
        if not warmup and resources["status"] != "PASS":
            raise HarnessError(f"timed resource gates failed: {resources}")
        runtime.remove_container("timed-container-rm")

        container = runtime.launch("postack")
        acknowledged = runtime.state(MOUNT, "state-04-acknowledged-reopen")
        if acknowledged["ref"] != acknowledged_ref:
            raise HarnessError("acknowledged Store ref mismatch on first reopen")
        runtime.sigkill("crash-02-acknowledged")

        container = runtime.launch("verify")
        acknowledged_reopen = runtime.state(MOUNT, "state-05-acknowledged-reopen-after-crash")
        same_state(acknowledged, acknowledged_reopen, "acknowledged durable root")
        cleanup = remove_tree(container, scenario_path)
        write_json(output / "acknowledged-cleanup.json", cleanup)
        cleanup_barrier = runtime.barrier("barrier-02-clean-final")
        final_state = runtime.state(MOUNT, "state-06-clean-final")
        terminal = runtime.graceful_terminal("terminal")
        clean = (
            final_state["snapshot"]["descendant_count"] == 0
            and terminal_is_clean(terminal)
            and terminal.get("root") == final_state["ref"]["root"]
            and terminal.get("generation") == final_state["ref"]["generation"]
        )
        classification = (
            "CLEAN_FINAL_STATE_BARRIER_COMMITTED"
            if clean
            else "INCOMPLETE_CLEAN_FINAL_STATE"
        )
        checks = {
            "acknowledged_reopen_exact": acknowledged == acknowledged_reopen,
            "acknowledged_ref_exact_before_terminal": acknowledged_ref["root"]
            == timed_terminal["root"]
            and acknowledged_ref["generation"] == timed_terminal["generation"],
            "clean_final_state": clean,
            "prepared_root_accepted_before_timing": prepared["ref"] == prepared_ref_reopen,
            "timed_resources_pass": warmup or resources["status"] == "PASS",
            "timed_terminal_pass": timed_terminal.get("status") == "PASS",
            "terminal_pass": terminal.get("status") == "PASS",
        }
        receipt = {
            "schema": "layerfs-stage2-015-durable-sample-v1",
            "status": "PASS" if all(checks.values()) else "FAIL",
            "checks": checks,
            "classification": classification,
            "timed_durability_scope": (
                "FINAL_STATE_ONLY_OPERATION_HISTORY_NOT_CLAIMED"
                if name == "rm 1000 files"
                else "ACCEPTED_FINAL_STATE"
            ),
            "operation_history_oracle": "history-oracle/receipt.json",
            "scenario": name,
            "command": command,
            "prep": prep,
            "warmup": warmup,
            "repetition": repetition,
            "timing_disposition": "WARMUP_EXCLUDED" if warmup else "MEASURED",
            "resource_disposition": "WARMUP_DIAGNOSTIC" if warmup else "CONTROLLING",
            "T_live_ns": timing["T_live_ns"],
            "T_checkpoint_ns": timing["T_checkpoint_ns"],
            "T_to_durable_ns": timing["T_to_durable_ns"],
            "prepared_barrier_ns": prep_barrier["fsyncdir_ns"],
            "clean_final_barrier_ns": cleanup_barrier["fsyncdir_ns"],
            "pristine_ref": pristine["ref"],
            "prepared_ref": prepared["ref"],
            "acknowledged_ref": acknowledged_ref,
            "clean_final_ref": final_state["ref"],
            "resources": resources,
            "cleanup": cleanup,
        }
        write_json(output / "receipt.json", receipt)
        if receipt["status"] != "PASS":
            raise HarnessError(f"sample verification failed: {checks}")
        runtime.remove_success()
        append_json(campaign / "samples.jsonl", receipt)
        return receipt
    except BaseException as error:
        runtime.preserve_failure(error)
        append_json(
            campaign / "samples.jsonl",
            {
                "status": "PRESERVED_FAILURE",
                "sample": sample_id,
                "scenario": name,
                "error": f"{type(error).__name__}: {error}",
                "container": runtime.container,
                "volume": runtime.volume,
            },
        )
        raise


def history_oracle(campaign: Path, owner: str, run_id: str) -> dict[str, object]:
    output = campaign / "history-oracle"
    output.mkdir()
    prefix = f"lfs-d015-{owner}-{run_id}-history"
    volume = prefix.replace("-", "_") + "_store"
    runtime = Runtime(output, volume, prefix)
    probe = f"{MOUNT}/history-probe"
    try:
        runtime.create_volume()
        container = runtime.launch("create")
        pristine = runtime.state(MOUNT, "state-00-pristine")
        run(
            [
                "docker",
                "exec",
                container,
                "python3",
                "-c",
                "import os,sys; os.mkdir(sys.argv[1]); open(sys.argv[1]+'/value','xb').write(b'history')",
                probe,
            ]
        )
        barrier_create = runtime.barrier("barrier-01-create")
        created = runtime.state(MOUNT, "state-01-created")
        runtime.sigkill("crash-01-created")

        container = runtime.launch("delete")
        created_reopen = runtime.state(MOUNT, "state-02-created-reopen")
        same_state(created, created_reopen, "history create barrier")
        cleanup_create = remove_tree(container, probe)
        write_json(output / "acknowledged-delete.json", cleanup_create)
        barrier_delete = runtime.barrier("barrier-02-delete")
        deleted = runtime.state(MOUNT, "state-03-deleted")
        runtime.sigkill("crash-02-deleted")

        runtime.launch("verify")
        deleted_reopen = runtime.state(MOUNT, "state-04-deleted-reopen")
        same_state(deleted, deleted_reopen, "history delete barrier")
        terminal = runtime.graceful_terminal("terminal")
        checks = {
            "create_barrier_reopen_exact": created == created_reopen,
            "create_generation_advanced": created["ref"]["generation"]
            == pristine["ref"]["generation"] + 1,
            "create_root_changed": created["ref"]["root"] != pristine["ref"]["root"],
            "delete_barrier_reopen_exact": deleted == deleted_reopen,
            "delete_generation_advanced": deleted["ref"]["generation"]
            == created["ref"]["generation"] + 1,
            "delete_root_changed": deleted["ref"]["root"] != created["ref"]["root"],
            "final_namespace_empty": deleted["snapshot"]["descendant_count"] == 0,
            "terminal_clean": terminal_is_clean(terminal),
        }
        receipt = {
            "schema": "layerfs-stage2-015-two-barrier-history-v1",
            "status": "PASS" if all(checks.values()) else "FAIL",
            "checks": checks,
            "classification": "CREATE_AND_DELETE_PUBLISHED_IN_DISTINCT_BARRIERS",
            "create_barrier_ns": barrier_create["fsyncdir_ns"],
            "delete_barrier_ns": barrier_delete["fsyncdir_ns"],
            "pristine_ref": pristine["ref"],
            "created_ref": created["ref"],
            "deleted_ref": deleted["ref"],
        }
        write_json(output / "receipt.json", receipt)
        if receipt["status"] != "PASS":
            raise HarnessError(f"history oracle failed: {checks}")
        runtime.remove_success()
        return receipt
    except BaseException as error:
        runtime.preserve_failure(error)
        raise


def timing_summary(
    samples: list[dict[str, object]],
    scenarios: tuple[tuple[str, str, str], ...],
    reps: int,
) -> dict[str, object]:
    result: dict[str, object] = {}
    for name, _, _ in scenarios:
        rows = [row for row in samples if row["scenario"] == name and not row["warmup"]]
        if len(rows) != reps:
            raise HarnessError(f"{name} has {len(rows)} measured rows, expected {reps}")
        timing = {}
        for field in ("T_live_ns", "T_checkpoint_ns", "T_to_durable_ns"):
            values = [int(row[field]) for row in rows]
            timing[field] = {
                "samples": values,
                "mean": sum(values) // len(values),
                "median": int(statistics.median(values)),
                "minimum": min(values),
                "maximum": max(values),
            }
        result[name] = timing
    return result


def population_resource_summary(
    samples: list[dict[str, object]],
    probe: bool,
) -> dict[str, object]:
    measured = [row for row in samples if not row["warmup"]]
    durable_ns = sum(int(row["T_to_durable_ns"]) for row in measured)
    throttled_ns = sum(int(row["resources"]["throttled_usec_delta"]) * 1000 for row in measured)
    callback_wall_ns = sum(int(row["resources"]["callback_wall_ns"]) for row in measured)
    mount_lock_wait_ns = sum(int(row["resources"]["mount_lock_wait_ns"]) for row in measured)
    checks = {
        "measured_rows_nonempty": bool(measured),
        "all_per_row_resource_gates_pass": all(
            row["resources"]["status"] == "PASS" for row in measured
        ),
        "aggregate_throttle_bounded": durable_ns > 0
        and 20 * throttled_ns <= durable_ns,
        "aggregate_mount_lock_bounded": callback_wall_ns > 0
        and 10 * mount_lock_wait_ns <= callback_wall_ns,
        "probe_rows_individually_bounded": not probe
        or all(
            row["resources"]["throttle_ratio_bounded"]
            and row["resources"]["mount_lock_ratio_bounded"]
            for row in measured
        ),
    }
    return {
        "status": "PASS" if all(checks.values()) else "FAIL",
        "checks": checks,
        "measured_rows": len(measured),
        "durable_wall_ns": durable_ns,
        "throttled_ns": throttled_ns,
        "throttle_ratio": throttled_ns / durable_ns if durable_ns else None,
        "callback_wall_ns": callback_wall_ns,
        "mount_lock_wait_ns": mount_lock_wait_ns,
        "mount_lock_ratio": mount_lock_wait_ns / callback_wall_ns
        if callback_wall_ns
        else None,
    }


def run_campaign(owner: str, run_id: str, probe: bool) -> None:
    scenarios = (
        tuple(scenario for scenario in SCENARIOS if scenario[0] in PROBE_SCENARIOS)
        if probe
        else SCENARIOS
    )
    warmup_count = 0 if probe else WARMUP
    reps = 1 if probe else REPS
    DURABLE_ROOT.mkdir(exist_ok=True)
    campaign = DURABLE_ROOT / run_id
    campaign.mkdir()
    events = campaign / "events.jsonl"
    started = time.time_ns()
    append_json(events, {"event": "campaign-start", "unix_ns": started})
    try:
        binding = image_binding()
        binding["harness_sha256"] = hashlib.sha256(Path(__file__).read_bytes()).hexdigest()
        write_json(campaign / "binding.json", binding)
        if binding["status"] != "PASS":
            raise HarnessError(f"frozen image binding failed: {binding['checks']}")
        history = None if probe else history_oracle(campaign, owner, run_id)
        if history is not None:
            append_json(events, {"event": "history-oracle-pass", "receipt": history})
        samples = []
        ordinal = 0
        for scenario in scenarios:
            for repetition in range(warmup_count + reps):
                ordinal += 1
                warmup = repetition < warmup_count
                append_json(
                    events,
                    {
                        "event": "sample-start",
                        "ordinal": ordinal,
                        "scenario": scenario[0],
                        "warmup": warmup,
                    },
                )
                receipt = run_sample(
                    campaign,
                    owner,
                    run_id,
                    ordinal,
                    scenario,
                    max(0, repetition - warmup_count + 1),
                    warmup,
                )
                samples.append(receipt)
                append_json(events, {"event": "sample-pass", "ordinal": ordinal})
        resources = population_resource_summary(samples, probe)
        summary = {
            "schema": "layerfs-stage2-015-durable-campaign-v1",
            "status": "PASS" if resources["status"] == "PASS" else "FAIL",
            "image": IMAGE,
            "image_id": IMAGE_ID,
            "source_commit": SOURCE_COMMIT,
            "source_tree": SOURCE_TREE,
            "fs_bench_sha256": FS_BENCH_SHA256,
            "scenarios_sha256": SCENARIOS_SHA256,
            "probe": probe,
            "warmup": warmup_count,
            "reps": reps,
            "fresh_store_samples": len(samples),
            "measured_samples": len([row for row in samples if not row["warmup"]]),
            "history_oracle": history,
            "resources": resources,
            "timings": timing_summary(samples, scenarios, reps),
            "started_unix_ns": started,
            "ended_unix_ns": time.time_ns(),
        }
        write_json(campaign / "summary.json", summary)
        if summary["status"] != "PASS":
            raise HarnessError(f"population resource gates failed: {resources}")
        append_json(events, {"event": "campaign-pass", "unix_ns": summary["ended_unix_ns"]})
    except BaseException as error:
        failure = {
            "schema": "layerfs-stage2-015-durable-campaign-failure-v1",
            "status": "PRESERVED_FAILURE",
            "error": f"{type(error).__name__}: {error}",
            "unix_ns": time.time_ns(),
        }
        if not (campaign / "failure.json").exists():
            write_json(campaign / "failure.json", failure)
        append_json(events, {"event": "campaign-failure", **failure})
        raise


def self_check() -> None:
    encoded = json.dumps(SCENARIOS, separators=(",", ":"), ensure_ascii=True).encode()
    assert hashlib.sha256(encoded).hexdigest() == SCENARIOS_SHA256
    assert len(SCENARIOS) == len({name for name, _, _ in SCENARIOS}) == 12
    assert PROBE_SCENARIOS == {
        "stat 1000 files",
        "mkdir tree (10x10x10)",
        "copy 64 MiB",
        "overwrite 64 MiB",
    }
    assert len({slug(name) for name, _, _ in SCENARIOS}) == 12
    assert [bool(prep) for _, _, prep in SCENARIOS].count(True) == 3
    assert WARMUP == 1 and REPS == 3 and len(SCENARIOS) * (WARMUP + REPS) == 48
    assert ENVELOPE.count("--cpus") == 1 and ENVELOPE[ENVELOPE.index("--cpus") + 1] == "1"
    assert "--privileged" not in ENVELOPE and ("--network", "none") in zip(
        ENVELOPE, ENVELOPE[1:]
    )
    assert IMAGE == "layerfs-fuse:frozen-7e82abc"
    assert DAEMON_ARGS[DAEMON_ARGS.index("--integrity") + 1] == "verified"
    assert all(len(value) == 40 for value in (SOURCE_COMMIT, SOURCE_TREE))
    assert len(FS_BENCH_SHA256) == len(IMAGE_ID.removeprefix("sha256:")) == 64
    assert "subprocess.run" in TIMED_SCRIPT and "os.fsync" in TIMED_SCRIPT
    assert TIMED_SCRIPT.index("subprocess.run") < TIMED_SCRIPT.index("os.fsync")
    assert '"T_checkpoint_ns": durable_done - live_done' in TIMED_SCRIPT
    assert '"schedstat_runtime_ns": schedstat(pid)' in TIMED_SCRIPT
    assert '"memory_peak_bytes": cgroup_scalar("memory.peak")' in TIMED_SCRIPT
    assert "period_before_alignment" in TIMED_SCRIPT
    for script in (ROOT_SCRIPT, SNAPSHOT_SCRIPT, BARRIER_SCRIPT, TIMED_SCRIPT, SAMPLER_SCRIPT):
        compile(script, "<embedded-helper>", "exec")
    assert validate_token("durable-015") == "durable-015"
    print(
        json.dumps(
            {
                "status": "PASS",
                "scenario_count": len(SCENARIOS),
                "scenarios_sha256": SCENARIOS_SHA256,
                "fresh_store_samples": 48,
                "image": IMAGE,
            },
            sort_keys=True,
        )
    )


def main() -> None:
    if sys.argv[1:] == ["--self-check"]:
        self_check()
        return
    parser = argparse.ArgumentParser()
    parser.add_argument("--owner", required=True, type=validate_token)
    parser.add_argument("--run-id", required=True, type=validate_token)
    parser.add_argument("--probe", action="store_true")
    arguments = parser.parse_args()
    run_campaign(arguments.owner, arguments.run_id, arguments.probe)


if __name__ == "__main__":
    main()
