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
from pathlib import Path, PurePosixPath
import re
import signal
import subprocess
import threading
import time
import uuid
from typing import Iterable, Mapping, Sequence


PREPARED_ROOT = "/var/lib/fs-bench/prepared"
SAMPLE_ROOT = "/var/lib/fs-bench/sample"
COORDINATOR = "/usr/local/bin/fs-benchmark-pro"
GIB = 1024**3
DEFAULT_OUTPUT_LIMIT = 1024 * 1024
OWNER_LABEL = "dev.layerfs.fs-bench.owner"
ROLE_LABEL = "dev.layerfs.fs-bench.role"
CACHE_KEY_LABEL = "dev.layerfs.fs-bench.cache-key"
RUNTIME_IMAGE_LABEL = "dev.layerfs.fs-bench.runtime-image"
PREPARED_BYTES_LABEL = "dev.layerfs.fs-bench.prepared-bytes"
CACHE_TAG_LABEL = "dev.layerfs.fs-bench.cache-tag"
OWNER = "benchmark-infrastructure-v1"

_NAME = re.compile(r"[A-Za-z0-9][A-Za-z0-9_.-]{0,127}\Z")
_IMAGE = re.compile(r"[A-Za-z0-9][A-Za-z0-9_./:@+-]{0,254}\Z")
_LABEL_KEY = re.compile(r"[A-Za-z0-9][A-Za-z0-9_.-]{0,127}\Z")
_ENV_KEY = re.compile(r"[A-Za-z_][A-Za-z0-9_]*\Z")
_CACHE_KEY = re.compile(r"[a-f0-9]{16,128}\Z")
_RELATIVE = re.compile(r"[A-Za-z0-9][A-Za-z0-9_.@/+\-=]{0,511}\Z")


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


def _internal_path(value: str, *, root: str = "/var/lib/fs-bench") -> str:
    path = PurePosixPath(value)
    base = PurePosixPath(root)
    if not path.is_absolute() or path == base or base not in path.parents or ".." in path.parts:
        raise ValueError(f"unsafe benchmark container path: {value}")
    return str(path)


