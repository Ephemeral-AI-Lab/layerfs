#!/usr/bin/env python3
"""Run the three source-bound LayerFS lifecycle custody oracles."""

from __future__ import annotations

import argparse
import concurrent.futures
import datetime as dt
import json
from pathlib import Path
import platform
import subprocess
import threading
import time


REPO = Path(__file__).resolve().parents[4]
CANDIDATE = Path(__file__).resolve().parents[1]
FOCUSED = CANDIDATE / "focused"
IMAGE = "sha256:f8647b84580c75d4688a18665e4c60cd6dcf5b2d3092cf22bce34dfbd86b59b0"
SOURCE_COMMIT = "7e82abcd7320f6a214be336d82488ba0527b6025"
SOURCE_TREE = "df13d88eb7e7d2471971b0c58ca6425bb81b0b03"
FS_BENCH_SHA256 = "0e2004339a9269b88c2de451d56575957477bde16b96a10cf1837519e37230ef"
EVENT_LOCK = threading.Lock()


class OracleFailure(RuntimeError):
    pass


def now() -> dict[str, object]:
    return {
        "time_ns": time.time_ns(),
        "monotonic_ns": time.monotonic_ns(),
        "time_utc": dt.datetime.now(dt.timezone.utc).isoformat(),
    }


def write_json(path: Path, value: object) -> None:
    with path.open("x") as output:
        json.dump(value, output, indent=2, sort_keys=True)
        output.write("\n")


def event(root: Path, name: str, detail: object = None) -> dict[str, object]:
    row = {"event": name, "detail": detail, **now()}
    with EVENT_LOCK, (root / "events.jsonl").open("a") as output:
        output.write(json.dumps(row, sort_keys=True) + "\n")
    return row


def capture(
    root: Path,
    label: str,
    argv: list[str],
    *,
    timeout: float = 60,
    input_text: str | None = None,
) -> dict[str, object]:
    started = now()
    timed_out = False
    try:
        result = subprocess.run(
            argv,
            cwd=REPO,
            input=input_text,
            capture_output=True,
            text=True,
            timeout=timeout,
            check=False,
        )
        returncode = result.returncode
        stdout = result.stdout
        stderr = result.stderr
    except subprocess.TimeoutExpired as error:
        timed_out = True
        returncode = None
        stdout = error.stdout or ""
        stderr = error.stderr or ""
        if isinstance(stdout, bytes):
            stdout = stdout.decode(errors="replace")
        if isinstance(stderr, bytes):
            stderr = stderr.decode(errors="replace")
    ended = now()
    record = {
        "argv": argv,
        "cwd": str(REPO),
        "started": started,
        "ended": ended,
        "elapsed_ns": ended["monotonic_ns"] - started["monotonic_ns"],
        "returncode": returncode,
        "timed_out": timed_out,
    }
    (root / f"{label}.stdout").write_text(stdout)
    (root / f"{label}.stderr").write_text(stderr)
    write_json(root / f"{label}.command.json", record)
    event(root, f"command:{label}", {"returncode": returncode, "timed_out": timed_out})
    return {**record, "stdout": stdout, "stderr": stderr}


def require(condition: bool, message: str) -> None:
    if not condition:
        raise OracleFailure(message)


def common_run(name: str, volume: str) -> list[str]:
    return [
        "docker", "run", "-d", "--name", name,
        "--platform", "linux/arm64", "--init", "--stop-timeout", "30",
        "--cpus", "1", "--memory", "512m", "--pids-limit", "512",
        "--device", "/dev/fuse:rwm", "--cap-add", "SYS_ADMIN",
        "--network", "none", "-v", f"{volume}:/var/lib/layerfs", IMAGE,
        "--store", "/var/lib/layerfs/store.sqlite", "--mount", "/workspace",
        "--spool", "/var/tmp/layerfs-owned/spool",
        "--receipt", "/var/tmp/layerfs-owned/terminal.json",
        "--ref", "main", "--integrity", "verified", "--uid", "0", "--gid", "0",
    ]


