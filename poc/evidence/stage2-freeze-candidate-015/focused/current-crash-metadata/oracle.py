#!/usr/bin/env python3
"""Focused post-ack SIGKILL/reopen proof for the frozen candidate-015 image."""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import math
from pathlib import Path
import re
import subprocess
import sys
import time


ROOT = Path(__file__).resolve().parents[2]
HARNESS_PATH = ROOT / "runtime" / "durable-fs-bench.py"
spec = importlib.util.spec_from_file_location("durable_fs_bench", HARNESS_PATH)
if spec is None or spec.loader is None:
    raise RuntimeError(f"could not load {HARNESS_PATH}")
harness = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = harness
spec.loader.exec_module(harness)


TIMED_CRASH_SCRIPT = r'''
import json, os, sqlite3, subprocess, sys, time

directory, command = sys.argv[1:]

def daemon_pid():
    matches = []
    for name in os.listdir("/proc"):
        if not name.isdigit():
            continue
        try:
            if os.path.realpath(f"/proc/{name}/exe") == "/usr/local/bin/layerfs-fuse":
                matches.append(int(name))
        except (FileNotFoundError, PermissionError):
            pass
    if len(matches) != 1:
        raise RuntimeError(f"expected one LayerFS daemon, found {matches}")
    return matches[0]

def status(pid):
    wanted = {"Threads", "VmHWM", "VmRSS"}
    result = {}
    with open(f"/proc/{pid}/status") as source:
        for line in source:
            fields = line.split()
            if fields and fields[0].rstrip(":") in wanted:
                key = fields[0].rstrip(":")
                result[key] = int(fields[1]) * (1024 if key.startswith("Vm") else 1)
    if set(result) != wanted:
        raise RuntimeError(f"incomplete daemon status: {result}")
    result["FDs"] = len(os.listdir(f"/proc/{pid}/fd"))
    return result

def schedstat(pid):
    result = {}
    for task in sorted(os.listdir(f"/proc/{pid}/task"), key=int):
        with open(f"/proc/{pid}/task/{task}/schedstat") as source:
            fields = source.read().split()
        if len(fields) < 3:
            raise RuntimeError(f"short schedstat for task {task}")
        result[task] = int(fields[0])
    if not result:
        raise RuntimeError("daemon has no schedstat tasks")
    return result

def cgroup_values(name):
    result = {}
    with open(f"/sys/fs/cgroup/{name}") as source:
        for line in source:
            key, value = line.split()
            result[key] = int(value)
    return result

def scalar(name):
    with open(f"/sys/fs/cgroup/{name}") as source:
        return int(source.read().strip())

def ref_state():
    connection = sqlite3.connect(
        "file:/var/lib/layerfs/store.sqlite?mode=ro", uri=True
    )
    row = connection.execute(
        "SELECT generation, root_id FROM layerfs_refs WHERE name='main'"
    ).fetchone()
    connection.close()
    if row is None:
        raise RuntimeError("main ref missing after fsyncdir")
    return {"generation": row[0], "root": bytes(row[1]).hex()}

pid = daemon_pid()
before = {
    "daemon_status": status(pid),
    "schedstat_runtime_ns": schedstat(pid),
    "cpu_stat": cgroup_values("cpu.stat"),
    "memory_events": cgroup_values("memory.events"),
    "memory_current_bytes": scalar("memory.current"),
    "memory_peak_bytes": scalar("memory.peak"),
}
descriptor = os.open("/workspace", os.O_RDONLY | os.O_DIRECTORY)
started_ns = time.perf_counter_ns()
started_unix_ns = time.time_ns()
completed = subprocess.run(
    ["bash", "-c", command],
    cwd=directory,
    stdout=subprocess.PIPE,
    stderr=subprocess.PIPE,
)
live_done_ns = time.perf_counter_ns()
if completed.returncode:
    os.close(descriptor)
    print(json.dumps({
        "command_returncode": completed.returncode,
        "command_stdout": completed.stdout.decode(errors="replace"),
        "command_stderr": completed.stderr.decode(errors="replace"),
    }, sort_keys=True))
    raise SystemExit(completed.returncode)
os.fsync(descriptor)
durable_done_ns = time.perf_counter_ns()
durable_done_unix_ns = time.time_ns()
os.close(descriptor)
acknowledged_ref = ref_state()
after = {
    "daemon_status": status(pid),
    "schedstat_runtime_ns": schedstat(pid),
    "cpu_stat": cgroup_values("cpu.stat"),
    "memory_events": cgroup_values("memory.events"),
    "memory_current_bytes": scalar("memory.current"),
    "memory_peak_bytes": scalar("memory.peak"),
}
emitted_ns = time.perf_counter_ns()
emitted_unix_ns = time.time_ns()
print(json.dumps({
    "schema": "layerfs-stage2-015-focused-crash-timing-v1",
    "command_returncode": 0,
    "command_stdout": completed.stdout.decode(errors="strict"),
    "command_stderr": completed.stderr.decode(errors="strict"),
    "T_live_ns": live_done_ns - started_ns,
    "T_checkpoint_ns": durable_done_ns - live_done_ns,
    "T_to_durable_ns": durable_done_ns - started_ns,
    "command_started_unix_ns": started_unix_ns,
    "durability_acknowledged_unix_ns": durable_done_unix_ns,
    "helper_emitted_unix_ns": emitted_unix_ns,
    "post_ack_helper_ns": emitted_ns - durable_done_ns,
    "acknowledged_ref": acknowledged_ref,
    "resources": {
        "daemon_pid": pid,
        "command_started_ns": started_ns,
        "durability_acknowledged_ns": durable_done_ns,
        "before": before,
        "after": after,
    },
}, sort_keys=True))
'''


