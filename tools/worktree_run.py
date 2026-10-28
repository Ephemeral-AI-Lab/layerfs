"""Run pinned SDK artifacts with per-run Linux state and exact teardown receipts."""

import json
import os
from pathlib import Path
import subprocess
import time
import uuid

from cleanup_state import Refused, run
from worktree_build import verify_bundle
from worktree_resources import LABEL, digest, lease, load, timestamp, write


def bounded(command, deadline, allowed=(0,)):
    remaining = deadline - time.monotonic()
    if remaining <= 0:
        raise Refused("Run teardown budget exhausted; exact resources retained in receipt")
    try:
        result = subprocess.run(command, capture_output=True, timeout=min(5, remaining))
    except subprocess.TimeoutExpired as error:
        raise Refused("Docker teardown command timed out; no retry") from error
    if result.returncode not in allowed:
        raise Refused("Docker teardown failed: " + result.stderr.decode(errors="replace")[-2000:])
    return result.stdout


def inspect_owned(prefix, client, deadline):
    ids = bounded(["docker", "ps", "-aq", "--no-trunc", "--filter", "name=" + prefix], deadline).decode().split()
    if client and client not in ids:
        existing = bounded(["docker", "ps", "-aq", "--no-trunc"], deadline).decode().split()
        if client in existing:
            ids.append(client)
    if not ids:
        return []
    records = json.loads(bounded(["docker", "inspect", *ids], deadline))
    for record in records:
        if not record["Name"].lstrip("/").startswith(prefix):
            raise Refused("Container name does not match this run's unique namespace")
    return list({record["Id"]: record for record in records}.values())


def finish(record, success, deadline):
    """Stop only this run's exact IDs; retain stopped failure state and Store volume."""
    containers = inspect_owned(record["prefix"], record.get("client_id"), deadline)
    record["containers"] = [{"id": c["Id"], "name": c["Name"]} for c in containers]
    # Stop test descendants first on every outcome. They can outlive the SDK process.
    for container in containers:
        if container["State"]["Running"]:
            bounded(["docker", "kill", container["Id"]], deadline)
    if success:
        for container in containers:
            bounded(["docker", "rm", container["Id"]], deadline)
        consumers = bounded(["docker", "ps", "-aq", "--filter", "volume=" + record["store_volume"]], deadline)
        if consumers.strip():
            raise Refused("Store volume still has consumers; retained")
        check_store(record, deadline)
        bounded(["docker", "volume", "rm", record["store_volume"]], deadline)
        record["released"] = True
    else:
        record["released"] = False
        record["release_condition"] = (
            "Owner finishes diagnosis; explicitly release exact stopped containers and Store volume"
        )


def check_store(record, deadline):
    info = json.loads(bounded(["docker", "volume", "inspect", record["store_volume"]], deadline))[0]
    labels = info.get("Labels") or {}
    run_id = record.get("run_id", record["prefix"].rstrip("-").rsplit("-", 1)[-1])
    if labels.get(LABEL) != record["worktree_id"] or labels.get("layerfs.dev.run") != run_id:
        raise Refused("Store volume ownership changed; preserve it")
    before = record.get("volume_created_at")
    if before is not None and info.get("CreatedAt") != before:
        raise Refused("Store volume was recreated; preserve it")