def base_plan(scenario: str, names: list[str], volume: str) -> dict[str, object]:
    return {
        "schema": "layerfs-stage2-lifecycle-execution-plan-v1",
        "scenario": scenario,
        "source_commit": SOURCE_COMMIT,
        "source_tree": SOURCE_TREE,
        "image_id": IMAGE,
        "host_architecture": platform.machine(),
        "container_names": names,
        "volume_name": volume,
        "launch_argv": [common_run(name, volume) for name in names],
        "envelope": {
            "platform": "linux/arm64",
            "cpus": 1,
            "memory_bytes": 512 * 1024 * 1024,
            "pids_limit": 512,
            "network": "none",
            "integrity": "Verified",
            "mount_backend": "native Linux FUSE",
        },
    }


def preflight(root: Path, names: list[str], volume: str) -> None:
    for name in names:
        result = capture(root, f"preflight-container-{name}", ["docker", "container", "inspect", name])
        require(result["returncode"] != 0, f"owned container already exists: {name}")
    result = capture(root, "preflight-volume", ["docker", "volume", "inspect", volume])
    require(result["returncode"] != 0, f"owned volume already exists: {volume}")
    image = capture(root, "image-inspect", ["docker", "image", "inspect", IMAGE])
    require(image["returncode"] == 0, "exact image is unavailable")
    product = capture(
        root,
        "product-diff",
        ["git", "diff", "--exit-code", SOURCE_COMMIT, "--", "Cargo.toml", "Cargo.lock", "crates", "containers", "tools"],
    )
    require(product["returncode"] == 0, "product differs from the frozen source")


def create_volume(root: Path, volume: str) -> None:
    result = capture(root, "volume-create", ["docker", "volume", "create", volume])
    require(result["returncode"] == 0 and result["stdout"].strip() == volume, "volume creation failed")
    inspect = capture(root, "volume-inspect", ["docker", "volume", "inspect", volume])
    require(inspect["returncode"] == 0, "volume inspection failed")


def wait_ready(root: Path, name: str, label: str) -> dict[str, object]:
    started = now()
    line = None
    attempts = 0
    while attempts < 400:
        attempts += 1
        logs = subprocess.run(["docker", "logs", name], capture_output=True, text=True, check=False)
        line = next((item for item in logs.stdout.splitlines() if '"backend":"layerfs-fuse"' in item), None)
        if logs.returncode == 0 and line is not None:
            break
        inspect = subprocess.run(
            ["docker", "inspect", "--format", "{{.State.Running}} {{.State.ExitCode}}", name],
            capture_output=True,
            text=True,
            check=False,
        )
        if inspect.returncode == 0 and inspect.stdout.startswith("false"):
            raise OracleFailure(f"{name} exited before readiness")
        time.sleep(0.025)
    else:
        raise OracleFailure(f"{name} did not become ready")
    ended = now()
    receipt = {
        "container": name,
        "attempts": attempts,
        "started": started,
        "ended": ended,
        "elapsed_ns": ended["monotonic_ns"] - started["monotonic_ns"],
        "matched_stdout_line": line,
    }
    write_json(root / f"{label}-readiness.json", receipt)
    event(root, f"ready:{label}", {"container": name, "attempts": attempts})
    return receipt


def running_evidence(root: Path, name: str, label: str) -> None:
    for suffix, argv in (
        ("inspect", ["docker", "inspect", name]),
        ("mountinfo", ["docker", "exec", name, "cat", "/proc/1/mountinfo"]),
        ("proc-status", ["docker", "exec", name, "cat", "/proc/1/status"]),
        ("top", ["docker", "top", name, "-eo", "pid,ppid,lstart,stat,comm,args"]),
        (
            "fuse-holders",
            [
                "docker", "exec", name, "sh", "-c",
                'for p in /proc/[0-9]*; do c=$(readlink "$p/cwd" 2>/dev/null || :); case "$c" in /workspace*) printf "%s %s\\n" "$p" "$c";; esac; done',
            ],
        ),
    ):
        result = capture(root, f"{label}-{suffix}", argv)
        require(result["returncode"] == 0, f"failed to capture {label} {suffix}")
    mountinfo = (root / f"{label}-mountinfo.stdout").read_text()
    require(" /workspace " in mountinfo and " - fuse layerfs " in mountinfo, "real LayerFS FUSE mount absent")


