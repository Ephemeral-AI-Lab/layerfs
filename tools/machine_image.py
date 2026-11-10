"""Prepare versioned machine images with public LayerFS artifacts installed.

Preparation is explicit and separate from machine.start. No warm pool, saved
memory, host-specific seed image or provider PATH wrapper is used. Each command
is bounded; failed seeds are retained for diagnosis and never published.
"""

import argparse
import hashlib
import json
import os
import platform
import secrets
import shlex
import shutil
import socket
import struct
import subprocess
import sys
import tarfile
import tempfile
import time
import uuid
from pathlib import Path

from cleanup_state import Refused
from linux_bundle import BINARIES, bundle_hash, protocol
from worktree_resources import digest, write

PAYLOAD = Path(__file__).with_name("machine_image_guest")
ARCHES = {"arm64": "aarch64", "aarch64": "aarch64", "x86_64": "x86_64"}


def run(command, env, receipt, stage, *, input=None, timeout=120):
    started = time.monotonic_ns()
    try:
        result = subprocess.run(command, env=env, input=input, capture_output=True, timeout=timeout, check=False)
    except subprocess.TimeoutExpired as error:
        receipt["status"] = "uncertain"
        receipt["steps"].append(
            {"stage": stage, "command": command, "elapsed_ns": time.monotonic_ns() - started, "outcome": "Unknown"}
        )
        write(Path(receipt["work"]) / "receipt.json", receipt)
        Path(receipt["work"], stage + ".log").write_bytes((error.stdout or b"") + (error.stderr or b""))
        raise Refused(
            f"{stage} timed out; outcome unknown, inspect {receipt['work']}/{stage}.log; nothing was replayed"
        ) from error
    row = {"stage": stage, "command": command, "elapsed_ns": time.monotonic_ns() - started, "exit": result.returncode}
    receipt["steps"].append(row)
    write(Path(receipt["work"]) / "receipt.json", receipt)
    Path(receipt["work"], stage + ".log").write_bytes(result.stdout + result.stderr)
    print(f"{stage}: {row['elapsed_ns'] / 1e9:.3f} s (exit {result.returncode})", flush=True)
    if result.returncode:
        raise Refused(f"{stage} failed; inspect {receipt['work']}/{stage}.log; seed retained")
    return result.stdout


def bundle_manifest(bundle):
    manifest = json.loads((bundle / "bundle.json").read_text())
    arch = ARCHES[platform.machine()]
    expected = protocol(Path(__file__).resolve().parents[1])
    if manifest.get("platform") != "linux-" + arch or manifest.get("protocol", expected) != expected:
        raise Refused("bundle architecture or protocol differs from this builder")
    # Metadata checks are followed by a real guest worker hello before sealing.
    for name in (*BINARIES, "sandbox-image.tar"):
        if digest(bundle / name) != manifest["files"][name]:
            raise Refused("bundle file digest differs: " + name)
    content = manifest["image"].removeprefix("sha256:")
    if len(content) != 64 or any(c not in "0123456789abcdef" for c in content):
        raise Refused("bundle image identity is not sha256")
    return {
        **manifest,
        "protocol": expected,
        "runtime": manifest.get("runtime") or bundle_hash({name: manifest["files"][name] for name in BINARIES}),
    }


def image_identity(provider, manifest, base):
    payload = {p.name: digest(p) for p in sorted(PAYLOAD.iterdir()) if p.is_file()}
    inputs = {
        "schema": 1,
        "provider": provider,
        "bundle": manifest["hash"],
        "platform": manifest["platform"],
        "base": base,
        "payload": payload,
    }
    return hashlib.sha256(json.dumps(inputs, sort_keys=True).encode()).hexdigest()


def publish(root, receipt, manifest, os_image):
    value = {
        "version": 1,
        "provider": receipt["provider"],
        "platform": manifest["platform"],
        "protocol": manifest["protocol"],
        "image": manifest["image"],
        "bundle": manifest["hash"],
        "runtime": manifest.get("runtime"),
        "sanitized": True,
        "identity": receipt["identity"],
        "os_image": os_image,
    }
    folder = root / receipt["provider"] / manifest["platform"] / manifest["image"].removeprefix("sha256:")
    folder.mkdir(parents=True, exist_ok=True)
    # The immutable image is complete before this one atomic pointer publication.
    write(folder / "current.json", value)
    receipt["status"] = "sealed"
    receipt["mapping"] = str(folder / "current.json")
    write(Path(receipt["work"]) / "receipt.json", receipt)
    print(json.dumps(value), flush=True)
    return value