def launch(args):
    resources = load(args.resources)
    bundle = Path(args.bundle).resolve()
    manifest = verify_bundle(bundle)
    if manifest["platform"] != "linux" or manifest["worktree_id"] != resources["id"]:
        raise Refused("E2E needs this worktree's sealed Linux bundle")
    if manifest["images"] != resources["images"]:
        raise Refused("Bundle and registration images differ")
    command = list(args.arguments)
    if command[:1] == ["--"]:
        command.pop(0)
    if not command or command[0] != "node":
        raise Refused("E2E command must start with node and use the pinned SDK bundle")
    if not 0 < args.timeout <= 100:
        raise Refused("E2E timeout must be in (0, 100] seconds; teardown shares the 120-second ceiling")
    token = uuid.uuid4().hex
    prefix = "lfs-e2e-" + resources["id"] + "-" + token + "-"
    directory = Path(resources["state"]) / "runs" / token
    directory.mkdir(mode=0o700)
    state = {
        "version": 1,
        "status": "starting",
        "at": timestamp(),
        "worktree_id": resources["id"],
        "run_id": token,
        "bundle": str(bundle),
        "bundle_manifest_sha256": digest(bundle / "manifest.json"),
        "prefix": prefix,
        "store_volume": prefix + "store",
        "command": command,
        "timeout_seconds": args.timeout,
        "store_profile": "Disposable (SDK native route)",
        "released": False,
    }
    receipt = directory / "receipt.json"
    write(receipt, state)
    # Refuse an existing exact name, so create can never adopt someone else's mutable state.
    names = run(["docker", "volume", "ls", "--format", "{{.Name}}"]).stdout.decode().splitlines()
    if state["store_volume"] in names:
        raise Refused("Run volume already exists")
    run(
        [
            "docker",
            "volume",
            "create",
            "--label",
            f"{LABEL}={resources['id']}",
            "--label",
            "layerfs.dev.run=" + token,
            state["store_volume"],
        ]
    )
    info = json.loads(run(["docker", "volume", "inspect", state["store_volume"]]).stdout)[0]
    state["volume_created_at"] = info.get("CreatedAt")
    write(receipt, state)
    endpoint = json.loads(run(["docker", "context", "inspect"]).stdout)[0]["Endpoints"]["docker"]["Host"]
    if not endpoint.startswith("unix://"):
        raise Refused("E2E needs a local Unix Docker daemon")
    # Docker Desktop's context endpoint is a macOS proxy socket. Containers need
    # the Linux engine socket, as the existing SDK verification helper uses.
    socket = "/var/run/docker.sock" if os.uname().sysname == "Darwin" else endpoint[len("unix://") :]
    env = {
        "LAYERFS_HOME": "/layerfs-store/sdk-home",
        "LAYERFS_STORE_PATH": "/layerfs-store/global/store.sqlite",
        "LAYERFS_DOCKER": "/var/run/docker.sock",
        "LAYERFS_IMAGE": resources["images"]["sandbox"],
        "LAYERFS_VOLUME": state["store_volume"],
        "LAYERFS_DAEMON": str(bundle / "layerfs-daemon"),
        "LAYERFS_NATIVE": str(bundle / "layerfs.node"),
        "LAYERFS_CONTAINER_PREFIX": prefix,
        "LAYERFS_CONSTRUCTION_WORKERS": "1",
        "LAYERFS_TRACE": "/receipts/trace.jsonl",
        "LAYERFS_DAEMON_HOST": "host.docker.internal" if os.uname().sysname == "Darwin" else "127.0.0.1",
    }
    # The same absolute bundle path is visible to host, client and Docker API callers.
    script = 'umask 077; mkdir -p /layerfs-store/sdk-home /layerfs-store/global; umask 022; exec "$@"'
    create = [
        "docker",
        "create",
        "--pull",
        "never",
        "--name",
        prefix + "client",
        "--label",
        f"{LABEL}={resources['id']}",
        "--label",
        "layerfs.dev.run=" + token,
        "--mount",
        f"type=bind,src={bundle},dst={bundle},readonly",
        "--mount",
        f"type=bind,src={directory},dst=/receipts",
        "--mount",
        f"type=bind,src={socket},dst=/var/run/docker.sock",
        "--mount",
        f"type=volume,src={state['store_volume']},dst=/layerfs-store,volume-nocopy",
        *[v for k, value in env.items() for v in ("-e", k + "=" + value)],
        *(["--network", "host"] if os.uname().sysname == "Linux" else []),
        "-w",
        str(bundle),
        "--entrypoint",
        "/bin/sh",
        resources["images"]["runtime"],
        "-c",
        script,
        "sh",
        *command,
    ]
    success = False
    deadline = time.monotonic() + 120
    try:
        state["client_id"] = run(create).stdout.decode().strip()
        state["environment"] = env
        state["status"] = "running"
        write(receipt, state)
        with (directory / "output.log").open("wb") as output:
            process = subprocess.Popen(
                ["docker", "start", "--attach", state["client_id"]], stdout=output, stderr=subprocess.STDOUT
            )
            try:
                state["attach_exit_code"] = process.wait(
                    timeout=min(args.timeout, max(0, deadline - time.monotonic() - 20))
                )
            except BaseException:
                process.kill()
                process.wait()
                raise
        actual = json.loads(bounded(["docker", "inspect", state["client_id"]], deadline))[0]["State"]
        state["exit_code"] = actual["ExitCode"]
        success = state["attach_exit_code"] == 0 and state["exit_code"] == 0 and not actual["Running"]
        state["status"] = "passed" if success else "failed"
    except BaseException as error:
        state["status"] = "failed"
        state["error"] = str(error)
    finally:
        try:
            finish(state, success, deadline)
        except BaseException as error:
            state["status"] = "failed"
            state["teardown_error"] = str(error)
        state["finished_at"] = timestamp()
        write(receipt, state)
    print(receipt)
    return 0 if state["status"] == "passed" and state["released"] else 1


