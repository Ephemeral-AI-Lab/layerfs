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


IMAGE = "layerfs-fuse:frozen-292be84"
IMAGE_ID = "sha256:62b459af3f03dc8bbe97419b8522ed3599ab6d562b12ebe8b8ed5efb7f22f5fc"
SOURCE_COMMIT = "292be840c31052d85ab6e9441706298af3cd3d15"
SOURCE_TREE = "e3055bcd7a41921879fa149c11918891517e4522"
FS_BENCH_SHA256 = "0e2004339a9269b88c2de451d56575957477bde16b96a10cf1837519e37230ef"
SCENARIOS_SHA256 = "ee96f7ce161f09e7a60a39d3893458fef9f6449827c08ce4db9b430fb986b61b"
WARMUP = 1
REPS = 3
OWNED = "/var/tmp/layerfs-owned"
STORE = "/var/lib/layerfs/store.sqlite"
MOUNT = "/workspace"
DURABLE_ROOT = Path(__file__).resolve().parents[1] / "durable"

# These are the exact twelve command/prep strings already frozen in the
# candidate-014 harness for the upstream fs-bench.sh scenarios.
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
print(json.dumps({
    "command_returncode": 0,
    "T_live_ns": live_done - started,
    "T_checkpoint_ns": durable_done - live_done,
    "T_to_durable_ns": durable_done - started,
}, sort_keys=True))
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
        "schema": "layerfs-stage2-014-durable-binding-v1",
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
        self._wait_for_mount()
        inspected = json.loads(run(["docker", "inspect", container]).stdout)[0]
        write_json(launch_path.with_suffix(".inspect.json"), inspected)
        write_text(
            launch_path.with_suffix(".cpu.max.txt"),
            run(["docker", "exec", container, "cat", "/sys/fs/cgroup/cpu.max"])
            .stdout.decode(),
        )
        write_text(
            launch_path.with_suffix(".mountinfo.txt"),
            run(["docker", "exec", container, "cat", "/proc/self/mountinfo"])
            .stdout.decode(),
        )
        startup = self._startup_receipt()
        write_json(launch_path.with_suffix(".startup.json"), startup)
        script_hash = run(
            ["docker", "exec", container, "sha256sum", "/usr/local/bin/fs-bench.sh"]
        ).stdout.decode()
        write_text(launch_path.with_suffix(".fs-bench.sha256"), script_hash)
        if script_hash.split()[0] != FS_BENCH_SHA256:
            raise HarnessError("runtime fs-bench hash mismatch")
        return container

    def _wait_for_mount(self) -> None:
        assert self.container is not None
        for _ in range(200):
            if run(
                ["docker", "exec", self.container, "mountpoint", "-q", MOUNT],
                check=False,
                timeout=10,
            ).returncode == 0:
                return
            time.sleep(0.05)
        raise HarnessError(f"{MOUNT} did not become a mountpoint")

    def _startup_receipt(self) -> dict[str, object]:
        assert self.container is not None
        lines = run(["docker", "logs", self.container]).stdout.decode().splitlines()
        for line in lines:
            try:
                value = json.loads(line)
            except json.JSONDecodeError:
                continue
            if value.get("backend") == "layerfs-fuse":
                if (
                    value.get("source_commit") != SOURCE_COMMIT
                    or value.get("source_tree") != SOURCE_TREE
                    or value.get("fs_bench_sha256") != FS_BENCH_SHA256
                    or value.get("integrity") != "Verified"
                ):
                    raise HarnessError(f"runtime startup binding mismatch: {value}")
                return value
        raise HarnessError("LayerFS startup receipt missing")

    def state(self, path: str, artifact: str) -> dict[str, object]:
        assert self.container is not None
        value = {
            "ref": exec_json(self.container, ROOT_SCRIPT),
            "snapshot": exec_json(self.container, SNAPSHOT_SCRIPT, path),
        }
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

    def preserve_failure(self, error: BaseException) -> None:
        value = {
            "schema": "layerfs-stage2-014-durable-preserved-failure-v1",
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
        isinstance(mounted, dict)
        and isinstance(engine, dict)
        and all(mounted.get(name) == 0 for name in RESOURCE_FIELDS)
        and mounted.get("lookup_refs")
        == mounted.get("live_nodes")
        == mounted.get("inode_mappings")
        == 1
        and engine.get("connections_terminal") == 0
    )


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


def measured_command(container: str, directory: str, command: str) -> dict[str, object]:
    completed = run(
        ["docker", "exec", container, "python3", "-c", TIMED_SCRIPT, directory, command],
        check=False,
        timeout=300,
    )
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
    return value