def _relative(value: str) -> str:
    path = PurePosixPath(value)
    if path.is_absolute() or ".." in path.parts or str(path) in {"", "."}:
        raise ValueError(f"unsafe relative prepared path: {value}")
    if not _RELATIVE.fullmatch(str(path)):
        raise ValueError(f"unsupported relative prepared path: {value}")
    return str(path)


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

    def exec_coordinator(
        self,
        args: Sequence[str],
        *,
        deadline: Deadline,
        output_limit: int = DEFAULT_OUTPUT_LIMIT,
        env: Mapping[str, str] | None = None,
        check: bool = True,
    ) -> CommandResult:
        values = {
            "LAYERFS_BENCH_LOCAL_RUNTIME": "1",
            "LAYERFS_EXEC_TRANSPORT": "daemon",
            "LAYERFS_FUSE_TRANSPORT": "daemon",
            "LAYERFS_DAEMON_TCP_ENDPOINT": "127.0.0.1:41273",
            "LAYERFS_DAEMON_CAPABILITY": self._capability,
            "LAYERFS_DAEMON_CONTAINER_ID": self.id,
            "LAYERFS_FUSE_HOST": "127.0.0.1",
            "LAYERFS_BENCH_WORKLOAD": "/usr/local/bin/fs-benchmark-workload",
            "LAYERFS_V013_GIT_REFERENCE": "/qualified/git-reference",
        }
        values.update(env or {})
        return self.exec(
            [COORDINATOR, *args],
            deadline=deadline,
            env=values,
            output_limit=output_limit,
            check=check,
        )

    def copy_in(
        self,
        source: os.PathLike[str] | str,
        destination: str,
        *,
        deadline: Deadline,
        max_bytes: int = DEFAULT_OUTPUT_LIMIT,
    ) -> CommandResult:
        path = Path(source).resolve()
        if not path.is_file() or path.stat().st_size > max_bytes:
            raise ValueError("copy-in is limited to one bounded regular file")
        destination = _internal_path(destination)
        return run(
            ["docker", "cp", str(path), f"{self.name}:{destination}"],
            deadline=deadline,
        )

    def copy_out(
        self,
        source: str,
        destination: os.PathLike[str] | str,
        *,
        deadline: Deadline,
        max_bytes: int = DEFAULT_OUTPUT_LIMIT,
    ) -> CommandResult:
        source = _internal_path(source, root="/")
        target = Path(destination).resolve()
        if target.exists():
            raise ValueError("copy-out destination exists")
        size = self.exec(
            ["/usr/bin/stat", "-c", "%s", source],
            deadline=deadline,
            output_limit=128,
        )
        try:
            length = int(size.stdout_text().strip())
        except ValueError as error:
            raise RuntimeFailure("invalid copy-out size") from error
        if length < 0 or length > max_bytes:
            raise RuntimeFailure("copy-out exceeds compact artifact limit")
        return run(
            ["docker", "cp", f"{self.name}:{source}", str(target)],
            deadline=deadline,
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
    host_store: bool = False,
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
        "docker", "create", "--name", name, "--network", "bridge" if host_store else "none",
        "--cpus", str(cpus), "--memory", str(memory_bytes),
        "--memory-swap", str(memory_bytes), "--pids-limit", str(pids),
        "--device", "/dev/fuse", "--cap-add", "SYS_ADMIN",
        "--security-opt", "apparmor=unconfined",
        "--env", "LAYERFS_BENCH_LOCAL_RUNTIME=" + ("0" if host_store else "1"),
        "--env", "LAYERFS_DAEMON_TCP_LISTEN=" + ("0.0.0.0:41273" if host_store else "127.0.0.1:41273"),
        "--env", "LAYERFS_FUSE_HOST=" + ("host.docker.internal" if host_store else "127.0.0.1"),
        "--env", "LAYERFS_V013_GIT_REFERENCE=/qualified/git-reference",
        "--entrypoint", "/bin/sh",
    ]
    if host_store:
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
            host_store=host_store,
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
    host_store: bool = False,
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
    if host_store:
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
    elif host.get("PortBindings") or any(ports.values()):
        raise RuntimeFailure("sample has a published port binding")
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
        "LAYERFS_BENCH_LOCAL_RUNTIME=" + ("0" if host_store else "1"),
        "LAYERFS_DAEMON_TCP_LISTEN=" + ("0.0.0.0:41273" if host_store else "127.0.0.1:41273"),
        "LAYERFS_FUSE_HOST=" + ("host.docker.internal" if host_store else "127.0.0.1"),
    ):
        if value not in environment:
            raise RuntimeFailure("sample loopback environment mismatch")


@dataclass(frozen=True)
class _CacheEntry:
    id: str
    tag: str
    key: str
    created: str
    data_bytes: int
    labels: Mapping[str, str]


