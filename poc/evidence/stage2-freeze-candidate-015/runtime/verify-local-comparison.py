#!/usr/bin/env python3
"""Verify the matched local live and restart-durability comparison."""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import re
import sys
from pathlib import Path


LAYERFS_SOURCE = "7e82abcd7320f6a214be336d82488ba0527b6025"
LAYERFS_IMAGE = "sha256:f8647b84580c75d4688a18665e4c60cd6dcf5b2d3092cf22bce34dfbd86b59b0"
CLOUDFLARE_SOURCE = "de87919a4fd37242e960e13b7b3ba802d1eef0a0"
CLOUDFLARE_TREE = "4fb409d7e1356e1098439293d77d2fdc2dbf2190"
CLOUDFLARE_IMAGE = "sha256:8c5100fabfd873de4ee7aabf908027e946b3fdac5328e15f9dabbf9731200bb0"
FS_BENCH = "0e2004339a9269b88c2de451d56575957477bde16b96a10cf1837519e37230ef"
PAYLOAD_BYTES = 64 * 1024 * 1024
LOCAL_ENVELOPE = {
    "cpus": 1,
    "memoryBytes": 512 * 1024 * 1024,
    "memorySwapBytes": 512 * 1024 * 1024,
    "network": "none",
    "platform": "linux/arm64",
    "workspace": "native FUSE",
}
CLOUDFLARE_WRAPPER_COMMIT = "510b4850385c90311a7a12fcd6a5469812ef5fa0"
CLOUDFLARE_WRAPPER_TREE = "21ab7d1e269b3543d11a10068c15e74015929ee8"
CLOUDFLARE_UPSTREAM = "/Users/yifanxu/Ephemeral-AI-Lab/cloudflare-computer-bench/upstream"
CLOUDFLARE_CONTAINER = "layerfs-stage2-015-cloudflare-local-same-container"
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
ANSI = re.compile(r"\x1b\[[0-9;]*m")


def kv(path: Path) -> dict[str, int]:
    return {parts[0]: int(parts[1]) for line in path.read_text().splitlines() if len(parts := line.split()) == 2}


def matrix(path: Path) -> tuple[dict[str, dict[str, int]], dict[str, object]]:
    raw = json.loads(path.read_text())
    rows = raw.get("results", [])
    values = {
        target: {row["scenario"]: row for row in rows if row.get("target") == target}
        for target in ("computerd", "base")
    }
    return values, raw


def aggregate(values: dict[str, dict[str, int]]) -> dict[str, float | int]:
    fuse = [values["computerd"][name]["medianNs"] for name in SCENARIOS]
    base = [values["base"][name]["medianNs"] for name in SCENARIOS]
    ratios = [left / right for left, right in zip(fuse, base)]
    return {
        "fuse_median_sum_ns": sum(fuse),
        "control_median_sum_ns": sum(base),
        "rsum": sum(fuse) / sum(base),
        "geometric_mean_ratio": math.exp(sum(math.log(value) for value in ratios) / len(ratios)),
        "spread": sum(values["computerd"][name]["maxNs"] for name in SCENARIOS) / sum(fuse),
    }