def copy_terminal(root: Path, name: str, label: str) -> dict[str, object]:
    result = capture(
        root,
        f"{label}-terminal-copy",
        ["docker", "cp", f"{name}:/var/tmp/layerfs-owned/terminal.json", str(root / f"{label}-terminal.json")],
    )
    require(result["returncode"] == 0, f"missing {label} terminal receipt")
    return json.loads((root / f"{label}-terminal.json").read_text())


def wait_stopped(root: Path, name: str, label: str, *, timeout: float = 40) -> tuple[int, dict[str, object]]:
    result = capture(root, f"{label}-wait", ["docker", "wait", name], timeout=timeout)
    if result["timed_out"]:
        capture(root, f"{label}-timeout-kill", ["docker", "kill", "--signal", "KILL", name])
        raise OracleFailure(f"{name} did not exit within {timeout}s")
    require(result["returncode"] == 0, f"docker wait failed for {name}")
    exit_code = int(result["stdout"].strip())
    logs = capture(root, f"{label}-logs", ["docker", "logs", name])
    require(logs["returncode"] == 0, f"docker logs failed for {name}")
    stopped = capture(root, f"{label}-stopped-inspect", ["docker", "inspect", name])
    require(stopped["returncode"] == 0, f"stopped inspect failed for {name}")
    after_top = capture(root, f"{label}-post-stop-top", ["docker", "top", name, "-eo", "pid,ppid,stat,comm,args"])
    require(after_top["returncode"] != 0 or not after_top["stdout"].strip(), "owned process survived stop")
    return exit_code, json.loads(stopped["stdout"])[0]


def terminal_resources_clean(receipt: dict[str, object]) -> bool:
    mounted = receipt["mounted"]
    engine = receipt["engine"]
    return (
        mounted["operation_q_terminal_bytes"] == 0
        and mounted["spool_live_bytes"] == 0
        and mounted["spool_dead_bytes"] == 0
        and mounted["spool_physical_bytes"] == 0
        and engine["connections_terminal"] == 0
    )


def root_only(receipt: dict[str, object]) -> bool:
    mounted = receipt["mounted"]
    return mounted["lookup_refs"] == mounted["live_nodes"] == mounted["inode_mappings"] == 1


def verify_binding(root: Path, inspect_label: str, receipt: dict[str, object]) -> dict[str, bool]:
    inspect = json.loads((root / f"{inspect_label}-inspect.stdout").read_text())[0]
    image = json.loads((root / "image-inspect.stdout").read_text())[0]
    labels = image["Config"]["Labels"]
    return {
        "image_id_exact": image["Id"] == IMAGE and inspect["Image"] == IMAGE,
        "source_identity_exact": labels["org.opencontainers.image.layerfs.source-commit"] == SOURCE_COMMIT
        and labels["org.opencontainers.image.layerfs.source-tree"] == SOURCE_TREE
        and receipt["source_commit"] == SOURCE_COMMIT
        and receipt["source_tree"] == SOURCE_TREE,
        "native_arm64": image["Architecture"] == "arm64" and platform.machine() == "arm64",
        "exact_envelope": inspect["HostConfig"]["NanoCpus"] == 1_000_000_000
        and inspect["HostConfig"]["Memory"] == 512 * 1024 * 1024
        and inspect["HostConfig"]["PidsLimit"] == 512
        and inspect["HostConfig"]["NetworkMode"] == "none"
        and inspect["HostConfig"].get("Tmpfs") in ({}, None),
        "verified_integrity": receipt["integrity"] == "Verified",
        "upstream_fs_bench_identity": receipt["fs_bench_sha256"] == FS_BENCH_SHA256,
    }