PAYLOAD_ANALYSIS_SCRIPT = r'''
import hashlib, json, math, sys
path = sys.argv[1]
digest = hashlib.sha256()
histogram = [0] * 256
size = 0
with open(path, "rb", buffering=0) as source:
    while True:
        block = source.read(1024 * 1024)
        if not block:
            break
        digest.update(block)
        size += len(block)
        for value in block:
            histogram[value] += 1
entropy = -sum(
    (count / size) * math.log2(count / size)
    for count in histogram if count
)
print(json.dumps({
    "size": size,
    "sha256": digest.hexdigest(),
    "shannon_entropy_bits_per_byte": entropy,
    "distinct_byte_values": sum(count > 0 for count in histogram),
    "histogram": histogram,
}, separators=(",", ":"), sort_keys=True))
'''


COMMANDS = {
    "metadata": (
        "mkdir crash-case; cd crash-case; "
        "for a in $(seq 1 10); do for b in $(seq 1 10); do mkdir -p $a/$b; "
        "for c in $(seq 1 10); do touch $a/$b/$c; done; done; done",
        "lfs-c015-crash-metadata",
        "lfs_c015_crash_metadata_store",
    ),
    "payload": (
        "mkdir crash-case; "
        "dd if=/dev/urandom of=crash-case/payload.bin bs=1M count=64 status=none; "
        "sha256sum crash-case/payload.bin",
        "lfs-c015-crash-payload",
        "lfs_c015_crash_payload_store",
    ),
}


def normalized_snapshot(snapshot: dict[str, object]) -> list[dict[str, object]]:
    result = []
    for item in snapshot["entries"]:
        if item["path_hex"] == b".".hex():
            continue
        normalized = {
            "path_hex": item["path_hex"],
            "type": item["type"],
            "mode": item["mode"],
        }
        for key in ("size", "sha256", "target_hex"):
            if key in item:
                normalized[key] = item[key]
        result.append(normalized)
    return sorted(result, key=lambda item: bytes.fromhex(item["path_hex"]))


def expected_metadata() -> list[dict[str, object]]:
    empty = hashlib.sha256(b"").hexdigest()
    result: list[dict[str, object]] = [
        {"path_hex": b"crash-case".hex(), "type": "directory", "mode": 0o755}
    ]
    for a in range(1, 11):
        one = f"crash-case/{a}".encode()
        result.append({"path_hex": one.hex(), "type": "directory", "mode": 0o755})
        for b in range(1, 11):
            two = f"crash-case/{a}/{b}".encode()
            result.append({"path_hex": two.hex(), "type": "directory", "mode": 0o755})
            for c in range(1, 11):
                path = f"crash-case/{a}/{b}/{c}".encode()
                result.append({
                    "path_hex": path.hex(),
                    "type": "file",
                    "mode": 0o644,
                    "size": 0,
                    "sha256": empty,
                })
    return sorted(result, key=lambda item: bytes.fromhex(item["path_hex"]))