class PreparedCache:
    """A finite set of exactly labeled, composed prepared Docker images."""

    def __init__(
        self,
        prefix: str = "layerfs-fsbench-prepared",
        *,
        max_entries: int = 8,
        max_bytes: int = 10 * GIB,
    ) -> None:
        _name(prefix)
        if max_entries <= 0 or max_bytes <= 0:
            raise ValueError("prepared cache limits must be positive")
        self.prefix = prefix
        self.max_entries = max_entries
        self.max_bytes = max_bytes

    def tag(self, key: str) -> str:
        if not _CACHE_KEY.fullmatch(key):
            raise ValueError("prepared cache key must be lowercase hexadecimal")
        return f"{self.prefix}:{key}"

    def lookup(
        self,
        key: str,
        expected_labels: Mapping[str, str],
        *,
        deadline: Deadline,
    ) -> dict | None:
        tag = self.tag(key)
        result = run(
            ["docker", "image", "inspect", tag],
            deadline=deadline,
            output_limit=4 * DEFAULT_OUTPUT_LIMIT,
            check=False,
        )
        if result.timed_out:
            raise CommandFailure(result)
        if result.returncode != 0:
            return None
        values = _json_result(result, "prepared image inspection")
        if not isinstance(values, list) or len(values) != 1:
            raise RuntimeFailure("ambiguous prepared image tag")
        info = values[0]
        labels = info.get("Config", {}).get("Labels") or {}
        required = {
            OWNER_LABEL: OWNER,
            ROLE_LABEL: "prepared",
            CACHE_KEY_LABEL: key,
            CACHE_TAG_LABEL: tag,
            **expected_labels,
        }
        if any(labels.get(name) != value for name, value in required.items()):
            raise RuntimeFailure("prepared cache label mismatch")
        if info.get("Config", {}).get("Volumes") not in (None, {}):
            raise RuntimeFailure("prepared image declares volumes")
        manifest = _read_image_manifest(info["Id"], deadline)
        return {
            "image": info["Id"],
            "cache_key": key,
            "cache_hit": True,
            "manifest": manifest,
            "prepared_bytes": _positive_int(labels.get(PREPARED_BYTES_LABEL), "prepared bytes"),
            "cache_tag": tag,
            "one_shot": False,
        }

    def entries(self, *, deadline: Deadline) -> list[_CacheEntry]:
        listed = run(
            [
                "docker", "image", "ls", "--no-trunc", "--quiet",
                "--filter", f"label={OWNER_LABEL}={OWNER}",
                "--filter", f"label={ROLE_LABEL}=prepared",
            ],
            deadline=deadline,
            output_limit=DEFAULT_OUTPUT_LIMIT,
        )
        ids = sorted(set(listed.stdout_text().split()))
        if not ids:
            return []
        inspected = run(
            ["docker", "image", "inspect", *ids],
            deadline=deadline,
            output_limit=8 * DEFAULT_OUTPUT_LIMIT,
        )
        values = _json_result(inspected, "prepared cache inventory")
        entries = []
        for value in values:
            labels = value.get("Config", {}).get("Labels") or {}
            tag = labels.get(CACHE_TAG_LABEL, "")
            key = labels.get(CACHE_KEY_LABEL, "")
            if not _IMAGE.fullmatch(tag) or not _CACHE_KEY.fullmatch(key):
                raise RuntimeFailure("malformed owned prepared image labels")
            entries.append(
                _CacheEntry(
                    value["Id"],
                    tag,
                    key,
                    value.get("Created", ""),
                    _positive_int(labels.get(PREPARED_BYTES_LABEL), "prepared bytes"),
                    labels,
                )
            )
        return sorted(entries, key=lambda entry: (entry.created, entry.id))

    def evict(
        self,
        *,
        deadline: Deadline,
        protected: Iterable[str] = (),
        incoming_bytes: int = 0,
        incoming_entries: int = 0,
    ) -> list[str]:
        protected_set = set(protected)
        entries = self.entries(deadline=deadline)
        total = sum(entry.data_bytes for entry in entries)
        removed = []
        while len(entries) + incoming_entries > self.max_entries or total + incoming_bytes > self.max_bytes:
            candidate = next(
                (
                    entry
                    for entry in entries
                    if entry.id not in protected_set
                    and entry.tag not in protected_set
                    and not self._in_use(entry.id, deadline)
                ),
                None,
            )
            if candidate is None:
                raise RuntimeFailure("prepared cache capacity is held by protected/active images")
            run(
                ["docker", "image", "rm", candidate.tag],
                deadline=deadline,
                output_limit=DEFAULT_OUTPUT_LIMIT,
            )
            entries.remove(candidate)
            total -= candidate.data_bytes
            removed.append(candidate.id)
        return removed

    def publish(
        self,
        stage_image: str,
        key: str,
        labels: Mapping[str, str],
        *,
        deadline: Deadline,
    ) -> dict:
        tag = self.tag(key)
        if self.lookup(key, labels, deadline=deadline) is not None:
            raise RuntimeFailure("prepared cache key already published")
        run(["docker", "tag", stage_image, tag], deadline=deadline, output_limit=4096)
        result = self.lookup(key, labels, deadline=deadline)
        if result is None:
            raise RuntimeFailure("prepared cache publication missing")
        return result

    @staticmethod
    def _in_use(image_id: str, deadline: Deadline) -> bool:
        result = run(
            ["docker", "ps", "--all", "--quiet", "--filter", f"ancestor={image_id}"],
            deadline=deadline,
            output_limit=DEFAULT_OUTPUT_LIMIT,
        )
        return bool(result.stdout_text().strip())


