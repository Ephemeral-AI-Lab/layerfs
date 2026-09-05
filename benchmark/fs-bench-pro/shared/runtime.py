#!/usr/bin/env python3
"""Small Docker runtime primitives for the v0.1.3 benchmark harness.

This module owns process deadlines and benchmark-owned Docker objects.  It
does not select families, define benchmark timing, or retain result policy.
"""

from __future__ import annotations

from dataclasses import dataclass, field
import json
import hashlib
import shutil
import sqlite3
import os
from pathlib import Path
import re
import signal
import subprocess
import threading
import time
from typing import Mapping, Sequence


GIB = 1024**3
DEFAULT_OUTPUT_LIMIT = 1024 * 1024
OWNER_LABEL = "dev.layerfs.fs-bench.owner"
ROLE_LABEL = "dev.layerfs.fs-bench.role"
OWNER = "benchmark-infrastructure-v1"

_NAME = re.compile(r"[A-Za-z0-9][A-Za-z0-9_.-]{0,127}\Z")
_IMAGE = re.compile(r"[A-Za-z0-9][A-Za-z0-9_./:@+-]{0,254}\Z")
_LABEL_KEY = re.compile(r"[A-Za-z0-9][A-Za-z0-9_.-]{0,127}\Z")
_ENV_KEY = re.compile(r"[A-Za-z_][A-Za-z0-9_]*\Z")


class RuntimeFailure(RuntimeError):
    """Base failure for an owned runtime operation."""


class DeadlineExpired(RuntimeFailure):
    pass


class CommandFailure(RuntimeFailure):
    def __init__(self, result: "CommandResult") -> None:
        self.result = result
        status = "deadline" if result.timed_out else f"exit {result.returncode}"
        super().__init__(f"{Path(result.argv[0]).name} failed: {status}")


@dataclass(frozen=True)
class Deadline:
    """An absolute ``time.monotonic()`` deadline."""

    end: float

    @classmethod
    def after(cls, seconds: float) -> "Deadline":
        if not isinstance(seconds, (int, float)) or seconds <= 0:
            raise ValueError("deadline seconds must be positive")
        return cls(time.monotonic() + float(seconds))

    def remaining(self) -> float:
        return max(0.0, self.end - time.monotonic())

    def require(self, label: str, reserve: float = 0.0) -> float:
        remaining = self.remaining() - reserve
        if remaining <= 0:
            raise DeadlineExpired(f"deadline expired before {label}")
        return remaining


@dataclass(frozen=True)
class CommandResult:
    argv: tuple[str, ...]
    returncode: int
    stdout: bytes
    stderr: bytes
    wall_ns: int
    truncated: bool
    timed_out: bool

    def stdout_text(self) -> str:
        return self.stdout.decode("utf-8", errors="replace")

    def stderr_text(self) -> str:
        return self.stderr.decode("utf-8", errors="replace")


class _Capture:
    def __init__(self, limit: int) -> None:
        self.limit = limit
        self.used = 0
        self.truncated = False
        self.lock = threading.Lock()

    def append(self, target: bytearray, data: bytes) -> None:
        with self.lock:
            available = max(0, self.limit - self.used)
            kept = data[:available]
            target.extend(kept)
            self.used += len(kept)
            self.truncated |= len(kept) != len(data)


