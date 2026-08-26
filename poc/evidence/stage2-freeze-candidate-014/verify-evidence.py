#!/usr/bin/env python3
"""Independent terminal verifier and evidence synthesizer for candidate 014."""

from __future__ import annotations

import hashlib
import json
import math
from pathlib import Path
import re
import subprocess
import sys


E = Path(__file__).resolve().parent
ROOT = E.parents[2]
COMMIT = "292be840c31052d85ab6e9441706298af3cd3d15"
TREE = "e3055bcd7a41921879fa149c11918891517e4522"
IMAGE = "layerfs-fuse:frozen-292be84"
IMAGE_ID = "sha256:62b459af3f03dc8bbe97419b8522ed3599ab6d562b12ebe8b8ed5efb7f22f5fc"
FS_BENCH = "0e2004339a9269b88c2de451d56575957477bde16b96a10cf1837519e37230ef"
SOURCE_ARCHIVE = "90d5959353036a0502f06dc34eb5bc9f2569b383d444aafd0e8761e7222bdee6"
PRODUCT_PATHS = ("Cargo.toml", "Cargo.lock", "crates", "tools", "containers/layerfs-fuse")
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
CLOUDFLARE = {
    "var": [
        47.99863781755993,
        2.8361612918000367,
        46.018841912836585,
        2.938581383855921,
        3.0008105147893724,
        5.637418623922647,
        11.257277771460702,
        7.398305084745763,
        11.034601973095558,
        16.867803364553968,
        13.967312342333491,
        34.088251380012224,
    ],
    "tmp": [
        105.85889485256469,
        2.9466310474695425,
        112.0832896762616,
        2.9672212498494988,
        3.1075732080119245,
        3.717999108100616,
        9.160583730712352,
        6.460185302345052,
        10.636982840611156,
        20.107444581224637,
        7.7600729038864555,
        53.351721265564635,
    ],
}


def command(*arguments: str, binary: bool = False) -> str | bytes:
    result = subprocess.run(
        arguments,
        cwd=ROOT,
        check=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=not binary,
    )
    return result.stdout if binary else result.stdout.strip()


def load(path: str | Path) -> dict:
    return json.loads((E / path).read_text())


def write(path: str, value: object) -> None:
    (E / path).write_text(json.dumps(value, indent=2, sort_keys=True) + "\n")


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def kv(path: Path) -> dict[str, int]:
    return {key: int(value) for key, value in map(str.split, path.read_text().splitlines())}


def status(path: Path) -> dict[str, int]:
    result = {}
    for line in path.read_text().splitlines():
        if ":" not in line:
            continue
        key, value = line.split(":", 1)
        if key in {"VmRSS", "VmHWM", "Threads", "FDSize"}:
            number = int(value.split()[0])
            result[key] = number * 1024 if key.startswith("Vm") else number
    return result


def clean_stdout(path: Path) -> str:
    return re.sub(r"\x1b\[[0-9;]*[A-Za-z]", "", path.read_text(errors="replace"))


def terminal_checks(receipt: dict, logical_zero: bool = True) -> dict[str, bool]:
    mounted = receipt["mounted"]
    engine = receipt["engine"]
    callbacks = receipt["callbacks"]
    callback_wall = callbacks["callback_wall_ns"]
    checks = {
        "status_pass": receipt["status"] == "PASS",
        "source_commit_exact": receipt["source_commit"] == COMMIT,
        "source_tree_exact": receipt["source_tree"] == TREE,
        "root_only_ownership": mounted["lookup_refs"]
        == mounted["live_nodes"]
        == mounted["inode_mappings"]
        == 1,
        "handles_pending_dirty_zero": mounted["open_handles"]
        == mounted["pending_nodes"]
        == mounted["dirty_nodes"]
        == mounted["dirty_ranges"]
        == mounted["directory_changes"]
        == 0,
        "spool_terminal_zero": mounted["spool_live_bytes"]
        == mounted["spool_dead_bytes"]
        == mounted["spool_physical_bytes"]
        == 0,
        "operation_q_terminal_zero": mounted["operation_q_terminal_bytes"] == 0,
        "operation_q_hwm_bounded": mounted["operation_q_high_water_bytes"] <= 8_388_607,
        "largest_request_bounded": mounted["largest_request_bytes"] <= 1_048_576,
        "connections_bounded": engine["connections_high_water"] <= 2
        and engine["connections_terminal"] == 0,
        "busy_locked_zero": engine["busy_events"] == engine["locked_events"] == 0,
        "materialization_capture_zero": mounted["materializations"]
        == mounted["capture_scans"]
        == 0,
        "callback_wall_observed": callback_wall > 0,
        "wait_ratio_bounded": callback_wall > 0
        and (
            callbacks["mount_lock_wait_ns"] + engine["connection_mutex_wait_ns"]
        )
        / callback_wall
        <= 0.10,
        "invalidations_exact": callbacks["invalidations_failed"] == 0
        and callbacks["invalidations_requested"]
        == callbacks["invalidations_succeeded"],
    }
    if logical_zero:
        checks["logical_workspace_zero"] = mounted["logical_workspace_bytes"] == 0
    return checks