def create_prepared_image(
    runtime_image: str,
    key: str,
    prepare_args: Sequence[str],
    labels: Mapping[str, str],
    *,
    deadline: Deadline,
    cache: PreparedCache,
    allowed_entries: Iterable[str] = (
        "payload", "reference", "manifest.json", "fixture.json", "selection.tsv", "qualification.tsv", "input-manifest.tsv", "input-qualification.tsv"
    ),
    retain: bool = True,
) -> dict:
    """Create/reuse a sanitized prepared image without moving fixture data to host."""
    cleanup_deadline = deadline
    deadline = Deadline(deadline.end - 5.0)
    if not _CACHE_KEY.fullmatch(key):
        raise ValueError("invalid prepared key")
    if len(prepare_args) < 2 or prepare_args[0] != "infra-prepare" or prepare_args[-1] != PREPARED_ROOT:
        raise ValueError("prepared command must be infra-prepare ending at PREPARED_ROOT")
    runtime = _inspect_image(runtime_image, deadline)
    runtime_id = runtime["Id"]
    expected = {RUNTIME_IMAGE_LABEL: runtime_id, **labels}
    if retain:
        hit = cache.lookup(key, expected, deadline=deadline)
        if hit is not None:
            return hit

    token = uuid.uuid4().hex
    stage_container = _name(f"fsbench-prepare-{token[:16]}")
    runtime_alias = _image(f"fsbench-runtime-stage:{token}")
    temporary_image = _image(f"fsbench-prepare-stage:{token}")
    build_tag = _image(f"fsbench-prepared-build:{token}")
    one_shot_tag = _image(f"fsbench-prepared-once:{token}")
    final_tag = cache.tag(key) if retain else one_shot_tag
    runtime_alias_created = False
    created = False
    temporary_created = False
    build_created = False
    published = False
    manifest: dict | None = None
    try:
        run(
            ["docker", "tag", runtime_id, runtime_alias],
            deadline=deadline,
            output_limit=4096,
        )
        runtime_alias_created = True
        pinned = _inspect_image(runtime_alias, deadline)
        if pinned.get("Id") != runtime_id:
            raise RuntimeFailure("temporary runtime tag changed pinned image identity")
        command = [
            "docker", "create", "--name", stage_container, "--network", "none",
            "--cpus", "2", "--memory", str(2 * GIB), "--memory-swap", str(2 * GIB),
            "--pids-limit", "256", "--entrypoint", "/bin/sh",
        ]
        _labels(command, {OWNER_LABEL: OWNER, ROLE_LABEL: "preparation-stage", **labels})
        command.extend(
            (
                runtime_alias,
                "-ceu",
                "trap 'exit 0' TERM INT; while :; do sleep 3600; done",
            )
        )
        run(command, deadline=deadline, output_limit=4096)
        created = True
        run(["docker", "start", stage_container], deadline=deadline, output_limit=4096)
        _exec_container(
            stage_container,
            [
                "/bin/sh", "-ceu",
                'if test -d "$1"; then rmdir -- "$1"; fi',
                "clear-empty-prepared-root", PREPARED_ROOT,
            ],
            deadline=deadline,
            output_limit=4096,
        )
        prepared = run(
            [
                "docker", "exec", "--env", "LAYERFS_BENCH_LOCAL_RUNTIME=1",
                "--env", f"LAYERFS_V013_IMAGE={runtime_id}",
                stage_container, COORDINATOR, *prepare_args,
            ],
            deadline=deadline,
            output_limit=DEFAULT_OUTPUT_LIMIT,
        )
        manifest_result = _exec_container(
            stage_container,
            ["/bin/cat", f"{PREPARED_ROOT}/manifest.json"],
            deadline=deadline,
            output_limit=DEFAULT_OUTPUT_LIMIT,
        )
        try:
            manifest = json.loads(manifest_result.stdout)
        except (UnicodeDecodeError, json.JSONDecodeError) as error:
            raise RuntimeFailure("invalid preparation manifest") from error
        if not isinstance(manifest, dict):
            raise RuntimeFailure("preparation manifest is not an object")
        _validate_prepared_tree(stage_container, allowed_entries, deadline)
        prepared_bytes_result = _exec_container(
            stage_container,
            ["/usr/bin/du", "-sb", PREPARED_ROOT],
            deadline=deadline,
            output_limit=256,
        )
        prepared_bytes = _positive_int(
            prepared_bytes_result.stdout_text().split()[0], "prepared directory bytes"
        )
        if retain and prepared_bytes <= cache.max_bytes:
            cache.evict(
                deadline=deadline,
                incoming_bytes=prepared_bytes,
                incoming_entries=1,
            )
            role = "prepared"
        else:
            retain = False
            final_tag = one_shot_tag
            role = "prepared-one-shot"
        run(
            ["docker", "commit", "--pause=true", stage_container, temporary_image],
            deadline=deadline,
            output_limit=4096,
        )
        temporary_created = True
        image_labels = {
            OWNER_LABEL: OWNER,
            ROLE_LABEL: role,
            CACHE_KEY_LABEL: key,
            CACHE_TAG_LABEL: final_tag,
            RUNTIME_IMAGE_LABEL: runtime_id,
            PREPARED_BYTES_LABEL: str(prepared_bytes),
            **labels,
        }
        dockerfile_bytes = _composition_dockerfile(
            runtime_alias,
            temporary_image,
            image_labels,
        )
        run(
            ["docker", "build", "--tag", build_tag, "-"],
            deadline=deadline,
            input_bytes=dockerfile_bytes,
            output_limit=4 * DEFAULT_OUTPUT_LIMIT,
        )
        build_created = True
        built = _inspect_image(build_tag, deadline)
        if built.get("Config", {}).get("Volumes") not in (None, {}):
            raise RuntimeFailure("prepared image declares volumes")
        if (built.get("Config", {}).get("Labels") or {}) != {
            **(runtime.get("Config", {}).get("Labels") or {}),
            **image_labels,
        }:
            raise RuntimeFailure("prepared image label/source binding mismatch")
        if retain and cache.lookup(key, expected, deadline=deadline) is not None:
            raise RuntimeFailure("prepared cache key raced publication")
        run(["docker", "tag", built["Id"], final_tag], deadline=deadline, output_limit=4096)
        final = _inspect_image(final_tag, deadline)
        final_manifest = _read_image_manifest(final["Id"], deadline)
        if final_manifest != manifest:
            raise RuntimeFailure("prepared image manifest changed during composition")
        published = True
        return {
            "image": final["Id"],
            "cache_key": key,
            "cache_hit": False,
            "manifest": manifest,
            "prepared_bytes": prepared_bytes,
            "cache_tag": final_tag,
            "one_shot": not retain,
        }
    finally:
        if created and cleanup_deadline.remaining() > 0.05:
            run(
                ["docker", "rm", "--force", stage_container],
                deadline=cleanup_deadline,
                output_limit=4096,
                check=False,
            )
        if temporary_created and cleanup_deadline.remaining() > 0.05:
            run(
                ["docker", "image", "rm", temporary_image],
                deadline=cleanup_deadline,
                output_limit=4096,
                check=False,
            )
        if runtime_alias_created and cleanup_deadline.remaining() > 0.05:
            run(
                ["docker", "image", "rm", runtime_alias],
                deadline=cleanup_deadline,
                output_limit=4096,
                check=False,
            )
        if build_created and cleanup_deadline.remaining() > 0.05:
            run(
                ["docker", "image", "rm", build_tag],
                deadline=cleanup_deadline,
                output_limit=4096,
                check=False,
            )
        if not published and cleanup_deadline.remaining() > 0.05:
            run(
                ["docker", "image", "rm", final_tag],
                deadline=cleanup_deadline,
                output_limit=4096,
                check=False,
            )