def run(
    argv: Sequence[os.PathLike[str] | str],
    *,
    deadline: Deadline,
    input_bytes: bytes | None = None,
    output_limit: int = DEFAULT_OUTPUT_LIMIT,
    env: Mapping[str, str] | None = None,
    cwd: os.PathLike[str] | str | None = None,
    check: bool = True,
) -> CommandResult:
    """Run one command under an absolute deadline while draining bounded output."""
    if not argv:
        raise ValueError("empty command")
    if output_limit < 0:
        raise ValueError("negative output limit")
    deadline.require("command start")
    command = tuple(str(value) for value in argv)
    process_env = os.environ.copy()
    if env:
        process_env.update({str(key): str(value) for key, value in env.items()})
    started = time.monotonic_ns()
    process = subprocess.Popen(
        command,
        cwd=None if cwd is None else str(cwd),
        env=process_env,
        stdin=subprocess.PIPE if input_bytes is not None else subprocess.DEVNULL,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        start_new_session=True,
    )
    capture = _Capture(output_limit)
    stdout = bytearray()
    stderr = bytearray()

    def drain(stream, target: bytearray) -> None:
        try:
            while True:
                data = stream.read(64 * 1024)
                if not data:
                    return
                capture.append(target, data)
        finally:
            stream.close()

    readers = [
        threading.Thread(target=drain, args=(process.stdout, stdout), daemon=True),
        threading.Thread(target=drain, args=(process.stderr, stderr), daemon=True),
    ]
    for reader in readers:
        reader.start()

    writer = None
    if input_bytes is not None:
        def write_input() -> None:
            try:
                process.stdin.write(input_bytes)
                process.stdin.flush()
            except (BrokenPipeError, OSError):
                pass
            finally:
                process.stdin.close()

        writer = threading.Thread(target=write_input, daemon=True)
        writer.start()

    timed_out = False
    try:
        process.wait(timeout=deadline.require("command completion"))
    except (subprocess.TimeoutExpired, DeadlineExpired):
        timed_out = True
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        process.wait()
    finally:
        if writer is not None:
            writer.join(timeout=1)
        for reader in readers:
            reader.join(timeout=1)
        if any(reader.is_alive() for reader in readers):
            capture.truncated = True

    result = CommandResult(
        argv=command,
        returncode=process.returncode,
        stdout=bytes(stdout),
        stderr=bytes(stderr),
        wall_ns=time.monotonic_ns() - started,
        truncated=capture.truncated,
        timed_out=timed_out,
    )
    if check and (timed_out or result.returncode != 0):
        raise CommandFailure(result)
    return result


def build_image(
    repo: os.PathLike[str] | str,
    tag: str,
    build_args: Mapping[str, str],
    *,
    deadline: Deadline,
    jobs: int = 8,
) -> CommandResult:
    """Build the sealed runtime image with a bounded Cargo worker count."""
    _image(tag)
    if not 1 <= jobs <= 8:
        raise ValueError("build jobs must be in 1..=8")
    root = Path(repo).resolve()
    dockerfile = root / "benchmark/fs-bench-pro/Dockerfile.layerfs"
    if not dockerfile.is_file():
        raise ValueError("benchmark Dockerfile absent")
    command = [
        "docker", "build", "--file", str(dockerfile), "--tag", tag,
        "--build-arg", f"CARGO_BUILD_JOBS={jobs}",
    ]
    for key, value in sorted(build_args.items()):
        if not _LABEL_KEY.fullmatch(key):
            raise ValueError(f"invalid build argument: {key}")
        command.extend(("--build-arg", f"{key}={value}"))
    command.append(str(root))
    return run(command, deadline=deadline, cwd=root, output_limit=4 * DEFAULT_OUTPUT_LIMIT)


def _json_result(result: CommandResult, label: str):
    try:
        return json.loads(result.stdout)
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise RuntimeFailure(f"invalid {label} JSON") from error


def _inspect_image(reference: str, deadline: Deadline) -> dict:
    _image(reference)
    result = run(
        ["docker", "image", "inspect", reference],
        deadline=deadline,
        output_limit=4 * DEFAULT_OUTPUT_LIMIT,
        check=False,
    )
    if result.returncode != 0:
        raise CommandFailure(result)
    values = _json_result(result, "image inspection")
    if not isinstance(values, list) or len(values) != 1:
        raise RuntimeFailure("ambiguous image inspection")
    return values[0]


def _inspect_container(reference: str, deadline: Deadline) -> dict:
    _name(reference)
    result = run(
        ["docker", "inspect", reference],
        deadline=deadline,
        output_limit=4 * DEFAULT_OUTPUT_LIMIT,
    )
    values = _json_result(result, "container inspection")
    if not isinstance(values, list) or len(values) != 1:
        raise RuntimeFailure("ambiguous container inspection")
    return values[0]


def _labels(arguments: list[str], labels: Mapping[str, str]) -> None:
    for key, value in sorted(labels.items()):
        if not _LABEL_KEY.fullmatch(key) or "\x00" in value or "\n" in value:
            raise ValueError("invalid Docker label")
        arguments.extend(("--label", f"{key}={value}"))