def sample_names(owner: str, run_id: str, ordinal: int) -> tuple[str, str]:
    base = f"layerfs-stage2-final014-dur-{owner}-{run_id}-s{ordinal:02d}"
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
            "schema": "layerfs-stage2-014-durable-sample-plan-v1",
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
        prepared_reopen = runtime.state(MOUNT, "state-02-prepared-reopen")
        same_state(prepared, prepared_reopen, "prepared initial root")
        timing = measured_command(container, scenario_path, command)
        write_json(output / "timing.json", timing)
        acknowledged = runtime.state(MOUNT, "state-03-acknowledged")
        runtime.sigkill("crash-02-acknowledged")

        container = runtime.launch("verify")
        acknowledged_reopen = runtime.state(MOUNT, "state-04-acknowledged-reopen")
        same_state(acknowledged, acknowledged_reopen, "acknowledged durable root")
        cleanup = remove_tree(container, scenario_path)
        write_json(output / "acknowledged-cleanup.json", cleanup)
        cleanup_barrier = runtime.barrier("barrier-02-clean-final")
        final_state = runtime.state(MOUNT, "state-05-clean-final")
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
            "clean_final_state": clean,
            "prepared_reopen_exact": prepared == prepared_reopen,
            "prepared_root_accepted_before_timing": prepared["ref"]
            == prepared_reopen["ref"],
            "terminal_pass": terminal.get("status") == "PASS",
        }
        receipt = {
            "schema": "layerfs-stage2-014-durable-sample-v1",
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
            "T_live_ns": timing["T_live_ns"],
            "T_checkpoint_ns": timing["T_checkpoint_ns"],
            "T_to_durable_ns": timing["T_to_durable_ns"],
            "prepared_barrier_ns": prep_barrier["fsyncdir_ns"],
            "clean_final_barrier_ns": cleanup_barrier["fsyncdir_ns"],
            "pristine_ref": pristine["ref"],
            "prepared_ref": prepared["ref"],
            "acknowledged_ref": acknowledged["ref"],
            "clean_final_ref": final_state["ref"],
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
    prefix = f"lfs-d014-{owner}-{run_id}-history"
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
            "schema": "layerfs-stage2-014-two-barrier-history-v1",
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


def timing_summary(samples: list[dict[str, object]]) -> dict[str, object]:
    result: dict[str, object] = {}
    for name, _, _ in SCENARIOS:
        rows = [row for row in samples if row["scenario"] == name and not row["warmup"]]
        if len(rows) != REPS:
            raise HarnessError(f"{name} has {len(rows)} measured rows, expected {REPS}")
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


def run_campaign(owner: str, run_id: str) -> None:
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
        history = history_oracle(campaign, owner, run_id)
        append_json(events, {"event": "history-oracle-pass", "receipt": history})
        samples = []
        ordinal = 0
        for scenario in SCENARIOS:
            for repetition in range(WARMUP + REPS):
                ordinal += 1
                warmup = repetition < WARMUP
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
                    max(0, repetition - WARMUP + 1),
                    warmup,
                )
                samples.append(receipt)
                append_json(events, {"event": "sample-pass", "ordinal": ordinal})
        summary = {
            "schema": "layerfs-stage2-014-durable-campaign-v1",
            "status": "PASS",
            "image": IMAGE,
            "image_id": IMAGE_ID,
            "source_commit": SOURCE_COMMIT,
            "source_tree": SOURCE_TREE,
            "fs_bench_sha256": FS_BENCH_SHA256,
            "scenarios_sha256": SCENARIOS_SHA256,
            "warmup": WARMUP,
            "reps": REPS,
            "fresh_store_samples": len(samples),
            "measured_samples": len([row for row in samples if not row["warmup"]]),
            "history_oracle": history,
            "timings": timing_summary(samples),
            "started_unix_ns": started,
            "ended_unix_ns": time.time_ns(),
        }
        write_json(campaign / "summary.json", summary)
        append_json(events, {"event": "campaign-pass", "unix_ns": summary["ended_unix_ns"]})
    except BaseException as error:
        failure = {
            "schema": "layerfs-stage2-014-durable-campaign-failure-v1",
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
    assert len({slug(name) for name, _, _ in SCENARIOS}) == 12
    assert [bool(prep) for _, _, prep in SCENARIOS].count(True) == 3
    assert WARMUP == 1 and REPS == 3 and len(SCENARIOS) * (WARMUP + REPS) == 48
    assert ENVELOPE.count("--cpus") == 1 and ENVELOPE[ENVELOPE.index("--cpus") + 1] == "1"
    assert "--privileged" not in ENVELOPE and ("--network", "none") in zip(
        ENVELOPE, ENVELOPE[1:]
    )
    assert IMAGE == "layerfs-fuse:frozen-292be84"
    assert DAEMON_ARGS[DAEMON_ARGS.index("--integrity") + 1] == "verified"
    assert all(len(value) == 40 for value in (SOURCE_COMMIT, SOURCE_TREE))
    assert len(FS_BENCH_SHA256) == len(IMAGE_ID.removeprefix("sha256:")) == 64
    assert "subprocess.run" in TIMED_SCRIPT and "os.fsync" in TIMED_SCRIPT
    assert TIMED_SCRIPT.index("subprocess.run") < TIMED_SCRIPT.index("os.fsync")
    assert '"T_checkpoint_ns": durable_done - live_done' in TIMED_SCRIPT
    for script in (ROOT_SCRIPT, SNAPSHOT_SCRIPT, BARRIER_SCRIPT, TIMED_SCRIPT):
        compile(script, "<embedded-helper>", "exec")
    assert validate_token("durable-014") == "durable-014"
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
    arguments = parser.parse_args()
    run_campaign(arguments.owner, arguments.run_id)


if __name__ == "__main__":
    main()