def cloudflare_population(root: Path, control: str) -> tuple[dict[str, dict[str, int]], dict[str, object]]:
    directory = root / f"authoritative-{control}"
    values, raw = matrix(directory / "fs-bench.json")
    plan = json.loads((directory / "plan.json").read_text())
    capture = json.loads((directory / "capture.json").read_text())
    inspect = json.loads((directory / "docker-inspect.json").read_text())[0]
    cleanup = json.loads((directory / "cleanup.json").read_text())
    backend = json.loads((directory / "backend.json").read_text())
    before_cpu, after_cpu = kv(directory / "before-cpu.stat.txt"), kv(directory / "after-cpu.stat.txt")
    before_memory, after_memory = kv(directory / "before-memory.events.txt"), kv(directory / "after-memory.events.txt")
    wall_ns = int((directory / "wall-end-unix-ns.txt").read_text()) - int(
        (directory / "wall-start-unix-ns.txt").read_text()
    )
    expected = {(scenario, target) for scenario in SCENARIOS for target in ("computerd", "base")}
    keys = {(row.get("scenario"), row.get("target")) for row in raw.get("results", [])}
    host = inspect["HostConfig"]
    stdout = ANSI.sub("", (directory / "benchmark.stdout").read_text())
    throttle_ns = (after_cpu.get("throttled_usec", 0) - before_cpu.get("throttled_usec", 0)) * 1000
    checks = {
        "source": plan.get("source") == CLOUDFLARE_SOURCE and plan.get("tree") == CLOUDFLARE_TREE,
        "image": plan.get("image_id") == CLOUDFLARE_IMAGE,
        "fs_bench": plan.get("fs_bench_sha256") == FS_BENCH,
        "scope": plan.get("scope") == "LOCAL_NATIVE_FUSE_PROCESS_LOCAL_SQLITE",
        "capture": capture.get("status") == "CAPTURED" and capture.get("matrix_exact") is True,
        "matrix": len(keys) == len(raw.get("results", [])) == 24 and keys == expected,
        "config": raw.get("config")
        == {"reps": 3, "warmup": 1, "randomizeTargets": 1, "mount": "/workspace", "base": "/var/tmp" if control == "var" else "/tmp"},
        "samples": all(row.get("samples") == 3 and row.get("medianNs", 0) > 0 for row in raw.get("results", [])),
        "fail_zero": "FAIL" not in stdout,
        "network_rows_zero": all(item not in stdout for item in ("git clone", "npm init", "go mod init")),
        "one_cpu": host.get("NanoCpus") == 1_000_000_000 and (directory / "cpu.max.txt").read_text().strip() == "100000 100000",
        "memory_512m": host.get("Memory") == 512 * 1024 * 1024 and (directory / "memory.max.txt").read_text().strip() == str(512 * 1024 * 1024),
        "network_none": host.get("NetworkMode") == "none",
        "workspace_not_tmpfs": set((host.get("Tmpfs") or {}).keys()) == {"/tmp"},
        "native_arm64": (directory / "uname.txt").read_text().strip() == "aarch64" and (directory / "node-arch.txt").read_text().strip() == "arm64",
        "real_fuse": backend.get("backend", {}).get("kind") == "fuse"
        and any(" /workspace " in line and " - fuse /dev/fuse " in line for line in (directory / "mountinfo.txt").read_text().splitlines()),
        "throttle": 0 <= throttle_ns / wall_ns <= 0.05,
        "memory_peak": int((directory / "after-memory.peak.txt").read_text()) <= 512 * 1024 * 1024,
        "oom": after_memory.get("oom", 0) == before_memory.get("oom", 0)
        and after_memory.get("oom_kill", 0) == before_memory.get("oom_kill", 0),
        "cleanup": cleanup.get("status") == "PASS" and cleanup.get("container_absent") is True,
    }
    return values, {
        "status": "PASS" if all(checks.values()) else "REVISE",
        "checks": checks,
        "population_throttle_ratio": throttle_ns / wall_ns,
        "memory_peak_bytes": int((directory / "after-memory.peak.txt").read_text()),
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("candidate", type=Path)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    layerfs_verification = json.loads((args.candidate / "live-current/verification.json").read_text())
    layerfs_durable = json.loads(
        (args.candidate / "focused/current-crash-payload/receipt.json").read_text()
    )
    negative_root = args.candidate / "cloudflare-local-same-container-restart"
    durable_root = args.candidate / "cloudflare-local-authority-volume-durable"
    cloudflare_negative = json.loads((negative_root / "receipt.json").read_text())
    cloudflare_durable = json.loads((durable_root / "receipt.json").read_text())
    populations = []
    for control in ("var", "tmp"):
        cloudflare, cloudflare_checks = cloudflare_population(args.candidate / "cloudflare-local-512", control)
        layerfs, _ = matrix(args.candidate / f"live-current/authoritative-{control}/fs-bench.json")
        cloudflare_aggregate = aggregate(cloudflare)
        layerfs_aggregate = aggregate(layerfs)
        rows = []
        for scenario in SCENARIOS:
            cf_fuse = cloudflare["computerd"][scenario]["medianNs"]
            cf_base = cloudflare["base"][scenario]["medianNs"]
            lf_fuse = layerfs["computerd"][scenario]["medianNs"]
            lf_base = layerfs["base"][scenario]["medianNs"]
            rows.append(
                {
                    "scenario": scenario,
                    "cloudflare_fuse_median_ns": cf_fuse,
                    "cloudflare_control_median_ns": cf_base,
                    "cloudflare_ratio": cf_fuse / cf_base,
                    "layerfs_fuse_median_ns": lf_fuse,
                    "layerfs_control_median_ns": lf_base,
                    "layerfs_ratio": lf_fuse / lf_base,
                    "layerfs_to_cloudflare_absolute": lf_fuse / cf_fuse,
                    "normalized_overhead_ratio": (lf_fuse / lf_base) / (cf_fuse / cf_base),
                }
            )
        populations.append(
            {
                "control": "/var/tmp" if control == "var" else "/tmp",
                "status": "PASS_LOCAL_LIVE_COMPARISON"
                if cloudflare_checks["status"] == "PASS"
                else "REVISE",
                "cloudflare_validation": cloudflare_checks,
                "cloudflare": cloudflare_aggregate,
                "layerfs": layerfs_aggregate,
                "layerfs_to_cloudflare_fuse_sum": layerfs_aggregate["fuse_median_sum_ns"]
                / cloudflare_aggregate["fuse_median_sum_ns"],
                "normalized_rsum_ratio": layerfs_aggregate["rsum"] / cloudflare_aggregate["rsum"],
                "rows": rows,
            }
        )
    negative_plan = json.loads((negative_root / "plan.json").read_text())
    image_inspect = json.loads((negative_root / "image.inspect.json").read_text())[0]
    negative_inspects = {
        name: json.loads((negative_root / f"{name}.inspect.json").read_text())[0]
        for name in ("before", "stopped", "after", "final-stopped")
    }
    before_inspect = negative_inspects["before"]
    host_config = before_inspect.get("HostConfig", {})
    expected_host_envelope = {
        "Binds": None,
        "NetworkMode": "none",
        "AutoRemove": False,
        "CapAdd": ["CAP_SYS_ADMIN"],
        "Privileged": False,
        "ReadonlyRootfs": False,
        "Tmpfs": {"/tmp": "rw,nosuid,nodev,size=1g,mode=1777"},
        "NanoCpus": 1_000_000_000,
        "Memory": 512 * 1024 * 1024,
        "MemorySwap": 1024 * 1024 * 1024,
        "PidsLimit": 512,
        "Devices": [
            {"PathOnHost": "/dev/fuse", "PathInContainer": "/dev/fuse", "CgroupPermissions": "rwm"}
        ],
        "Init": True,
    }
    observed_host_envelope = {key: host_config.get(key) for key in expected_host_envelope}
    labels = image_inspect.get("Config", {}).get("Labels", {})
    inspect_labels = [value.get("Config", {}).get("Labels", {}) for value in negative_inspects.values()]
    before_backend = json.loads((negative_root / "before.backend.json").read_text())
    after_backend = json.loads((negative_root / "after.backend.json").read_text())
    before_mountinfo = (negative_root / "before.mountinfo.txt").read_text()
    after_mountinfo = (negative_root / "after.mountinfo.txt").read_text()
    before_process_raw = (negative_root / "before.process.txt").read_text()
    after_process_raw = (negative_root / "after.process.txt").read_text()
    process_pattern = re.compile(
        r"pid=1\npidns=(pid:\[\d+\])\nstart=(\d+)\n([0-9a-f]{64})  /proc/1/exe\n"
    )
    before_process_match = process_pattern.fullmatch(before_process_raw)
    after_process_match = process_pattern.fullmatch(after_process_raw)
    container_output = f"{CLOUDFLARE_CONTAINER}\n"
    stopped_backing_error = (
        "Error response from daemon: Could not find the file /workspace/payload in container "
        f"{CLOUDFLARE_CONTAINER}\n"
    )
    expected_backend = {"backend": {"kind": "fuse"}, "mountPoint": "/workspace", "port": 45678}
    negative_checks = {
        "schema": cloudflare_negative.get("schema")
        == "layerfs-stage2-015-cloudflare-same-container-restart-v1",
        "status": cloudflare_negative.get("status") == "PASS_EXPECTED_PROCESS_LOCAL_STATE_LOSS",
        "classification": cloudflare_negative.get("classification") == "DIAGNOSTIC_ONLY",
        "identity": cloudflare_negative.get("source") == CLOUDFLARE_SOURCE
        and cloudflare_negative.get("tree") == CLOUDFLARE_TREE
        and cloudflare_negative.get("image_id") == CLOUDFLARE_IMAGE,
        "process_local": cloudflare_negative.get("persistence_class") == "PROCESS_LOCAL_NON_DURABLE"
        and cloudflare_negative.get("durability_claim") is False
        and cloudflare_negative.get("cloudflare_durable_object_present") is False
        and cloudflare_negative.get("sync_peer_present") is False,
        "payload": cloudflare_negative.get("payload_bytes") == PAYLOAD_BYTES
        and re.fullmatch(r"[0-9a-f]{64}", cloudflare_negative.get("payload_sha256", "")) is not None,
        "same_container_restart": cloudflare_negative.get("same_container_id") is True
        and cloudflare_negative.get("same_container_restart") is True
        and cloudflare_negative.get("docker_rm_before_restart") is False,
        "new_process": cloudflare_negative.get("computerd_process_restart") is True
        and cloudflare_negative.get("process_identity_changed") is True,
        "expected_loss": cloudflare_negative.get("stopped_container_exit") == "137"
        and cloudflare_negative.get("reopen_exit") == 44
        and cloudflare_negative.get("reopen_output") == "ABSENT"
        and cloudflare_negative.get("survived_process_restart") is False,
        "cleanup": cloudflare_negative.get("cleanup_exit") == 0
        and cloudflare_negative.get("owned_container_absent") is True
        and cloudflare_negative.get("post_cleanup_inventory_empty") is True,
        "raw_plan": negative_plan.get("schema")
        == "layerfs-stage2-015-cloudflare-same-container-plan-v1"
        and negative_plan.get("source") == CLOUDFLARE_SOURCE
        and negative_plan.get("tree") == CLOUDFLARE_TREE
        and negative_plan.get("image_id") == CLOUDFLARE_IMAGE
        and negative_plan.get("persistence_class") == "PROCESS_LOCAL_SQLITE_MEMORY"
        and negative_plan.get("retention_contract")
        == "same stopped container and same writable layer; no docker rm before verification"
        and negative_plan.get("kill_argv")
        == ["docker", "kill", "--signal", "KILL", CLOUDFLARE_CONTAINER]
        and negative_plan.get("wait_argv") == ["docker", "wait", CLOUDFLARE_CONTAINER]
        and negative_plan.get("restart_argv") == ["docker", "start", CLOUDFLARE_CONTAINER],
        "raw_image": image_inspect.get("Id") == CLOUDFLARE_IMAGE
        and image_inspect.get("Architecture") == "arm64"
        and image_inspect.get("Os") == "linux"
        and labels.get("dev.layerfs.upstream-commit") == CLOUDFLARE_SOURCE
        and labels.get("dev.layerfs.upstream-tree") == CLOUDFLARE_TREE,
        "raw_same_container": re.fullmatch(r"[0-9a-f]{64}", before_inspect.get("Id", ""))
        is not None
        and bool(before_inspect.get("Created"))
        and len({value.get("Id") for value in negative_inspects.values()}) == 1
        and len({value.get("Created") for value in negative_inspects.values()}) == 1
        and len({value.get("Image") for value in negative_inspects.values()}) == 1
        and next(iter(negative_inspects.values())).get("Id") == cloudflare_negative.get("container_id")
        and next(iter(negative_inspects.values())).get("Image") == CLOUDFLARE_IMAGE
        and all(value.get("HostConfig") == host_config for value in negative_inspects.values()),
        "raw_envelope": observed_host_envelope == expected_host_envelope
        and all(
            value.get("dev.layerfs.upstream-commit") == CLOUDFLARE_SOURCE
            and value.get("dev.layerfs.upstream-tree") == CLOUDFLARE_TREE
            for value in inspect_labels
        ),
        "raw_lifecycle": before_inspect.get("State", {}).get("Running") is True
        and negative_inspects["stopped"].get("State", {}).get("ExitCode") == 137
        and negative_inspects["stopped"].get("State", {}).get("OOMKilled") is False
        and negative_inspects["after"].get("State", {}).get("Running") is True
        and negative_inspects["final-stopped"].get("State", {}).get("ExitCode") == 143
        and negative_inspects["final-stopped"].get("State", {}).get("OOMKilled") is False,
        "raw_fuse": before_backend == after_backend == expected_backend
        and sum(" /workspace " in line and " - fuse /dev/fuse " in line for line in before_mountinfo.splitlines())
        == 1
        and sum(" /workspace " in line and " - fuse /dev/fuse " in line for line in after_mountinfo.splitlines())
        == 1,
        "raw_process_changed": bool(
            before_process_match
            and after_process_match
            and before_process_raw != after_process_raw
            and before_process_match.group(1) == after_process_match.group(1)
            and before_process_match.group(2) != after_process_match.group(2)
            and before_process_match.group(3) == after_process_match.group(3)
        ),
        "raw_kill_wait_restart": (negative_root / "kill.stdout").read_text() == container_output
        and (negative_root / "kill.stderr").read_bytes() == b""
        and (negative_root / "wait.stdout").read_text() == "137\n"
        and (negative_root / "wait.stderr").read_bytes() == b""
        and (negative_root / "restart.stdout").read_text() == container_output
        and (negative_root / "restart.stderr").read_bytes() == b"",
        "raw_absent": (negative_root / "verify.stdout").read_text() == "ABSENT\n"
        and (negative_root / "verify.stderr").read_bytes() == b""
        and (negative_root / "stopped-backing-check.stdout").read_bytes() == b""
        and (negative_root / "stopped-backing-check.stderr").read_text() == stopped_backing_error,
        "raw_cleanup": (negative_root / "cleanup-kill.stdout").read_text() == container_output
        and (negative_root / "cleanup-kill.stderr").read_bytes() == b""
        and (negative_root / "cleanup-wait.stdout").read_text() == "143\n"
        and (negative_root / "cleanup-wait.stderr").read_bytes() == b""
        and (negative_root / "cleanup-rm.stdout").read_text() == container_output
        and (negative_root / "cleanup-rm.stderr").read_bytes() == b""
        and (negative_root / "post-cleanup-inventory.stdout").read_bytes() == b""
        and (negative_root / "post-cleanup-inventory.stderr").read_bytes() == b"",
    }
    durable_plan = json.loads((durable_root / "plan.json").read_text())
    raw_harness = json.loads((durable_root / "harness.stdout").read_text())
    raw_receipt = dict(cloudflare_durable)
    for wrapper_field in (
        "classification",
        "cloudflareDurableObjectPresent",
        "cloudflareDeploymentPresent",
        "terminalEligible",
    ):
        raw_receipt.pop(wrapper_field, None)
    raw_volume = raw_harness.get("volume", {})
    raw_volume_inspect = raw_volume.get("inspect", {})
    raw_cleanup = raw_harness.get("cleanup", {})
    payload = cloudflare_durable.get("payload", {})
    pull = cloudflare_durable.get("pull", {})
    authority = pull.get("inventory", {})
    fresh_db = cloudflare_durable.get("freshDbProcess", {})
    fresh_fuse = cloudflare_durable.get("freshFuse", {})
    before_process = cloudflare_durable.get("processIdentity", {}).get("before", {})
    after_process = cloudflare_durable.get("processIdentity", {}).get("after", {})
    authoritative_cleanup = cloudflare_durable.get("authoritativeCleanup", {})
    cleanup_checkpoint = authoritative_cleanup.get("checkpoint", {})
    cleanup = cloudflare_durable.get("cleanup", {})
    container_id = cloudflare_durable.get("containerId")
    digest = payload.get("sha256")
    volume_name = raw_volume_inspect.get("Name")
    volume_labels = raw_volume_inspect.get("Labels", {})
    volume_owner = volume_labels.get("dev.layerfs.owner")
    persistence_class = "LOCAL_AUTHORITATIVE_SQLITE_DURABLE"
    durable_checks = {
        "schema": cloudflare_durable.get("schema") == "cloudflare-local-authoritative-sqlite-durable-v1",
        "status": cloudflare_durable.get("status") == "PASS",
        "classification": cloudflare_durable.get("classification") == "DIAGNOSTIC_ONLY"
        and cloudflare_durable.get("terminalEligible") is False,
        "persistence_class": cloudflare_durable.get("persistenceClass")
        == persistence_class,
        "identity": cloudflare_durable.get("source") == CLOUDFLARE_SOURCE
        and cloudflare_durable.get("tree") == CLOUDFLARE_TREE
        and cloudflare_durable.get("image") == CLOUDFLARE_IMAGE
        and cloudflare_durable.get("imageLabels", {}).get("dev.layerfs.upstream-commit")
        == CLOUDFLARE_SOURCE
        and cloudflare_durable.get("imageLabels", {}).get("dev.layerfs.upstream-tree")
        == CLOUDFLARE_TREE,
        "envelope": cloudflare_durable.get("envelope") == LOCAL_ENVELOPE,
        "workspace_native_fuse": cloudflare_durable.get("envelope", {}).get("workspace")
        == "native FUSE",
        "same_container_restart": bool(container_id)
        and cloudflare_durable.get("stopped", {}).get("containerId") == container_id
        and cloudflare_durable.get("restart", {}).get("containerId") == container_id
        and cloudflare_durable.get("stopped", {}).get("exitCode") == 137,
        "new_process": isinstance(before_process.get("hostPid"), int)
        and before_process.get("hostPid", 0) > 0
        and isinstance(after_process.get("hostPid"), int)
        and after_process.get("hostPid", 0) > 0
        and before_process.get("hostPid") != after_process.get("hostPid")
        and bool(before_process.get("inside"))
        and bool(after_process.get("inside"))
        and before_process.get("inside") != after_process.get("inside")
        and bool(before_process.get("startedAt"))
        and bool(after_process.get("startedAt"))
        and before_process.get("startedAt") != after_process.get("startedAt"),
        "pull": pull.get("pulled", 0) > 0,
        "restore": cloudflare_durable.get("restore", {}).get("pushed", 0) > 0,
        "payload": payload.get("bytes") == PAYLOAD_BYTES
        and re.fullmatch(r"[0-9a-f]{64}", digest or "") is not None,
        "authority_exact": authority.get("entries") == ["payload.bin"]
        and authority.get("size") == PAYLOAD_BYTES
        and authority.get("sha256") == digest,
        "fresh_process_exact": fresh_db.get("entries") == ["payload.bin"]
        and fresh_db.get("size") == PAYLOAD_BYTES
        and fresh_db.get("sha256") == digest,
        "fresh_fuse_exact": fresh_fuse.get("mount") == "fuse"
        and fresh_fuse.get("inventory") == f"f payload.bin {PAYLOAD_BYTES}\n"
        and fresh_fuse.get("sha256") == digest,
        "raw_named_volume_authority": bool(volume_owner)
        and volume_name == f"{volume_owner}-store"
        and raw_volume_inspect.get("Driver") == "local"
        and raw_volume_inspect.get("Scope") == "local"
        and raw_volume_inspect.get("Options") is None
        and raw_volume_inspect.get("Mountpoint")
        == f"/var/lib/docker/volumes/{volume_name}/_data"
        and re.fullmatch(
            r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z", raw_volume_inspect.get("CreatedAt", "")
        )
        is not None
        and volume_labels
        == {
            "dev.layerfs.owner": volume_owner,
            "dev.layerfs.persistence-class": persistence_class,
        }
        and raw_volume.get("createArgv")
        == [
            "docker",
            "volume",
            "create",
            "--driver",
            "local",
            "--label",
            f"dev.layerfs.owner={volume_owner}",
            "--label",
            f"dev.layerfs.persistence-class={persistence_class}",
            volume_name,
        ]
        and raw_volume.get("createStdout") == volume_name,
        "cleanup": cleanup.get("containerAbsent") is True
        and cleanup.get("volumeAbsent") is True
        and authoritative_cleanup.get("inventory", {}).get("entries") == []
        and cleanup_checkpoint == {"busy": 0, "checkpointed": 0, "log": 0},
        "raw_volume_cleanup": raw_cleanup.get("containerAbsent") is True
        and raw_cleanup.get("volumeAbsent") is True
        and raw_cleanup.get("volumeRemoveArgv") == ["docker", "volume", "rm", volume_name]
        and raw_cleanup.get("volumeRemoveStdout") == volume_name,
        "raw_plan": durable_plan.get("schema") == "layerfs-stage2-015-cloudflare-local-authority-plan-v1"
        and durable_plan.get("classification") == "DIAGNOSTIC_ONLY"
        and durable_plan.get("wrapper_commit") == CLOUDFLARE_WRAPPER_COMMIT
        and durable_plan.get("wrapper_tree") == CLOUDFLARE_WRAPPER_TREE
        and durable_plan.get("cwd") == CLOUDFLARE_UPSTREAM
        and durable_plan.get("argv")
        == [
            "node",
            "--experimental-sqlite",
            "--no-warnings",
            f"{CLOUDFLARE_UPSTREAM}/script/local-durable-fs-bench.mjs",
        ],
        "raw_harness_hash": hashlib.sha256(
            (durable_root / "local-durable-fs-bench.mjs").read_bytes()
        ).hexdigest()
        == durable_plan.get("harness_sha256")
        and hashlib.sha256(
            (durable_root / "local-durable-fs-bench.test.mjs").read_bytes()
        ).hexdigest()
        == durable_plan.get("test_sha256"),
        "raw_execution": (durable_root / "git-status.stdout").read_bytes() == b""
        and (durable_root / "git-status.stderr").read_bytes() == b""
        and (durable_root / "exit.txt").read_text() == "0\n"
        and (durable_root / "harness.stderr").read_bytes() == b"",
        "raw_receipt_exact": raw_harness == raw_receipt,
    }
    durable_comparison = (
        "BOTH_RESTART_VISIBLE_WITH_DIFFERENT_LOCAL_AUTHORITIES"
        if layerfs_durable.get("status") == "PASS"
        and all(negative_checks.values())
        and all(durable_checks.values())
        else "REVISE"
    )
    durable = {
        "workload": "64 MiB high-entropy write and restart-visible SHA-256 verification",
        "layerfs": {
            "status": "PASS_DURABLE" if layerfs_durable.get("status") == "PASS" else "REVISE",
            "T_live_ns": layerfs_durable.get("T_live_ns"),
            "T_checkpoint_ns": layerfs_durable.get("T_checkpoint_ns"),
            "T_to_durable_ns": layerfs_durable.get("T_to_durable_ns"),
            "ack_to_kill_request_ns": layerfs_durable.get("ack_to_kill_request_ns"),
            "survived_restart": layerfs_durable.get("verification", {}).get("payload_sha256_exact") is True,
        },
        "cloudflare_process_local_negative_control": {
            "status": "PASS" if all(negative_checks.values()) else "REVISE",
            "checks": negative_checks,
        },
        "cloudflare_local_authoritative_sqlite": {
            "status": "PASS" if all(durable_checks.values()) else "REVISE",
            "checks": durable_checks,
        },
        "comparison": durable_comparison,
        "diagnostic_only": True,
        "persistence_latency_comparison_ns": None,
        "persistence_latency_comparison_reason": (
            "Undefined: LayerFS Store and Cloudflare local authoritative SQLite use different "
            "commands, clocks, sync endpoints, media, and retention contracts."
        ),
    }
    status = (
        "PASS_LOCAL_ONLY"
        if layerfs_verification.get("status") == "PASS_LIVE_MOUNT"
        and all(population["status"] == "PASS_LOCAL_LIVE_COMPARISON" for population in populations)
        and durable["comparison"] == "BOTH_RESTART_VISIBLE_WITH_DIFFERENT_LOCAL_AUTHORITIES"
        else "REVISE"
    )
    receipt = {
        "schema": "layerfs-stage2-015-local-comparison-v2",
        "status": status,
        "scope": "LOCAL_ONLY_NO_CLOUD_DEPLOYMENT_NO_DURABLE_OBJECT",
        "identities": {
            "layerfs_source": LAYERFS_SOURCE,
            "layerfs_image": LAYERFS_IMAGE,
            "cloudflare_source": CLOUDFLARE_SOURCE,
            "cloudflare_tree": CLOUDFLARE_TREE,
            "cloudflare_image": CLOUDFLARE_IMAGE,
            "fs_bench_sha256": FS_BENCH,
        },
        "durable_restart": durable,
        "live_populations": populations,
    }
    encoded = json.dumps(receipt, indent=2, sort_keys=True) + "\n"
    if args.output:
        args.output.write_text(encoded)
    sys.stdout.write(encoded)
    if status != "PASS_LOCAL_ONLY":
        raise SystemExit(1)


if __name__ == "__main__":
    main()