def seed_template(base):
    return {
        "vmType": "vz",
        "cpus": 2,
        "memory": "4GiB",
        "disk": "12GiB",
        "images": [{"location": str(base)}],
        "mounts": [],
        "containerd": {"system": False, "user": False},
        "ssh": {"loadDotSSHPubKeys": False, "forwardAgent": False},
        "user": {"name": "layerfs-image-build", "uid": 1000, "home": "/home/layerfs-image-build", "shell": "/bin/bash"},
    }


def guest(env, receipt, script, stage):
    config = Path(receipt["lima_home"]) / "seed" / "ssh.config"
    command = ["ssh", "-F", str(config), "lima-seed", "--", shlex.join(["sudo", "-n", "--", "sh", "-c", script])]
    return run(command, env, receipt, stage)


def copy_guest(env, receipt, source, destination, stage):
    archive = Path(receipt["work"]) / (stage + ".tar")
    with tarfile.open(archive, "w") as stream:
        stream.add(
            source,
            arcname=".",
            filter=lambda member: (
                None if "__pycache__" in Path(member.name).parts or member.name.endswith(".pyc") else member
            ),
        )
    config = Path(receipt["lima_home"]) / "seed" / "ssh.config"
    command = [
        "ssh",
        "-F",
        str(config),
        "lima-seed",
        "--",
        shlex.join(["sudo", "-n", "--", "sh", "-c", 'mkdir -p "$1" && tar -C "$1" -xf -', "sh", destination]),
    ]
    started = time.monotonic_ns()
    with archive.open("rb") as source_file:
        result = subprocess.run(command, env=env, stdin=source_file, capture_output=True, timeout=120, check=False)
    receipt["steps"].append(
        {
            "stage": stage,
            "bytes": archive.stat().st_size,
            "elapsed_ns": time.monotonic_ns() - started,
            "exit": result.returncode,
        }
    )
    write(Path(receipt["work"]) / "receipt.json", receipt)
    Path(receipt["work"], stage + ".log").write_bytes(result.stdout + result.stderr)
    archive.unlink()
    if result.returncode:
        raise Refused(f"{stage} failed; seed retained")


def worker_check(manifest):
    # The secret is generated in the guest, never printed or put in an argv.
    return (
        "python3 - <<'VERIFY'\n"
        + """import json, os, pathlib, secrets, socket, struct, subprocess, time
secret = secrets.token_hex(32)
path = pathlib.Path('/tmp/layerfs-image-secret')
path.write_text(secret)
path.chmod(0o600)
env = dict(os.environ, LAYERFS_WORKER_LISTEN='127.0.0.1:7070', LAYERFS_WORKER_SECRET=str(path))
p = subprocess.Popen(['/layerfs/bundle/layerfs', 'worker'], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
try:
    deadline = time.monotonic() + 10
    while True:
        try:
            s = socket.create_connection(('127.0.0.1', 7070), timeout=1)
            break
        except OSError:
            if p.poll() is not None or time.monotonic() >= deadline:
                raise RuntimeError('worker failed readiness')
            time.sleep(.05)
    with s:
        s.settimeout(2)
        body = json.dumps({'secret': secret}).encode()
        s.sendall(struct.pack('>I', len(body)) + body)
        def exact(n):
            data = bytearray()
            while len(data) < n:
                chunk = s.recv(n - len(data))
                if not chunk: raise RuntimeError('truncated worker hello')
                data.extend(chunk)
            return data
        length = struct.unpack('>I', exact(4))[0]
        if length > 4096: raise RuntimeError('oversized worker hello')
        hello = json.loads(exact(length))
        assert hello['ready'] is True
"""
        + f"        assert hello['protocol'] == {manifest['protocol']!r}\n        assert hello['bundle'] == {manifest['hash']!r}\n"
        + """finally:
    p.terminate()
    p.wait(timeout=5)
    path.unlink(missing_ok=True)
VERIFY"""
    )


