#!/usr/bin/env python3
"""Independently verify candidate-015 unchanged-upstream live populations."""

from __future__ import annotations

import argparse
import json
import math
import re
import sys
from pathlib import Path


SOURCE = "7e82abcd7320f6a214be336d82488ba0527b6025"
TREE = "df13d88eb7e7d2471971b0c58ca6425bb81b0b03"
IMAGE_ID = "sha256:f8647b84580c75d4688a18665e4c60cd6dcf5b2d3092cf22bce34dfbd86b59b0"
FS_BENCH = "0e2004339a9269b88c2de451d56575957477bde16b96a10cf1837519e37230ef"
SCENARIOS = {
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
}
ANSI = re.compile(r"\x1b\[[0-9;]*m")


def key_values(path: Path) -> dict[str, int]:
    return {parts[0]: int(parts[1]) for line in path.read_text().splitlines() if len(parts := line.split()) == 2}


def population(root: Path, control: str) -> dict[str, object]:
    directory = root / f"authoritative-{control}"
    raw = json.loads((directory / "fs-bench.json").read_text())
    plan = json.loads((directory / "plan.json").read_text())
    capture = json.loads((directory / "capture.json").read_text())
    terminal = json.loads((directory / "terminal.json").read_text())
    inspect = json.loads((directory / "docker-inspect.json").read_text())[0]
    cleanup = json.loads((directory / "cleanup.json").read_text())
    before_cpu = key_values(directory / "before-cpu.stat.txt")
    after_cpu = key_values(directory / "after-cpu.stat.txt")
    before_memory = key_values(directory / "before-memory.events.txt")
    after_memory = key_values(directory / "after-memory.events.txt")
    wall_ns = int((directory / "wall-end-unix-ns.txt").read_text()) - int(
        (directory / "wall-start-unix-ns.txt").read_text()
    )
    rows = raw.get("results", [])
    keys = {(row.get("scenario"), row.get("target")) for row in rows}
    expected_keys = {(scenario, target) for scenario in SCENARIOS for target in ("computerd", "base")}
    by_target = {
        target: {row["scenario"]: row for row in rows if row.get("target") == target}
        for target in ("computerd", "base")
    }
    layerfs_medians = [by_target["computerd"][name]["medianNs"] for name in sorted(SCENARIOS)]
    base_medians = [by_target["base"][name]["medianNs"] for name in sorted(SCENARIOS)]
    ratios = [left / right for left, right in zip(layerfs_medians, base_medians)]
    sl = sum(layerfs_medians)
    base_sum = sum(base_medians)
    rsum = sl / base_sum
    geometric = math.exp(sum(math.log(value) for value in ratios) / len(ratios))
    spread = sum(by_target["computerd"][name]["maxNs"] for name in SCENARIOS) / sl
    throttle_ns = (after_cpu.get("throttled_usec", 0) - before_cpu.get("throttled_usec", 0)) * 1000
    throttle_ratio = throttle_ns / wall_ns
    mounted = terminal.get("mounted", {})
    engine = terminal.get("engine", {})
    callbacks = terminal.get("callbacks", {})
    host = inspect.get("HostConfig", {})
    limits = (
        {"sl": 4_500_000_000, "rsum": 2.85, "geometric": 7.0, "spread": 1.15}
        if control == "var"
        else {"sl": 4_500_000_000, "rsum": 3.10, "geometric": 7.75, "spread": 1.15}
    )
    stdout = ANSI.sub("", (directory / "benchmark.stdout").read_text())
    checks = {
        "capture_exact": capture.get("status") == "CAPTURED" and capture.get("matrix_exact") is True,
        "benchmark_exit_zero": (directory / "benchmark.exit").read_text().strip() == "0",
        "stderr_empty": (directory / "benchmark.stderr").read_bytes() == b"",
        "fail_markers_zero": "FAIL" not in stdout,
        "network_scenarios_zero": all(value not in stdout for value in ("git clone", "npm init", "go mod init")),
        "config_exact": raw.get("config")
        == {"reps": 3, "warmup": 1, "randomizeTargets": 1, "mount": "/workspace", "base": "/var/tmp" if control == "var" else "/tmp"},
        "matrix_exact": len(rows) == len(keys) == 24 and keys == expected_keys,
        "samples_exact": all(row.get("samples") == 3 and row.get("medianNs", 0) > 0 for row in rows),
        "source_bound": plan.get("product_source") == SOURCE and plan.get("product_tree") == TREE,
        "image_bound": plan.get("image_id") == IMAGE_ID,
        "fs_bench_bound": plan.get("fs_bench_sha256") == FS_BENCH,
        "one_cpu": host.get("NanoCpus") == 1_000_000_000 and (directory / "cpu.max.txt").read_text().strip() == "100000 100000",
        "memory_limit_512m": host.get("Memory") == 512 * 1024 * 1024,
        "network_none": host.get("NetworkMode") == "none",
        "workspace_not_tmpfs": set((host.get("Tmpfs") or {}).keys()) == {"/tmp"},
        "real_fuse": any(" /workspace " in line and " - fuse layerfs " in line for line in (directory / "mountinfo.txt").read_text().splitlines()),
        "terminal_pass": terminal.get("status") == "PASS" and terminal.get("backend") == "layerfs-fuse",
        "terminal_source": terminal.get("source_commit") == SOURCE and terminal.get("source_tree") == TREE,
        "terminal_verified": terminal.get("integrity") == "Verified",
        "terminal_root_only": mounted.get("lookup_refs") == mounted.get("live_nodes") == mounted.get("inode_mappings") == 1,
        "terminal_clean": mounted.get("logical_workspace_bytes") == mounted.get("spool_live_bytes") == mounted.get("spool_physical_bytes") == mounted.get("operation_q_terminal_bytes") == 0,
        "no_materialization_or_capture": mounted.get("materializations") == mounted.get("capture_scans") == 0,
        "connections_terminal_zero": engine.get("connections_terminal") == 0,
        "invalidations_fail_closed": callbacks.get("invalidations_failed") == callbacks.get("invalidations_unsupported") == 0,
        "mount_lock_ratio": callbacks.get("mount_lock_wait_ns", 1) / max(callbacks.get("callback_wall_ns", 0), 1) <= 0.10,
        "population_throttle_ratio": 0 <= throttle_ratio <= 0.05,
        "memory_peak": int((directory / "after-memory.peak.txt").read_text()) <= 512 * 1024 * 1024,
        "oom_zero": after_memory.get("oom", 0) == before_memory.get("oom", 0) and after_memory.get("oom_kill", 0) == before_memory.get("oom_kill", 0),
        "cleanup": cleanup.get("status") == "PASS",
        "sl": sl <= limits["sl"],
        "rsum": rsum <= limits["rsum"],
        "geometric": geometric <= limits["geometric"],
        "spread": spread <= limits["spread"],
    }
    return {
        "control": "/var/tmp" if control == "var" else "/tmp",
        "status": "PASS_LIVE_MOUNT" if all(checks.values()) else "REVISE",
        "checks": checks,
        "metrics": {
            "sl_ns": sl,
            "base_sum_ns": base_sum,
            "rsum": rsum,
            "geometric_mean_ratio": geometric,
            "spread": spread,
            "population_throttle_ratio": throttle_ratio,
            "mount_lock_ratio": callbacks.get("mount_lock_wait_ns", 1) / max(callbacks.get("callback_wall_ns", 0), 1),
            "memory_peak_bytes": int((directory / "after-memory.peak.txt").read_text()),
        },
        "limits": limits,
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("root", type=Path)
    parser.add_argument("--output", type=Path)
    arguments = parser.parse_args()
    populations = [population(arguments.root, control) for control in ("var", "tmp")]
    receipt = {
        "schema": "layerfs-stage2-015-live-verification-v1",
        "status": "PASS_LIVE_MOUNT" if all(item["status"] == "PASS_LIVE_MOUNT" for item in populations) else "REVISE",
        "disposition": "LIVE_MOUNT_DIAGNOSTIC_ONLY_NOT_PERSISTENCE_INCLUSIVE",
        "populations": populations,
    }
    encoded = json.dumps(receipt, indent=2, sort_keys=True) + "\n"
    if arguments.output:
        arguments.output.write_text(encoded)
    sys.stdout.write(encoded)
    if receipt["status"] != "PASS_LIVE_MOUNT":
        raise SystemExit(1)


if __name__ == "__main__":
    main()