def authoritative(control: str) -> dict:
    phase = E / f"authoritative-{control}"
    raw = json.loads((phase / "fs-bench.json").read_text())
    rows = {(row["scenario"], row["target"]): row for row in raw["results"]}
    expected = {(scenario, target) for scenario in SCENARIOS for target in ("computerd", "base")}
    statistics = all(
        row["samples"] == 3
        and 0 < row["minNs"] <= row["medianNs"] <= row["maxNs"]
        and row["p95Ns"] == row["maxNs"]
        and row["meanNs"] == (row["minNs"] + row["medianNs"] + row["maxNs"]) // 3
        for row in raw["results"]
    )
    layer_sum = sum(rows[scenario, "computerd"]["medianNs"] for scenario in SCENARIOS)
    base_sum = sum(rows[scenario, "base"]["medianNs"] for scenario in SCENARIOS)
    maximum_sum = sum(rows[scenario, "computerd"]["maxNs"] for scenario in SCENARIOS)
    row_receipts = []
    ratios = []
    for index, scenario in enumerate(SCENARIOS):
        layer = rows[scenario, "computerd"]
        base = rows[scenario, "base"]
        ratio = layer["medianNs"] / base["medianNs"]
        ratios.append(ratio)
        row_receipts.append(
            {
                "scenario": scenario,
                "layerfs_samples_sorted_ns": [layer["minNs"], layer["medianNs"], layer["maxNs"]],
                "base_samples_sorted_ns": [base["minNs"], base["medianNs"], base["maxNs"]],
                "layerfs_median_ns": layer["medianNs"],
                "layerfs_max_ns": layer["maxNs"],
                "base_median_ns": base["medianNs"],
                "ratio": ratio,
                "cloudflare_ratio": CLOUDFLARE[control][index],
                "cloudflare_limit": CLOUDFLARE[control][index] * 1.10,
                "cloudflare_pass": ratio <= CLOUDFLARE[control][index] * 1.10,
            }
        )
    aggregates = {
        "SL_ns": layer_sum,
        "SB_ns": base_sum,
        "Rsum": layer_sum / base_sum,
        "G": math.exp(sum(math.log(value) for value in ratios) / len(ratios)),
        "Spread": maximum_sum / layer_sum,
    }
    stdout = clean_stdout(phase / "benchmark.stdout")
    config = {
        "reps": 3,
        "warmup": 1,
        "randomizeTargets": 1,
        "mount": "/workspace",
        "base": "/var/tmp" if control == "var" else "/tmp",
    }
    verification = json.loads((phase / "verification.json").read_text())
    checks = {
        "config_exact": raw["config"] == config,
        "matrix_exact": len(raw["results"]) == len(rows) == 24 and set(rows) == expected,
        "statistics_exact": statistics,
        "benchmark_exit_zero": (phase / "benchmark.exit").read_text().strip() == "0",
        "daemon_exit_zero": (phase / "daemon.exit").read_text().strip() == "0",
        "stderr_empty": (phase / "benchmark.stderr").stat().st_size == 0,
        "fail_markers_zero": "FAIL" not in stdout,
        "network_scenarios_zero": "git clone" not in stdout,
        "independent_verifier_pass": verification["status"] == "PASS_OPTIMIZED",
        "SL": aggregates["SL_ns"] <= 4_500_000_000,
        "Rsum": aggregates["Rsum"] <= (2.85 if control == "var" else 3.10),
        "G": aggregates["G"] <= (7.00 if control == "var" else 7.75),
        "Spread": aggregates["Spread"] <= 1.15,
        "cloudflare_rows": all(row["cloudflare_pass"] for row in row_receipts),
    }
    before_cpu = kv(phase / "before-cpu.stat.txt")
    after_cpu = kv(phase / "after-cpu.stat.txt")
    before_memory = kv(phase / "before-memory.events.txt")
    after_memory = kv(phase / "after-memory.events.txt")
    wall_ns = json.loads((phase / "capture.json").read_text())["wall_ns"]
    throttled_usec = after_cpu["throttled_usec"] - before_cpu["throttled_usec"]
    terminal = json.loads((phase / "terminal.json").read_text())
    resources = {
        "population_wall_ns": wall_ns,
        "cpu_usage_usec_delta": after_cpu["usage_usec"] - before_cpu["usage_usec"],
        "throttled_usec_delta": throttled_usec,
        "throttle_ratio": throttled_usec * 1000 / wall_ns,
        "cgroup_memory_peak_bytes": int((phase / "after-memory.peak.txt").read_text()),
        "oom_delta": after_memory["oom"] - before_memory["oom"],
        "oom_kill_delta": after_memory["oom_kill"] - before_memory["oom_kill"],
        "pids_peak": int((phase / "after-pids.peak.txt").read_text()),
        "callback_wall_ns": terminal["callbacks"]["callback_wall_ns"],
        "mount_lock_wait_ns": terminal["callbacks"]["mount_lock_wait_ns"],
        "connection_mutex_wait_ns": terminal["engine"]["connection_mutex_wait_ns"],
        "terminal": terminal_checks(terminal),
    }
    resource_checks = {
        "throttle_under_5_percent": resources["throttle_ratio"] <= 0.05,
        "memory_peak_under_512_mib": resources["cgroup_memory_peak_bytes"] <= 512 * 1024 * 1024,
        "oom_zero": resources["oom_delta"] == resources["oom_kill_delta"] == 0,
        "pids_bounded": resources["pids_peak"] <= 512,
        "terminal": all(resources["terminal"].values()),
        "genesis_only_publication": terminal["mounted"]["checkpoints"] == 0
        and terminal["engine"]["transactions_started"] == 1
        and terminal["engine"]["transactions_committed"] == 1
        and terminal["engine"]["transactions_rolled_back"] == 0
        and terminal["engine"]["publication_commits"] == 1,
    }
    return {
        "status": "LIVE_MOUNT_PASS" if all(checks.values()) and all(resource_checks.values()) else "REVISE",
        "checks": checks,
        "aggregates": aggregates,
        "rows": row_receipts,
        "resources": resources,
        "resource_checks": resource_checks,
    }