def _name(value: str) -> str:
    if not _NAME.fullmatch(value):
        raise ValueError(f"invalid Docker object name: {value}")
    return value


def _image(value: str) -> str:
    if not _IMAGE.fullmatch(value):
        raise ValueError(f"invalid Docker image reference: {value}")
    return value


@dataclass
class SampleContainer:
    id: str
    name: str
    image: str
    _capability: str = field(repr=False)
    removed: bool = False
    observation: dict = field(default_factory=dict)

    def exec(
        self,
        argv: Sequence[str],
        *,
        deadline: Deadline,
        env: Mapping[str, str] | None = None,
        output_limit: int = DEFAULT_OUTPUT_LIMIT,
        check: bool = True,
    ) -> CommandResult:
        if self.removed:
            raise RuntimeFailure("sample container already removed")
        command = ["docker", "exec"]
        process_env: dict[str, str] = {}
        for key, value in sorted((env or {}).items()):
            if not _ENV_KEY.fullmatch(key):
                raise ValueError(f"invalid container environment name: {key}")
            command.extend(("--env", key))
            process_env[key] = value
        command.append(self.name)
        command.extend(argv)
        return run(
            command,
            deadline=deadline,
            env=process_env,
            output_limit=output_limit,
            check=check,
        )

    def remove(self, deadline: Deadline) -> CommandResult:
        if self.removed:
            return CommandResult(("docker", "rm", self.name), 0, b"", b"", 0, False, False)
        result = run(
            ["docker", "rm", "--force", self.name],
            deadline=deadline,
            output_limit=4096,
            check=False,
        )
        if result.returncode == 0:
            self.removed = True
        else:
            raise CommandFailure(result)
        return result


def start_sample(
    image: str,
    name: str,
    labels: Mapping[str, str],
    *,
    deadline: Deadline,
    cpus: float = 2,
    memory_bytes: int = 2 * GIB,
    pids: int = 256,
) -> SampleContainer:
    """Start one fresh no-mount sample container and authenticate its daemon."""
    cleanup_deadline = deadline
    deadline = Deadline(deadline.end - 2.0)
    _image(image)
    _name(name)
    if not 0 < cpus <= 8 or memory_bytes <= 0 or pids <= 0:
        raise ValueError("invalid sample resource limits")
    image_info = _inspect_image(image, deadline)
    if image_info.get("Config", {}).get("Volumes") not in (None, {}):
        raise RuntimeFailure("runtime image declares volumes")
    owned = {OWNER_LABEL: OWNER, ROLE_LABEL: "sample", **labels}
    command = [
        "docker", "create", "--name", name, "--network", "bridge",
        "--cpus", str(cpus), "--memory", str(memory_bytes),
        "--memory-swap", str(memory_bytes), "--pids-limit", str(pids),
        "--device", "/dev/fuse", "--cap-add", "SYS_ADMIN",
        "--security-opt", "apparmor=unconfined",
        "--env", "LAYERFS_DAEMON_TCP_LISTEN=0.0.0.0:41273",
        "--env", "LAYERFS_FUSE_HOST=host.docker.internal",
        "--env", "LAYERFS_V013_GIT_REFERENCE=/qualified/git-reference",
        "--entrypoint", "/bin/sh",
    ]
    command.extend(["--publish", "127.0.0.1::41273"])
    _labels(command, owned)
    command.extend([image, "-c",
        "/usr/local/bin/layerfs-daemon-entrypoint & daemon_pid=$!; "
        "wait \"$daemon_pid\"; daemon_status=$?; "
        "printf '%s\\n' \"$daemon_status\" >/run/layerfs/daemon-exit-code; "
        "exec sleep infinity"])
    created = run(command, deadline=deadline, output_limit=4096)
    container_id = created.stdout_text().strip()
    sample = SampleContainer(container_id, name, image_info["Id"], "")
    try:
        run(["docker", "start", name], deadline=deadline, output_limit=4096)
        readiness_deadline = Deadline(min(deadline.end, time.monotonic() + 5.0))
        while True:
            ready = run(
                [
                    "docker", "exec", name, "/bin/bash", "-ceu",
                    'test "$(wc -c </run/layerfs/capability)" -eq 32; '
                    'exec 3<>/dev/tcp/127.0.0.1/41273; exec 3>&-; exec 3<&-',
                ],
                deadline=readiness_deadline,
                output_limit=4096,
                check=False,
            )
            if ready.returncode == 0:
                break
            inspection = _inspect_container(name, deadline)
            if not inspection.get("State", {}).get("Running"):
                raise RuntimeFailure("sample daemon exited before readiness")
            readiness_deadline.require("sample daemon TCP readiness", reserve=0.05)
            time.sleep(0.05)
        capability = run(
            [
                "docker", "exec", name, "/bin/sh", "-ceu",
                "od -An -tx1 -v /run/layerfs/capability | tr -d ' \\n'",
            ],
            deadline=deadline,
            output_limit=128,
        ).stdout_text().strip()
        if not re.fullmatch(r"[a-f0-9]{64}", capability):
            raise RuntimeFailure("invalid sample daemon capability")
        inspection = _inspect_container(name, deadline)
        _validate_sample_inspection(
            inspection,
            image_info["Id"],
            owned,
            cpus,
            memory_bytes,
            pids,
        )
        sample.id = inspection["Id"]
        sample._capability = capability
        sample.observation = {"mounts": inspection.get("Mounts"), "image_volumes": image_info.get("Config", {}).get("Volumes"),
            "host_config": {key: inspection.get("HostConfig", {}).get(key) for key in
                ("Binds", "Mounts", "NetworkMode", "PortBindings", "Devices", "CapAdd", "NanoCpus", "Memory", "MemorySwap", "PidsLimit")},
            "ports": inspection.get("NetworkSettings", {}).get("Ports"), "validated": True}
        return sample
    except BaseException:
        if cleanup_deadline.remaining() > 0.05:
            run(
                ["docker", "rm", "--force", name],
                deadline=cleanup_deadline,
                output_limit=4096,
                check=False,
            )
        raise