def cleanup(root: Path, names: list[str], volume: str) -> dict[str, object]:
    results: dict[str, object] = {"containers": {}}
    for name in names:
        remove = capture(root, f"cleanup-rm-{name}", ["docker", "rm", "-f", name])
        inspect = capture(root, f"cleanup-inspect-{name}", ["docker", "container", "inspect", name])
        results["containers"][name] = {
            "remove_returncode": remove["returncode"],
            "absent": inspect["returncode"] != 0,
        }
    remove_volume = capture(root, "cleanup-volume-rm", ["docker", "volume", "rm", volume])
    inspect_volume = capture(root, "cleanup-volume-inspect", ["docker", "volume", "inspect", volume])
    results["volume"] = {
        "name": volume,
        "remove_returncode": remove_volume["returncode"],
        "absent": inspect_volume["returncode"] != 0,
    }
    results["all_absent"] = all(item["absent"] for item in results["containers"].values()) and results["volume"]["absent"]
    write_json(root / "cleanup.json", results)
    event(root, "cleanup-complete", {"all_absent": results["all_absent"]})
    return results


def execute(name: str, body) -> bool:
    root = FOCUSED / name
    root.mkdir(parents=False, exist_ok=False)
    error = None
    checks: dict[str, bool] = {}
    status = "FAIL"
    try:
        checks = body(root)
        status = "PASS" if all(checks.values()) else "FAIL"
        if status != "PASS":
            first = next(key for key, value in checks.items() if not value)
            raise OracleFailure(f"first failing equation: {first}")
    except Exception as caught:  # fail closed and preserve the entire attempt
        error = f"{type(caught).__name__}: {caught}"
        event(root, "oracle-failure", error)
    finally:
        if not (root / "oracle.json").exists():
            write_json(
                root / "oracle.json",
                {
                    "schema": "layerfs-stage2-lifecycle-custody-oracle-v1",
                    "status": status if error is None else "FAIL",
                    "checks": checks,
                    "error": error,
                },
            )
    return error is None and status == "PASS"


def immediate_term(root: Path) -> dict[str, bool]:
    name = "layerfs-c015-lifecycle-immediate"
    volume = "layerfs_c015_lifecycle_immediate_store"
    names = [name]
    write_json(root / "execution-plan.json", base_plan("IMMEDIATE_TERM_AFTER_READINESS", names, volume))
    event(root, "scenario-start")
    cleanup_result = None
    checks: dict[str, bool] = {}
    try:
        preflight(root, names, volume)
        create_volume(root, volume)
        launch = capture(root, "daemon-launch", common_run(name, volume))
        require(launch["returncode"] == 0, "daemon launch failed")
        running_evidence(root, name, "daemon-running")
        ready = wait_ready(root, name, "daemon")
        signal_started = now()
        signal = capture(root, "term-delivery-1", ["docker", "kill", "--signal", "TERM", name])
        require(signal["returncode"] == 0, "TERM delivery failed")
        write_json(
            root / "readiness-to-term.json",
            {
                "readiness_observed": ready["ended"],
                "term_command_started": signal_started,
                "elapsed_ns": signal_started["monotonic_ns"] - ready["ended"]["monotonic_ns"],
            },
        )
        exit_code, stopped = wait_stopped(root, name, "daemon")
        receipt = copy_terminal(root, name, "daemon")
        checks = verify_binding(root, "daemon-running", receipt)
        checks.update(
            {
                "term_after_readiness": signal_started["monotonic_ns"] >= ready["ended"]["monotonic_ns"],
                "bounded_zero_exit": exit_code == 0 and stopped["State"]["ExitCode"] == 0 and not stopped["State"]["OOMKilled"],
                "single_signal_terminal": receipt["signal"] == 15,
                "terminal_pass": receipt["status"] == "PASS" and receipt["error"] is None,
                "session_joined_and_cache_released": receipt["session_terminated"] and receipt["kernel_cache_released"] and receipt["terminal_snapshot_complete"],
                "exactly_one_init_destroy": receipt["callbacks"]["init"] == 1 and receipt["callbacks"]["destroy"] == 1,
                "no_filesystem_mutation": receipt["callbacks"]["create"] == receipt["callbacks"]["write"] == receipt["callbacks"]["fsync"] == receipt["callbacks"]["fsyncdir"] == 0,
                "single_noop_shutdown_checkpoint": receipt["mounted"]["checkpoints"] == 0 and receipt["mounted"]["no_op_checkpoints"] == 1,
                "terminal_resources_zero": terminal_resources_clean(receipt),
                "terminal_root_only": root_only(receipt) and receipt["mounted"]["logical_workspace_bytes"] == 0,
            }
        )
    finally:
        cleanup_result = cleanup(root, names, volume)
    checks["owned_process_container_volume_cleanup"] = bool(cleanup_result and cleanup_result["all_absent"])
    write_json(root / "result.json", {"status": "PASS" if all(checks.values()) else "FAIL", "checks": checks})
    event(root, "scenario-complete", {"status": "PASS" if all(checks.values()) else "FAIL"})
    return checks