def start_sampler(output: Path, container: str, sidecar: str) -> tuple[Path, Path, Path]:
    ready = output / "sampler-ready"
    stop = output / "sampler-stop"
    sampler_output = output / "sampler.json"
    argv = [
        "docker", "run", "-d", "--name", sidecar,
        "--platform", "linux/arm64",
        "--pid", f"container:{container}",
        "--cap-add", "SYS_PTRACE",
        "--cgroupns", "host",
        "--network", "none",
        "--read-only",
        "--cpus", "0.25",
        "--memory", "128m",
        "--pids-limit", "32",
        "--mount", f"type=bind,src={output.resolve()},dst=/evidence",
        "--entrypoint", "python3", harness.IMAGE,
        "-c", harness.SAMPLER_SCRIPT,
        "/evidence/sampler-ready", "/evidence/sampler-stop", "/evidence/sampler.json",
    ]
    harness.write_json(output / "sampler-plan.json", {"argv": argv, "sidecar": sidecar})
    launched = harness.run(argv, check=False)
    harness.write_text(output / "sampler-launch.stdout", launched.stdout.decode())
    harness.write_text(output / "sampler-launch.stderr", launched.stderr.decode())
    if launched.returncode:
        raise harness.HarnessError(f"resource sampler did not launch: {sidecar}")
    for _ in range(500):
        if ready.exists():
            return ready, stop, sampler_output
        time.sleep(0.01)
    raise harness.HarnessError(f"resource sampler did not become ready: {sidecar}")


def stop_sampler(output: Path, sidecar: str, sampler_output: Path) -> dict[str, object]:
    waited = harness.run(["docker", "wait", sidecar], check=False)
    harness.write_text(output / "sampler-exit", waited.stdout.decode())
    logs = harness.run(["docker", "logs", sidecar], check=False)
    harness.write_text(output / "sampler-logs.stdout", logs.stdout.decode())
    harness.write_text(output / "sampler-logs.stderr", logs.stderr.decode())
    inspected = harness.run(["docker", "inspect", sidecar], check=False)
    if inspected.returncode == 0:
        harness.write_json(output / "sampler-inspect.json", json.loads(inspected.stdout)[0])
    removed = harness.run(["docker", "rm", sidecar], check=False)
    harness.write_text(output / "sampler-rm.stdout", removed.stdout.decode())
    harness.write_text(output / "sampler-rm.stderr", removed.stderr.decode())
    if waited.returncode or waited.stdout.strip() != b"0" or removed.returncode:
        raise harness.HarnessError(f"resource sampler failed: {sidecar}")
    return json.loads(sampler_output.read_text())