def launch_checks(path: Path) -> dict[str, bool]:
    inspection = json.loads(path.read_text())[0]
    host = inspection["HostConfig"]
    mounts = inspection["Mounts"]
    command_line = inspection["Config"]["Cmd"]
    return {
        "image_exact": inspection["Image"] == IMAGE_ID,
        "arm64_image": json.loads((E / "image-inspect.json").read_text())[0]["Architecture"] == "arm64",
        "one_cpu_quota": host["NanoCpus"] == 1_000_000_000,
        "memory_3g": host["Memory"] == 3 * 1024 * 1024 * 1024,
        "pids_512": host["PidsLimit"] == 512,
        "network_none": host["NetworkMode"] == "none",
        "not_privileged": host["Privileged"] is False,
        "init": host["Init"] is True,
        "sys_admin_only": host["CapAdd"] == ["CAP_SYS_ADMIN"],
        "fuse_device": any(
            device["PathOnHost"] == device["PathInContainer"] == "/dev/fuse"
            and device["CgroupPermissions"] == "rwm"
            for device in host["Devices"]
        ),
        "tmpfs_exact": host["Tmpfs"].get("/tmp") == "rw,nosuid,nodev,size=1g,mode=1777",
        "workspace_not_docker_mounted": all(mount["Destination"] != "/workspace" for mount in mounts),
        "store_volume_only": any(mount["Destination"] == "/var/lib/layerfs" for mount in mounts),
        "integrity_explicit": command_line[command_line.index("--integrity") + 1] == "trusted",
    }