def lima(root, work, bundle, base, manifest, receipt):
    # UNIX_PATH_MAX on macOS also counts Lima's instance socket suffix.
    lima_home = Path(tempfile.mkdtemp(prefix="lfs-image-", dir="/tmp"))
    receipt["lima_home"] = str(lima_home)
    env = {**os.environ, "LIMA_HOME": str(lima_home)}
    run(
        ["limactl", "create", "--name", "seed", "--tty=false", "-"],
        env,
        receipt,
        "seed-create",
        input=json.dumps(seed_template(base)).encode(),
    )
    run(["limactl", "start", "--tty=false", "seed"], env, receipt, "seed-boot")
    copy_guest(env, receipt, bundle, "/layerfs/bundle", "install-bundle")
    copy_guest(env, receipt, PAYLOAD, "/opt/layerfs-image", "install-payload")
    guest(env, receipt, worker_check(manifest), "worker-hello")
    guest(env, receipt, "sh /opt/layerfs-image/prepare.sh", "prepare-boot")
    guest(env, receipt, "python3 /opt/layerfs-image/sanitize.py", "sanitize-identity")
    run(["limactl", "stop", "seed"], env, receipt, "seed-stop")
    return seal_lima(root, work, manifest, receipt, env)


def seal_lima(root, work, manifest, receipt, env):
    lima_home = Path(receipt["lima_home"])
    final = root / "objects" / receipt["identity"]
    final.mkdir(parents=True, exist_ok=True)
    image = final / "root.img"
    if image.exists():
        raise Refused("refusing to overwrite an existing immutable image")
    disk = lima_home / "seed/disk"
    if not disk.is_file():
        disk = lima_home / "seed/diffdisk"
    # Flatten Lima 2.x's disk (or the older diffdisk) without backing references.
    temporary = work / "root.img.partial"
    run(["qemu-img", "convert", "-O", "raw", str(disk), str(temporary)], env, receipt, "flatten-image")
    temporary.chmod(0o444)
    temporary.rename(image)
    run(["limactl", "delete", "--force", "seed"], env, receipt, "seed-remove")
    shutil.rmtree(lima_home)
    return publish(root, receipt, manifest, str(image))


def apple(root, work, bundle, base, manifest, receipt):
    context = work / "context"
    context.mkdir()
    # Symlinks outside a build context are not followed; use private APFS clones
    # when available and ordinary private copies elsewhere.
    shutil.copytree(bundle, context / "bundle", copy_function=shutil.copy2)
    (context / "Dockerfile").write_text(f"FROM {base}\nCOPY bundle/ /layerfs/bundle/\n")
    tag = "layerfs-machine:" + receipt["identity"]
    run(
        [
            "container",
            "build",
            "--progress",
            "plain",
            "--output",
            "type=oci,compression=uncompressed",
            "--tag",
            tag,
            str(context),
        ],
        dict(os.environ),
        receipt,
        "oci-build",
    )
    verify_apple(tag, manifest, receipt)
    # No seed instance or user credential was installed in this OCI build.
    return publish(root, receipt, manifest, tag)


