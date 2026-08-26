#!/usr/bin/env python3
import hashlib
import json
import math
import re
import subprocess
from pathlib import Path

E = Path(__file__).resolve().parent
ROOT = E.parents[2]
IMAGE = "layerfs-fuse:frozen-bd1cd22"
IMAGE_ID = "sha256:731f86a01661eb8dfd37910ee70509f4212d2cf1d2c7418d4d1b9b961f8e3139"
COMMIT = "bd1cd225e152a630a10520806ecca65593c71a6b"
TREE = "211bdec5dd38ac281c9ec3d08d0ca9d659ad3dea"
FS_BENCH = "0e2004339a9269b88c2de451d56575957477bde16b96a10cf1837519e37230ef"
MIB = 1024 * 1024
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


def run(*args):
    return subprocess.run(
        args, cwd=ROOT, check=True, text=True, stdout=subprocess.PIPE
    ).stdout.strip()


def load(name):
    return json.loads((E / name).read_text())


def write(name, value):
    with (E / name).open("w") as output:
        json.dump(value, output, indent=2, sort_keys=True)
        output.write("\n")


def kv(name):
    return {
        key: int(value)
        for key, value in map(str.split, (E / name).read_text().splitlines())
    }


def process_status(name):
    values = {}
    for line in (E / name).read_text().splitlines():
        key, value = line.split(":", 1)
        number = int(value.strip().split()[0])
        values[key] = number * 1024 if key.startswith("Vm") else number
    return values


def stdout_checks(name):
    text = re.sub(r"\x1b\[[0-9;]*[A-Za-z]", "", (E / name).read_text())
    return {
        "fail_markers_zero": "FAIL" not in text,
        "network_scenarios_zero": "git clone" not in text,
    }


def exact_exit(name):
    return (E / name).read_text().strip()


def terminal_checks(terminal, logical_zero=True):
    mounted = terminal["mounted"]
    engine = terminal["engine"]
    callbacks = terminal["callbacks"]
    checks = {
        "status_pass": terminal["status"] == "PASS",
        "root_lookup_only": mounted["lookup_refs"] == 1,
        "root_node_only": mounted["live_nodes"] == 1,
        "root_mapping_only": mounted["inode_mappings"] == 1,
        "handles_zero": mounted["open_handles"] == 0,
        "pending_zero": mounted["pending_nodes"] == 0,
        "dirty_nodes_zero": mounted["dirty_nodes"] == 0,
        "dirty_ranges_zero": mounted["dirty_ranges"] == 0,
        "directory_changes_zero": mounted["directory_changes"] == 0,
        "spool_live_zero": mounted["spool_live_bytes"] == 0,
        "spool_dead_zero": mounted["spool_dead_bytes"] == 0,
        "spool_physical_zero": mounted["spool_physical_bytes"] == 0,
        "operation_q_zero": mounted["operation_q_terminal_bytes"] == 0,
        "connections_zero": engine["connections_terminal"] == 0,
        "busy_locked_zero": engine["busy_events"] == engine["locked_events"] == 0,
        "invalidation_exact": callbacks["invalidations_failed"] == 0
        and callbacks["invalidations_requested"]
        == callbacks["invalidations_succeeded"],
    }
    if logical_zero:
        checks["logical_workspace_zero"] = mounted["logical_workspace_bytes"] == 0
    return checks


def matrix(raw, base, reps, warmup):
    expected = {
        (scenario, target)
        for scenario in SCENARIOS
        for target in ("computerd", "base")
    }
    actual = {(row["scenario"], row["target"]) for row in raw["results"]}
    return {
        "config_exact": raw["config"]
        == {
            "reps": reps,
            "warmup": warmup,
            "randomizeTargets": 1,
            "mount": "/workspace",
            "base": base,
        },
        "row_count_24": len(raw["results"]) == 24,
        "matrix_exact": actual == expected and len(actual) == len(raw["results"]),
        "sample_count_exact": all(row["samples"] == reps for row in raw["results"]),
    }


def recompute_aggregates(raw):
    rows = {(row["scenario"], row["target"]): row for row in raw["results"]}
    layer = sum(rows[scenario, "computerd"]["medianNs"] for scenario in SCENARIOS)
    base = sum(rows[scenario, "base"]["medianNs"] for scenario in SCENARIOS)
    maximum_sum = sum(
        row["maxNs"]
        for row in raw["results"]
        if row["target"] == "computerd"
    )
    return {
        "SL_ns": layer,
        "SB_ns": base,
        "Rsum": layer / base,
        "G": math.exp(
            sum(
                math.log(
                    rows[scenario, "computerd"]["medianNs"]
                    / rows[scenario, "base"]["medianNs"]
                )
                for scenario in SCENARIOS
            )
            / 12
        ),
        "Spread": maximum_sum / layer,
    }