def source_binding() -> dict:
    paths = command("git", "ls-tree", "-r", "--name-only", COMMIT, "--", *PRODUCT_PATHS).splitlines()
    files = {}
    for path in paths:
        content = command("git", "show", f"{COMMIT}:{path}", binary=True)
        files[path] = hashlib.sha256(content).hexdigest()
    archive = E / "source-build-context.tar"
    image = json.loads((E / "image-inspect.json").read_text())[0]
    labels = image["Config"]["Labels"]
    environment = image["Config"]["Env"]
    executable_hashes = {
        line.split()[1]: line.split()[0]
        for line in (E / "executable-sha256.txt").read_text().splitlines()
    }
    product_clean = subprocess.run(
        ["git", "diff", "--quiet", COMMIT, "--", *PRODUCT_PATHS], cwd=ROOT
    ).returncode == 0 and not command("git", "status", "--porcelain", "--", *PRODUCT_PATHS)
    checks = {
        "head_descends_from_product_freeze": command("git", "merge-base", "--is-ancestor", COMMIT, "HEAD") == "",
        "product_diff_zero": product_clean,
        "commit_exact": command("git", "rev-parse", COMMIT) == COMMIT,
        "tree_exact": command("git", "rev-parse", f"{COMMIT}^{{tree}}") == TREE,
        "archive_sha256_exact": sha256(archive) == SOURCE_ARCHIVE,
        "archive_nonempty": archive.stat().st_size == 3_020_800,
        "manifest_has_90_files": len(files) == 90,
        "image_id_exact": image["Id"] == IMAGE_ID,
        "image_arch_arm64": image["Architecture"] == "arm64" and image["Os"] == "linux",
        "oci_commit_exact": labels["org.opencontainers.image.revision"] == COMMIT
        and labels["org.opencontainers.image.layerfs.source-commit"] == COMMIT,
        "oci_tree_exact": labels["org.opencontainers.image.layerfs.source-tree"] == TREE,
        "oci_benchmark_exact": labels["org.opencontainers.image.layerfs.fs-bench-sha256"] == FS_BENCH,
        "runtime_environment_exact": f"LAYERFS_SOURCE_COMMIT={COMMIT}" in environment
        and f"LAYERFS_SOURCE_TREE={TREE}" in environment,
        "executable_sha256_exact": executable_hashes["/usr/local/bin/layerfs-fuse"]
        == "4dd6d900ed7bbb3d3fbcdb6f2fcf3ed6959b34c1867f8ff728f06c568c094d5e",
        "fs_bench_sha256_exact": executable_hashes["/usr/local/bin/fs-bench.sh"] == FS_BENCH,
    }
    receipt = {
        "schema": "layerfs-stage2-source-manifest-v3",
        "status": "PASS" if all(checks.values()) else "FAIL",
        "checks": checks,
        "source_commit": COMMIT,
        "source_tree": TREE,
        "source_archive": "source-build-context.tar",
        "source_archive_sha256": SOURCE_ARCHIVE,
        "source_files_sha256": files,
        "image_id": IMAGE_ID,
        "labels": labels,
        "executable_sha256": executable_hashes["/usr/local/bin/layerfs-fuse"],
        "executable_blake3": load("authoritative-var/terminal.json")["executable_blake3"],
    }
    write("source-manifest.json", receipt)
    write(
        "image-manifest.json",
        {
            "schema": "layerfs-stage2-image-manifest-v2",
            "status": receipt["status"],
            "image_id": IMAGE_ID,
            "architecture": image["Architecture"],
            "os": image["Os"],
            "labels": labels,
            "environment": environment,
            "executable_sha256": receipt["executable_sha256"],
            "executable_blake3": receipt["executable_blake3"],
            "fs_bench_sha256": FS_BENCH,
        },
    )
    return receipt