def kill_after_ack(
    runtime: object,
    output: Path,
    helper_returned_unix_ns: int,
    acknowledged_unix_ns: int,
) -> dict[str, object]:
    container = runtime.container
    if container is None:
        raise harness.HarnessError("no active container to kill")
    kill_requested_unix_ns = time.time_ns()
    killed = harness.run(["docker", "kill", "--signal", "KILL", container], check=False)
    kill_returned_unix_ns = time.time_ns()
    waited = harness.run(["docker", "wait", container], check=False)
    wait_returned_unix_ns = time.time_ns()
    harness.write_text(output / "crash.kill.stdout", killed.stdout.decode())
    harness.write_text(output / "crash.kill.stderr", killed.stderr.decode())
    harness.write_text(output / "crash.wait.stdout", waited.stdout.decode())
    harness.write_text(output / "crash.wait.stderr", waited.stderr.decode())
    logs = harness.run(["docker", "logs", container], check=False)
    harness.write_text(output / "crash.logs.stdout", logs.stdout.decode())
    harness.write_text(output / "crash.logs.stderr", logs.stderr.decode())
    stopped = json.loads(harness.run(["docker", "inspect", container]).stdout)[0]
    harness.write_json(output / "crash.stopped-inspect.json", stopped)
    unexpected = harness.run(
        ["docker", "cp", f"{container}:{harness.OWNED}/terminal.json", str(output / "crash.unexpected-terminal.json")],
        check=False,
    )
    harness.write_text(output / "crash.terminal-copy.stdout", unexpected.stdout.decode())
    harness.write_text(output / "crash.terminal-copy.stderr", unexpected.stderr.decode())
    checks = {
        "ack_to_kill_request_under_250ms": 0 <= kill_requested_unix_ns - acknowledged_unix_ns <= 250_000_000,
        "helper_return_to_kill_request_under_20ms": 0 <= kill_requested_unix_ns - helper_returned_unix_ns <= 20_000_000,
        "kill_command_succeeded": killed.returncode == 0,
        "exit_137": waited.returncode == 0 and waited.stdout.strip() == b"137",
        "not_oom_killed": stopped["State"]["OOMKilled"] is False,
        "no_graceful_terminal_receipt": unexpected.returncode != 0,
    }
    receipt = {
        "schema": "layerfs-stage2-015-focused-crash-window-v1",
        "status": "PASS" if all(checks.values()) else "FAIL",
        "checks": checks,
        "durability_acknowledged_unix_ns": acknowledged_unix_ns,
        "helper_returned_unix_ns": helper_returned_unix_ns,
        "kill_requested_unix_ns": kill_requested_unix_ns,
        "kill_returned_unix_ns": kill_returned_unix_ns,
        "wait_returned_unix_ns": wait_returned_unix_ns,
        "ack_to_kill_request_ns": kill_requested_unix_ns - acknowledged_unix_ns,
        "helper_return_to_kill_request_ns": kill_requested_unix_ns - helper_returned_unix_ns,
    }
    harness.write_json(output / "crash-window.json", receipt)
    if receipt["status"] != "PASS":
        raise harness.HarnessError(f"post-ack crash window failed: {checks}")
    removed = harness.run(["docker", "rm", container], check=False)
    harness.write_text(output / "crash.rm.stdout", removed.stdout.decode())
    harness.write_text(output / "crash.rm.stderr", removed.stderr.decode())
    if removed.returncode or not harness.docker_absent("container", container):
        raise harness.HarnessError(f"could not remove killed container {container}")
    runtime.container = None
    return receipt


def resource_receipt(timing: dict[str, object], sampler: dict[str, object]) -> dict[str, object]:
    before = timing["resources"]["before"]
    after = timing["resources"]["after"]
    tasks_before = before["schedstat_runtime_ns"]
    tasks_after = after["schedstat_runtime_ns"]
    stable = set(tasks_before) == set(tasks_after) and bool(tasks_before)
    monotonic = stable and all(tasks_after[key] >= tasks_before[key] for key in tasks_before)
    daemon_cpu_ns = sum(tasks_after.values()) - sum(tasks_before.values()) if monotonic else -1
    oom = after["memory_events"].get("oom", 0) - before["memory_events"].get("oom", 0)
    oom_kill = after["memory_events"].get("oom_kill", 0) - before["memory_events"].get("oom_kill", 0)
    checks = {
        "task_set_stable": stable,
        "schedstat_monotonic": monotonic,
        "oom_zero": oom == oom_kill == 0,
        "cgroup_memory_peak_bounded": after["memory_peak_bytes"] <= 512 * 1024 * 1024,
        "threads_bounded": sampler["threads_high_water"] <= 8,
        "fd_bounded": sampler["fd_high_water"] <= sampler["fd_baseline"] + 64,
        "sampler_started_before_command": sampler["sampler_started_ns"] <= timing["resources"]["command_started_ns"],
        "sampler_samples_nonzero": sampler["samples"] > 0,
    }
    return {
        "schema": "layerfs-stage2-015-focused-crash-resources-v1",
        "status": "PASS" if all(checks.values()) else "FAIL",
        "checks": checks,
        "daemon_cpu_ns": daemon_cpu_ns,
        "cpu_stat_before": before["cpu_stat"],
        "cpu_stat_after": after["cpu_stat"],
        "memory_events_before": before["memory_events"],
        "memory_events_after": after["memory_events"],
        "memory_peak_bytes": after["memory_peak_bytes"],
        "daemon_status_before": before["daemon_status"],
        "daemon_status_after": after["daemon_status"],
        "sampler": sampler,
    }


def write_checksums(output: Path) -> None:
    lines = []
    for path in sorted(output.rglob("*")):
        if not path.is_file() or path.name == "FILES.sha256":
            continue
        lines.append(f"{hashlib.sha256(path.read_bytes()).hexdigest()}  {path.relative_to(output)}")
    harness.write_text(output / "FILES.sha256", "\n".join(lines) + "\n")