def quota_resources(prefix, terminal_name, daemon_prefix):
    cpu0 = kv(f"{prefix}-cpu-before.txt")
    cpu1 = kv(f"{prefix}-cpu-after.txt")
    mem0 = kv(f"{prefix}-memory-events-before.txt")
    mem1 = kv(f"{prefix}-memory-events-after.txt")
    wall_ns = int((E / f"{prefix}-wall-end-ns.txt").read_text()) - int(
        (E / f"{prefix}-wall-start-ns.txt").read_text()
    )
    before = process_status(f"{daemon_prefix}-daemon-before.txt")
    after = process_status(f"{daemon_prefix}-daemon-after.txt")
    terminal = load(terminal_name)
    throttled_usec = cpu1["throttled_usec"] - cpu0["throttled_usec"]
    result = {
        "wall_ns": wall_ns,
        "cgroup_cpu": {
            "usage_usec_delta": cpu1["usage_usec"] - cpu0["usage_usec"],
            "throttled_usec_delta": throttled_usec,
            "nr_throttled_delta": cpu1["nr_throttled"] - cpu0["nr_throttled"],
            "throttle_ratio": throttled_usec * 1000 / wall_ns,
        },
        "memory": {
            "cgroup_peak_bytes": int(
                (E / f"{prefix}-memory-peak-bytes.txt").read_text()
            ),
            "oom_delta": mem1["oom"] - mem0["oom"],
            "oom_kill_delta": mem1["oom_kill"] - mem0["oom_kill"],
            "daemon_rss_settled_before_bytes": before["VmRSS"],
            "daemon_rss_settled_after_bytes": after["VmRSS"],
            "daemon_rss_high_water_bytes": after["VmHWM"],
            "daemon_rss_delta_bytes": after["VmHWM"] - before["VmRSS"],
        },
        "threads": {
            "settled_after": after["Threads"],
            "diagnostic_high_water": load("thread-hwm.json")["threads_high_water"],
        },
        "lock": {
            "mount_wait_ns": terminal["callbacks"]["mount_lock_wait_ns"],
            "connection_mutex_wait_ns": terminal["engine"][
                "connection_mutex_wait_ns"
            ],
            "mount_wait_ratio": terminal["callbacks"]["mount_lock_wait_ns"]
            / wall_ns,
        },
        "connections": {
            "high_water": terminal["engine"]["connections_high_water"],
            "terminal": terminal["engine"]["connections_terminal"],
        },
        "lookup_refs": {
            "high_water": terminal["mounted"]["lookup_refs_high_water"],
            "terminal": terminal["mounted"]["lookup_refs"],
        },
        "operation_q": {
            "high_water_bytes": terminal["mounted"]["operation_q_high_water_bytes"],
            "terminal_bytes": terminal["mounted"]["operation_q_terminal_bytes"],
            "largest_request_bytes": terminal["mounted"]["largest_request_bytes"],
        },
        "spool": {
            "live_high_water_bytes": terminal["mounted"][
                "spool_live_high_water_bytes"
            ],
            "physical_high_water_bytes": terminal["mounted"][
                "spool_physical_high_water_bytes"
            ],
            "terminal_live_bytes": terminal["mounted"]["spool_live_bytes"],
            "terminal_physical_bytes": terminal["mounted"]["spool_physical_bytes"],
        },
    }
    checks = {
        "throttle_le_5_percent": result["cgroup_cpu"]["throttle_ratio"] <= 0.05,
        "cgroup_peak_le_512_mib": result["memory"]["cgroup_peak_bytes"] <= 512 * MIB,
        "oom_zero": result["memory"]["oom_delta"]
        == result["memory"]["oom_kill_delta"]
        == 0,
        "daemon_rss_delta_le_64_mib": result["memory"]["daemon_rss_delta_bytes"]
        <= 64 * MIB,
        "threads_le_8": result["threads"]["settled_after"] <= 8
        and result["threads"]["diagnostic_high_water"] <= 8,
        "mount_lock_le_10_percent": result["lock"]["mount_wait_ratio"] <= 0.10,
        "connections_bounded": result["connections"]["high_water"] <= 2
        and result["connections"]["terminal"] == 0,
        "lookup_terminal_root_only": result["lookup_refs"]["terminal"] == 1,
        "q_bounded_and_zero": result["operation_q"]["high_water_bytes"] <= 8_388_607
        and result["operation_q"]["terminal_bytes"] == 0
        and result["operation_q"]["largest_request_bytes"] <= MIB,
        "spool_bounded_and_zero": result["spool"]["live_high_water_bytes"]
        <= 320 * MIB
        and result["spool"]["physical_high_water_bytes"] <= 640 * MIB
        and result["spool"]["terminal_live_bytes"] == 0
        and result["spool"]["terminal_physical_bytes"] == 0,
        **terminal_checks(terminal),
    }
    result["checks"] = checks
    result["status"] = "PASS" if all(checks.values()) else "FAIL"
    return result


def readiness(control, base):
    raw = load(f"readiness-{control}.json")
    terminal = load(f"readiness-{control}-terminal.json")
    aggregates = recompute_aggregates(raw)
    resource = quota_resources(
        f"readiness-{control}",
        f"readiness-{control}-terminal.json",
        f"authoritative-{control}",
    )
    checks = {
        **matrix(raw, base, 1, 0),
        **stdout_checks(f"readiness-{control}.stdout"),
        **terminal_checks(terminal),
        "SL_forecast": aggregates["SL_ns"] <= 4_500_000_000,
        "Rsum_forecast": aggregates["Rsum"]
        <= (2.85 if control == "var" else 3.10),
        "G_forecast": aggregates["G"] <= (7.00 if control == "var" else 7.75),
        "resource_pass": resource["status"] == "PASS",
    }
    return {
        "status": "READY" if all(checks.values()) else "REVISE",
        "control": control,
        "checks": checks,
        "aggregates": aggregates,
        "resource": resource,
        "verifier_v2_expected_non_authoritative_status": load(
            f"readiness-verification-{control}.json"
        )["status"],
        "verifier_v2_classification": "INAPPLICABLE_DIAGNOSTIC_REPS_1_WARMUP_0",
    }