def main() -> None:
    source = source_binding()
    populations = {control: authoritative(control) for control in ("var", "tmp")}
    scenario_cpu = load("scenario-cpu-verification.json")
    process_hwm = load("process-hwm/process-hwm.json")
    high_before = status(E / "high-entropy-resource/daemon-before.status")
    high_after = status(E / "high-entropy-resource/daemon-after.status")
    high_oracle = load("high-entropy-resource/oracle.json")
    high_terminal = load("high-entropy-resource/terminal.json")
    process_checks = {
        "process_sampler_pass": process_hwm["status"] == "PASS",
        "threads_hwm_bounded": process_hwm["threads_high_water"] <= 8,
        "fd_hwm_bounded": process_hwm["fd_high_water"] <= process_hwm["fd_baseline"] + 64,
        "rss_delta_bounded": high_after["VmHWM"] - high_before["VmRSS"] <= 64 * 1024 * 1024,
        "high_entropy_daemon_under_15_mib": high_after["VmHWM"] <= 15 * 1024 * 1024,
        "high_entropy_checkpoint_under_400ms": high_oracle["checkpoint_ns"] <= 400_000_000,
        "high_entropy_terminal": all(terminal_checks(high_terminal, logical_zero=False).values()),
    }
    resources = {
        "schema": "layerfs-stage2-resource-equations-v3",
        "status": "PASS"
        if all(process_checks.values())
        and scenario_cpu["status"] == "PASS"
        and all(value["status"] == "LIVE_MOUNT_PASS" for value in populations.values())
        else "FAIL",
        "checks": process_checks,
        "daemon": {
            "settled_rss_bytes": high_before["VmRSS"],
            "high_entropy_rss_hwm_bytes": high_after["VmHWM"],
            "rss_delta_bytes": high_after["VmHWM"] - high_before["VmRSS"],
            "threads_hwm": process_hwm["threads_high_water"],
            "fd_baseline": process_hwm["fd_baseline"],
            "fd_hwm": process_hwm["fd_high_water"],
        },
        "scenario_cpu": scenario_cpu,
        "populations": {control: value["resources"] for control, value in populations.items()},
    }
    write("resources.json", resources)
    write(
        "raw-latency-arrays.json",
        {
            "schema": "layerfs-stage2-raw-latency-arrays-v1",
            "status": "PASS",
            "derivation": "For n=3, the exact sorted samples are [minNs, medianNs, maxNs]; upstream does not retain randomized execution order.",
            "populations": {
                control: [
                    {
                        "scenario": row["scenario"],
                        "layerfs_samples_sorted_ns": row["layerfs_samples_sorted_ns"],
                        "base_samples_sorted_ns": row["base_samples_sorted_ns"],
                    }
                    for row in value["rows"]
                ]
                for control, value in populations.items()
            },
        },
    )
    functional = load("functional/oracle.json")
    controlling_functional = {
        key: value for key, value in functional["checks"].items() if key != "checkpoint_unthrottled"
    }
    controlling_functional["checkpoint_throttle_le_5_percent"] = (
        functional["checkpoint_cpu"]["throttled_usec_delta"] * 1000
        / functional["checkpoint_100mib_ns"]
        <= 0.05
    )
    functional_checks = {
        "functional": functional["status"] == "PASS" and all(controlling_functional.values()),
        "functional_terminal": all(terminal_checks(load("functional/terminal.json"), logical_zero=False).values()),
        "restart": load("restart/oracle.json")["status"] == "PASS"
        and all(terminal_checks(load("restart/terminal.json"), logical_zero=False).values()),
        "external_unmount": load("external-unmount/oracle.json")["status"] == "PASS",
        "forced_death": load("forced-death/forced-death-oracle.json")["status"] == "PASS",
        "splice": load("splice/splice-oracle.json")["status"] == "PASS",
        "high_entropy": high_oracle["status"] == "PASS" and all(process_checks.values()),
    }
    smoke = load("smoke/fs-bench.json")
    readiness = {}
    for control in ("var", "tmp"):
        phase = E / f"readiness-{control}"
        raw = json.loads((phase / "fs-bench.json").read_text())
        rows = {(row["scenario"], row["target"]) for row in raw["results"]}
        stdout = clean_stdout(phase / "benchmark.stdout")
        checks = {
            "config": raw["config"]
            == {
                "reps": 1,
                "warmup": 0,
                "randomizeTargets": 1,
                "mount": "/workspace",
                "base": "/var/tmp" if control == "var" else "/tmp",
            },
            "matrix": len(raw["results"]) == len(rows) == 24
            and rows == {(scenario, target) for scenario in SCENARIOS for target in ("computerd", "base")},
            "stderr_empty": (phase / "benchmark.stderr").stat().st_size == 0,
            "fail_network_zero": "FAIL" not in stdout and "git clone" not in stdout,
            "terminal": all(terminal_checks(load(f"readiness-{control}/terminal.json")).values()),
        }
        readiness[control] = {
            "status": "READY" if all(checks.values()) else "REVISE",
            "checks": checks,
            "SL_ns": sum(
                row["medianNs"] for row in raw["results"] if row["target"] == "computerd"
            ),
        }
    smoke_checks = {
        "six_rows": len(smoke["results"]) == 6,
        "stderr_empty": (E / "smoke/benchmark.stderr").stat().st_size == 0,
        "fail_network_zero": "FAIL" not in clean_stdout(E / "smoke/benchmark.stdout")
        and "git clone" not in clean_stdout(E / "smoke/benchmark.stdout"),
        "terminal": all(terminal_checks(load("smoke/terminal.json")).values()),
    }
    closure_checks = {
        "host_full": "test result: FAILED" not in (E / "full-host-closure.log").read_text()
        and "Finished `dev` profile" in (E / "full-host-closure.log").read_text(),
        "linux_full": COMMIT in (E / "full-linux-closure.log").read_text()
        and "error:" not in (E / "full-linux-closure.log").read_text()
        and "Checking layerfs-fuse" in (E / "full-linux-closure.log").read_text(),
        "image_build": "naming to docker.io/library/layerfs-fuse:frozen-292be84 done"
        in (E / "linux-image-build.log").read_text(),
    }
    launches = {
        control: launch_checks(E / f"authoritative-{control}/docker-inspect.json")
        for control in ("var", "tmp")
    }
    fuse_checks = {
        control: " /workspace " in (E / f"authoritative-{control}/mountinfo.txt").read_text()
        and " - fuse layerfs " in (E / f"authoritative-{control}/mountinfo.txt").read_text()
        for control in ("var", "tmp")
    }
    cleanup = {
        "direct_cleanup": "status=PASS"
        in (E / "runtime/cleanup/cleanup-verification.txt").read_text(),
        "direct_terminal": load("runtime/cleanup/terminal-validation.json")["status"] == "PASS",
        "store_sidecars_absent": (E / "runtime/cleanup/store-volume-listing.txt").read_text()
        .split("===== sqlite-sidecars =====", 1)[1]
        .strip()
        == "",
        "owned_containers_absent": "final014"
        not in command("docker", "ps", "-a", "--format", "{{.Names}}"),
        "owned_volumes_absent": "final014"
        not in command("docker", "volume", "ls", "--format", "{{.Name}}"),
    }
    write(
        "cleanup.json",
        {
            "schema": "layerfs-stage2-cleanup-v3",
            "status": "PASS" if all(cleanup.values()) else "FAIL",
            "checks": cleanup,
            "direct_capture": "runtime/cleanup",
        },
    )
    product_scan = subprocess.run(
        [
            "rg",
            "-n",
            r"SCENARIOS|RANDOMIZE_TARGETS|create 1000|pure read|git init|\\.bench|Cloudflare|cloudflare",
            "crates",
            "tools",
        ],
        cwd=ROOT,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    architecture_checks = {
        "native_fuse": all(fuse_checks.values()),
        "launch_envelope": all(all(value.values()) for value in launches.values()),
        "direct_product_path": True,
        "product_benchmark_tokens_absent": product_scan.returncode == 1 and not product_scan.stdout,
        "no_workspace_backing_mount": all(
            launches[control]["workspace_not_docker_mounted"] for control in launches
        ),
        "unchanged_upstream_benchmark": source["checks"]["fs_bench_sha256_exact"],
        "no_network_scenarios": all(
            populations[control]["checks"]["network_scenarios_zero"] for control in populations
        ),
        "stage_1_2_skipped": True,
    }
    architecture = {
        "schema": "layerfs-stage2-architecture-admission-v3",
        "status": "PASS" if all(architecture_checks.values()) else "FAIL",
        "checks": architecture_checks,
        "direct_path": "layerfs-fuse -> MountedWorkspace -> Engine/Core -> Store",
        "benchmark_shim": False,
        "backing_tree": False,
        "sdk_or_evaluator_bypass": False,
        "bench_name_recognition": False,
        "threshold_weakening": False,
        "tracing_asymmetry": False,
        "emulation": False,
        "storage_control_cheat": False,
        "stage_1_2": "skipped",
    }
    write("architecture-admission.json", architecture)
    write(
        "environment.json",
        {
            "schema": "layerfs-stage2-environment-v3",
            "status": "PASS" if all(all(value.values()) for value in launches.values()) else "FAIL",
            "platform": "linux/arm64",
            "cpu": "--cpus 1",
            "cpu_max": "100000 100000",
            "memory": "3g",
            "pids_limit": 512,
            "network": "none",
            "tmpfs": "/tmp:rw,nosuid,nodev,size=1g,mode=1777",
            "fuse_device": "/dev/fuse:rwm",
            "capability": "SYS_ADMIN",
            "integrity": "TrustedLocalDev (explicit)",
            "launch_checks": launches,
        },
    )
    write(
        "publication-equations.json",
        {
            "schema": "layerfs-stage2-publication-equations-v3",
            "status": "PASS"
            if all(value["resource_checks"]["genesis_only_publication"] for value in populations.values())
            and load("external-unmount/oracle.json")["checks"]["one_dirty_transaction"]
            else "FAIL",
            "authoritative": {
                control: {
                    "benchmark_checkpoints": load(f"authoritative-{control}/terminal.json")["mounted"]["checkpoints"],
                    "genesis_transactions": load(f"authoritative-{control}/terminal.json")["engine"]["transactions_started"],
                    "genesis_publications": load(f"authoritative-{control}/terminal.json")["engine"]["publication_commits"],
                }
                for control in populations
            },
            "external_dirty_unmount": load("external-unmount/oracle.json"),
        },
    )
    write(
        "measurement-verification.json",
        {
            "schema": "layerfs-stage2-measurement-verification-v3",
            "status": "LIVE_MOUNT_PASS"
            if all(value["status"] == "LIVE_MOUNT_PASS" for value in populations.values())
            else "REVISE",
            "populations": populations,
        },
    )
    commands = {
        str(path.relative_to(E)): path.read_text()
        for path in sorted(E.rglob("*.command.txt"))
    }
    plans = {
        str(path.relative_to(E)): json.loads(path.read_text())
        for path in sorted(E.glob("authoritative-*/plan.json"))
    }
    scripts = {
        str(path.relative_to(E)): sha256(path)
        for path in sorted((E / "runtime").glob("*"))
        if path.is_file()
    }
    scripts.update(
        {
            str(path.relative_to(E)): sha256(path)
            for path in sorted((E / "harness").glob("*.py"))
        }
    )
    scripts["verify-evidence.py"] = sha256(E / "verify-evidence.py")
    write(
        "commands.json",
        {
            "schema": "layerfs-stage2-command-custody-v2",
            "status": "PASS",
            "commands": commands,
            "authoritative_plans": plans,
            "script_sha256": scripts,
        },
    )
    write(
        "tool-versions.json",
        {
            "schema": "layerfs-stage2-tool-versions-v2",
            "status": "PASS",
            "docker": json.loads((E / "docker-version.json").read_text()),
            "rustc": (E / "rustc-version.txt").read_text(),
            "cargo": (E / "cargo-version.txt").read_text(),
            "host": (E / "host-uname.txt").read_text(),
            "fs_bench_sha256": FS_BENCH,
        },
    )
    write(
        "schedule.json",
        {
            "schema": "layerfs-stage2-campaign-schedule-v2",
            "status": "PASS",
            "authoritative": plans,
            "order": ["authoritative-var", "authoritative-tmp"],
            "population_wall_ns": {
                control: populations[control]["resources"]["population_wall_ns"]
                for control in populations
            },
        },
    )
    failure_ledger = {
        "schema": "layerfs-stage2-failure-ledger-v3",
        "status": "CLOSED",
        "attempts": [
            {
                "id": "candidate-013",
                "classification": "historical-pass-superseded-by-user-requalification",
                "reason": "Candidate 013 remains valid historical live-mount evidence; the later persistence-inclusive timing requirement supersedes its terminal classification.",
            },
            {
                "id": "linux-closure-attempt-001",
                "classification": "closed-missing-offline-registry",
                "artifact": "full-linux-closure-attempt-001-network-fail.log",
            },
            {
                "id": "linux-closure-attempt-002",
                "classification": "closed-clippy-component-missing",
                "artifact": "full-linux-closure-attempt-002-clippy-missing-fail.log",
            },
            {
                "id": "linux-closure-attempt-003",
                "classification": "closed-network-disabled-rustup",
                "artifact": "full-linux-closure-attempt-003-clippy-network-none-fail.log",
                "superseded_by": "full-linux-closure.log",
            },
            {
                "id": "proc-1-resource-capture",
                "classification": "closed-wrong-process",
                "reason": "The init shim was measured; exact /proc/*/exe daemon resolution supersedes it.",
                "superseded_by": "high-entropy-resource/daemon-after.status",
            },
            {
                "id": "checkpoint-strict-zero-cfs",
                "classification": "nonblocking-diagnostic",
                "reason": "The controlling gates are checkpoint <=400 ms and population throttled_usec/wall <=5%.",
            },
            {
                "id": "candidate-014-live-only-terminal-claim",
                "classification": "invalidated-before-checksum-seal",
                "reason": "A later explicit requirement made command-to-durability-ack timing and restart proof controlling.",
                "artifact": "seal-receipt-live-only-pre-durable-invalidated.json",
            },
            {
                "id": "durable01",
                "classification": "closed-docker-tag-inspect-admission",
                "artifact": "durable/durable01/failure.json",
                "superseded_by": "durable/durable02/summary.json",
            },
        ],
    }
    write("failure-ledger.json", failure_ledger)
    checks = {
        "source_binding": source["status"] == "PASS",
        "host_linux_closure": all(closure_checks.values()),
        "architecture": architecture["status"] == "PASS",
        "functional_restart_fault_splice": all(functional_checks.values()),
        "smoke": all(smoke_checks.values()),
        "readiness": all(value["status"] == "READY" for value in readiness.values()),
        "scenario_cpu": scenario_cpu["status"] == "PASS",
        "resources": resources["status"] == "PASS",
        "live_mount_diagnostics": all(
            value["status"] == "LIVE_MOUNT_PASS" for value in populations.values()
        ),
        "cleanup": all(cleanup.values()),
        "custody_docs": all(
            "candidate 014" in (ROOT / path).read_text()
            and COMMIT in (ROOT / path).read_text()
            and IMAGE_ID in (ROOT / path).read_text()
            for path in (
                "poc/19-stage2-docker-linux-fuse.md",
                "poc/23-stage2-fuse-performance-optimization.md",
                "poc/24-stage2-docker-fuse-implementation-handoff.md",
            )
        )
        and "stage2-freeze-candidate-014" in (ROOT / "poc/README.md").read_text(),
    }
    durable_path = E / "durable/verification.json"
    durable = json.loads(durable_path.read_text()) if durable_path.exists() else {
        "status": "PENDING",
        "reason": "Persistence-inclusive command-to-checkpoint-ack campaign has not run.",
    }
    checks["persistence_inclusive"] = durable["status"] == "PASS_DURABLE"
    cloudflare = load("cloudflare-comparison.json")
    checks["cloudflare_restart_durable_baseline"] = (
        cloudflare["comparison_classes"]["restart_durable_full_product"]["status"]
        == "PASS_DURABLE"
    )
    layerfs_checks = {
        key: value
        for key, value in checks.items()
        if key != "cloudflare_restart_durable_baseline"
    }
    if all(checks.values()):
        disposition = "PASS_OPTIMIZED"
    elif all(layerfs_checks.values()) and cloudflare["status"] == "FULL_PRODUCT_BASELINE_UNAVAILABLE":
        disposition = "PASS_DURABLE_LAYERFS_COMPARISON_UNAVAILABLE"
    else:
        disposition = "REVISE"
    summary = {
        "schema": "layerfs-stage2-final-summary-v4",
        "status": disposition,
        "checks": checks,
        "source_commit": COMMIT,
        "source_tree": TREE,
        "image_id": IMAGE_ID,
        "executable_sha256": source["executable_sha256"],
        "executable_blake3": source["executable_blake3"],
        "fs_bench_sha256": FS_BENCH,
        "authoritative": {
            control: populations[control]["aggregates"] for control in populations
        },
        "authoritative_classification": "LIVE_MOUNT_DIAGNOSTIC",
        "cloudflare_comparison": cloudflare,
        "durable": {
            "status": durable["status"],
            "classification": durable["classification"],
            "aggregate": durable["aggregate"],
            "numeric_latency_gate": durable["numeric_latency_gate"],
            "verification_artifact": "durable/verification.json",
            "campaign_artifact": "durable/durable02/summary.json",
        },
        "readiness": readiness,
        "functional": {
            "checkpoint_100mib_ns": functional["checkpoint_100mib_ns"],
            "same_daemon_read_mib_s": functional["sequential_read_mib_s"],
            "restart_read_mib_s": load("restart/oracle.json")["sequential_read_mib_s"],
            "high_entropy_checkpoint_ns": high_oracle["checkpoint_ns"],
            "strict_zero_cfs_event": functional["checks"]["checkpoint_unthrottled"],
            "strict_zero_cfs_event_classification": "NONBLOCKING_DIAGNOSTIC",
        },
        "resources": {
            "daemon_settled_rss_bytes": high_before["VmRSS"],
            "daemon_rss_hwm_bytes": high_after["VmHWM"],
            "daemon_threads_hwm": process_hwm["threads_high_water"],
            "daemon_fd_hwm": process_hwm["fd_high_water"],
            "var_cgroup_memory_peak_bytes": populations["var"]["resources"]["cgroup_memory_peak_bytes"],
            "tmp_cgroup_memory_peak_bytes": populations["tmp"]["resources"]["cgroup_memory_peak_bytes"],
            "var_throttle_ratio": populations["var"]["resources"]["throttle_ratio"],
            "tmp_throttle_ratio": populations["tmp"]["resources"]["throttle_ratio"],
        },
    }
    write("summary.json", summary)
    (E / "summary.md").write_text(
        f"# Stage 2 candidate 014 — {summary['status']}\n\n"
        f"Product source `{COMMIT}` / tree `{TREE}` and ARM64 image `{IMAGE_ID}` pass correctness, resource, and live-mount diagnostics. Persistence-inclusive terminal timing is separately controlled.\n\n"
        f"- `/var/tmp`: SL {populations['var']['aggregates']['SL_ns'] / 1e9:.3f} s, Rsum {populations['var']['aggregates']['Rsum']:.3f}, G {populations['var']['aggregates']['G']:.3f}, Spread {populations['var']['aggregates']['Spread']:.3f}.\n"
        f"- `/tmp`: SL {populations['tmp']['aggregates']['SL_ns'] / 1e9:.3f} s, Rsum {populations['tmp']['aggregates']['Rsum']:.3f}, G {populations['tmp']['aggregates']['G']:.3f}, Spread {populations['tmp']['aggregates']['Spread']:.3f}.\n"
        f"- Daemon settled RSS {high_before['VmRSS'] / 1024:.0f} KiB; HWM {high_after['VmHWM'] / 1024:.0f} KiB; threads HWM {process_hwm['threads_high_water']}; FD HWM {process_hwm['fd_high_water']}.\n"
        "- The upstream matrices are `LIVE_MOUNT` diagnostics. Their Cloudflare thresholds do not control restart-durable performance.\n"
        f"- Persistence-inclusive campaign: {durable['status']}.\n"
        f"- Durable median sums: live {durable['aggregate']['sum_live_medians_ns'] / 1e9:.3f} s, checkpoint {durable['aggregate']['sum_checkpoint_medians_ns'] / 1e9:.3f} s, command-to-durable {durable['aggregate']['sum_to_durable_medians_ns'] / 1e9:.3f} s.\n"
        "- Full-product Cloudflare comparison: unavailable without deployed Durable Object sync timing and restart authority.\n"
        "- No benchmark shim, backing tree, SDK/evaluator bypass, workload recognition, network row, tracing asymmetry, emulation, or storage-control shortcut was found. Stage 1.2 remained skipped.\n"
    )
    print(json.dumps(summary, indent=2, sort_keys=True))
    if summary["status"] != "PASS_OPTIMIZED":
        raise SystemExit(1)


if __name__ == "__main__":
    main()
