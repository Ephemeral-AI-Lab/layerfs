#!/usr/bin/env python3
"""Run Cloudflare Computer's local native-FUSE benchmark under the matched envelope."""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import time
from pathlib import Path


IMAGE_ID = "sha256:8c5100fabfd873de4ee7aabf908027e946b3fdac5328e15f9dabbf9731200bb0"
SOURCE = "de87919a4fd37242e960e13b7b3ba802d1eef0a0"
TREE = "4fb409d7e1356e1098439293d77d2fdc2dbf2190"
FS_BENCH = "0e2004339a9269b88c2de451d56575957477bde16b96a10cf1837519e37230ef"
SCENARIOS = (
    "create 1000 files",
    "stat 1000 files",
    "rm 1000 files",
    "mkdir tree (10x10x10)",
    "find tree",
    "write 64 MiB",
    "copy 64 MiB",
    "read 64 MiB",
    "pure read 64 MiB",
    "pure copy 64 MiB",
    "overwrite 64 MiB",
    "git init + commit 100 files",
)
FULL_SCENARIOS = ",".join(SCENARIOS)
CGROUP_FILES = ("cpu.stat", "memory.current", "memory.peak", "memory.events", "pids.current", "pids.peak")
ANSI = re.compile(r"\x1b\[[0-9;]*m")


def run(argv: list[str], *, check: bool = True) -> subprocess.CompletedProcess[bytes]:
    return subprocess.run(argv, check=check, stdout=subprocess.PIPE, stderr=subprocess.PIPE)


def write_text(path: Path, value: str) -> None:
    path.write_text(value)


def write_json(path: Path, value: object) -> None:
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n")


def docker_absent(kind: str, name: str) -> bool:
    return subprocess.run(
        ["docker", kind, "inspect", name], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL
    ).returncode != 0


def capture_cgroup(container: str, output: Path, boundary: str) -> None:
    for name in CGROUP_FILES:
        result = run(["docker", "exec", container, "cat", f"/sys/fs/cgroup/{name}"])
        output.joinpath(f"{boundary}-{name}.txt").write_bytes(result.stdout)


