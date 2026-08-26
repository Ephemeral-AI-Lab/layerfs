#!/usr/bin/env python3
import json
import os
import shutil
import subprocess
import sys
import time

REQUESTS = [
    (
        "write 64 MiB",
        "dd if=/dev/zero of=big bs=1M count=64 status=none",
        "write",
    ),
    (
        "copy 64 MiB",
        "dd if=/dev/zero of=big bs=1M count=64 status=none; cp big big2",
        "copy",
    ),
    (
        "read 64 MiB",
        "dd if=/dev/zero of=big bs=1M count=64 status=none; cat big >/dev/null",
        "read",
    ),
]
TARGETS = [("/workspace", "computerd"), ("/var/tmp", "base")]
OWNED = "/var/tmp/layerfs-owned"
pid = int(sys.argv[1])
ticks_per_second = os.sysconf("SC_CLK_TCK")


def process_ticks():
    stat = open(f"/proc/{pid}/stat").read()
    fields = stat[stat.rfind(")") + 2 :].split()
    return int(fields[11]) + int(fields[12])


pairs = []
with open(f"{OWNED}/scenario-cpu-exact-collisions.stdout", "x") as stdout:
    for request_index, (scenario, command, slug) in enumerate(REQUESTS, 1):
        rows = []
        returncodes = []
        ticks_before = process_ticks()
        pair_started = time.perf_counter_ns()
        for target_index, (target, label) in enumerate(TARGETS, 1):
            directory = f"{target}/.bench.exact-cpu.{os.getpid()}.{request_index}.{target_index}"
            if os.path.exists(directory):
                raise AssertionError(f"preexisting exact target: {directory}")
            os.mkdir(directory)
            started = time.perf_counter_ns()
            result = subprocess.run(
                ["bash", "-c", command],
                cwd=directory,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
            elapsed = time.perf_counter_ns() - started
            returncodes.append(result.returncode)
            shutil.rmtree(directory)
            rows.append(
                {
                    "scenario": scenario,
                    "target": label,
                    "meanNs": elapsed,
                    "medianNs": elapsed,
                    "p95Ns": elapsed,
                    "minNs": elapsed,
                    "maxNs": elapsed,
                    "samples": 1,
                }
            )
            stdout.write(
                f"OK {scenario} {label} elapsed_ns={elapsed} returncode={result.returncode}\n"
            )
        pair_wall_ns = time.perf_counter_ns() - pair_started
        ticks_after = process_ticks()
        daemon_cpu_ns = (
            (ticks_after - ticks_before) * 1_000_000_000 // ticks_per_second
        )
        raw = {
            "schema": "layerfs-stage2-exact-external-scenario-v1",
            "config": {
                "reps": 1,
                "warmup": 0,
                "randomizeTargets": 0,
                "mount": "/workspace",
                "base": "/var/tmp",
                "external_wrapper": True,
            },
            "requested_scenario": scenario,
            "command": command,
            "results": rows,
        }
        raw_path = f"{OWNED}/scenario-cpu-exact-{slug}.json"
        with open(raw_path, "x") as output:
            json.dump(raw, output, indent=2, sort_keys=True)
            output.write("\n")
        actual = {row["scenario"] for row in rows}
        checks = {
            "actual_scenario_singleton": actual == {scenario},
            "returncodes_zero": returncodes == [0, 0],
            "row_count_two": len(rows) == 2,
            "targets_exact": {row["target"] for row in rows}
            == {"computerd", "base"},
            "cpu_within_gate": daemon_cpu_ns
            <= int(pair_wall_ns * 1.05) + 5_000_000,
        }
        pairs.append(
            {
                "scenario": scenario,
                "raw_artifact": os.path.basename(raw_path),
                "wall_ns": pair_wall_ns,
                "daemon_cpu_ns": daemon_cpu_ns,
                "limit_ns": int(pair_wall_ns * 1.05) + 5_000_000,
                "checks": checks,
                "pass": all(checks.values()),
            }
        )

receipt = {
    "schema": "layerfs-stage2-exact-collision-cpu-v1",
    "status": "PASS" if len(pairs) == 3 and all(pair["pass"] for pair in pairs) else "FAIL",
    "ticks_per_second": ticks_per_second,
    "pairs": pairs,
}
with open(f"{OWNED}/scenario-cpu-exact-collisions.json", "x") as output:
    json.dump(receipt, output, indent=2, sort_keys=True)
    output.write("\n")
if receipt["status"] != "PASS":
    raise SystemExit(1)
