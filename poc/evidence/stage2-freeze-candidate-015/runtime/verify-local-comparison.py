#!/usr/bin/env python3
"""Verify the matched local live and restart-durability comparison."""

from __future__ import annotations

import argparse
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
    cloudflare_durable = json.loads((args.candidate / "cloudflare-local-restart/receipt.json").read_text())
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
    durable = {
        "workload": "64 MiB high-entropy write, SHA-256, explicit file/directory sync, immediate forced death, fresh-container reopen",
        "layerfs": {
            "status": "PASS_DURABLE" if layerfs_durable.get("status") == "PASS" else "REVISE",
            "T_live_ns": layerfs_durable.get("T_live_ns"),
            "T_checkpoint_ns": layerfs_durable.get("T_checkpoint_ns"),
            "T_to_durable_ns": layerfs_durable.get("T_to_durable_ns"),
            "ack_to_kill_request_ns": layerfs_durable.get("ack_to_kill_request_ns"),
            "survived_restart": layerfs_durable.get("verification", {}).get("payload_sha256_exact") is True,
        },
        "cloudflare_local": {
            "status": cloudflare_durable.get("status"),
            "local_acknowledgement_ns": cloudflare_durable.get("local_acknowledgement_ns"),
            "ack_to_kill_request_ns": cloudflare_durable.get("acknowledgement_to_kill_request_ns"),
            "survived_restart": cloudflare_durable.get("survived_restart"),
            "reopen_output": cloudflare_durable.get("reopen_output"),
        },
        "comparison": "LAYERFS_ONLY_RESTART_DURABLE"
        if layerfs_durable.get("status") == "PASS" and cloudflare_durable.get("status") == "FAIL_DURABILITY"
        else "REVISE",
        "cloudflare_local_ack_is_not_a_durable_latency": cloudflare_durable.get("status") == "FAIL_DURABILITY",
    }
    status = (
        "PASS_LOCAL_ONLY"
        if layerfs_verification.get("status") == "PASS_LIVE_MOUNT"
        and all(population["status"] == "PASS_LOCAL_LIVE_COMPARISON" for population in populations)
        and durable["comparison"] == "LAYERFS_ONLY_RESTART_DURABLE"
        else "REVISE"
    )
    receipt = {
        "schema": "layerfs-stage2-015-local-comparison-v1",
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
