#!/usr/bin/env python3
"""Test whether local Cloudflare Computer state survives a forced container restart."""

from __future__ import annotations

import argparse
import json
import subprocess
import time
from pathlib import Path


IMAGE_ID = "sha256:8c5100fabfd873de4ee7aabf908027e946b3fdac5328e15f9dabbf9731200bb0"
SOURCE = "de87919a4fd37242e960e13b7b3ba802d1eef0a0"
TREE = "4fb409d7e1356e1098439293d77d2fdc2dbf2190"
PAYLOAD_BYTES = 64 * 1024 * 1024
MUTATE = "layerfs-stage2-015-cloudflare-local-durable-mutate"
REOPEN = "layerfs-stage2-015-cloudflare-local-durable-reopen"


def run(argv: list[str], *, check: bool = True) -> subprocess.CompletedProcess[bytes]:
    return subprocess.run(argv, check=check, stdout=subprocess.PIPE, stderr=subprocess.PIPE)


def write_json(path: Path, value: object) -> None:
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n")


def absent(name: str) -> bool:
    return subprocess.run(
        ["docker", "container", "inspect", name], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL
    ).returncode != 0


def launch(name: str, output: Path) -> list[str]:
    if not absent(name):
        raise RuntimeError(f"owned container already exists: {name}")
    argv = [
        "docker",
        "run",
        "-d",
        "--name",
        name,
        "--platform",
        "linux/arm64",
        "--init",
        "--stop-timeout",
        "1",
        "--cpus",
        "1",
        "--memory",
        "512m",
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
        "-e",
        "FUSE_MOUNT=fuse",
        "-e",
        "MOUNT_POINT=/workspace",
        "-e",
        "PORT=45678",
        IMAGE_ID,
    ]
    result = run(argv, check=False)
    output.joinpath(f"{name}.run.stdout").write_bytes(result.stdout)
    output.joinpath(f"{name}.run.stderr").write_bytes(result.stderr)
    if result.returncode:
        raise RuntimeError(f"docker run failed for {name}: {result.returncode}")
    for _ in range(200):
        health = subprocess.run(
            ["docker", "exec", name, "curl", "-fsS", "http://127.0.0.1:45678/health"],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        mounted = subprocess.run(
            ["docker", "exec", name, "mountpoint", "-q", "/workspace"],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        if health.returncode == mounted.returncode == 0:
            output.joinpath(f"{name}.inspect.json").write_bytes(run(["docker", "inspect", name]).stdout)
            output.joinpath(f"{name}.mountinfo.txt").write_bytes(
                run(["docker", "exec", name, "cat", "/proc/self/mountinfo"]).stdout
            )
            output.joinpath(f"{name}.backend.json").write_bytes(
                run(["docker", "exec", name, "curl", "-fsS", "http://127.0.0.1:45678/__computerd/info"]).stdout
            )
            return argv
        time.sleep(0.05)
    raise RuntimeError(f"{name} did not become ready")


def cleanup(name: str, signal: str, output: Path) -> dict[str, object]:
    killed = run(["docker", "kill", "--signal", signal, name], check=False)
    waited = run(["docker", "wait", name], check=False)
    output.joinpath(f"{name}.logs.stdout").write_bytes(run(["docker", "logs", name], check=False).stdout)
    output.joinpath(f"{name}.logs.stderr").write_bytes(run(["docker", "logs", name], check=False).stderr)
    output.joinpath(f"{name}.stopped-inspect.json").write_bytes(run(["docker", "inspect", name]).stdout)
    removed = run(["docker", "rm", name], check=False)
    return {
        "signal": signal,
        "kill_exit": killed.returncode,
        "wait_exit": waited.returncode,
        "container_exit": waited.stdout.decode().strip(),
        "remove_exit": removed.returncode,
        "container_absent": absent(name),
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("root", type=Path)
    arguments = parser.parse_args()
    output = arguments.root
    if output.exists():
        raise SystemExit(f"evidence root already exists: {output}")
    output.mkdir(parents=True)
    actual_image = run(["docker", "image", "inspect", IMAGE_ID, "--format", "{{.Id}} {{.Architecture}}"]).stdout.decode().strip()
    if actual_image != f"{IMAGE_ID} arm64":
        raise SystemExit(f"image admission mismatch: {actual_image}")
    plan = {
        "schema": "layerfs-stage2-015-cloudflare-local-restart-plan-v1",
        "scope": "LOCAL_NATIVE_FUSE_PROCESS_LOCAL_SQLITE",
        "source": SOURCE,
        "tree": TREE,
        "image_id": IMAGE_ID,
        "payload_bytes": PAYLOAD_BYTES,
        "expected_contract": "acknowledged 64 MiB bytes survive forced container restart",
    }
    mutate_launch = launch(MUTATE, output)
    command = [
        "docker",
        "exec",
        MUTATE,
        "bash",
        "-lc",
        "set -euo pipefail; dd if=/dev/urandom of=/workspace/payload bs=1M count=64 status=none; sha256sum /workspace/payload; stat -c '%s' /workspace/payload; sync -f /workspace/payload; sync -f /workspace",
    ]
    plan["mutate_launch_argv"] = mutate_launch
    plan["timed_command_argv"] = command
    write_json(output / "plan.json", plan)
    started = time.time_ns()
    completed = run(command, check=False)
    acknowledged = time.time_ns()
    output.joinpath("timed-command.stdout").write_bytes(completed.stdout)
    output.joinpath("timed-command.stderr").write_bytes(completed.stderr)
    if completed.returncode:
        cleanup(MUTATE, "KILL", output)
        raise RuntimeError(f"Cloudflare local acknowledgement command failed: {completed.returncode}")
    fields = completed.stdout.decode().splitlines()
    digest = fields[0].split()[0] if fields else ""
    size = int(fields[1]) if len(fields) > 1 else -1
    if len(digest) != 64 or size != PAYLOAD_BYTES:
        cleanup(MUTATE, "KILL", output)
        raise RuntimeError("pre-crash payload identity mismatch")
    kill_requested = time.time_ns()
    mutate_cleanup = cleanup(MUTATE, "KILL", output)
    reopen_launch = launch(REOPEN, output)
    reopen = run(
        [
            "docker",
            "exec",
            REOPEN,
            "bash",
            "-lc",
            "if test -e /workspace/payload; then sha256sum /workspace/payload; stat -c '%s' /workspace/payload; exit 0; else echo ABSENT; exit 44; fi",
        ],
        check=False,
    )
    output.joinpath("reopen.stdout").write_bytes(reopen.stdout)
    output.joinpath("reopen.stderr").write_bytes(reopen.stderr)
    reopen_cleanup = cleanup(REOPEN, "TERM", output)
    survived = reopen.returncode == 0 and reopen.stdout.decode().split()[0] == digest
    receipt = {
        "schema": "layerfs-stage2-015-cloudflare-local-restart-v1",
        "status": "PASS_DURABLE" if survived else "FAIL_DURABILITY",
        "scope": "LOCAL_NATIVE_FUSE_PROCESS_LOCAL_SQLITE",
        "source": SOURCE,
        "tree": TREE,
        "image_id": IMAGE_ID,
        "payload_bytes": size,
        "payload_sha256": digest,
        "acknowledgement_command_exit": completed.returncode,
        "local_acknowledgement_ns": acknowledged - started,
        "acknowledgement_to_kill_request_ns": kill_requested - acknowledged,
        "mutate_cleanup": mutate_cleanup,
        "reopen_launch_argv": reopen_launch,
        "reopen_exit": reopen.returncode,
        "reopen_output": reopen.stdout.decode().strip(),
        "survived_restart": survived,
        "reopen_cleanup": reopen_cleanup,
        "owned_containers_absent": absent(MUTATE) and absent(REOPEN),
        "comparison_disposition": "Cloudflare local acknowledgement is not restart-durable" if not survived else "Cloudflare local acknowledgement survived restart",
    }
    write_json(output / "receipt.json", receipt)
    if not receipt["owned_containers_absent"]:
        raise SystemExit("owned Cloudflare containers remain")


if __name__ == "__main__":
    main()