def _composition_dockerfile(
    runtime_alias: str,
    temporary_image: str,
    labels: Mapping[str, str],
) -> bytes:
    """Render a no-context Dockerfile using local tags, never registry-like IDs."""
    _image(runtime_alias)
    _image(temporary_image)
    if runtime_alias.startswith("sha256:"):
        raise ValueError("generated Dockerfile FROM requires a pinned local tag")
    lines = [
        f"FROM {runtime_alias}",
        f"COPY --from={temporary_image} {PREPARED_ROOT} {PREPARED_ROOT}",
    ]
    for label_key, value in sorted(labels.items()):
        if not _LABEL_KEY.fullmatch(label_key) or "\x00" in value:
            raise ValueError("invalid prepared image label")
        lines.append(f"LABEL {label_key}={json.dumps(value)}")
    return ("\n".join(lines) + "\n").encode()


def _exec_container(
    container: str,
    argv: Sequence[str],
    *,
    deadline: Deadline,
    output_limit: int,
    check: bool = True,
) -> CommandResult:
    _name(container)
    return run(
        ["docker", "exec", container, *argv],
        deadline=deadline,
        output_limit=output_limit,
        check=check,
    )


def _last_json_object(data: bytes, label: str) -> dict:
    for line in reversed(data.decode("utf-8", errors="strict").splitlines()):
        try:
            value = json.loads(line)
        except json.JSONDecodeError:
            continue
        if isinstance(value, dict):
            return value
    raise RuntimeFailure(f"missing {label}")


