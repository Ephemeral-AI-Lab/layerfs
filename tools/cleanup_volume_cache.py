"""Clear released Cargo volume caches while retaining their direct binaries."""

import json
import hashlib
import re
from pathlib import Path
import uuid
from cleanup_state import Refused, run, docker_state, ensure_volume_idle, volume_identity, same_volume

IMAGE = "node:24.21.0-bookworm"
SCRIPT = Path(__file__).with_suffix(".mjs").resolve()


def helper(name, image, output, mode, cache=None, target_subdir=None):
    if target_subdir is not None and (
        not isinstance(target_subdir, str) or not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9_.-]*", target_subdir)
    ):
        raise Refused("target_subdir must be one literal directory name inside the volume")
    mount = f"type=volume,src={name},dst=/target"
    if target_subdir is not None:
        mount += ",volume-subpath=" + target_subdir
    if mode == "plan":
        mount += ",readonly"
    token = uuid.uuid4().hex
    prefix = output / ("cargo-volume-" + token)
    args = [
        "docker",
        "create",
        "--name",
        "layerfs-cleanup-cache-" + token,
        "--label",
        "layerfs.cleanup.pass=" + output.name,
        "--pull",
        "never",
        "--network",
        "none",
        "--read-only",
        "--mount",
        mount,
        "--mount",
        f"type=bind,src={SCRIPT},dst=/cleanup.mjs,readonly",
    ]
    if cache is not None:
        plan = prefix.with_suffix(".plan.json")
        plan.write_text(json.dumps(cache) + "\n")
        args += ["--mount", f"type=bind,src={plan},dst=/plan.json,readonly"]
    args += ["--entrypoint", "node", image, "/cleanup.mjs", mode, "/plan.json"]
    ident = run(args).stdout.decode().strip()
    prefix.with_suffix(".container.json").write_text(json.dumps({"id": ident, "command": args}, indent=2) + "\n")
    completed = False
    try:
        # Our created helper is the sole allowed consumer; every other owner,
        # including stopped containers, still protects this volume.
        ensure_volume_idle(name, [], [c for c in docker_state() if c["id"] != ident])
        result = run(["docker", "start", "--attach", ident])
        prefix.with_suffix(".output.json").write_bytes(result.stdout)
        state = json.loads(run(["docker", "inspect", ident]).stdout)[0]["State"]
        if state["Running"] or state["ExitCode"] != 0:
            raise Refused("Cargo cache helper did not exit successfully; inspect " + ident)
        value = json.loads(result.stdout)
        completed = True
        return value
    finally:
        state = json.loads(run(["docker", "inspect", ident], timeout=10).stdout)[0]["State"]
        if state["Running"]:
            run(["docker", "kill", ident], timeout=10)
        if completed:
            run(["docker", "rm", ident], timeout=10)
        # Failed helpers remain stopped for custody; never repeat a removal.


def prepare(action, scope, containers, output):
    name = action["name"]
    ensure_volume_idle(name, scope.get("protected_volumes", []), containers)
    identity = volume_identity(name)
    image = json.loads(run(["docker", "image", "inspect", IMAGE]).stdout)[0]["Id"]
    cache = helper(name, image, output, "plan", target_subdir=action.get("target_subdir"))
    if not same_volume(identity, volume_identity(name)):
        raise Refused("Cargo volume identity changed during planning")
    return {
        **action,
        "identity": identity,
        "image_id": image,
        "cache": cache,
        "helper_sha256": hashlib.sha256(SCRIPT.read_bytes()).hexdigest(),
    }


def clear(action, scope, output, event):
    name = action["name"]
    if hashlib.sha256(SCRIPT.read_bytes()).hexdigest() != action["helper_sha256"]:
        raise Refused("Cargo cache helper changed since plan")
    ensure_volume_idle(name, scope.get("protected_volumes", []), docker_state())
    if not same_volume(action["identity"], volume_identity(name)):
        raise Refused("Cargo volume identity changed since plan")
    event(output, state="removing", action=action)
    value = helper(
        name, action["image_id"], output, "apply", action["cache"], target_subdir=action.get("target_subdir")
    )
    if not same_volume(action["identity"], volume_identity(name)):
        raise Refused("Cargo volume identity changed during cleanup")
    return {**value, "volume": "retained", "image_id": action["image_id"]}