def cleanup_scope(args):
    """Export exact cache releases; pins and failed runs stay protected."""
    resources = load(args.resources)
    if args.release_caches:
        with lease(resources, "macos"), lease(resources, "linux"):
            return export_scope(args, resources)
    return export_scope(args, resources)


def export_scope(args, resources):
    state = Path(resources["state"])
    actions = []
    if args.release_caches:
        if not args.reason:
            raise Refused("Cache release needs a handoff/owner reason")
        base = {"owner": resources["owner"], "reason": args.reason, "released": True}
        for profile in ("debug", "release"):
            target = Path(resources["macos_target"])
            if (target / profile).exists():
                actions.append(
                    {**base, "kind": "cargo-cache", "path": str(target / profile), "target_root": str(target)}
                )
        names = run(["docker", "volume", "ls", "--format", "{{.Name}}"]).stdout.decode().splitlines()
        for key in ("linux_target_volume", "linux_cargo_volume"):
            if resources[key] in names:
                info = json.loads(run(["docker", "volume", "inspect", resources[key]]).stdout)[0]
                if (info.get("Labels") or {}).get(LABEL) != resources["id"]:
                    raise Refused("Cache volume ownership does not match this registration")
                actions.append({**base, "kind": "docker-volume", "name": resources[key]})
    protected_volumes = []
    for receipt in (state / "runs").glob("*/receipt.json"):
        run_state = json.loads(receipt.read_text())
        disposed = any(json.loads(p.read_text()).get("released") for p in receipt.parent.glob("disposal-*.json"))
        if not run_state.get("released") and not disposed:
            protected_volumes.append(run_state["store_volume"])
    scope = {
        "version": 1,
        "protected_paths": [str(state)],
        "protected_volumes": protected_volumes,
        "scan_roots": [],
        "actions": actions,
    }
    destination = Path(args.out).resolve()
    if destination.exists():
        raise Refused("Scope output already exists")
    write(destination, scope)
    print(destination)


def teardown(args):
    resources = load(args.resources)
    receipt = Path(args.run_receipt).resolve()
    root = Path(resources["state"]) / "runs"
    if receipt.name != "receipt.json" or receipt.parent.parent != root:
        raise Refused("Select a run receipt belonging to this registration")
    state = json.loads(receipt.read_text())
    if state["worktree_id"] != resources["id"]:
        raise Refused("Run belongs to another worktree")
    disposed = any(json.loads(p.read_text()).get("released") for p in receipt.parent.glob("disposal-*.json"))
    if state.get("released") or disposed:
        print("Run resources were already released; no operation attempted")
        return
    if not args.release_failed or not args.reason:
        raise Refused("Retained failure state needs --release-failed and an owner diagnosis --reason")
    # Keep the original failed receipt immutable. This is an explicit disposal record.
    release = dict(state, release_reason=args.reason, release_at=timestamp())
    output = receipt.parent / ("disposal-" + uuid.uuid4().hex + ".json")
    try:
        finish(release, True, time.monotonic() + 20)
    except BaseException as error:
        release["disposal_error"] = str(error)
        raise
    finally:
        write(output, release)
    print(output)