def _read_image_manifest(image: str, deadline: Deadline) -> dict:
    result = run(
        [
            "docker", "run", "--rm", "--network", "none",
            "--entrypoint", "/bin/cat", image, f"{PREPARED_ROOT}/manifest.json",
        ],
        deadline=deadline,
        output_limit=DEFAULT_OUTPUT_LIMIT,
    )
    try:
        value = json.loads(result.stdout)
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise RuntimeFailure("invalid prepared manifest") from error
    if not isinstance(value, dict):
        raise RuntimeFailure("prepared manifest is not an object")
    return value


def _validate_prepared_tree(
    container: str,
    allowed_entries: Iterable[str],
    deadline: Deadline,
) -> None:
    allowed = {_relative(value) for value in allowed_entries}
    if any("/" in value for value in allowed):
        raise ValueError("prepared allowlist entries must be direct children")
    names = _exec_container(
        container,
        [
            "/usr/bin/find", PREPARED_ROOT, "-mindepth", "1", "-maxdepth", "1",
            "-printf", "%f\\n",
        ],
        deadline=deadline,
        output_limit=64 * 1024,
    ).stdout_text().splitlines()
    if not names or "manifest.json" not in names or not set(names) <= allowed:
        raise RuntimeFailure("prepared root differs from direct-child allowlist: " + repr(sorted(set(names) - allowed)))
    forbidden = _exec_container(
        container,
        [
            "/bin/sh", "-ceu",
            r"""find "$1" -xdev \
  \( -type b -o -type c -o -type p -o -type s \
     -o -name .ssh -o -name .aws -o -name .docker -o -name .netrc \
     -o -name credentials -o -name id_rsa -o -name id_ed25519 \
     -o -name capability -o -name daemon.sock -o -name docker.sock \
     -o -name '*-journal' -o -name '*.sqlite-wal' -o -name '*.sqlite-shm' \) \
  -print -quit""",
            "validate-prepared", PREPARED_ROOT,
        ],
        deadline=deadline,
        output_limit=4096,
    ).stdout_text().strip()
    if forbidden:
        raise RuntimeFailure("prepared root contains credentials, runtime state, or SQLite sidecars")