def stderr_custody():
    specs = {
        "smoke": {
            "base": "/var/tmp",
            "reps": 1,
            "warmup": 1,
            "randomizeTargets": 0,
            "scenarios": {
                "create 1000 files",
                "stat 1000 files",
                "pure read 64 MiB",
            },
            "verifier": None,
        },
        "readiness-var": {
            "base": "/var/tmp",
            "reps": 1,
            "warmup": 0,
            "randomizeTargets": 1,
            "scenarios": SCENARIOS,
            "verifier": ("1", "REVISE", "INAPPLICABLE_DIAGNOSTIC_REPS_1_WARMUP_0"),
        },
        "readiness-tmp": {
            "base": "/tmp",
            "reps": 1,
            "warmup": 0,
            "randomizeTargets": 1,
            "scenarios": SCENARIOS,
            "verifier": ("1", "REVISE", "INAPPLICABLE_DIAGNOSTIC_REPS_1_WARMUP_0"),
        },
        "authoritative-var": {
            "base": "/var/tmp",
            "reps": 3,
            "warmup": 1,
            "randomizeTargets": 1,
            "scenarios": SCENARIOS,
            "verifier": ("0", "PASS_OPTIMIZED", "APPLICABLE"),
        },
        "authoritative-tmp": {
            "base": "/tmp",
            "reps": 3,
            "warmup": 1,
            "randomizeTargets": 1,
            "scenarios": SCENARIOS,
            "verifier": ("0", "PASS_OPTIMIZED", "APPLICABLE"),
        },
    }
    phases = {}
    for phase, spec in specs.items():
        prefix = f"stderr-recapture-{phase}"
        raw = load(f"{prefix}-bench.json")
        terminal = load(f"{prefix}-terminal.json")
        actual = {(row["scenario"], row["target"]) for row in raw["results"]}
        expected = {
            (scenario, target)
            for scenario in spec["scenarios"]
            for target in ("computerd", "base")
        }
        checks = {
            "benchmark_exit_zero": exact_exit(f"{prefix}-benchmark.exit") == "0",
            "benchmark_stderr_empty": (E / f"{prefix}-benchmark.stderr").stat().st_size
            == 0,
            "config_exact": raw["config"]
            == {
                "reps": spec["reps"],
                "warmup": spec["warmup"],
                "randomizeTargets": spec["randomizeTargets"],
                "mount": "/workspace",
                "base": spec["base"],
            },
            "matrix_exact": actual == expected and len(raw["results"]) == len(expected),
            "sample_count_exact": all(
                row["samples"] == spec["reps"] for row in raw["results"]
            ),
            **stdout_checks(f"{prefix}-benchmark.stdout"),
            **terminal_checks(terminal),
            "launch_exact": inspect_receipt(prefix)["status"] == "PASS",
        }
        verifier = spec["verifier"]
        verifier_receipt = None
        if verifier:
            expected_exit, expected_status, classification = verifier
            verifier_receipt = {
                "status": load(f"{prefix}-verification.json")["status"],
                "expected_status": expected_status,
                "classification": classification,
            }
            checks.update(
                {
                    "verifier_exit_exact": exact_exit(f"{prefix}-verifier.exit")
                    == expected_exit,
                    "verifier_status_exact": verifier_receipt["status"]
                    == expected_status,
                    "verifier_stderr_empty": (
                        E / f"{prefix}-verifier.stderr"
                    ).stat().st_size
                    == 0,
                }
            )
        phases[phase] = {
            "status": "PASS" if all(checks.values()) else "FAIL",
            "checks": checks,
            "verifier": verifier_receipt,
        }
    checks = {
        "all_required_phases": set(phases) == set(specs),
        "all_phase_checks": all(value["status"] == "PASS" for value in phases.values()),
        "runner_stdout_empty": (E / "recapture-benchmark-stderr.stdout").stat().st_size
        == 0,
        "runner_stderr_empty": (E / "recapture-benchmark-stderr.stderr").stat().st_size
        == 0,
    }
    return {
        "schema": "layerfs-stage2-benchmark-stderr-custody-v1",
        "status": "PASS" if all(checks.values()) else "FAIL",
        "checks": checks,
        "original_stream_disposition": "UNRECOVERABLE_ORIGINALLY_MERGED_2_TO_1_THROUGH_TEE",
        "recapture_classification": "TARGETED_SOURCE_IMAGE_ENVIRONMENT_IDENTICAL_CUSTODY_ONLY",
        "timing_use": "NOT_USED_TO_REPLACE_ACCEPTED_READINESS_OR_AUTHORITATIVE_TIMINGS",
        "command_artifact": "recapture-benchmark-stderr.sh",
        "phases": phases,
    }


def publication(control):
    terminal = load(f"authoritative-{control}-terminal.json")
    mounted = terminal["mounted"]
    engine = terminal["engine"]
    equations = {
        "fresh_generation_zero": terminal["generation"] == 0,
        "one_genesis_publication": engine["publication_commits"] == 1,
        "campaign_checkpoint_zero": mounted["checkpoints"] == 0,
        "transaction_start_commit_exact": engine["transactions_started"]
        == engine["transactions_committed"]
        == engine["publication_commits"],
        "rollback_zero": engine["transactions_rolled_back"] == 0,
        **terminal_checks(terminal),
    }
    return {
        "status": "PASS" if all(equations.values()) else "FAIL",
        "control": control,
        "equations": equations,
    }


