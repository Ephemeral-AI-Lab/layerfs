"""Exact container retirement with retained rootfs, logs, configuration and volumes."""

import gzip
import hashlib
import json
from pathlib import Path
import subprocess
import shutil

from cleanup_state import Refused, host_bind_path, overlaps, run
from cleanup_archive import archive


def inspect(name):
    return json.loads(run(["docker", "container", "inspect", name]).stdout)[0]


def identity(info):
    return {key: info[key] for key in ("Id", "Created", "Image", "Config", "Mounts")} | {
        "state": {key: info["State"][key] for key in ("Status", "Running", "StartedAt", "FinishedAt")}
    }


def guard(info, action, scope):
    protected = scope.get("protected_containers", [])
    if info["Id"] in protected or info["Name"].lstrip("/") in protected:
        raise Refused("Protected container: " + info["Name"])
    for mount in info.get("Mounts", []):
        if mount.get("Name") in scope.get("protected_volumes", []):
            raise Refused("Container uses a protected volume: " + info["Name"])
        if mount.get("Type") == "bind" and any(
            overlaps(host_bind_path(mount["Source"]), Path(p).resolve()) for p in scope.get("protected_paths", [])
        ):
            raise Refused("Container binds a protected path: " + info["Name"])
    if info["State"]["Running"]:
        if action.get("retire_running") is not True:
            raise Refused("Running containers require explicit retirement selection")
        # Explicit retirement supports abandoned filesystem daemons only. Ordinary
        # commands, Exec descendants and other services remain protected.
        top = run(["docker", "top", info["Id"], "-eo", "pid,args"]).stdout.decode().splitlines()[1:]
        if len(top) != 1 or len(top[0].split()) < 2 or Path(top[0].split()[1]).name != "layerfs-daemon":
            raise Refused("Container has active commands or is not a lone LayerFS daemon")
    elif info["State"]["Status"] not in {"exited", "created"}:
        raise Refused("Container is not quiescent")


def retire(action, scope, output, pass_id, record_event, repo=None):
    info = inspect(action["name"])
    guard(info, action, scope)
    if identity(info) != action["identity"]:
        raise Refused("Container identity or execution state changed since plan")
    dest = output / ("container-" + info["Id"][:12])
    dest.mkdir(mode=0o700)
    (dest / "before.json").write_text(json.dumps(info, indent=2) + "\n")
    if info["State"]["Running"]:
        record_event(output, state="stopping", action=action)
        run(["docker", "stop", "--timeout", "10", info["Id"]], timeout=20)
        info = inspect(info["Id"])
        if info["State"]["Running"]:
            raise Refused("Container remains running after stop")
    stopped = identity(info)
    (dest / "stopped.json").write_text(json.dumps(info, indent=2) + "\n")
    (dest / "diff.txt").write_bytes(run(["docker", "diff", info["Id"]]).stdout)
    # Keep changed rootfs layers without exporting duplicate base image bytes.
    # Named/bind volumes remain intact and are never removed by this action.
    rootfs = {}
    if action.get("discard_rootfs") is True:
        selected = dest / "selected-rootfs"
        selected.mkdir()
        for index, source in enumerate(action.get("preserve_paths", [])):
            run(["docker", "cp", info["Id"] + ":" + source, str(selected / str(index))])
        rootfs = archive(repo, selected, dest, pass_id)
        rootfs["paths"] = action.get("preserve_paths", [])
        shutil.rmtree(selected)
        rootfs["limits"] = (
            "Other ephemeral rootfs data explicitly discarded; original image and all mounted data retained"
        )
    else:
        tag = "layerfs-cleanup-recovery:" + pass_id.lower() + "-" + info["Id"][:12]
        image = (
            run(["docker", "commit", "--message", "Cleanup recovery; retain mounted volumes", info["Id"], tag])
            .stdout.decode()
            .strip()
        )
        saved = json.loads(run(["docker", "image", "inspect", image]).stdout)[0]
        if saved["Id"] != image or saved.get("Parent") != info["Image"]:
            raise Refused("Recovery image identity/parent mismatch")
        rootfs = {"image": image, "image_tag": tag}
    log = dest / "logs.txt"
    with log.open("xb") as stream:
        run_logs = subprocess.run(
            ["docker", "logs", "--timestamps", info["Id"]], stdout=stream, stderr=subprocess.STDOUT, timeout=115
        )
    if run_logs.returncode:
        raise Refused("Container log capture failed; container and recovery image retained")
    original_hash = hashlib.sha256()
    with log.open("rb") as source, gzip.open(dest / "logs.txt.gz", "wb", compresslevel=1) as packed:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            original_hash.update(block)
            packed.write(block)
    verified = hashlib.sha256()
    with gzip.open(dest / "logs.txt.gz", "rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            verified.update(block)
    if verified.digest() != original_hash.digest():
        raise Refused("Archived log verification failed")
    log.unlink()
    recovery = {
        "directory": str(dest),
        "rootfs": rootfs,
        "logs_sha256": original_hash.hexdigest(),
        "volumes": "retained",
        "limits": "See rootfs recovery limits; mounted data remains at its original mounts",
    }
    (dest / "recovery.json").write_text(json.dumps(recovery, indent=2) + "\n")
    record_event(output, state="archived", action=action, recovery=recovery)
    current = inspect(info["Id"])
    guard(current, action, scope)
    if identity(current) != stopped:
        raise Refused("Container changed during recovery capture")
    record_event(output, state="removing", action=action, recovery=recovery)
    run(["docker", "rm", info["Id"]])
    if info["Id"] in run(["docker", "ps", "-aq", "--no-trunc"]).stdout.decode().split():
        raise Refused("Container remains after removal")
    return recovery