def verify_apple(tag, manifest, receipt):
    name = "lfs-image-check-" + uuid.uuid4().hex[:12]
    receipt["verification_container"] = name
    env = dict(os.environ)
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        port = listener.getsockname()[1]
    run(
        [
            "container",
            "run",
            "--detach",
            "--name",
            name,
            "--cpus",
            "2",
            "--memory",
            "2G",
            "--publish",
            f"127.0.0.1:{port}:7070",
            tag,
            "sh",
            "-c",
            "trap 'exit 0' TERM; sleep infinity & wait",
        ],
        env,
        receipt,
        "oci-check-create",
    )
    secret = secrets.token_hex(32)
    launch = "umask 077; cat > /tmp/image-check-secret; LAYERFS_WORKER_SECRET=/tmp/image-check-secret LAYERFS_WORKER_LISTEN=0.0.0.0:7070 nohup /layerfs/bundle/layerfs worker > /tmp/image-check.log 2>&1 < /dev/null &"
    run(["container", "exec", "-i", name, "sh", "-c", launch], env, receipt, "oci-worker-launch", input=secret.encode())
    # The slim OCI base intentionally needs no Python. Verify from the host,
    # over a private loopback port, using the real authenticated worker hello.
    started = time.monotonic_ns()
    deadline = time.monotonic() + 10
    while True:
        try:
            stream = socket.create_connection(("127.0.0.1", port), timeout=1)
            break
        except OSError:
            if time.monotonic() >= deadline:
                raise Refused("the OCI worker never became reachable") from None
            time.sleep(0.05)
    with stream:
        stream.settimeout(2)
        body = json.dumps({"secret": secret}).encode()
        stream.sendall(struct.pack(">I", len(body)) + body)

        def exact(length):
            data = bytearray()
            while len(data) < length:
                chunk = stream.recv(length - len(data))
                if not chunk:
                    raise Refused("the OCI worker hello was truncated")
                data.extend(chunk)
            return data

        length = struct.unpack(">I", exact(4))[0]
        if length > 4096:
            raise Refused("the OCI worker hello is oversized")
        hello = json.loads(exact(length))
        if (
            hello.get("ready") is not True
            or hello.get("protocol") != manifest["protocol"]
            or hello.get("bundle") != manifest["hash"]
        ):
            raise Refused("the OCI image worker is incompatible; no image was published")
    receipt["steps"].append({"stage": "oci-worker-hello", "elapsed_ns": time.monotonic_ns() - started, "exit": 0})
    run(["container", "delete", "--force", name], env, receipt, "oci-check-remove")


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("provider", choices=["lima", "apple-container"])
    parser.add_argument("--bundle", required=True, type=Path)
    parser.add_argument(
        "--base-image", required=True, help="Lima: cached stock OS file; Apple: installed OCI base reference"
    )
    parser.add_argument(
        "--cache-root",
        type=Path,
        default=Path(os.environ.get("XDG_CACHE_HOME", str(Path.home() / ".cache"))) / "layerfs/machine-images",
    )
    parser.add_argument(
        "--resume-sealed-seed", type=Path, help="explicitly finish export of a retained sanitized stopped seed"
    )
    args = parser.parse_args(argv)
    try:
        bundle = args.bundle.resolve(strict=True)
        manifest = bundle_manifest(bundle)
        base = args.base_image
        if args.provider == "lima":
            base = Path(base).resolve(strict=True)
            base_identity = digest(base)
        else:
            # Keep a mutable tag out of the image identity: record its installed digest.
            inspected = subprocess.run(
                ["container", "image", "inspect", base], capture_output=True, text=True, check=True, timeout=30
            )
            configuration = json.loads(inspected.stdout)[0]["configuration"]
            base_identity = configuration["descriptor"]["digest"]
            base = configuration["name"].rsplit(":", 1)[0] + "@" + base_identity
        if args.resume_sealed_seed:
            receipt = json.loads(args.resume_sealed_seed.read_text())
            if args.provider != "lima" or receipt["provider"] != "lima" or receipt["bundle"] != str(bundle):
                raise Refused("resume does not match this Lima preparation")
            stages = {row["stage"]: row for row in receipt["steps"]}
            if any(
                stages.get(stage, {}).get("exit") != 0 for stage in ["worker-hello", "sanitize-identity", "seed-stop"]
            ):
                raise Refused("seed is not verified, sanitized and stopped")
            env = {**os.environ, "LIMA_HOME": receipt["lima_home"]}
            state = json.loads(run(["limactl", "list", "--json", "seed"], env, receipt, "resume-observe"))
            if state["status"] != "Stopped":
                raise Refused("retained seed is not stopped")
            seal_lima(args.cache_root, Path(receipt["work"]), manifest, receipt, env)
            return 0
        identity = image_identity(args.provider, manifest, base_identity)
        work = args.cache_root / "builds" / (identity[:16] + "-" + uuid.uuid4().hex[:8])
        work.mkdir(parents=True, mode=0o700)
        receipt = {
            "version": 1,
            "identity": identity,
            "provider": args.provider,
            "work": str(work),
            "bundle": str(bundle),
            "base": str(base),
            "status": "preparing",
            "steps": [],
        }
        write(work / "receipt.json", receipt)
        if args.provider == "lima":
            lima(args.cache_root, work, bundle, base, manifest, receipt)
        else:
            apple(args.cache_root, work, bundle, base, manifest, receipt)
    except (Refused, OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        print(f"Machine image preparation failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
