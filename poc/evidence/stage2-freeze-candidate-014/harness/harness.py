#!/usr/bin/env python3
"""Exact Stage 2 benchmark and per-scenario CPU evidence collector.

Host modes launch fresh, quota-bound LayerFS containers.  The private
``_scenario-cpu`` mode runs inside one such container so it can read the
daemon's per-thread schedstat counters without lowering Docker isolation.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import time


FS_BENCH_SHA256 = "0e2004339a9269b88c2de451d56575957477bde16b96a10cf1837519e37230ef"
OWNED = "/var/tmp/layerfs-owned"
CGROUP_FILES = (
    "cpu.stat",
    "io.stat",
    "memory.current",
    "memory.peak",
    "memory.stat",
    "memory.events",
    "pids.current",
    "pids.peak",
)
SCENARIOS = (
    (
        "create 1000 files",
        "for i in $(seq 1 1000); do echo $i > f$i; done",
        "",
    ),
    (
        "stat 1000 files",
        "for i in $(seq 1 1000); do echo $i > f$i; done; "
        "for i in $(seq 1 1000); do stat f$i; done",
        "",
    ),
    (
        "rm 1000 files",
        "for i in $(seq 1 1000); do echo $i > f$i; done; rm f*",
        "",
    ),
    (
        "mkdir tree (10x10x10)",
        "for a in $(seq 1 10); do for b in $(seq 1 10); do mkdir -p $a/$b; "
        "for c in $(seq 1 10); do touch $a/$b/$c; done; done; done",
        "",
    ),
    (
        "find tree",
        "for a in $(seq 1 10); do for b in $(seq 1 10); do mkdir -p $a/$b; "
        "for c in $(seq 1 10); do touch $a/$b/$c; done; done; done; "
        "find . -type f | wc -l",
        "",
    ),
    (
        "write 64 MiB",
        "dd if=/dev/zero of=big bs=1M count=64 status=none",
        "",
    ),
    (
        "copy 64 MiB",
        "dd if=/dev/zero of=big bs=1M count=64 status=none; cp big big2",
        "",
    ),
    (
        "read 64 MiB",
        "dd if=/dev/zero of=big bs=1M count=64 status=none; cat big > /dev/null",
        "",
    ),
    (
        "pure read 64 MiB",
        "cat big > /dev/null",
        "dd if=/dev/zero of=big bs=1M count=64 status=none",
    ),
    (
        "pure copy 64 MiB",
        "cp big big2",
        "dd if=/dev/zero of=big bs=1M count=64 status=none",
    ),
    (
        "overwrite 64 MiB",
        "dd if=/dev/zero of=big bs=1M count=64 status=none conv=notrunc",
        "dd if=/dev/zero of=big bs=1M count=64 status=none",
    ),
    (
        "git init + commit 100 files",
        "git init -q; for i in $(seq 1 100); do echo $i > f$i; done; "
        "git add -A; git -c user.email=a@b -c user.name=a commit -qm init",
        "",
    ),
)
FULL_SCENARIOS = ",".join(name for name, _, _ in SCENARIOS)


def json_new(path: Path, value: object) -> None:
    with path.open("x") as output:
        json.dump(value, output, indent=2, sort_keys=True)
        output.write("\n")


def text_new(path: Path, value: str) -> None:
    with path.open("x") as output:
        output.write(value)


def run(argv: list[str], *, check: bool = True) -> subprocess.CompletedProcess[bytes]:
    return subprocess.run(argv, check=check, stdout=subprocess.PIPE, stderr=subprocess.PIPE)


def slug(name: str) -> str:
    return re.sub(r"[^a-z0-9]+", "-", name.lower()).strip("-")


def validate_owner(value: str) -> str:
    if not re.fullmatch(r"[a-z0-9][a-z0-9-]{0,31}", value):
        raise ValueError("owner must match [a-z0-9][a-z0-9-]{0,31}")
    return value


def phase_paths(evidence_root: Path, phase: str) -> Path:
    evidence_root.mkdir(parents=True, exist_ok=True)
    output = evidence_root / phase
    output.mkdir()
    return output


def absent(kind: str, name: str) -> bool:
    return subprocess.run(
        ["docker", kind, "inspect", name],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    ).returncode != 0


def cgroup_probe_argv(container: str) -> list[str]:
    code = (
        "import json,time; from pathlib import Path; "
        f"names={CGROUP_FILES!r}; root=Path('/sys/fs/cgroup'); "
        "print(json.dumps({'captured_unix_ns':time.time_ns(),"
        "'files':{n:(root/n).read_text() for n in names}},sort_keys=True))"
    )
    return ["docker", "exec", container, "python3", "-c", code]


def capture_cgroup(container: str, output: Path, boundary: str) -> None:
    result = run(cgroup_probe_argv(container))
    receipt = json.loads(result.stdout)
    json_new(output / f"{boundary}-cgroup.json", receipt)
    for name in CGROUP_FILES:
        text_new(output / f"{boundary}-{name}.txt", receipt["files"][name])


def wait_for_mount(container: str) -> None:
    for _ in range(100):
        if subprocess.run(
            ["docker", "exec", container, "mountpoint", "-q", "/workspace"],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        ).returncode == 0:
            return
        time.sleep(0.05)
    raise RuntimeError("/workspace did not become a mountpoint")


def copy_from(container: str, source: str, target: Path) -> None:
    result = run(["docker", "cp", f"{container}:{source}", str(target)], check=False)
    if result.returncode:
        raise RuntimeError(result.stderr.decode(errors="replace"))


def launch_runtime(image: str, owner: str, phase: str, output: Path) -> tuple[str, str, list[str]]:
    container = f"layerfs-stage2-014-{owner}-{phase}"
    volume = f"layerfs_stage2_014_{owner.replace('-', '_')}_{phase.replace('-', '_')}_store"
    if not absent("container", container) or not absent("volume", volume):
        raise RuntimeError(f"owned runtime already exists: {container} / {volume}")
    volume_result = run(["docker", "volume", "create", volume])
    text_new(output / "volume-create.stdout", volume_result.stdout.decode())
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
        "3g",
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
        "-v",
        f"{volume}:/var/lib/layerfs",
        image,
        "--store",
        "/var/lib/layerfs/store.sqlite",
        "--mount",
        "/workspace",
        "--spool",
        f"{OWNED}/spool",
        "--receipt",
        f"{OWNED}/terminal.json",
        "--ref",
        "main",
        "--integrity",
        "trusted",
        "--uid",
        "0",
        "--gid",
        "0",
    ]
    launched = run(launch, check=False)
    text_new(output / "docker-run.stdout", launched.stdout.decode())
    text_new(output / "docker-run.stderr", launched.stderr.decode())
    if launched.returncode:
        raise RuntimeError(f"docker run failed with {launched.returncode}")
    wait_for_mount(container)
    inspect = run(["docker", "inspect", container]).stdout
    with (output / "docker-inspect.json").open("xb") as target:
        target.write(inspect)
    cpu_max = run(["docker", "exec", container, "cat", "/sys/fs/cgroup/cpu.max"])
    text_new(output / "cpu.max.txt", cpu_max.stdout.decode())
    mountinfo = run(["docker", "exec", container, "cat", "/proc/self/mountinfo"])
    text_new(output / "mountinfo.txt", mountinfo.stdout.decode())
    script_hash = run(
        ["docker", "exec", container, "sha256sum", "/usr/local/bin/fs-bench.sh"]
    ).stdout.decode()
    text_new(output / "fs-bench.sha256", script_hash)
    if script_hash.split()[0] != FS_BENCH_SHA256:
        raise RuntimeError("image fs-bench hash mismatch")
    return container, volume, launch


def stop_runtime(container: str, volume: str, output: Path, success: bool) -> None:
    killed = run(["docker", "kill", "--signal", "TERM", container], check=False)
    text_new(output / "docker-kill.stdout", killed.stdout.decode())
    text_new(output / "docker-kill.stderr", killed.stderr.decode())
    waited = run(["docker", "wait", container], check=False)
    text_new(output / "daemon.exit", waited.stdout.decode())
    copy_from(container, f"{OWNED}/terminal.json", output / "terminal.json")
    if success and killed.returncode == waited.returncode == 0 and waited.stdout.strip() == b"0":
        removed = run(["docker", "rm", container], check=False)
        volume_removed = run(["docker", "volume", "rm", volume], check=False)
        json_new(
            output / "cleanup.json",
            {
                "container_remove_exit": removed.returncode,
                "volume_remove_exit": volume_removed.returncode,
                "status": "PASS"
                if removed.returncode == volume_removed.returncode == 0
                else "FAIL",
            },
        )
    else:
        json_new(
            output / "cleanup.json",
            {
                "status": "PRESERVED_FAILURE",
                "container": container,
                "volume": volume,
            },
        )


def authoritative_phase(image: str, owner: str, evidence_root: Path, control: str) -> None:
    base = "/var/tmp" if control == "var" else "/tmp"
    phase = f"authoritative-{control}"
    output = phase_paths(evidence_root, phase)
    container = volume = ""
    successful = False
    try:
        container, volume, launch = launch_runtime(image, owner, phase, output)
        raw_inside = f"{OWNED}/{phase}.json"
        benchmark_env = {
            "SCENARIOS": FULL_SCENARIOS,
            "REPS": "3",
            "WARMUP": "1",
            "RANDOMIZE_TARGETS": "1",
            "MOUNT": "/workspace",
            "BASE": base,
            "OUTPUT_JSON": raw_inside,
        }
        benchmark = ["docker", "exec"]
        for key, value in benchmark_env.items():
            benchmark.extend(["-e", f"{key}={value}"])
        benchmark.extend([container, "bash", "/usr/local/bin/fs-bench.sh"])
        json_new(
            output / "plan.json",
            {
                "schema": "layerfs-stage2-014-authoritative-plan-v1",
                "image": image,
                "container": container,
                "volume": volume,
                "daemon_argv": launch,
                "benchmark_argv": benchmark,
                "benchmark_env": benchmark_env,
                "cgroup_probe_argv": cgroup_probe_argv(container),
                "timeout_seconds": 120,
                "fs_bench_sha256": FS_BENCH_SHA256,
            },
        )
        capture_cgroup(container, output, "before")
        started = time.time_ns()
        with (output / "benchmark.stdout").open("xb") as stdout, (
            output / "benchmark.stderr"
        ).open("xb") as stderr:
            completed = subprocess.run(
                benchmark,
                stdout=stdout,
                stderr=stderr,
                timeout=120,
            )
        ended = time.time_ns()
        text_new(output / "wall-start-unix-ns.txt", f"{started}\n")
        text_new(output / "wall-end-unix-ns.txt", f"{ended}\n")
        text_new(output / "benchmark.exit", f"{completed.returncode}\n")
        capture_cgroup(container, output, "after")
        copy_from(container, raw_inside, output / "fs-bench.json")
        raw = json.loads((output / "fs-bench.json").read_text())
        keys = {(row.get("scenario"), row.get("target")) for row in raw.get("results", [])}
        expected = {(name, target) for name, _, _ in SCENARIOS for target in ("computerd", "base")}
        successful = completed.returncode == 0 and len(raw.get("results", [])) == 24 and keys == expected
        json_new(
            output / "capture.json",
            {
                "schema": "layerfs-stage2-014-authoritative-capture-v1",
                "status": "CAPTURED" if successful else "FAILED",
                "benchmark_exit": completed.returncode,
                "wall_ns": ended - started,
                "row_count": len(raw.get("results", [])),
                "matrix_exact": keys == expected and len(keys) == 24,
                "timing_disposition": "UNVERIFIED_RAW",
            },
        )
    except Exception as error:
        json_new(output / "failure.json", {"status": "FAIL", "error": str(error)})
        raise
    finally:
        if container and volume:
            stop_runtime(container, volume, output, successful)
    if not successful:
        raise RuntimeError(f"{phase} did not produce a complete raw capture")


def find_daemon(executable: str) -> int:
    matches = []
    for proc in Path("/proc").iterdir():
        if not proc.name.isdigit():
            continue
        try:
            if os.readlink(proc / "exe") == executable:
                matches.append(int(proc.name))
        except (FileNotFoundError, PermissionError):
            pass
    if len(matches) != 1:
        raise RuntimeError(f"expected one {executable} process, found {matches}")
    return matches[0]


def schedstat(pid: int) -> dict[str, int]:
    values = {}
    for task in sorted((Path(f"/proc/{pid}/task")).iterdir(), key=lambda path: int(path.name)):
        fields = (task / "schedstat").read_text().split()
        if len(fields) < 3:
            raise RuntimeError(f"short schedstat for task {task.name}")
        values[task.name] = int(fields[0])
    if not values:
        raise RuntimeError("daemon has no schedstat tasks")
    return values


def shell(command: str, cwd: Path) -> int:
    return subprocess.run(
        ["bash", "-c", command],
        cwd=cwd,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    ).returncode


def row(name: str, target: str, elapsed: int) -> dict[str, object]:
    return {
        "scenario": name,
        "target": target,
        "meanNs": elapsed,
        "medianNs": elapsed,
        "p95Ns": elapsed,
        "minNs": elapsed,
        "maxNs": elapsed,
        "samples": 1,
    }


def in_container_scenario_cpu(output: Path, daemon_executable: str) -> None:
    output.mkdir()
    pid = find_daemon(daemon_executable)
    manifest = []
    stable = True
    for index, (name, command, prep) in enumerate(SCENARIOS, 1):
        rows = []
        measurement = None
        prep_codes = {}
        command_codes = {}
        for target_path, target in (("/workspace", "computerd"), ("/var/tmp", "base")):
            directory = Path(target_path) / f".stage2-cpu-{os.getpid()}-{index}-{target}"
            if directory.exists():
                raise RuntimeError(f"target already exists: {directory}")
            directory.mkdir()
            try:
                prep_code = shell(prep, directory) if prep else 0
                prep_codes[target] = prep_code
                if prep_code:
                    raise RuntimeError(f"prep failed for {name} on {target}")
                before = schedstat(pid) if target == "computerd" else None
                started = time.perf_counter_ns()
                command_code = shell(command, directory)
                ended = time.perf_counter_ns()
                after = schedstat(pid) if target == "computerd" else None
                command_codes[target] = command_code
                if command_code:
                    raise RuntimeError(f"command failed for {name} on {target}")
                elapsed = ended - started
                rows.append(row(name, target, elapsed))
                if target == "computerd":
                    stable = stable and set(before or ()) == set(after or ())
                    measurement = {
                        "daemon_pid": pid,
                        "command_wall_ns": elapsed,
                        "schedstat_runtime_ns_before": before,
                        "schedstat_runtime_ns_after": after,
                    }
            finally:
                shutil.rmtree(directory, ignore_errors=True)
        artifact = {
            "schema": "layerfs-stage2-014-scenario-cpu-raw-v1",
            "config": {
                "reps": 1,
                "warmup": 0,
                "randomizeTargets": 0,
                "mount": "/workspace",
                "base": "/var/tmp",
                "external_exact_scenario": True,
            },
            "scenario": name,
            "command": command,
            "prep": prep,
            "prep_returncodes": prep_codes,
            "command_returncodes": command_codes,
            "results": rows,
            "layerfs_cpu_measurement": measurement,
        }
        filename = f"{index:02d}-{slug(name)}.json"
        json_new(output / filename, artifact)
        manifest.append(filename)
    json_new(
        output / "collection.json",
        {
            "schema": "layerfs-stage2-014-scenario-cpu-collection-v1",
            "status": "CAPTURED" if stable else "UNSTABLE_TASK_SET",
            "daemon_pid": pid,
            "raw_artifacts": manifest,
            "gate_disposition": "UNVERIFIED_RAW",
        },
    )
    if not stable:
        raise RuntimeError("daemon task set changed across a command boundary")


def scenario_cpu_phase(image: str, owner: str, evidence_root: Path) -> None:
    phase = "scenario-cpu"
    output = phase_paths(evidence_root, phase)
    container = volume = ""
    successful = False
    try:
        container, volume, launch = launch_runtime(image, owner, phase, output)
        inside_script = f"{OWNED}/harness.py"
        inside_output = f"{OWNED}/scenario-cpu"
        run(["docker", "cp", str(Path(__file__).resolve()), f"{container}:{inside_script}"])
        collector = [
            "docker",
            "exec",
            container,
            "python3",
            inside_script,
            "_scenario-cpu",
            "--output",
            inside_output,
            "--daemon-executable",
            "/usr/local/bin/layerfs-fuse",
        ]
        json_new(
            output / "plan.json",
            {
                "schema": "layerfs-stage2-014-scenario-cpu-plan-v1",
                "image": image,
                "container": container,
                "volume": volume,
                "daemon_argv": launch,
                "collector_argv": collector,
                "collector_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
                "cpu_source": "/proc/<daemon-pid>/task/*/schedstat field 1",
            },
        )
        with (output / "collector.stdout").open("xb") as stdout, (
            output / "collector.stderr"
        ).open("xb") as stderr:
            completed = subprocess.run(collector, stdout=stdout, stderr=stderr)
        text_new(output / "collector.exit", f"{completed.returncode}\n")
        copy_from(container, inside_output, output / "raw")
        collection = json.loads((output / "raw" / "collection.json").read_text())
        successful = completed.returncode == 0 and collection["status"] == "CAPTURED"
    except Exception as error:
        json_new(output / "failure.json", {"status": "FAIL", "error": str(error)})
        raise
    finally:
        if container and volume:
            stop_runtime(container, volume, output, successful)
    if not successful:
        raise RuntimeError("scenario CPU collection failed")


def self_check() -> None:
    names = [name for name, _, _ in SCENARIOS]
    assert len(names) == len(set(names)) == 12
    assert len({slug(name) for name in names}) == 12
    assert FULL_SCENARIOS.count(",") == 11
    assert len(FS_BENCH_SHA256) == 64
    assert len(CGROUP_FILES) == len(set(CGROUP_FILES)) == 8
    assert validate_owner("audit-014") == "audit-014"


def main() -> None:
    if sys.argv[1:] == ["--self-check"]:
        self_check()
        return
    parser = argparse.ArgumentParser()
    subparsers = parser.add_subparsers(dest="mode", required=True)
    for mode in ("authoritative", "scenario-cpu"):
        child = subparsers.add_parser(mode)
        child.add_argument("--image", required=True)
        child.add_argument("--owner", required=True, type=validate_owner)
        child.add_argument("--evidence-root", required=True, type=Path)
        if mode == "authoritative":
            child.add_argument("--control", choices=("var", "tmp", "both"), default="both")
    inside = subparsers.add_parser("_scenario-cpu")
    inside.add_argument("--output", required=True, type=Path)
    inside.add_argument("--daemon-executable", required=True)
    arguments = parser.parse_args()
    if arguments.mode == "_scenario-cpu":
        in_container_scenario_cpu(arguments.output, arguments.daemon_executable)
    elif arguments.mode == "scenario-cpu":
        scenario_cpu_phase(arguments.image, arguments.owner, arguments.evidence_root.resolve())
    else:
        controls = ("var", "tmp") if arguments.control == "both" else (arguments.control,)
        for control in controls:
            authoritative_phase(
                arguments.image,
                arguments.owner,
                arguments.evidence_root.resolve(),
                control,
            )


if __name__ == "__main__":
    main()