def _validate_sample_inspection(
    inspection: Mapping,
    image_id: str,
    labels: Mapping[str, str],
    cpus: float,
    memory_bytes: int,
    pids: int,
) -> None:
    host = inspection.get("HostConfig", {})
    config = inspection.get("Config", {})
    ports = inspection.get("NetworkSettings", {}).get("Ports") or {}
    devices = host.get("Devices") or []
    if inspection.get("Image") != image_id or not inspection.get("State", {}).get("Running"):
        raise RuntimeFailure("sample image/running identity mismatch")
    if inspection.get("Mounts") or host.get("Binds") or host.get("Mounts"):
        raise RuntimeFailure("sample container has a runtime mount")
    if config.get("Volumes") not in (None, {}):
        raise RuntimeFailure("sample container declares a volume")
    if host.get("PublishAllPorts"):
        raise RuntimeFailure("sample publishes all ports")
    if host.get("NetworkMode") != "bridge":
        raise RuntimeFailure("host Store requires bridge network")
    for bindings in (host.get("PortBindings") or {}, ports):
        if set(bindings) != {"41273/tcp"} or len(bindings["41273/tcp"] or []) != 1:
            raise RuntimeFailure("host Store daemon port set mismatch")
        binding = bindings["41273/tcp"][0]
        port = binding.get("HostPort", "")
        if binding.get("HostIp") != "127.0.0.1" or (port and (not port.isdigit() or not 1 <= int(port) <= 65535)):
            raise RuntimeFailure("host Store daemon must publish only to loopback")
    if not ports["41273/tcp"][0].get("HostPort"):
        raise RuntimeFailure("host Store daemon has no allocated port")
    if len(devices) != 1 or devices[0].get("PathOnHost") != "/dev/fuse":
        raise RuntimeFailure("sample device set differs from /dev/fuse only")
    capabilities = host.get("CapAdd") or []
    if len(capabilities) != 1 or capabilities[0] not in {"SYS_ADMIN", "CAP_SYS_ADMIN"}:
        raise RuntimeFailure("sample capability set differs from SYS_ADMIN only")
    if host.get("NanoCpus") != round(cpus * 1_000_000_000):
        raise RuntimeFailure("sample CPU cap mismatch")
    if host.get("Memory") != memory_bytes or host.get("MemorySwap") != memory_bytes:
        raise RuntimeFailure("sample memory cap mismatch")
    if host.get("PidsLimit") != pids:
        raise RuntimeFailure("sample PID cap mismatch")
    actual_labels = config.get("Labels") or {}
    if any(actual_labels.get(key) != value for key, value in labels.items()):
        raise RuntimeFailure("sample ownership label mismatch")
    environment = set(config.get("Env") or [])
    for value in (
        "LAYERFS_DAEMON_TCP_LISTEN=0.0.0.0:41273",
        "LAYERFS_FUSE_HOST=host.docker.internal",
    ):
        if value not in environment:
            raise RuntimeFailure("sample loopback environment mismatch")