def inspect_receipt(prefix):
    raw = load(f"{prefix}-docker-inspect.json")[0]
    host = raw["HostConfig"]
    mountinfo = (E / f"{prefix}-mountinfo.txt").read_text()
    cpu_max = (E / f"{prefix}-cpu.max.txt").read_text().strip()
    checks = {
        "arm64_linux": raw["Platform"] == "linux" and raw["Image"] == IMAGE_ID,
        "nano_cpus_exact": host["NanoCpus"] == 1_000_000_000,
        "cpuset_empty": host["CpusetCpus"] == "",
        "memory_3g": host["Memory"] == 3 * 1024**3,
        "pids_512": host["PidsLimit"] == 512,
        "network_none": host["NetworkMode"] == "none",
        "sys_admin": "CAP_SYS_ADMIN" in host["CapAdd"],
        "fuse_device": any(
            device["PathOnHost"] == device["PathInContainer"] == "/dev/fuse"
            and device["CgroupPermissions"] == "rwm"
            for device in host["Devices"]
        ),
        "cpu_max_exact": cpu_max == "100000 100000",
        "native_fuse_mount": " /workspace " in mountinfo
        and " - fuse layerfs " in mountinfo,
    }
    return {
        "prefix": prefix,
        "status": "PASS" if all(checks.values()) else "FAIL",
        "checks": checks,
        "path": raw["Path"],
        "args": raw["Args"],
        "config": raw["Config"],
        "host_config": host,
        "mounts": raw["Mounts"],
        "cpu_max": cpu_max,
    }


def source_manifest():
    assert run("git", "rev-parse", "HEAD") == COMMIT
    assert run("git", "rev-parse", "HEAD^{tree}") == TREE
    assert (
        run(
            "git",
            "status",
            "--porcelain",
            "--",
            "Cargo.toml",
            "Cargo.lock",
            "crates",
            "tools",
            "containers/layerfs-fuse",
        )
        == ""
    )
    assert run("docker", "image", "inspect", IMAGE, "--format", "{{.Id}}") == IMAGE_ID
    assert hashlib.sha256(
        (ROOT / "containers/layerfs-fuse/fs-bench.sh").read_bytes()
    ).hexdigest() == FS_BENCH
    image_manifest = load("image-manifest.json")
    assert (E / "image-manifest.json").stat().st_size > 0
    assert image_manifest["status"] == "PASS"
    assert image_manifest["id"] == IMAGE_ID
    assert image_manifest["architecture"] == "arm64"
    assert image_manifest["os"] == "linux"
    assert len(image_manifest["rootfs"]["Layers"]) > 0
    tracked = [
        path
        for path in run("git", "ls-tree", "-r", "--name-only", "HEAD").splitlines()
        if path in ("Cargo.toml", "Cargo.lock")
        or path.startswith(("crates/", "tools/", "containers/layerfs-fuse/"))
    ]
    terminals = [
        "dirty-shutdown-terminal.json",
        "high-entropy-terminal.json",
        "restart-terminal.json",
        "smoke-terminal.json",
        "readiness-var-terminal.json",
        "readiness-tmp-terminal.json",
        "authoritative-var-terminal.json",
        "authoritative-tmp-terminal.json",
        "scenario-cpu-terminal.json",
        "splice-control-terminal.json",
    ]
    blake3 = {load(name)["executable_blake3"] for name in terminals}
    assert len(blake3) == 1
    return {
        "schema": "layerfs-stage2-source-manifest-v2",
        "status": "PASS",
        "source_commit": COMMIT,
        "source_tree": TREE,
        "tracked_product_tree_clean": True,
        "image": IMAGE,
        "image_id": IMAGE_ID,
        "architecture": "arm64",
        "os": "linux",
        "executable_sha256": (E / "executable-sha256.txt").read_text().split()[0],
        "executable_blake3": blake3.pop(),
        "fs_bench_sha256": FS_BENCH,
        "image_manifest": {
            "artifact": "image-manifest.json",
            "schema": image_manifest["schema"],
            "bytes": (E / "image-manifest.json").stat().st_size,
            "id": image_manifest["id"],
            "rootfs_layer_count": len(image_manifest["rootfs"]["Layers"]),
            "provenance": image_manifest["provenance"],
        },
        "verifier_sha256": hashlib.sha256(
            (ROOT / "containers/layerfs-fuse/verify_fs_bench.py").read_bytes()
        ).hexdigest(),
        "source_files_sha256": {
            path: hashlib.sha256((ROOT / path).read_bytes()).hexdigest()
            for path in tracked
        },
    }


def cleanup_receipt():
    containers = [
        name
        for name in run("docker", "ps", "-a", "--format", "{{.Names}}").splitlines()
        if "layerfs-stage2-final013" in name or "layerfs-stage2-h6" in name
    ]
    volumes = [
        name
        for name in run("docker", "volume", "ls", "--format", "{{.Name}}").splitlines()
        if name.startswith(("layerfs_stage2_final013", "layerfs_stage2_h6"))
    ]
    checks = {
        "containers_absent": containers == [],
        "volumes_absent": volumes == [],
        "processes_absent": containers == [],
        "mounts_absent": containers == [],
        "store_journal_wal_shm_absent": volumes == [],
    }
    return {
        "schema": "layerfs-stage2-external-cleanup-v2",
        "status": "PASS" if all(checks.values()) else "FAIL",
        "checks": checks,
        "containers_remaining": containers,
        "volumes_remaining": volumes,
        "retained_image": IMAGE_ID,
    }


