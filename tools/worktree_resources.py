"""Worktree-owned development resources; no shared writable Cargo caches."""

from contextlib import contextmanager
from datetime import datetime, timezone
import fcntl
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import signal
import subprocess
import uuid

from cleanup_state import Refused, docker_state, ensure_idle, git, open_files, overlaps, run

TOOLCHAIN = "1.85.1"
LABEL = "layerfs.dev.worktree"


def timestamp():
    return datetime.now(timezone.utc).isoformat()


def write(path, value):
    temp = path.with_name(path.name + ".new-" + uuid.uuid4().hex)
    temp.write_text(json.dumps(value, indent=2) + "\n")
    temp.replace(path)


def digest(path):
    h = hashlib.sha256()
    with Path(path).open("rb") as stream:
        for data in iter(lambda: stream.read(1024 * 1024), b""):
            h.update(data)
    return h.hexdigest()


def identity(path):
    path = Path(path).resolve()
    label = re.sub(r"[^a-z0-9-]+", "-", path.name.lower()).strip("-")[:24] or "worktree"
    return label + "-" + hashlib.sha256(os.fsencode(path)).hexdigest()[:12]


def load(path):
    path = Path(path).resolve()
    record = json.loads(path.read_text())
    if record.get("version") != 1 or path != Path(record["state"]) / "resources.json":
        raise Refused("Invalid resource registration")
    if identity(record["worktree"]) != record["id"]:
        raise Refused("Worktree identity mismatch")
    return record


def image_id(reference):
    # Resolve an installed image once; subsequent commands use its immutable ID.
    return json.loads(run(["docker", "image", "inspect", reference]).stdout)[0]["Id"]


def register(args):
    source = Path(args.worktree).resolve()
    if Path(git(source, "rev-parse", "--show-toplevel").decode().strip()).resolve() != source:
        raise Refused("Select a Git checkout root")
    if not (source / "Cargo.toml").is_file():
        raise Refused("Worktree has no active workspace manifest")
    state = Path(args.root).expanduser().resolve() / identity(source)
    if overlaps(state, source):
        raise Refused("Resource state must be outside the worktree")
    images = {k: image_id(getattr(args, k + "_image")) for k in ("build", "runtime", "sandbox")}
    record = {
        "version": 1,
        "id": identity(source),
        "owner": args.owner,
        "worktree": str(source),
        "state": str(state),
        "toolchain": TOOLCHAIN,
        "macos_target": str(source / "target"),
        "macos_cargo_home": str(state / "cargo-home-macos"),
        "linux_target_volume": "lfs-dev-" + identity(source) + "-target",
        "linux_cargo_volume": "lfs-dev-" + identity(source) + "-cargo",
        "images": images,
    }
    path = state / "resources.json"
    if path.exists():
        if load(path) != record:
            raise Refused("Registration already exists with different inputs; do not silently reassign it")
    else:
        state.mkdir(mode=0o700, parents=True, exist_ok=True)
        for child in ("builds", "runs", "artifacts", "leases"):
            (state / child).mkdir(mode=0o700, exist_ok=True)
        write(path, record)
    print(path)