def wait_ready(container: str) -> None:
    for _ in range(200):
        health = subprocess.run(
            ["docker", "exec", container, "curl", "-fsS", "http://127.0.0.1:45678/health"],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        mounted = subprocess.run(
            ["docker", "exec", container, "mountpoint", "-q", "/workspace"],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        if health.returncode == mounted.returncode == 0:
            return
        time.sleep(0.05)
    raise RuntimeError("Cloudflare computerd did not become ready on native FUSE")


def population(root: Path, control: str) -> None:
    output = root / f"authoritative-{control}"
    output.mkdir(parents=True)
    base = "/var/tmp" if control == "var" else "/tmp"
    container = f"layerfs-stage2-015-cloudflare-local-{control}"
    if not docker_absent("container", container):
        raise RuntimeError(f"owned container already exists: {container}")
    launch = [
        "docker",
        "run",
        "-d",
        "--name",
        container,
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
    benchmark_env = {
        "SCENARIOS": FULL_SCENARIOS,
        "REPS": "3",
        "WARMUP": "1",
        "RANDOMIZE_TARGETS": "1",
        "MOUNT": "/workspace",
        "BASE": base,
        "OUTPUT_JSON": f"/var/lib/cloudflare-bench-{control}.json",
    }
    benchmark = ["docker", "exec"]
    for key, value in benchmark_env.items():
        benchmark.extend(["-e", f"{key}={value}"])
    benchmark.extend([container, "timeout", "--signal=TERM", "--kill-after=5s", "120s", "bash", "script/fs-bench.sh"])
    write_json(
        output / "plan.json",
        {
            "schema": "layerfs-stage2-015-cloudflare-local-plan-v1",
            "scope": "LOCAL_NATIVE_FUSE_PROCESS_LOCAL_SQLITE",
            "source": SOURCE,
            "tree": TREE,
            "image_id": IMAGE_ID,
            "fs_bench_sha256": FS_BENCH,
            "container": container,
            "launch_argv": launch,
            "benchmark_argv": benchmark,
            "benchmark_env": benchmark_env,
        },
    )
    launched = run(launch, check=False)
    output.joinpath("docker-run.stdout").write_bytes(launched.stdout)
    output.joinpath("docker-run.stderr").write_bytes(launched.stderr)
    if launched.returncode:
        raise RuntimeError(f"docker run failed: {launched.returncode}")
    successful = False
    try:
        wait_ready(container)
        output.joinpath("docker-inspect.json").write_bytes(run(["docker", "inspect", container]).stdout)
        write_text(output / "cpu.max.txt", run(["docker", "exec", container, "cat", "/sys/fs/cgroup/cpu.max"]).stdout.decode())
        write_text(output / "memory.max.txt", run(["docker", "exec", container, "cat", "/sys/fs/cgroup/memory.max"]).stdout.decode())
        write_text(output / "mountinfo.txt", run(["docker", "exec", container, "cat", "/proc/self/mountinfo"]).stdout.decode())
        write_text(output / "uname.txt", run(["docker", "exec", container, "uname", "-m"]).stdout.decode())
        write_text(output / "node-arch.txt", run(["docker", "exec", container, "node", "-p", "process.arch"]).stdout.decode())
        write_text(output / "backend.json", run(["docker", "exec", container, "curl", "-fsS", "http://127.0.0.1:45678/__computerd/info"]).stdout.decode())
        script_hash = run(["docker", "exec", container, "sha256sum", "script/fs-bench.sh"]).stdout.decode()
        write_text(output / "fs-bench.sha256", script_hash)
        if script_hash.split()[0] != FS_BENCH:
            raise RuntimeError("fs-bench identity mismatch")
        capture_cgroup(container, output, "before")
        started = time.time_ns()
        completed = run(benchmark, check=False)
        ended = time.time_ns()
        output.joinpath("benchmark.stdout").write_bytes(completed.stdout)
        output.joinpath("benchmark.stderr").write_bytes(completed.stderr)
        write_text(output / "benchmark.exit", f"{completed.returncode}\n")
        write_text(output / "wall-start-unix-ns.txt", f"{started}\n")
        write_text(output / "wall-end-unix-ns.txt", f"{ended}\n")
        capture_cgroup(container, output, "after")
        copied = run(
            ["docker", "cp", f"{container}:{benchmark_env['OUTPUT_JSON']}", str(output / "fs-bench.json")],
            check=False,
        )
        output.joinpath("docker-cp.stdout").write_bytes(copied.stdout)
        output.joinpath("docker-cp.stderr").write_bytes(copied.stderr)
        raw = json.loads((output / "fs-bench.json").read_text()) if copied.returncode == 0 else {}
        keys = {(row.get("scenario"), row.get("target")) for row in raw.get("results", [])}
        expected = {(name, target) for name in SCENARIOS for target in ("computerd", "base")}
        clean_stdout = ANSI.sub("", completed.stdout.decode(errors="replace"))
        successful = (
            completed.returncode == copied.returncode == 0
            and len(raw.get("results", [])) == len(keys) == 24
            and keys == expected
            and "FAIL" not in clean_stdout
        )
        write_json(
            output / "capture.json",
            {
                "schema": "layerfs-stage2-015-cloudflare-local-capture-v1",
                "status": "CAPTURED" if successful else "FAILED",
                "benchmark_exit": completed.returncode,
                "copy_exit": copied.returncode,
                "row_count": len(raw.get("results", [])),
                "matrix_exact": keys == expected,
                "fail_markers": clean_stdout.count("FAIL"),
                "wall_ns": ended - started,
            },
        )
    finally:
        logs = run(["docker", "logs", container], check=False)
        output.joinpath("daemon.stdout").write_bytes(logs.stdout)
        output.joinpath("daemon.stderr").write_bytes(logs.stderr)
        killed = run(["docker", "kill", "--signal", "TERM", container], check=False)
        waited = run(["docker", "wait", container], check=False)
        removed = run(["docker", "rm", container], check=False)
        write_json(
            output / "cleanup.json",
            {
                "kill_exit": killed.returncode,
                "wait_exit": waited.returncode,
                "container_exit": waited.stdout.decode().strip(),
                "remove_exit": removed.returncode,
                "container_absent": docker_absent("container", container),
                "status": "PASS" if removed.returncode == 0 and docker_absent("container", container) else "FAIL",
            },
        )
    if not successful:
        raise RuntimeError(f"Cloudflare local {control} population failed")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("root", type=Path)
    arguments = parser.parse_args()
    if arguments.root.exists():
        raise SystemExit(f"evidence root already exists: {arguments.root}")
    actual = run(["docker", "image", "inspect", IMAGE_ID, "--format", "{{.Id}} {{.Architecture}}"]).stdout.decode().strip()
    if actual != f"{IMAGE_ID} arm64":
        raise SystemExit(f"image admission mismatch: {actual}")
    for control in ("var", "tmp"):
        population(arguments.root, control)


if __name__ == "__main__":
    main()