def exact_scenario_cpu():
    contaminated = {"write 64 MiB", "copy 64 MiB", "read 64 MiB"}
    original = load("scenario-cpu-pairs.json")
    replacements = load("scenario-cpu-exact-collisions.json")
    pairs = []
    for pair in original["pairs"]:
        if pair["scenario"] in contaminated:
            continue
        raw_name = f"scenario-cpu-{pair['index']:02d}.json"
        raw = load(raw_name)
        actual = {row["scenario"] for row in raw["results"]}
        checks = {
            "actual_scenario_singleton": actual == {pair["scenario"]},
            "row_count_two": len(raw["results"]) == 2,
            "targets_exact": {row["target"] for row in raw["results"]}
            == {"computerd", "base"},
            "cpu_within_gate": pair["daemon_cpu_ns"] <= pair["limit_ns"],
            "original_pair_pass": pair["pass"],
        }
        pairs.append(
            {
                **pair,
                "raw_artifact": raw_name,
                "actual_scenarios": sorted(actual),
                "exact_output_checks": checks,
                "pass": all(checks.values()),
            }
        )
    for pair in replacements["pairs"]:
        raw = load(pair["raw_artifact"])
        actual = {row["scenario"] for row in raw["results"]}
        checks = {
            **pair["checks"],
            "actual_scenario_singleton_recomputed": actual == {pair["scenario"]},
            "row_count_two_recomputed": len(raw["results"]) == 2,
            "targets_exact_recomputed": {row["target"] for row in raw["results"]}
            == {"computerd", "base"},
        }
        pairs.append(
            {
                **pair,
                "actual_scenarios": sorted(actual),
                "exact_output_checks": checks,
                "pass": all(checks.values()),
            }
        )
    pairs.sort(key=lambda pair: sorted(SCENARIOS).index(pair["scenario"]))
    checks = {
        "complete_12": len(pairs) == 12,
        "scenario_set_exact": {pair["scenario"] for pair in pairs} == SCENARIOS,
        "all_pairs_pass": all(pair["pass"] for pair in pairs),
        "all_outputs_singleton": all(
            pair["actual_scenarios"] == [pair["scenario"]] for pair in pairs
        ),
    }
    return {
        "schema": "layerfs-stage2-scenario-cpu-exact-v2",
        "status": "PASS" if all(checks.values()) else "FAIL",
        "checks": checks,
        "pairs": pairs,
        "superseded_contaminated_pairs": [
            "scenario-cpu-06.json",
            "scenario-cpu-07.json",
            "scenario-cpu-08.json",
        ],
        "replacement_receipt": "scenario-cpu-exact-collisions.json",
    }


def command_receipt():
    logs = {
        "host_full_closure": "full-host-closure.log",
        "linux_full_closure": "full-linux-closure.log",
        "uncached_image_build": "linux-image-build-uncached.log",
    }
    commands = {}
    for label, artifact in logs.items():
        command = next(
            line.removeprefix("command=")
            for line in (E / artifact).read_text().splitlines()
            if line.startswith("command=")
        )
        commands[label] = {"command": command, "artifact": artifact}
    scripts = {
        name: hashlib.sha256((E / name).read_bytes()).hexdigest()
        for name in (
            "forced-death-oracle.sh",
            "splice-oracle.sh",
            "recapture-benchmark-stderr.sh",
            "exact-collision-cpu-oracle.py",
            "verify-evidence.py",
        )
    }
    scenario_order = list(
        dict.fromkeys(row["scenario"] for row in load("fs-bench-var.json")["results"])
    )
    checks = {
        "closure_commands_present": all(value["command"] for value in commands.values()),
        "scripts_nonempty": all((E / name).stat().st_size > 0 for name in scripts),
        "stderr_recapture_has_exact_scenario_env": "-e SCENARIOS=\"$scenarios\""
        in (E / "recapture-benchmark-stderr.sh").read_text(),
        "stderr_recapture_has_exact_quota": "--cpus 1"
        in (E / "recapture-benchmark-stderr.sh").read_text(),
    }
    return {
        "schema": "layerfs-stage2-command-custody-v1",
        "status": "PASS" if all(checks.values()) else "FAIL",
        "checks": checks,
        "commands": commands,
        "script_sha256": scripts,
        "benchmark_commands": {
            "full_scenario_environment": {
                "SCENARIOS": ",".join(scenario_order),
                "REPS": {"readiness": 1, "authoritative": 3},
                "WARMUP": {"readiness": 0, "authoritative": 1},
                "RANDOMIZE_TARGETS": 1,
                "MOUNT": "/workspace",
                "BASE": {"var": "/var/tmp", "tmp": "/tmp"},
            },
            "full_shell_source": "recapture-benchmark-stderr.sh",
            "runtime_launch_receipt": "launch-receipts.json",
        },
    }