WRITE_100_MIB = r'''
import hashlib, json, os
path = "/workspace/dirty.bin"
digest = hashlib.sha256()
block = bytes(range(256)) * 4096
with open(path, "wb", buffering=0) as target:
    for _ in range(100):
        written = target.write(block)
        if written != len(block):
            raise RuntimeError(f"short write: {written}")
        digest.update(block)
with open("/var/tmp/layerfs-owned/expected.json", "x") as output:
    json.dump({"path": path, "size": 100 * 1024 * 1024, "sha256": digest.hexdigest(), "explicit_fsync": False}, output, sort_keys=True)
    output.write("\n")
'''.strip()


DELETE_AND_FSYNCDIR = r'''
import os
os.unlink("/workspace/dirty.bin")
directory = os.open("/workspace", os.O_RDONLY | os.O_DIRECTORY)
try:
    os.fsync(directory)
finally:
    os.close(directory)
'''.strip()


def repeated_term(root: Path) -> dict[str, bool]:
    dirty_name = "layerfs-c015-lifecycle-repeat-dirty"
    reopen_name = "layerfs-c015-lifecycle-repeat-reopen"
    volume = "layerfs_c015_lifecycle_repeat_store"
    names = [dirty_name, reopen_name]
    plan = base_plan("REPEATED_TERM_DURING_DIRTY_100_MIB_CHECKPOINT", names, volume)
    plan["workload_argv"] = ["docker", "exec", dirty_name, "python3", "-c", WRITE_100_MIB]
    plan["signal_argv"] = ["docker", "kill", "--signal", "TERM", dirty_name]
    plan["reopen_hash_argv"] = ["docker", "exec", reopen_name, "sha256sum", "/workspace/dirty.bin"]
    write_json(root / "execution-plan.json", plan)
    event(root, "scenario-start")
    cleanup_result = None
    checks: dict[str, bool] = {}
    try:
        preflight(root, names, volume)
        create_volume(root, volume)
        launch = capture(root, "dirty-launch", common_run(dirty_name, volume))
        require(launch["returncode"] == 0, "dirty daemon launch failed")
        ready = wait_ready(root, dirty_name, "dirty")
        running_evidence(root, dirty_name, "dirty-running")
        write = capture(root, "dirty-write", ["docker", "exec", dirty_name, "python3", "-c", WRITE_100_MIB], timeout=60)
        require(write["returncode"] == 0, "100 MiB dirty write failed")
        expected_copy = capture(
            root,
            "expected-copy",
            ["docker", "cp", f"{dirty_name}:/var/tmp/layerfs-owned/expected.json", str(root / "expected.json")],
        )
        require(expected_copy["returncode"] == 0, "expected digest copy failed")
        expected = json.loads((root / "expected.json").read_text())
        spool = capture(root, "dirty-spool-stat", ["docker", "exec", dirty_name, "stat", "-c", "%n %s %f %i", "/var/tmp/layerfs-owned/spool"])
        require(spool["returncode"] == 0, "dirty spool is absent")
        event(root, "dirty-write-released-without-fsync", expected)
        first = capture(root, "term-delivery-1", ["docker", "kill", "--signal", "TERM", dirty_name])
        require(first["returncode"] == 0, "first TERM delivery failed")
        time.sleep(0.005)
        running_between = capture(root, "running-between-signals", ["docker", "inspect", "--format", "{{.State.Running}}", dirty_name])
        require(running_between["returncode"] == 0 and running_between["stdout"].strip() == "true", "daemon exited before repeated TERM")
        second = capture(root, "term-delivery-2", ["docker", "kill", "--signal", "TERM", dirty_name])
        require(second["returncode"] == 0, "second TERM delivery failed")
        dirty_exit, dirty_stopped = wait_stopped(root, dirty_name, "dirty", timeout=45)
        dirty_receipt = copy_terminal(root, dirty_name, "dirty")

        reopen_launch = capture(root, "reopen-launch", common_run(reopen_name, volume))
        require(reopen_launch["returncode"] == 0, "reopen daemon launch failed")
        wait_ready(root, reopen_name, "reopen")
        running_evidence(root, reopen_name, "reopen-running")
        reopened_hash = capture(root, "reopen-hash", ["docker", "exec", reopen_name, "sha256sum", "/workspace/dirty.bin"])
        require(reopened_hash["returncode"] == 0, "Verified reopen hash failed")
        reopened_stat = capture(root, "reopen-stat", ["docker", "exec", reopen_name, "stat", "-c", "%s", "/workspace/dirty.bin"])
        require(reopened_stat["returncode"] == 0, "Verified reopen stat failed")
        cleanup_command = capture(root, "accepted-cleanup", ["docker", "exec", reopen_name, "python3", "-c", DELETE_AND_FSYNCDIR])
        require(cleanup_command["returncode"] == 0, "accepted file cleanup failed")
        reopen_signal = capture(root, "reopen-term-delivery", ["docker", "kill", "--signal", "TERM", reopen_name])
        require(reopen_signal["returncode"] == 0, "reopen TERM delivery failed")
        reopen_exit, reopen_stopped = wait_stopped(root, reopen_name, "reopen")
        reopen_receipt = copy_terminal(root, reopen_name, "reopen")

        checks = verify_binding(root, "dirty-running", dirty_receipt)
        checks.update(
            {
                "readiness_preceded_signals": first["started"]["monotonic_ns"] >= ready["ended"]["monotonic_ns"],
                "two_term_deliveries_while_running": first["returncode"] == second["returncode"] == 0 and running_between["stdout"].strip() == "true",
                "dirty_bounded_zero_exit": dirty_exit == 0 and dirty_stopped["State"]["ExitCode"] == 0 and not dirty_stopped["State"]["OOMKilled"],
                "dirty_terminal_pass": dirty_receipt["status"] == "PASS" and dirty_receipt["signal"] == 15 and dirty_receipt["error"] is None,
                "single_dirty_checkpoint": dirty_receipt["mounted"]["checkpoints"] == 1 and dirty_receipt["mounted"]["no_op_checkpoints"] == 0,
                "single_dirty_shutdown_publication": dirty_receipt["engine"]["transactions_started"] == 2
                and dirty_receipt["engine"]["transactions_committed"] == 2
                and dirty_receipt["engine"]["transactions_rolled_back"] == 0
                and dirty_receipt["engine"]["publication_commits"] == 2,
                "dirty_write_was_unfsynced": not expected["explicit_fsync"] and dirty_receipt["callbacks"]["write"] > 0 and dirty_receipt["callbacks"]["fsync"] == dirty_receipt["callbacks"]["fsyncdir"] == 0,
                "dirty_terminal_resources_zero": terminal_resources_clean(dirty_receipt) and root_only(dirty_receipt),
                "dirty_logical_bytes_exact": dirty_receipt["mounted"]["logical_workspace_bytes"] == expected["size"],
                "verified_independent_reopen_exact": reopened_hash["stdout"].split()[0] == expected["sha256"] and int(reopened_stat["stdout"].strip()) == expected["size"],
                "reopen_source_binding_exact": reopen_receipt["source_commit"] == SOURCE_COMMIT and reopen_receipt["source_tree"] == SOURCE_TREE and reopen_receipt["integrity"] == "Verified",
                "cleanup_bounded_zero_exit": reopen_exit == 0 and reopen_stopped["State"]["ExitCode"] == 0 and not reopen_stopped["State"]["OOMKilled"],
                "cleanup_publication_exact": reopen_receipt["mounted"]["checkpoints"] == 1
                and reopen_receipt["engine"]["transactions_started"] == 1
                and reopen_receipt["engine"]["transactions_committed"] == 1
                and reopen_receipt["engine"]["transactions_rolled_back"] == 0
                and reopen_receipt["engine"]["publication_commits"] == 1,
                "generation_and_root_advanced": dirty_receipt["generation"] == 1 and reopen_receipt["generation"] == 2 and dirty_receipt["root"] != reopen_receipt["root"],
                "cleanup_terminal_pass_zero": reopen_receipt["status"] == "PASS"
                and reopen_receipt["session_terminated"]
                and reopen_receipt["kernel_cache_released"]
                and terminal_resources_clean(reopen_receipt)
                and root_only(reopen_receipt)
                and reopen_receipt["mounted"]["logical_workspace_bytes"] == 0,
            }
        )
    finally:
        cleanup_result = cleanup(root, names, volume)
    checks["owned_process_container_volume_cleanup"] = bool(cleanup_result and cleanup_result["all_absent"])
    write_json(root / "result.json", {"status": "PASS" if all(checks.values()) else "FAIL", "checks": checks})
    event(root, "scenario-complete", {"status": "PASS" if all(checks.values()) else "FAIL"})
    return checks


