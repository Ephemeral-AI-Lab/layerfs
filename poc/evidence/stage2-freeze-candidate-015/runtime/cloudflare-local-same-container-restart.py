#!/usr/bin/env python3
"""Prove local Cloudflare process-state behavior across restart of one container."""

from __future__ import annotations

import argparse
import json
import subprocess
import time
from pathlib import Path


IMAGE_ID = "sha256:8c5100fabfd873de4ee7aabf908027e946b3fdac5328e15f9dabbf9731200bb0"
SOURCE = "de87919a4fd37242e960e13b7b3ba802d1eef0a0"
TREE = "4fb409d7e1356e1098439293d77d2fdc2dbf2190"
CONTAINER = "layerfs-stage2-015-cloudflare-local-same-container"
PAYLOAD_BYTES = 64 * 1024 * 1024


def run(argv: list[str], *, check: bool = True) -> subprocess.CompletedProcess[bytes]:
    return subprocess.run(argv, check=check, stdout=subprocess.PIPE, stderr=subprocess.PIPE)


def absent() -> bool:
    return subprocess.run(
        ["docker", "container", "inspect", CONTAINER],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    ).returncode != 0


def save(path: Path, result: subprocess.CompletedProcess[bytes]) -> None:
    path.with_suffix(path.suffix + ".stdout").write_bytes(result.stdout)
    path.with_suffix(path.suffix + ".stderr").write_bytes(result.stderr)


def write_json(path: Path, value: object) -> None:
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n")