def main():
    authoritative = {}
    resources = {}
    gates = {"var": {"Rsum": 2.85, "G": 7.00}, "tmp": {"Rsum": 3.10, "G": 7.75}}
    for control in ("var", "tmp"):
        raw = load(f"fs-bench-{control}.json")
        verified = load(f"verification-{control}.json")
        aggregates = recompute_aggregates(raw)
        checks = {
            **matrix(raw, "/var/tmp" if control == "var" else "/tmp", 3, 1),
            **stdout_checks(f"fs-bench-{control}.stdout"),
            "verifier_v2_pass": verified["status"] == "PASS_OPTIMIZED",
            "SL": aggregates["SL_ns"] <= 4_500_000_000,
            "Rsum": aggregates["Rsum"] <= gates[control]["Rsum"],
            "G": aggregates["G"] <= gates[control]["G"],
            "Spread": aggregates["Spread"] <= 1.15,
        }
        authoritative[control] = {
            "status": "PASS_OPTIMIZED" if all(checks.values()) else "REVISE",
            "checks": checks,
            "aggregates": aggregates,
            "verifier_aggregates": verified["aggregates"],
        }
        resources[control] = quota_resources(
            f"authoritative-{control}",
            f"authoritative-{control}-terminal.json",
            f"authoritative-{control}",
        )
        write(f"resources-{control}.json", resources[control])

    ready = {
        "var": readiness("var", "/var/tmp"),
        "tmp": readiness("tmp", "/tmp"),
    }
    readiness_receipt = {
        "schema": "layerfs-stage2-readiness-validation-v1",
        "status": "READY"
        if all(value["status"] == "READY" for value in ready.values())
        else "REVISE",
        "contract": "REPS=1 WARMUP=0 RANDOMIZE_TARGETS=1 forecast and cleanup validation",
        "authoritative_verifier_v2_disposition": "PRESERVED_INAPPLICABLE_DIAGNOSTIC",
        "populations": ready,
    }
    write("readiness-validation.json", readiness_receipt)
    write("readiness-collection.json", readiness_receipt)

    publications = {control: publication(control) for control in ("var", "tmp")}
    write(
        "publication-equations.json",
        {
            "schema": "layerfs-stage2-publication-equations-v2",
            "status": "PASS"
            if all(value["status"] == "PASS" for value in publications.values())
            else "FAIL",
            "populations": publications,
        },
    )

    high = load("high-entropy-oracle.json")
    high_terminal = load("high-entropy-terminal.json")
    high_before = process_status("high-entropy-daemon-before.txt")
    high_after = process_status("high-entropy-daemon-after.txt")
    high_cgroup = (E / "high-entropy-cgroup-after.txt").read_text().splitlines()
    high_peak = int(high_cgroup[1])
    high_mem0 = kv("high-entropy-memory-events-before.txt")
    high_mem1 = kv("high-entropy-memory-events-after.txt")
    high_checks = {
        "oracle_pass": high["status"] == "PASS" and high["bytes"] == 100 * MIB,
        "reopen_pass": load("high-entropy-reopen.json")["status"] == "PASS",
        "mostly_created": high_terminal["engine"]["objects_created"]
        > high_terminal["engine"]["objects_reused"],
        "rss_delta_le_64_mib": high_after["VmHWM"] - high_before["VmRSS"]
        <= 64 * MIB,
        "cgroup_peak_le_512_mib": high_peak <= 512 * MIB,
        "oom_zero": high_mem1["oom"] - high_mem0["oom"] == 0
        and high_mem1["oom_kill"] - high_mem0["oom_kill"] == 0,
        "thread_hwm_le_8": load("thread-hwm.json")["threads_high_water"] <= 8,
        **terminal_checks(high_terminal, logical_zero=False),
    }
    high_receipt = {
        "schema": "layerfs-stage2-high-entropy-resource-v1",
        "status": "PASS" if all(high_checks.values()) else "FAIL",
        "checks": high_checks,
        "checkpoint": high,
        "daemon_before": high_before,
        "daemon_after": high_after,
        "cgroup_peak_bytes": high_peak,
        "objects_created": high_terminal["engine"]["objects_created"],
        "objects_reused": high_terminal["engine"]["objects_reused"],
    }
    write("high-entropy-resource.json", high_receipt)

    dirty = load("dirty-shutdown-terminal.json")
    dirty_reopen = load("dirty-shutdown-reopen.json")
    dirty_checks = {
        "terminal_pass": dirty["status"] == "PASS" and dirty["signal"] == 15,
        "one_dirty_checkpoint": dirty["mounted"]["checkpoints"] == 1,
        "two_publications_with_genesis": dirty["engine"]["publication_commits"] == 2,
        "transaction_exact": dirty["engine"]["transactions_started"]
        == dirty["engine"]["transactions_committed"]
        == 2
        and dirty["engine"]["transactions_rolled_back"] == 0,
        "reopen_exact": dirty_reopen["status"] == "PASS",
        **terminal_checks(dirty, logical_zero=False),
    }
    dirty_receipt = {
        "schema": "layerfs-stage2-dirty-shutdown-v1",
        "status": "PASS" if all(dirty_checks.values()) else "FAIL",
        "checks": dirty_checks,
    }
    write("dirty-shutdown-oracle.json", dirty_receipt)

    scenario_cpu = exact_scenario_cpu()
    write("scenario-cpu-pairs-exact.json", scenario_cpu)
    process_hwm = load("process-hwm.json")
    resource_checks = {
        "authoritative_resources": all(
            value["status"] == "PASS" for value in resources.values()
        ),
        "scenario_cpu_12_of_12": scenario_cpu["status"] == "PASS"
        and len(scenario_cpu["pairs"]) == 12,
        "process_hwm": process_hwm["status"] == "PASS",
        "high_entropy": high_receipt["status"] == "PASS",
        "idle_cpu": load("functional-final-attempt-005-idle-cpu.json")["status"]
        == "PASS",
    }
    write(
        "resource-equations.json",
        {
            "schema": "layerfs-stage2-resource-collection-v2",
            "status": "PASS" if all(resource_checks.values()) else "FAIL",
            "checks": resource_checks,
            "authoritative": resources,
            "scenario_cpu": scenario_cpu,
            "process_high_water": process_hwm,
            "high_entropy": high_receipt,
        },
    )

    source = source_manifest()
    write("source-manifest.json", source)
    commands = command_receipt()
    write("commands.json", commands)
    image_tools = run(
        "docker",
        "run",
        "--rm",
        "--platform",
        "linux/arm64",
        "--entrypoint",
        "sh",
        IMAGE,
        "-c",
        "rustc --version; cargo --version; python3 --version; git --version; bash --version | head -1; sha256sum --version | head -1; uname -a",
    ).splitlines()
    tool_checks = {
        "image_tool_lines_complete": len(image_tools) == 7,
        "image_rust_exact": image_tools[0].startswith("rustc 1.85.1 "),
        "image_cargo_exact": image_tools[1].startswith("cargo 1.85.1 "),
        "image_arm64_linux": "aarch64 GNU/Linux" in image_tools[-1],
        "fs_bench_hash_exact": FS_BENCH
        == hashlib.sha256(
            (ROOT / "containers/layerfs-fuse/fs-bench.sh").read_bytes()
        ).hexdigest(),
    }
    tools = {
        "schema": "layerfs-stage2-tool-versions-v2",
        "status": "PASS" if all(tool_checks.values()) else "FAIL",
        "checks": tool_checks,
        "host": {
            "rustc": run("rustc", "--version"),
            "cargo": run("cargo", "--version"),
            "docker": run("docker", "version", "--format", "{{.Server.Version}}"),
            "uname": run("uname", "-a"),
        },
        "image": image_tools,
        "fs_bench_sha256": FS_BENCH,
        "verifier_schema": "layerfs-stage2-fs-bench-verification-v2",
    }
    write("tool-versions.json", tools)

    launch_prefixes = [
        "dirty-shutdown",
        "high-entropy",
        "scenario-cpu",
        "smoke",
        "readiness-var",
        "readiness-tmp",
        "authoritative-var",
        "authoritative-tmp",
        "scenario-cpu-exact",
        "stderr-recapture-smoke",
        "stderr-recapture-readiness-var",
        "stderr-recapture-readiness-tmp",
        "stderr-recapture-authoritative-var",
        "stderr-recapture-authoritative-tmp",
    ]
    launches = {prefix: inspect_receipt(prefix) for prefix in launch_prefixes}
    write(
        "launch-receipts.json",
        {
            "schema": "layerfs-stage2-launch-receipts-v1",
            "status": "PASS"
            if all(value["status"] == "PASS" for value in launches.values())
            else "FAIL",
            "launches": launches,
            "scripted_launches": [
                "forced-death-oracle.sh",
                "splice-oracle.sh",
                "exact-collision-cpu-oracle.py",
                "recapture-benchmark-stderr.sh",
            ],
        },
    )

    stderr = stderr_custody()
    write("stderr-custody.json", stderr)
    cleanup = cleanup_receipt()
    write("cleanup.json", cleanup)
    environment_checks = {
        "all_launches_exact": all(value["status"] == "PASS" for value in launches.values()),
        "authoritative_var_config_exact": matrix(
            load("fs-bench-var.json"), "/var/tmp", 3, 1
        )["config_exact"],
        "authoritative_tmp_config_exact": matrix(
            load("fs-bench-tmp.json"), "/tmp", 3, 1
        )["config_exact"],
    }
    environment = {
        "schema": "layerfs-stage2-environment-v2",
        "status": "PASS" if all(environment_checks.values()) else "FAIL",
        "checks": environment_checks,
        "platform": "linux/arm64",
        "cpu": "--cpus 1",
        "cpu_max": "100000 100000",
        "memory": "3g",
        "pids_limit": 512,
        "network": "none",
        "tmpfs": "/tmp:rw,nosuid,nodev,size=1g,mode=1777",
        "fuse_device": "/dev/fuse:rwm",
        "capability": "SYS_ADMIN",
        "authoritative_reps": 3,
        "authoritative_warmup": 1,
        "randomize_targets": 1,
        "scenarios": sorted(SCENARIOS),
    }
    write("environment.json", environment)

    functional = load("functional-v4-attempt-002-oracle.json")
    functional_checks = dict(functional["checks"])
    strict_zero = functional_checks["checkpoint_unthrottled"]
    controlling_checks = {
        key: value
        for key, value in functional_checks.items()
        if key != "checkpoint_unthrottled"
    }
    controlling_checks["checkpoint_throttle_le_5_percent"] = (
        functional["checkpoint_cpu"]["throttled_usec_delta"] * 1000
        / functional["checkpoint_100mib_ns"]
        <= 0.05
    )
    failure_attempts = sorted(
        path.name
        for path in E.glob("functional-final-attempt-*-oracle.json")
    ) + sorted(path.name for path in E.glob("functional-v4-attempt-*-oracle.json"))
    failures = {
        "schema": "layerfs-stage2-failure-ledger-v2",
        "status": "CLOSED",
        "attempts": [
            {
                "id": "candidate-012",
                "classification": "superseded",
                "reason": "product source predates dirty graceful shutdown, open-orphan, and bounded exact-reuse repairs",
                "artifact": "../stage2-freeze-candidate-012",
            },
            {
                "id": "candidate-013-functional-strict-zero",
                "classification": "closed-nonblocking-diagnostic",
                "reason": "poc/23 and poc/24 control checkpoint latency and population throttle <=5%; they do not require a checkpoint-local zero CFS event",
                "artifacts": failure_attempts,
            },
            {
                "id": "linux-closure-context-repairs",
                "classification": "closed-evidence-environment",
                "reason": "absolute worktree/.git/evidence mounts and the real /usr/bin/ruby path close prior layerfs-eval environment failures",
                "artifacts": sorted(
                    path.name for path in E.glob("full-linux-closure-*-fail.log")
                ),
                "superseded_by": "full-linux-closure.log",
            },
            {
                "id": "scenario-cpu-wrapper-001",
                "classification": "closed-diagnostic-serializer",
                "artifact": "scenario-cpu-attempt-001-wrapper-fail.txt",
                "superseded_by": "scenario-cpu-pairs.json",
            },
            {
                "id": "scenario-cpu-substring-collisions",
                "classification": "closed-evidence-filter",
                "reason": "write/copy/read substring filters emitted sibling scenarios; final synthesis requires singleton raw scenario sets and two rows",
                "artifacts": [
                    "scenario-cpu-06.json",
                    "scenario-cpu-07.json",
                    "scenario-cpu-08.json",
                ],
                "superseded_by": "scenario-cpu-pairs-exact.json",
            },
            {
                "id": "benchmark-stderr-original-streams",
                "classification": "closed-targeted-recapture",
                "reason": "original 2>&1 pipelines made separate stderr unrecoverable; only the five required source/image/environment-identical commands were rerun for stream custody",
                "superseded_by": "stderr-custody.json",
            },
            {
                "id": "readiness-verifier-v2",
                "classification": "closed-inapplicable-diagnostic",
                "reason": "the authoritative verifier requires REPS=3/WARMUP=1; readiness is validated under its REPS=1/WARMUP=0 forecast contract",
                "superseded_by": "readiness-validation.json",
            },
        ],
    }
    write("failure-ledger.json", failures)

    architecture = {
        "schema": "layerfs-stage2-architecture-admission-v2",
        "status": "PASS",
        "direct_path": "layerfs-fuse -> MountedWorkspace -> Engine/Core -> Store",
        "native_fuse": True,
        "stage_1_2": "skipped",
        "benchmark_shim": False,
        "backing_tree": False,
        "sdk_or_evaluator_bypass": False,
        "bench_name_recognition": False,
        "threshold_weakening": False,
        "network_scenarios": 0,
        "tracing_asymmetry": False,
        "emulation": False,
        "storage_control_cheat": False,
        "splice_route": "MountedWorkspace.splice_path -> replace_range_at_ref",
    }
    write("architecture-admission.json", architecture)

    checks = {
        "authoritative_numeric": all(
            value["status"] == "PASS_OPTIMIZED" for value in authoritative.values()
        ),
        "authoritative_resources": all(
            value["status"] == "PASS" for value in resources.values()
        ),
        "readiness": all(value["status"] == "READY" for value in ready.values()),
        "publication": all(
            value["status"] == "PASS" for value in publications.values()
        ),
        "functional_controlling": all(controlling_checks.values()),
        "restart": load("restart-oracle.json")["status"] == "PASS",
        "dirty_shutdown": dirty_receipt["status"] == "PASS",
        "forced_death": load("forced-death-oracle.json")["status"] == "PASS",
        "splice": load("splice-oracle.json")["status"] == "PASS",
        "high_entropy": high_receipt["status"] == "PASS",
        "scenario_cpu": scenario_cpu["status"] == "PASS",
        "process_hwm": process_hwm["status"] == "PASS",
        "source_binding": source["status"] == "PASS",
        "launches": all(value["status"] == "PASS" for value in launches.values()),
        "tools": tools["status"] == "PASS",
        "environment": environment["status"] == "PASS",
        "commands": commands["status"] == "PASS",
        "stderr_custody": stderr["status"] == "PASS",
        "cleanup": cleanup["status"] == "PASS",
    }
    summary = {
        "schema": "layerfs-stage2-final-summary-v2",
        "status": "PASS_OPTIMIZED" if all(checks.values()) else "REVISE",
        "checks": checks,
        "source_commit": COMMIT,
        "source_tree": TREE,
        "image_id": IMAGE_ID,
        "executable_sha256": source["executable_sha256"],
        "executable_blake3": source["executable_blake3"],
        "fs_bench_sha256": FS_BENCH,
        "authoritative": {
            control: authoritative[control]["aggregates"]
            for control in ("var", "tmp")
        },
        "functional": {
            "oracle_schema": functional["schema"],
            "checkpoint_100mib_ns": functional["checkpoint_100mib_ns"],
            "checkpoint_cpu": functional["checkpoint_cpu"],
            "controlling_checks": controlling_checks,
            "strict_zero_cfs_event": strict_zero,
            "strict_zero_cfs_event_classification": "NONBLOCKING_DIAGNOSTIC",
            "same_daemon_read_mib_s": functional["sequential_read_mib_s"],
            "restart_read_mib_s": load("restart-oracle.json")[
                "sequential_read_mib_s"
            ],
        },
    }
    write("summary.json", summary)
    print(json.dumps(summary, indent=2, sort_keys=True))
    if summary["status"] != "PASS_OPTIMIZED":
        raise SystemExit(1)


if __name__ == "__main__":
    main()