def file_sha256(path):
    with Path(path).open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def closed_store_copy(source, target, *, deadline):
    """Only benchmark-owned, quiescent masters; never a snapshot of a live Store."""
    source, target = Path(source), Path(target)
    deadline.require("closed Store copy")
    if source.is_symlink() or not source.is_file() or target.exists():
        raise RuntimeFailure("closed Store copy requires a regular source and absent destination")
    for suffix in ("-wal", "-shm", "-journal"):
        if Path(str(source) + suffix).exists():
            raise RuntimeFailure("master has SQLite sidecars; close/checkpoint it before preparing")
    before = file_sha256(source)
    # copyfileobj deliberately requests an independent byte copy, not APFS cloning.
    with source.open("rb") as src, target.open("xb") as dst:
        shutil.copyfileobj(src, dst, 1024 * 1024)
        dst.flush()
        os.fsync(dst.fileno())
    target.chmod(0o600)
    connection = sqlite3.connect(target.as_uri() + "?mode=ro", uri=True)
    try:
        connection.set_progress_handler(lambda: int(deadline.remaining() <= 0), 10000)
        if connection.execute("PRAGMA quick_check").fetchall() != [("ok",)]:
            raise RuntimeFailure("sample SQLite quick_check failed")
    finally:
        connection.close()
    if file_sha256(source) != before or file_sha256(target) != before:
        raise RuntimeFailure("master/sample identity changed during closed copy")
    if (source.stat().st_dev, source.stat().st_ino) == (target.stat().st_dev, target.stat().st_ino):
        raise RuntimeFailure("sample aliases master inode")
    deadline.require("closed Store validation")
    return {"clone_method": "closed-quiescent-byte-copy", "master_store_sha256": before,
            "sample_store_sha256": before, "store_bytes": target.stat().st_size,
            "master_store_inode": source.stat().st_ino, "sample_store_inode": target.stat().st_ino,
            "sqlite_quick_check": "ok", "master_unchanged": True}


def host_tree_identity(root, deadline):
    result = {}
    for path in sorted(Path(root).rglob("*")):
        deadline.require("host fixture identity")
        if path.is_symlink():
            result[str(path.relative_to(root))] = {"symlink": os.readlink(path)}
        elif path.is_file() and path.name != "host-cache.json":
            result[str(path.relative_to(root))] = {"bytes": path.stat().st_size, "sha256": file_sha256(path)}
    return result


def remove_host_owned(path):
    path = Path(path)
    if path.is_symlink() or json.loads((path / "host-owner.json").read_text()) != {"owner": OWNER}:
        raise RuntimeFailure("refusing unowned host benchmark cleanup")
    for child in path.rglob("*"):
        if child.is_dir() and not child.is_symlink():
            child.chmod(0o700)
    path.chmod(0o700)
    shutil.rmtree(path)


def evict_host_cache(root, protected, *, max_entries=8, max_bytes=10 * GIB):
    entries = []
    for folder in ("prepared", "fixtures"):
        for path in (Path(root) / folder).glob("*"):
            if path.is_symlink() or not (path / "host-cache.json").is_file():
                raise RuntimeFailure("unexpected host cache entry")
            manifest = json.loads((path / "host-cache.json").read_text())
            entries.append((path, manifest))
    removed = []
    while len(entries) > max_entries or sum(item[1]["data_bytes"] for item in entries) > max_bytes:
        candidates = [item for item in entries if str(item[0]) != str(protected)]
        if not candidates:
            raise RuntimeFailure("protected host cache exceeds budget")
        item = min(candidates, key=lambda item: item[1]["created_ns"])
        remove_host_owned(item[0])
        entries.remove(item)
        removed.append(str(item[0]))
    return removed