def wait_ready(output: Path, phase: str) -> None:
    for _ in range(200):
        health = subprocess.run(
            ["docker", "exec", CONTAINER, "curl", "-fsS", "http://127.0.0.1:45678/health"],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        mounted = subprocess.run(
            ["docker", "exec", CONTAINER, "mountpoint", "-q", "/workspace"],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        if health.returncode == mounted.returncode == 0:
            output.joinpath(f"{phase}.inspect.json").write_bytes(run(["docker", "inspect", CONTAINER]).stdout)
            output.joinpath(f"{phase}.mountinfo.txt").write_bytes(
                run(["docker", "exec", CONTAINER, "cat", "/proc/self/mountinfo"]).stdout
            )
            output.joinpath(f"{phase}.backend.json").write_bytes(
                run(
                    [
                        "docker",
                        "exec",
                        CONTAINER,
                        "curl",
                        "-fsS",
                        "http://127.0.0.1:45678/__computerd/info",
                    ]
                ).stdout
            )
            output.joinpath(f"{phase}.uname.txt").write_bytes(
                run(["docker", "exec", CONTAINER, "uname", "-m"]).stdout
            )
            output.joinpath(f"{phase}.node-arch.txt").write_bytes(
                run(["docker", "exec", CONTAINER, "node", "-p", "process.arch"]).stdout
            )
            diagnostic = run(
                [
                    "docker",
                    "exec",
                    CONTAINER,
                    "bash",
                    "-lc",
                    "for p in /proc/[0-9]*; do c=$(tr '\\0' ' ' <\"$p/cmdline\" 2>/dev/null || true); case \"$c\" in *computerd.cjs*) n=${p##*/}; echo pid=$n; echo pidns=$(readlink \"$p/ns/pid\"); echo start=$(awk '{print $22}' \"$p/stat\"); sha256sum \"$p/exe\"; break;; esac; done",
                ]
            )
            output.joinpath(f"{phase}.process.txt").write_bytes(diagnostic.stdout)
            return
        time.sleep(0.05)
    raise RuntimeError(f"Cloudflare container did not become ready during {phase}")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("root", type=Path)
    args = parser.parse_args()
    output = args.root
    if output.exists():
        raise SystemExit(f"evidence root already exists: {output}")
    if not absent():
        raise SystemExit(f"owned container already exists: {CONTAINER}")
    output.mkdir(parents=True)
    actual = run(["docker", "image", "inspect", IMAGE_ID, "--format", "{{.Id}} {{.Architecture}}"]).stdout.decode().strip()
    if actual != f"{IMAGE_ID} arm64":
        raise SystemExit(f"image identity mismatch: {actual}")
    output.joinpath("image.inspect.json").write_bytes(run(["docker", "image", "inspect", IMAGE_ID]).stdout)
    launch = [
        "docker",
        "run",
        "-d",
        "--name",
        CONTAINER,
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
    mutate = [
        "docker",
        "exec",
        CONTAINER,
        "bash",
        "-lc",
        "set -euo pipefail; dd if=/dev/urandom of=/workspace/payload bs=1M count=64 status=none; sha256sum /workspace/payload; stat -c '%s' /workspace/payload; sync -f /workspace/payload; sync -f /workspace",
    ]
    plan = {
        "schema": "layerfs-stage2-015-cloudflare-same-container-plan-v1",
        "source": SOURCE,
        "tree": TREE,
        "image_id": IMAGE_ID,
        "persistence_class": "PROCESS_LOCAL_SQLITE_MEMORY",
        "launch_argv": launch,
        "mutate_argv": mutate,
        "kill_argv": ["docker", "kill", "--signal", "KILL", CONTAINER],
        "wait_argv": ["docker", "wait", CONTAINER],
        "restart_argv": ["docker", "start", CONTAINER],
        "retention_contract": "same stopped container and same writable layer; no docker rm before verification",
    }
    write_json(output / "plan.json", plan)
    launched = run(launch, check=False)
    save(output / "launch", launched)
    if launched.returncode:
        raise RuntimeError(f"launch failed: {launched.returncode}")
    removed = False
    try:
        wait_ready(output, "before")
        container_id_before = json.loads((output / "before.inspect.json").read_text())[0]["Id"]
        started = time.time_ns()
        completed = run(mutate, check=False)
        acknowledged = time.time_ns()
        save(output / "mutate", completed)
        if completed.returncode:
            raise RuntimeError(f"mutate/sync failed: {completed.returncode}")
        fields = completed.stdout.decode().splitlines()
        digest = fields[0].split()[0] if fields else ""
        size = int(fields[1]) if len(fields) > 1 else -1
        if len(digest) != 64 or size != PAYLOAD_BYTES:
            raise RuntimeError("acknowledged payload identity mismatch")
        kill_requested = time.time_ns()
        killed = run(["docker", "kill", "--signal", "KILL", CONTAINER], check=False)
        kill_completed = time.time_ns()
        save(output / "kill", killed)
        waited = run(["docker", "wait", CONTAINER], check=False)
        wait_completed = time.time_ns()
        save(output / "wait", waited)
        write_json(
            output / "timing.json",
            {
                "command_started_unix_ns": started,
                "acknowledged_unix_ns": acknowledged,
                "kill_requested_unix_ns": kill_requested,
                "kill_completed_unix_ns": kill_completed,
                "wait_completed_unix_ns": wait_completed,
                "local_acknowledgement_ns": acknowledged - started,
                "acknowledgement_to_kill_request_ns": kill_requested - acknowledged,
            },
        )
        output.joinpath("stopped.inspect.json").write_bytes(run(["docker", "inspect", CONTAINER]).stdout)
        backing = run(
            [
                "docker",
                "cp",
                f"{CONTAINER}:/workspace/payload",
                str(output / "stopped-backing-payload"),
            ],
            check=False,
        )
        save(output / "stopped-backing-check", backing)
        restarted = run(["docker", "start", CONTAINER], check=False)
        save(output / "restart", restarted)
        if restarted.returncode:
            raise RuntimeError(f"same-container docker start failed: {restarted.returncode}")
        wait_ready(output, "after")
        container_id_after = json.loads((output / "after.inspect.json").read_text())[0]["Id"]
        verify = run(
            [
                "docker",
                "exec",
                CONTAINER,
                "bash",
                "-lc",
                "if test -e /workspace/payload; then sha256sum /workspace/payload; stat -c '%s' /workspace/payload; exit 0; else echo ABSENT; exit 44; fi",
            ],
            check=False,
        )
        save(output / "verify", verify)
        survived = verify.returncode == 0 and verify.stdout.decode().split()[0] == digest
        graceful = run(["docker", "kill", "--signal", "TERM", CONTAINER], check=False)
        save(output / "cleanup-kill", graceful)
        cleanup_wait = run(["docker", "wait", CONTAINER], check=False)
        save(output / "cleanup-wait", cleanup_wait)
        output.joinpath("final-stopped.inspect.json").write_bytes(run(["docker", "inspect", CONTAINER]).stdout)
        removal = run(["docker", "rm", CONTAINER], check=False)
        save(output / "cleanup-rm", removal)
        removed = removal.returncode == 0 and absent()
        inventory = run(
            [
                "docker",
                "ps",
                "-a",
                "--filter",
                f"name=^{CONTAINER}$",
                "--format",
                "{{.ID}} {{.Names}} {{.Status}}",
            ],
            check=False,
        )
        save(output / "post-cleanup-inventory", inventory)
        before_process = (output / "before.process.txt").read_text()
        after_process = (output / "after.process.txt").read_text()
        receipt = {
            "schema": "layerfs-stage2-015-cloudflare-same-container-restart-v1",
            "status": "PROCESS_STATE_SURVIVED" if survived else "PASS_EXPECTED_PROCESS_LOCAL_STATE_LOSS",
            "scope": "LOCAL_NATIVE_FUSE_PROCESS_LOCAL_SQLITE_NEGATIVE_CONTROL",
            "classification": "DIAGNOSTIC_ONLY",
            "source": SOURCE,
            "tree": TREE,
            "image_id": IMAGE_ID,
            "persistence_class": "PROCESS_LOCAL_NON_DURABLE",
            "durability_claim": False,
            "cloudflare_durable_object_present": False,
            "sync_peer_present": False,
            "terminal_eligible": False,
            "payload_bytes": size,
            "payload_sha256": digest,
            "standalone_local_barrier_wall_ns": acknowledged - started,
            "acknowledgement_to_kill_request_ns": kill_requested - acknowledged,
            "kill_exit": killed.returncode,
            "wait_exit": waited.returncode,
            "stopped_container_exit": waited.stdout.decode().strip(),
            "same_container_id": container_id_before == container_id_after,
            "same_container_restart": True,
            "computerd_process_restart": before_process != after_process,
            "container_id": container_id_after,
            "docker_rm_before_restart": False,
            "writable_layer_retained": True,
            "stopped_backing_payload_absent": backing.returncode != 0,
            "stopped_backing_check_exit": backing.returncode,
            "process_identity_changed": before_process != after_process,
            "reopen_exit": verify.returncode,
            "reopen_output": verify.stdout.decode().strip(),
            "survived_process_restart": survived,
            "cleanup_exit": removal.returncode,
            "owned_container_absent": removed,
            "post_cleanup_inventory_empty": inventory.stdout == b"",
            "comparison_disposition": "Persistence-latency comparison remains undefined without matched commands, clocks, sync endpoints, media, and retention semantics.",
        }
        write_json(output / "receipt.json", receipt)
    finally:
        if not removed and not absent():
            run(["docker", "rm", "-f", CONTAINER], check=False)
    if not removed:
        raise SystemExit("owned container cleanup failed")


if __name__ == "__main__":
    main()
