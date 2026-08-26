#!/usr/bin/env python3
import json
import os
import subprocess
import sys
import time

SCENARIOS = [
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
]
OWNED = "/var/tmp/layerfs-owned"
pid = int(sys.argv[1])
ticks_per_second = os.sysconf("SC_CLK_TCK")


def process_ticks():
    stat = open(f"/proc/{pid}/stat").read()
    fields = stat[stat.rfind(")") + 2 :].split()
    return int(fields[11]) + int(fields[12])


pairs = []
with open(f"{OWNED}/scenario-cpu.stdout", "x") as combined:
    for index, scenario in enumerate(SCENARIOS, 1):
        output = f"{OWNED}/scenario-cpu-{index:02d}.json"
        environment = os.environ.copy()
        environment.update(
            {
                "SCENARIOS": scenario,
                "REPS": "1",
                "WARMUP": "0",
                "RANDOMIZE_TARGETS": "0",
                "MOUNT": "/workspace",
                "BASE": "/var/tmp",
                "OUTPUT_JSON": output,
            }
        )
        ticks_before = process_ticks()
        wall_started = time.perf_counter_ns()
        result = subprocess.run(
            ["bash", "/usr/local/bin/fs-bench.sh"],
            env=environment,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
        )
        wall_ns = time.perf_counter_ns() - wall_started
        ticks_after = process_ticks()
        cpu_ns = (ticks_after - ticks_before) * 1_000_000_000 // ticks_per_second
        combined.write(f"===== {index:02d} {scenario} =====\n")
        combined.write(result.stdout)
        if not result.stdout.endswith("\n"):
            combined.write("\n")
        pairs.append(
            {
                "index": index,
                "scenario": scenario,
                "returncode": result.returncode,
                "wall_ns": wall_ns,
                "daemon_cpu_ns": cpu_ns,
                "limit_ns": int(wall_ns * 1.05) + 5_000_000,
                "pass": result.returncode == 0
                and cpu_ns <= int(wall_ns * 1.05) + 5_000_000,
            }
        )

stdout = open(f"{OWNED}/scenario-cpu.stdout").read()
checks = {
    "all_cpu_pairs_pass": all(pair["pass"] for pair in pairs),
    "all_outputs_present": all(
        os.path.isfile(f"{OWNED}/scenario-cpu-{index:02d}.json")
        for index in range(1, len(SCENARIOS) + 1)
    ),
    "complete_12": len(pairs) == 12,
    "no_fail_markers": "FAIL" not in stdout,
    "no_network_scenarios": "git clone" not in stdout,
}
receipt = {
    "schema": "layerfs-stage2-scenario-cpu-v1",
    "status": "PASS" if all(checks.values()) else "FAIL",
    "checks": checks,
    "ticks_per_second": ticks_per_second,
    "pairs": pairs,
}
with open(f"{OWNED}/scenario-cpu-pairs.json", "x") as output:
    json.dump(receipt, output, indent=2, sort_keys=True)
    output.write("\n")
if receipt["status"] != "PASS":
    raise SystemExit(1)