def unmount_busy(root: Path) -> dict[str, bool]:
    name = "layerfs-c015-lifecycle-busy"
    volume = "layerfs_c015_lifecycle_busy_store"
    names = [name]
    holder = 'cd /workspace && printf "%s\\n" "$$" > /var/tmp/layerfs-owned/holder.pid && exec sleep 120'
    plan = base_plan("TERM_EXTERNAL_UNMOUNT_EBUSY_RACE", names, volume)
    plan["holder_argv"] = ["docker", "exec", "-d", name, "sh", "-c", holder]
    plan["race_argv"] = {
        "term": ["docker", "kill", "--signal", "TERM", name],
        "external_unmount": ["docker", "exec", name, "/usr/bin/umount", "/workspace"],
    }
    write_json(root / "execution-plan.json", plan)
    event(root, "scenario-start")
    cleanup_result = None
    checks: dict[str, bool] = {}
    try:
        preflight(root, names, volume)
        create_volume(root, volume)
        launch = capture(root, "daemon-launch", common_run(name, volume))
        require(launch["returncode"] == 0, "daemon launch failed")
        wait_ready(root, name, "daemon")
        running_evidence(root, name, "daemon-running")
        holder_start = capture(root, "holder-start", ["docker", "exec", "-d", name, "sh", "-c", holder])
        require(holder_start["returncode"] == 0, "FUSE cwd holder failed to start")
        holder_pid = None
        for _ in range(100):
            pid_result = subprocess.run(
                ["docker", "exec", name, "cat", "/var/tmp/layerfs-owned/holder.pid"],
                capture_output=True,
                text=True,
                check=False,
            )
            if pid_result.returncode == 0 and pid_result.stdout.strip().isdigit():
                holder_pid = pid_result.stdout.strip()
                break
            time.sleep(0.01)
        require(holder_pid is not None, "FUSE cwd holder PID was not published")
        holder_pid_capture = capture(root, "holder-pid", ["docker", "exec", name, "cat", "/var/tmp/layerfs-owned/holder.pid"])
        holder_cwd = capture(root, "holder-cwd", ["docker", "exec", name, "readlink", f"/proc/{holder_pid}/cwd"])
        holder_status = capture(root, "holder-status", ["docker", "exec", name, "cat", f"/proc/{holder_pid}/status"])
        holder_top = capture(root, "holder-top", ["docker", "top", name, "-eo", "pid,ppid,lstart,stat,comm,args"])
        require(holder_pid_capture["returncode"] == holder_cwd["returncode"] == holder_status["returncode"] == holder_top["returncode"] == 0, "holder inspection failed")
        require(holder_cwd["stdout"].strip() == "/workspace", "holder does not own a cwd in the FUSE mount")
        event(root, "race-release", {"holder_pid": holder_pid})
        with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:
            external_future = pool.submit(capture, root, "race-external-unmount", ["docker", "exec", name, "/usr/bin/umount", "/workspace"])
            term_future = pool.submit(capture, root, "race-term-delivery", ["docker", "kill", "--signal", "TERM", name])
            external = external_future.result()
            term = term_future.result()
        require(term["returncode"] == 0, "TERM race delivery failed")
        exit_code, stopped = wait_stopped(root, name, "daemon", timeout=35)
        receipt = copy_terminal(root, name, "daemon")
        wait_record = json.loads((root / "daemon-wait.command.json").read_text())
        checks = verify_binding(root, "daemon-running", receipt)
        race_start_delta_ns = abs(
            external["started"]["monotonic_ns"] - term["started"]["monotonic_ns"]
        )
        checks.update(
            {
                "holder_owned_fuse_cwd": holder_cwd["stdout"].strip() == "/workspace",
                "race_commands_started_within_5ms": race_start_delta_ns <= 5_000_000,
                "external_unmount_attempt_did_not_succeed": external["returncode"] != 0,
                "term_delivered_in_race": term["returncode"] == 0,
                "bounded_nonzero_exit": not wait_record["timed_out"]
                and wait_record["elapsed_ns"] < 35_000_000_000
                and exit_code != 0
                and stopped["State"]["ExitCode"] != 0
                and not stopped["State"]["OOMKilled"],
                "fail_closed_busy_terminal": receipt["status"] == "FAIL"
                and receipt["signal"] == 15
                and receipt["error"] is not None
                and "Device or resource busy" in receipt["error"],
                "uncertain_session_not_released": not receipt["session_terminated"] and not receipt["kernel_cache_released"] and not receipt["terminal_snapshot_complete"],
                "single_noop_checkpoint_before_busy": receipt["mounted"]["checkpoints"] == 0 and receipt["mounted"]["no_op_checkpoints"] == 1,
                "snapshot_q_spool_connections_zero": terminal_resources_clean(receipt),
            }
        )
    finally:
        cleanup_result = cleanup(root, names, volume)
    checks["owned_process_container_volume_cleanup"] = bool(cleanup_result and cleanup_result["all_absent"])
    write_json(root / "result.json", {"status": "PASS" if all(checks.values()) else "FAIL", "checks": checks})
    event(root, "scenario-complete", {"status": "PASS" if all(checks.values()) else "FAIL"})
    return checks


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--scenario",
        choices=("all", "immediate", "repeated", "busy"),
        default="all",
    )
    arguments = parser.parse_args()
    selected = {
        "immediate": [("current-immediate-term-custody", immediate_term)],
        "repeated": [("current-repeated-term-custody", repeated_term)],
        "busy": [("current-unmount-busy-custody-attempt-002", unmount_busy)],
        "all": [
            ("current-immediate-term-custody", immediate_term),
            ("current-repeated-term-custody", repeated_term),
            ("current-unmount-busy-custody-attempt-002", unmount_busy),
        ],
    }[arguments.scenario]
    results = {name: execute(name, body) for name, body in selected}
    print(json.dumps(results, indent=2, sort_keys=True))
    return 0 if all(results.values()) else 1


if __name__ == "__main__":
    raise SystemExit(main())
