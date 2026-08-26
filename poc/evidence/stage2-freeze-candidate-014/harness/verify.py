#!/usr/bin/env python3
"""Independent verifier for candidate-014 per-scenario CPU raw artifacts."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import sys


EXPECTED = {
    "create 1000 files": (
        "for i in $(seq 1 1000); do echo $i > f$i; done",
        "",
    ),
    "stat 1000 files": (
        "for i in $(seq 1 1000); do echo $i > f$i; done; "
        "for i in $(seq 1 1000); do stat f$i; done",
        "",
    ),
    "rm 1000 files": (
        "for i in $(seq 1 1000); do echo $i > f$i; done; rm f*",
        "",
    ),
    "mkdir tree (10x10x10)": (
        "for a in $(seq 1 10); do for b in $(seq 1 10); do mkdir -p $a/$b; "
        "for c in $(seq 1 10); do touch $a/$b/$c; done; done; done",
        "",
    ),
    "find tree": (
        "for a in $(seq 1 10); do for b in $(seq 1 10); do mkdir -p $a/$b; "
        "for c in $(seq 1 10); do touch $a/$b/$c; done; done; done; "
        "find . -type f | wc -l",
        "",
    ),
    "write 64 MiB": ("dd if=/dev/zero of=big bs=1M count=64 status=none", ""),
    "copy 64 MiB": (
        "dd if=/dev/zero of=big bs=1M count=64 status=none; cp big big2",
        "",
    ),
    "read 64 MiB": (
        "dd if=/dev/zero of=big bs=1M count=64 status=none; cat big > /dev/null",
        "",
    ),
    "pure read 64 MiB": (
        "cat big > /dev/null",
        "dd if=/dev/zero of=big bs=1M count=64 status=none",
    ),
    "pure copy 64 MiB": (
        "cp big big2",
        "dd if=/dev/zero of=big bs=1M count=64 status=none",
    ),
    "overwrite 64 MiB": (
        "dd if=/dev/zero of=big bs=1M count=64 status=none conv=notrunc",
        "dd if=/dev/zero of=big bs=1M count=64 status=none",
    ),
    "git init + commit 100 files": (
        "git init -q; for i in $(seq 1 100); do echo $i > f$i; done; "
        "git add -A; git -c user.email=a@b -c user.name=a commit -qm init",
        "",
    ),
}
CONFIG = {
    "reps": 1,
    "warmup": 0,
    "randomizeTargets": 0,
    "mount": "/workspace",
    "base": "/var/tmp",
    "external_exact_scenario": True,
}


def cpu_fact(before: dict[str, int], after: dict[str, int], wall_ns: int) -> dict[str, object]:
    stable = set(before) == set(after) and bool(before)
    monotonic = stable and all(after[task] >= before[task] for task in before)
    cpu_ns = sum(after.values()) - sum(before.values()) if monotonic else -1
    limit_ns = wall_ns * 105 // 100 + 5_000_000
    return {
        "task_set_stable": stable,
        "schedstat_monotonic": monotonic,
        "daemon_cpu_ns": cpu_ns,
        "layerfs_row_wall_ns": wall_ns,
        "recomputed_limit_ns": limit_ns,
        "cpu_within_gate": monotonic and cpu_ns <= limit_ns,
    }


def verify(directory: Path) -> dict[str, object]:
    artifacts = sorted(path for path in directory.glob("*.json") if path.name != "collection.json")
    rows = []
    seen = set()
    for path in artifacts:
        raw = json.loads(path.read_text())
        scenario = raw.get("scenario")
        expected_command = EXPECTED.get(scenario)
        result_rows = raw.get("results", [])
        by_target = {row.get("target"): row for row in result_rows}
        measurement = raw.get("layerfs_cpu_measurement") or {}
        layerfs = by_target.get("computerd", {})
        before = measurement.get("schedstat_runtime_ns_before") or {}
        after = measurement.get("schedstat_runtime_ns_after") or {}
        wall_ns = layerfs.get("medianNs", -1)
        fact = cpu_fact(before, after, wall_ns) if wall_ns > 0 else cpu_fact({}, {}, 0)
        checks = {
            "schema_exact": raw.get("schema") == "layerfs-stage2-014-scenario-cpu-raw-v1",
            "scenario_expected": expected_command is not None,
            "scenario_unique": scenario not in seen,
            "command_exact": expected_command is not None and raw.get("command") == expected_command[0],
            "prep_exact": expected_command is not None and raw.get("prep") == expected_command[1],
            "config_exact": raw.get("config") == CONFIG,
            "two_rows": len(result_rows) == len(by_target) == 2,
            "targets_exact": set(by_target) == {"computerd", "base"},
            "singleton_scenario": {row.get("scenario") for row in result_rows} == {scenario},
            "returncodes_zero": raw.get("prep_returncodes") == {"computerd": 0, "base": 0}
            and raw.get("command_returncodes") == {"computerd": 0, "base": 0},
            "n1_statistics_exact": all(
                row.get("samples") == 1
                and len(
                    {
                        row.get("meanNs"),
                        row.get("medianNs"),
                        row.get("p95Ns"),
                        row.get("minNs"),
                        row.get("maxNs"),
                    }
                )
                == 1
                and row.get("medianNs", 0) > 0
                for row in result_rows
            ),
            "measurement_wall_matches_layerfs_row": measurement.get("command_wall_ns") == wall_ns,
            "task_set_stable": fact["task_set_stable"],
            "schedstat_monotonic": fact["schedstat_monotonic"],
            "cpu_within_gate": fact["cpu_within_gate"],
        }
        seen.add(scenario)
        rows.append(
            {
                "artifact": path.name,
                "scenario": scenario,
                "status": "PASS" if all(checks.values()) else "FAIL",
                "checks": checks,
                "recomputed": fact,
            }
        )
    collection_checks = {
        "artifact_count_12": len(artifacts) == 12,
        "scenario_set_exact": seen == set(EXPECTED),
        "all_rows_pass": len(rows) == 12 and all(row["status"] == "PASS" for row in rows),
    }
    return {
        "schema": "layerfs-stage2-014-scenario-cpu-verification-v1",
        "status": "PASS" if all(collection_checks.values()) else "FAIL",
        "checks": collection_checks,
        "gate": "summed daemon task schedstat runtime <= floor(1.05 * LayerFS command wall) + 5 ms",
        "trust_boundary": "collector limits and pass/fail fields are ignored",
        "rows": rows,
    }


def self_check() -> None:
    assert len(EXPECTED) == 12
    passing = cpu_fact({"1": 100, "2": 200}, {"1": 1_000_100, "2": 2_000_200}, 3_000_000)
    assert passing["daemon_cpu_ns"] == 3_000_000
    assert passing["cpu_within_gate"]
    failing = cpu_fact({"1": 0}, {"1": 20_000_000}, 1_000_000)
    assert not failing["cpu_within_gate"]
    unstable = cpu_fact({"1": 0}, {"2": 1}, 1_000_000)
    assert not unstable["task_set_stable"] and not unstable["cpu_within_gate"]


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("directory", nargs="?", type=Path)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--self-check", action="store_true")
    arguments = parser.parse_args()
    if arguments.self_check:
        self_check()
        return
    if arguments.directory is None:
        parser.error("directory is required unless --self-check is used")
    receipt = verify(arguments.directory)
    encoded = json.dumps(receipt, indent=2, sort_keys=True) + "\n"
    if arguments.output:
        with arguments.output.open("x") as output:
            output.write(encoded)
    sys.stdout.write(encoded)
    if receipt["status"] != "PASS":
        raise SystemExit(1)


if __name__ == "__main__":
    main()