def _positive_int(value, label: str) -> int:
    try:
        number = int(value)
    except (TypeError, ValueError) as error:
        raise RuntimeFailure(f"invalid {label}") from error
    if number <= 0:
        raise RuntimeFailure(f"nonpositive {label}")
    return number


def prepare_sample(
    container: SampleContainer,
    *,
    mode: str,
    deadline: Deadline,
    prepared_root: str = PREPARED_ROOT,
    sample_root: str = SAMPLE_ROOT,
    master_store_relative: str | None = "payload/store.sqlite",
    reuse_prepared_input: bool = False,
    output_store_relative: str | Sequence[str] = (
        "work/store.sqlite", "payload/store.sqlite"
    ),
) -> dict:
    """Create writable sample state, optionally reusing native source fixtures."""
    prepared_root = _internal_path(prepared_root)
    sample_root = _internal_path(sample_root)
    if prepared_root == sample_root:
        raise ValueError("prepared and sample roots must differ")
    if mode not in {"clone", "fresh", "fresh-output"}:
        raise ValueError("sample setup mode must be clone, fresh, or fresh-output")
    if reuse_prepared_input and mode != "fresh-output":
        raise ValueError("prepared input reuse requires fresh-output setup")
    output_store_relatives = (
        [_relative(output_store_relative)]
        if isinstance(output_store_relative, str)
        else [_relative(value) for value in output_store_relative]
    )
    if not output_store_relatives:
        raise ValueError("fresh-output requires at least one forbidden Store path")
    if mode in {"clone", "fresh"} and master_store_relative is None:
        raise ValueError("clone setup requires a master Store path")
    if master_store_relative is not None:
        master_store_relative = _relative(master_store_relative)
    started = time.monotonic_ns()
    container.exec(
        [
            "/bin/sh", "-ceu",
            'case "$2" in /var/lib/fs-bench/sample|/var/lib/fs-bench/sample/*) ;; *) exit 64;; esac; '
            'test -d "$1"; rm -rf -- "$2"; test ! -e "$2"',
            "reset-sample", prepared_root, sample_root,
        ],
        deadline=deadline,
        output_limit=4096,
    )
    if reuse_prepared_input:
        metadata_copy = container.exec(
            [
                "/bin/sh", "-ceu",
                'test -d "$1/payload/input"; mkdir -p "$2/payload"; '
                'find "$1" -maxdepth 1 -type f -exec cp -a --target-directory="$2" -- {} +',
                "reuse-prepared-input", prepared_root, sample_root,
            ],
            deadline=deadline,
            output_limit=4096,
        )
        method = "not-applicable"
        reflink_wall_ns = None
        fallback_wall_ns = None
    else:
        reflink = container.exec(
            ["/bin/cp", "-a", "--reflink=always", prepared_root, sample_root],
            deadline=deadline,
            output_limit=64 * 1024,
            check=False,
        )
        if reflink.returncode == 0:
            method = "linux-reflink"
            fallback_wall_ns = 0
        else:
            cleanup = container.exec(
                ["/bin/rm", "-rf", "--", sample_root],
                deadline=deadline,
                output_limit=4096,
            )
            copied = container.exec(
                ["/bin/cp", "-a", "--reflink=never", prepared_root, sample_root],
                deadline=deadline,
                output_limit=64 * 1024,
            )
            method = "byte-copy"
            fallback_wall_ns = cleanup.wall_ns + copied.wall_ns
        reflink_wall_ns = reflink.wall_ns
    manifest_pair = container.exec(
        [
            "/usr/bin/sha256sum",
            f"{prepared_root}/manifest.json",
            f"{sample_root}/manifest.json",
        ],
        deadline=deadline,
        output_limit=1024,
    ).stdout_text().splitlines()
    if len(manifest_pair) != 2:
        raise RuntimeFailure("sample manifest identity missing")
    manifest_hashes = [line.split()[0] for line in manifest_pair]
    if manifest_hashes[0] != manifest_hashes[1]:
        raise RuntimeFailure("sample manifest differs from prepared master")

    receipt = {
        "setup_mode": mode,
        "clone_method": method,
        "reflink_attempt_wall_ns": reflink_wall_ns,
        "fallback_wall_ns": fallback_wall_ns,
        "manifest_sha256": manifest_hashes[0],
    }
    if reuse_prepared_input:
        receipt.update(
            fixture_reuse_method="prepared-image-source",
            prepared_input_root=f"{prepared_root}/payload/input",
            metadata_copy_wall_ns=metadata_copy.wall_ns,
        )
    if mode in {"clone", "fresh"}:
        master = f"{prepared_root}/{master_store_relative}"
        sample = f"{sample_root}/{master_store_relative}"
        store = container.exec(
            [
                "/bin/sh", "-ceu",
                'test -f "$1"; test -f "$2"; chmod 0600 "$2"; '
                'sha256sum "$1" "$2"; stat -c "%d %i %s %b" "$1" "$2"',
                "verify-store-clone", master, sample,
            ],
            deadline=deadline,
            output_limit=2048,
        ).stdout_text().splitlines()
        if len(store) != 4:
            raise RuntimeFailure("sample Store clone receipt malformed")
        master_sha, sample_sha = store[0].split()[0], store[1].split()[0]
        master_stat = tuple(int(value) for value in store[2].split())
        sample_stat = tuple(int(value) for value in store[3].split())
        if master_sha != sample_sha or master_stat[2] != sample_stat[2]:
            raise RuntimeFailure("sample Store bytes differ from master")
        if master_stat[:2] == sample_stat[:2]:
            raise RuntimeFailure("sample Store is hard-linked to master")
        receipt.update(
            master_store_sha256=master_sha,
            sample_store_sha256=sample_sha,
            store_bytes=sample_stat[2],
            master_store_device=master_stat[0],
            master_store_inode=master_stat[1],
            sample_store_device=sample_stat[0],
            sample_store_inode=sample_stat[1],
            sample_store_allocated_bytes=sample_stat[3] * 512,
        )
    else:
        outputs = [f"{sample_root}/{relative}" for relative in output_store_relatives]
        absent = container.exec(
            [
                "/bin/sh", "-ceu",
                'for path do test ! -e "$path"; done',
                "fresh-output", *outputs,
            ],
            deadline=deadline,
            output_limit=4096,
            check=False,
        )
        if absent.returncode != 0:
            raise RuntimeFailure("initialization output Store is not fresh/absent")
        receipt["fresh_output_stores"] = outputs
    sync = container.exec(
        ["/bin/sync", "-f", sample_root],
        deadline=deadline,
        output_limit=4096,
    )
    receipt["sync_wall_ns"] = sync.wall_ns
    receipt["setup_wall_ns"] = time.monotonic_ns() - started
    receipt["prepared_root"] = prepared_root
    receipt["sample_root"] = sample_root
    return receipt


def _pure_self_check() -> None:
    """Resource-free checks for generated Docker input and deadline units."""
    dockerfile = _composition_dockerfile(
        "fsbench-runtime-stage:0123456789abcdef",
        "fsbench-prepare-stage:0123456789abcdef",
        {OWNER_LABEL: OWNER},
    ).decode()
    assert dockerfile.startswith("FROM fsbench-runtime-stage:0123456789abcdef\n")
    assert "FROM sha256:" not in dockerfile
    try:
        _composition_dockerfile(
            "sha256:" + "0" * 64,
            "fsbench-prepare-stage:0123456789abcdef",
            {OWNER_LABEL: OWNER},
        )
    except ValueError:
        pass
    else:
        raise AssertionError("raw image ID accepted in generated FROM")
    deadline = Deadline(time.monotonic() + 1.0)
    assert 0.0 < deadline.remaining() <= 1.0




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


if __name__ == "__main__":
    _pure_self_check()
    print("runtime_self_check=pass")