def failure_cleanup(output: Path, prefix: str, volume: str, sidecar: str) -> None:
    commands = [
        ["docker", "rm", "--force", f"{prefix}-00-mutate"],
        ["docker", "rm", "--force", f"{prefix}-01-verify"],
        ["docker", "rm", "--force", sidecar],
        ["docker", "volume", "rm", volume],
    ]
    rows = []
    for argv in commands:
        completed = harness.run(argv, check=False)
        rows.append({
            "argv": argv,
            "returncode": completed.returncode,
            "stdout": completed.stdout.decode(errors="replace"),
            "stderr": completed.stderr.decode(errors="replace"),
        })
    path = output / "failure-cleanup.json"
    if not path.exists():
        harness.write_json(path, {"commands": rows})


def run_case(case: str, output: Path) -> None:
    command, prefix, volume = COMMANDS[case]
    sidecar = f"{prefix}-sampler"
    if any(not harness.docker_absent(kind, name) for kind, name in (
        ("container", f"{prefix}-00-mutate"),
        ("container", f"{prefix}-01-verify"),
        ("container", sidecar),
        ("volume", volume),
    )):
        raise harness.HarnessError("focused proof resource name already exists")
    oracle_sha256 = hashlib.sha256(Path(__file__).read_bytes()).hexdigest()
    plan = {
        "schema": "layerfs-stage2-015-focused-crash-plan-v1",
        "case": case,
        "source_commit": harness.SOURCE_COMMIT,
        "source_tree": harness.SOURCE_TREE,
        "image": harness.IMAGE,
        "image_id": harness.IMAGE_ID,
        "integrity": "Verified",
        "mutation_command": command,
        "timer": "mutation command start through whole-workspace fsyncdir return",
        "crash": "immediate docker kill --signal KILL after helper acknowledgement",
        "crash_window_bounds_ns": {
            "ack_to_kill_request": 250_000_000,
            "helper_return_to_kill_request": 20_000_000,
        },
        "reopen": "fresh daemon on retained Store volume with --integrity verified",
        "oracle_sha256": oracle_sha256,
        "harness_sha256": hashlib.sha256(HARNESS_PATH.read_bytes()).hexdigest(),
        "prefix": prefix,
        "volume": volume,
    }
    harness.write_json(output / "plan.json", plan)
    harness.write_text(output / "oracle.sha256", f"{oracle_sha256}  {Path(__file__).name}\n")
    binding = harness.image_binding()
    harness.write_json(output / "binding.json", binding)
    if binding["status"] != "PASS":
        raise harness.HarnessError(f"image binding failed: {binding}")
    runtime = harness.Runtime(output, volume, prefix)
    try:
        runtime.create_volume()
        container = runtime.launch("mutate")
        pristine_ref = runtime.ref("ref-00-pristine")
        _, stop, sampler_output = start_sampler(output, container, sidecar)
        timed_argv = [
            "docker", "exec", container, "python3", "-c", TIMED_CRASH_SCRIPT,
            harness.MOUNT, command,
        ]
        harness.write_json(output / "timed-command.plan.json", {"argv": timed_argv})
        helper_requested_unix_ns = time.time_ns()
        completed = harness.run(timed_argv, check=False, timeout=300)
        helper_returned_unix_ns = time.time_ns()
        harness.write_text(output / "timed-command.stdout", completed.stdout.decode(errors="replace"))
        harness.write_text(output / "timed-command.stderr", completed.stderr.decode(errors="replace"))
        if completed.returncode:
            raise harness.HarnessError(f"timed mutation exited {completed.returncode}")
        timing = json.loads(completed.stdout)
        if timing["T_to_durable_ns"] != timing["T_live_ns"] + timing["T_checkpoint_ns"]:
            raise harness.HarnessError("durable timing equation is not exact")
        timing["host_helper_requested_unix_ns"] = helper_requested_unix_ns
        timing["host_helper_returned_unix_ns"] = helper_returned_unix_ns
        harness.write_text(stop, "stop\n")
        crash = kill_after_ack(
            runtime,
            output,
            helper_returned_unix_ns,
            timing["durability_acknowledged_unix_ns"],
        )
        sampler = stop_sampler(output, sidecar, sampler_output)
        timing["sampler"] = sampler
        harness.write_json(output / "timing.json", timing)
        resources = resource_receipt(timing, sampler)
        harness.write_json(output / "resources.json", resources)
        if resources["status"] != "PASS":
            raise harness.HarnessError(f"resource receipt failed: {resources['checks']}")

        runtime.launch("verify")
        verified = runtime.state(harness.MOUNT, "state-01-verified-reopen")
        actual_inventory = normalized_snapshot(verified["snapshot"])
        harness.write_json(output / "actual-inventory-normalized.json", actual_inventory)
        checks = {
            "verified_startup": True,
            "generation_advanced_exactly_once": verified["ref"]["generation"] == pristine_ref["generation"] + 1,
            "root_changed": verified["ref"]["root"] != pristine_ref["root"],
            "acknowledged_ref_exact_after_reopen": verified["ref"] == timing["acknowledged_ref"],
            "crash_window_pass": crash["status"] == "PASS",
            "resources_pass": resources["status"] == "PASS",
        }
        if case == "metadata":
            expected = expected_metadata()
            harness.write_json(output / "expected-inventory-normalized.json", expected)
            checks.update({
                "metadata_inventory_exact": actual_inventory == expected,
                "metadata_descendant_count_exact": verified["snapshot"]["descendant_count"] == 1111,
                "metadata_file_bytes_exact": all(
                    item.get("size") == 0 and item.get("sha256") == hashlib.sha256(b"").hexdigest()
                    for item in actual_inventory if item["type"] == "file"
                ),
            })
        else:
            match = re.fullmatch(r"([0-9a-f]{64})  crash-case/payload\.bin\n", timing["command_stdout"])
            expected_sha256 = match.group(1) if match else None
            analysis = harness.exec_json(runtime.container, PAYLOAD_ANALYSIS_SCRIPT, f"{harness.MOUNT}/crash-case/payload.bin")
            harness.write_json(output / "payload-analysis.json", analysis)
            checks.update({
                "payload_command_digest_parsed": expected_sha256 is not None,
                "payload_inventory_exact": actual_inventory == [
                    {"path_hex": b"crash-case".hex(), "type": "directory", "mode": 0o755},
                    {
                        "path_hex": b"crash-case/payload.bin".hex(),
                        "type": "file",
                        "mode": 0o644,
                        "size": 64 * 1024 * 1024,
                        "sha256": expected_sha256,
                    },
                ],
                "payload_size_exact": analysis["size"] == 64 * 1024 * 1024,
                "payload_sha256_exact": analysis["sha256"] == expected_sha256,
                "payload_high_entropy": analysis["shannon_entropy_bits_per_byte"] >= 7.99 and analysis["distinct_byte_values"] == 256,
            })
        verification = {
            "schema": "layerfs-stage2-015-focused-crash-verification-v1",
            "status": "PASS" if all(checks.values()) else "FAIL",
            "checks": checks,
            "pristine_ref": pristine_ref,
            "acknowledged_ref": timing["acknowledged_ref"],
            "verified_ref": verified["ref"],
            "verified_entries_sha256": verified["snapshot"]["entries_sha256"],
        }
        harness.write_json(output / "verification.json", verification)
        if verification["status"] != "PASS":
            raise harness.HarnessError(f"Verified reopen failed: {checks}")

        cleanup_argv = [
            "docker", "exec", runtime.container, "python3", "-c",
            "import shutil; shutil.rmtree('/workspace/crash-case')",
        ]
        harness.write_json(output / "cleanup-command.plan.json", {"argv": cleanup_argv})
        cleanup_started = time.time_ns()
        cleanup_completed = harness.run(cleanup_argv, check=False)
        cleanup_ended = time.time_ns()
        harness.write_text(output / "cleanup-command.stdout", cleanup_completed.stdout.decode())
        harness.write_text(output / "cleanup-command.stderr", cleanup_completed.stderr.decode())
        if cleanup_completed.returncode:
            raise harness.HarnessError("cleanup mutation failed")
        cleanup_barrier = runtime.barrier("barrier-02-cleanup")
        clean_state = runtime.state(harness.MOUNT, "state-02-clean")
        terminal = runtime.graceful_terminal("cleanup-terminal")
        cleanup_checks = {
            "cleanup_command_succeeded": cleanup_completed.returncode == 0,
            "cleanup_barrier_returned": cleanup_barrier["fsyncdir_ns"] > 0,
            "clean_inventory_empty": clean_state["snapshot"]["descendant_count"] == 0,
            "terminal_root_exact": terminal["generation"] == clean_state["ref"]["generation"] and terminal["root"] == clean_state["ref"]["root"],
            "terminal_resources_clean": harness.terminal_is_clean(terminal),
        }
        cleanup_timing = {
            "cleanup_started_unix_ns": cleanup_started,
            "cleanup_ended_unix_ns": cleanup_ended,
            "cleanup_wall_ns": cleanup_ended - cleanup_started,
            "cleanup_fsyncdir_ns": cleanup_barrier["fsyncdir_ns"],
        }
        harness.write_json(output / "cleanup-timing.json", cleanup_timing)
        if not all(cleanup_checks.values()):
            raise harness.HarnessError(f"clean final state failed: {cleanup_checks}")
        runtime.remove_success()
        ps = harness.run(["docker", "ps", "-a", "--filter", f"name=^{prefix}", "--format", "{{.Names}}"], check=False)
        volume_inspect = harness.run(["docker", "volume", "inspect", volume], check=False)
        harness.write_text(output / "cleanup-docker-ps.stdout", ps.stdout.decode())
        harness.write_text(output / "cleanup-docker-ps.stderr", ps.stderr.decode())
        harness.write_text(output / "cleanup-volume-inspect.stdout", volume_inspect.stdout.decode())
        harness.write_text(output / "cleanup-volume-inspect.stderr", volume_inspect.stderr.decode())
        cleanup_checks.update({
            "owned_containers_absent": ps.returncode == 0 and not ps.stdout.strip(),
            "owned_volume_absent": volume_inspect.returncode != 0,
            "sampler_absent": harness.docker_absent("container", sidecar),
        })
        cleanup_receipt = {
            "schema": "layerfs-stage2-015-focused-crash-cleanup-v1",
            "status": "PASS" if all(cleanup_checks.values()) else "FAIL",
            "checks": cleanup_checks,
            "terminal": {
                "generation": terminal["generation"],
                "root": terminal["root"],
                "mounted": terminal["mounted"],
                "engine": terminal["engine"],
            },
        }
        harness.write_json(output / "cleanup-verification.json", cleanup_receipt)
        if cleanup_receipt["status"] != "PASS":
            raise harness.HarnessError(f"owned cleanup failed: {cleanup_checks}")

        receipt = {
            "schema": "layerfs-stage2-015-focused-current-crash-v1",
            "status": "PASS",
            "case": case,
            "source_commit": harness.SOURCE_COMMIT,
            "source_tree": harness.SOURCE_TREE,
            "image_id": harness.IMAGE_ID,
            "T_live_ns": timing["T_live_ns"],
            "T_checkpoint_ns": timing["T_checkpoint_ns"],
            "T_to_durable_ns": timing["T_to_durable_ns"],
            "ack_to_kill_request_ns": crash["ack_to_kill_request_ns"],
            "pristine_ref": pristine_ref,
            "accepted_ref": verified["ref"],
            "verified_entries_sha256": verified["snapshot"]["entries_sha256"],
            "verification": verification["checks"],
            "resource_checks": resources["checks"],
            "cleanup_checks": cleanup_receipt["checks"],
        }
        harness.write_json(output / "receipt.json", receipt)
        write_checksums(output)
    except BaseException as error:
        failure = {
            "schema": "layerfs-stage2-015-focused-current-crash-failure-v1",
            "status": "PRESERVED_FAILURE",
            "case": case,
            "error": f"{type(error).__name__}: {error}",
            "unix_ns": time.time_ns(),
        }
        if not (output / "failure.json").exists():
            harness.write_json(output / "failure.json", failure)
        failure_cleanup(output, prefix, volume, sidecar)
        raise


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--case", choices=sorted(COMMANDS), required=True)
    parser.add_argument("--output", type=Path, required=True)
    arguments = parser.parse_args()
    if not arguments.output.is_dir():
        raise SystemExit("output directory must already exist")
    run_case(arguments.case, arguments.output.resolve())


if __name__ == "__main__":
    main()