@contextmanager
def lease(record, platform):
    """Fail immediately for this worktree/platform; never wait on another agent."""
    path = Path(record["state"]) / "leases" / (platform + ".lock")
    with path.open("a+") as stream:
        try:
            fcntl.flock(stream, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError as error:
            raise Refused(f"This worktree's {platform} cache has an active owner") from error
        stream.seek(0)
        stream.truncate()
        stream.write(json.dumps({"pid": os.getpid(), "at": timestamp()}))
        stream.flush()
        try:
            yield
        finally:
            fcntl.flock(stream, fcntl.LOCK_UN)


def volume(record, key):
    name = record[key]
    names = run(["docker", "volume", "ls", "--format", "{{.Name}}"]).stdout.decode().splitlines()
    if name not in names:
        run(
            [
                "docker",
                "volume",
                "create",
                "--label",
                f"{LABEL}={record['id']}",
                "--label",
                "layerfs.dev.role=" + key,
                name,
            ]
        )
    info = json.loads(run(["docker", "volume", "inspect", name]).stdout)[0]
    labels = info.get("Labels") or {}
    if labels.get(LABEL) != record["id"] or labels.get("layerfs.dev.role") != key or info["Driver"] != "local":
        raise Refused("Named volume exists without matching ownership: " + name)
    return name


def closed_volumes(names):
    for container in docker_state():
        if container["running"]:
            for mount in container["mounts"]:
                if mount.get("Name") in names:
                    raise Refused("Running container owns a selected cache: " + container["name"])


def environment(record, platform):
    for name in ("RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "CARGO_BUILD_TARGET", "CARGO_BUILD_RUSTFLAGS", "RUSTC"):
        if os.environ.get(name):
            raise Refused(f"Unset {name}; this launcher uses the repository's native ARM64 profile")
    if platform == "macos":
        if os.uname().sysname != "Darwin":
            raise Refused("macos builds require a macOS host")
        return {**os.environ, "CARGO_HOME": record["macos_cargo_home"], "CARGO_TARGET_DIR": record["macos_target"]}
    return {"CARGO_HOME": "/cargo-home", "CARGO_TARGET_DIR": "/work/target"}


def docker_build(record, command, extra=(), network="none"):
    for key in ("linux_target_volume", "linux_cargo_volume"):
        volume(record, key)
    Path(record["macos_target"]).mkdir(parents=True, exist_ok=True)
    env = environment(record, "linux")
    return [
        "docker",
        "run",
        "--rm",
        "--pull",
        "never",
        "--network",
        network,
        "--label",
        f"{LABEL}={record['id']}",
        "--mount",
        f"type=bind,src={record['worktree']},dst=/work,readonly",
        "--mount",
        f"type=volume,src={record['linux_target_volume']},dst=/work/target,volume-nocopy",
        "--mount",
        f"type=volume,src={record['linux_cargo_volume']},dst=/cargo-home,volume-nocopy",
        *[v for k, value in env.items() for v in ("-e", k + "=" + value)],
        *extra,
        "-w",
        "/work",
        "--entrypoint",
        command[0],
        record["images"]["build"],
        *command[1:],
    ]


def execute(command, cwd, env=None, timeout=None, log=None):
    """Stream into a retained log, never buffer an entire compiler output in memory."""
    container = None
    if command[:2] == ["docker", "run"]:
        create = ["docker", "create", *command[2:]]
        create.remove("--rm")
        container = run(create).stdout.decode().strip()
        write(Path(log).parent / "container.json", {"id": container, "create": create})
        command = ["docker", "start", "--attach", container]
    try:
        with Path(log).open("wb") if log else open(os.devnull, "wb") as stream:
            process = subprocess.Popen(
                command, cwd=cwd, env=env, stdout=stream, stderr=subprocess.STDOUT, start_new_session=True
            )
            try:
                status = process.wait(timeout=timeout)
            except BaseException:
                os.killpg(process.pid, signal.SIGKILL)
                process.wait()
                raise
        if status:
            raise Refused(f"Command exited {status}; inspect {log}")
    finally:
        if container:
            info = json.loads(run(["docker", "inspect", container], timeout=5).stdout)[0]
            if info["State"]["Running"]:
                run(["docker", "kill", container], timeout=5)
            elif "status" in locals() and status == 0 and info["State"]["ExitCode"] != 0:
                raise Refused(f"Container exited {info['State']['ExitCode']}; inspect {log}")
            # Failed/interrupted containers remain stopped for diagnosis.
            if "status" in locals() and status == 0:
                run(["docker", "rm", container], timeout=5)


def inputs(record):
    root = Path(record["worktree"])
    selected = [
        ".cargo",
        "Cargo.toml",
        "Cargo.lock",
        "core/crates",
        "engine/vendor",
        "engine/crates",
        "api/agent",
        "api/backend",
        "api/runtime",
        "api/package.json",
        "api/cli/src",
        "api/cli/test",
        "api/cli/package.json",
    ]
    result = {}
    for relative in selected:
        source = root / relative
        paths = []
        if source.is_file():
            paths.append(source)
        else:
            for base, dirs, files in os.walk(source):
                if any((Path(base) / d).is_symlink() for d in dirs):
                    raise Refused("Source directory symlinks need explicit handling: " + base)
                dirs[:] = [
                    d for d in dirs if not d.startswith("target") and d not in {"node_modules", ".git", "__pycache__"}
                ]
                paths.extend(Path(base) / name for name in sorted(files))
        for path in paths:
            if path.is_symlink():
                raise Refused("Build inputs require explicit handling of source symlinks: " + str(path))
            result[str(path.relative_to(root))] = digest(path)
    encoded = json.dumps(result, sort_keys=True).encode()
    return {
        "head": git(root, "rev-parse", "HEAD").decode().strip(),
        "files": result,
        "sha256": hashlib.sha256(encoded).hexdigest(),
    }


def copy_dependencies(seed, destination):
    seed = Path(seed).resolve()
    children = [seed / name for name in ("registry", "git") if (seed / name).exists()]
    ensure_idle(children, [], docker_state(), open_files())
    for child in children:
        dest = Path(destination) / child.name
        if dest.exists():
            raise Refused("Dependency seed destination already exists: " + str(dest))
        shutil.copytree(child, dest, symlinks=True)


def operation(record, name):
    path = Path(record["state"]) / "builds" / (name + "-" + uuid.uuid4().hex)
    path.mkdir(mode=0o700)
    return path
