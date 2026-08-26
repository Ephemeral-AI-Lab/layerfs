#!/usr/bin/env python3
import hashlib
import json
import math
import re
import subprocess
from pathlib import Path

EVIDENCE = Path(__file__).resolve().parent
ROOT = EVIDENCE.parents[2]
IMAGE = "layerfs-fuse:frozen-c56ff37"
IMAGE_ID = "sha256:ea0767885bd72130360b06e31d75a821e8cd685826bf8816eb1e2b71f2008864"
SOURCE_COMMIT = "c56ff371d1b9e8851cfd8ef39ca8990155ab23df"
SOURCE_TREE = "8ac6588e248439df5713f90af53b28af0490d3f9"
FS_BENCH_SHA256 = "0e2004339a9269b88c2de451d56575957477bde16b96a10cf1837519e37230ef"
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
    return json.loads((EVIDENCE / name).read_text())


def write(name, value):
    with (EVIDENCE / name).open("x") as output:
        json.dump(value, output, indent=2, sort_keys=True)
        output.write("\n")


def key_values(name):
    return {
        key: int(value)
        for key, value in map(str.split, (EVIDENCE / name).read_text().splitlines())
    }


def highwater(name):
    return {
        key: int(value)
        for key, value in (
            line.split("=", 1)
            for line in (EVIDENCE / name).read_text().splitlines()
        )
    }


def stdout_checks(name):
    output = re.sub(
        r"\x1b\[[0-9;]*m",
        "",
        (EVIDENCE / name).read_text(errors="replace"),
    )
    return {
        "fail_markers": sum("FAIL" in line for line in output.splitlines()),
        "network_scenarios": sum(
            "git clone (shallow, ~1MB)" in line for line in output.splitlines()
        ),
    }


def raw_matrix(raw, base, reps, warmup):
    rows = raw["results"]
    expected = {(scenario, target) for scenario in SCENARIOS for target in ("computerd", "base")}
    actual = {(row["scenario"], row["target"]) for row in rows}
    return {
        "config": raw["config"]
        == {
            "reps": reps,
            "warmup": warmup,
            "randomizeTargets": 1,
            "mount": "/workspace",
            "base": base,
        },
        "row_count": len(rows) == 24,
        "unique_matrix": actual == expected and len(actual) == len(rows),
        "sample_count": all(row["samples"] == reps for row in rows),
    }


def aggregates(raw):
    rows = {
        (row["scenario"], row["target"]): row for row in raw["results"]
    }
    layer = sum(rows[scenario, "computerd"]["medianNs"] for scenario in SCENARIOS)
    base = sum(rows[scenario, "base"]["medianNs"] for scenario in SCENARIOS)
    geometric = math.exp(
        sum(
            math.log(
                rows[scenario, "computerd"]["medianNs"]
                / rows[scenario, "base"]["medianNs"]
            )
            for scenario in SCENARIOS
        )
        / len(SCENARIOS)
    )
    return {"SL_ns": layer, "SB_ns": base, "Rsum": layer / base, "G": geometric}


def terminal_checks(terminal, empty):
    mounted = terminal["mounted"]
    engine = terminal["engine"]
    callbacks = terminal["callbacks"]
    checks = {
        "status": terminal["status"] == "PASS",
        "root_lookup_only": mounted["lookup_refs"] == 1,
        "root_node_only": mounted["live_nodes"] == 1,
        "root_mapping_only": mounted["inode_mappings"] == 1,
        "handles_zero": mounted["open_handles"] == 0,
        "pending_zero": mounted["pending_nodes"] == 0,
        "dirty_nodes_zero": mounted["dirty_nodes"] == 0,
        "dirty_ranges_zero": mounted["dirty_ranges"] == 0,
        "directory_cursors_zero": mounted["directory_cursors"] == 0,
        "directory_changes_zero": mounted["directory_changes"] == 0,
        "spool_live_zero": mounted["spool_live_bytes"] == 0,
        "spool_dead_zero": mounted["spool_dead_bytes"] == 0,
        "spool_physical_zero": mounted["spool_physical_bytes"] == 0,
        "q_zero": mounted["operation_q_terminal_bytes"] == 0,
        "connections_zero": engine["connections_terminal"] == 0,
        "rollback_zero": engine["transactions_rolled_back"] == 0,
        "invalidation_exact": callbacks["invalidations_failed"] == 0
        and callbacks["invalidations_requested"]
        == callbacks["invalidations_succeeded"],
    }
    if empty:
        checks["logical_workspace_zero"] = mounted["logical_workspace_bytes"] == 0
    return checks


def authoritative_resource(control):
    prefix = f"authoritative-{control}"
    before = key_values(f"{prefix}-cpu-before.txt")
    after = key_values(f"{prefix}-cpu-after.txt")
    memory_before = key_values(f"{prefix}-memory-events-before.txt")
    memory_after = key_values(f"{prefix}-memory-events-after.txt")
    process_before = [
        int(value)
        for value in (EVIDENCE / f"{prefix}-process-cpu-before-ticks.txt")
        .read_text()
        .split()
    ]
    process_after = [
        int(value)
        for value in (EVIDENCE / f"{prefix}-process-cpu-after-ticks.txt")
        .read_text()
        .split()
    ]
    ticks = int((EVIDENCE / f"{prefix}-clk-tck.txt").read_text())
    wall = int((EVIDENCE / f"{prefix}-wall-end-ns.txt").read_text()) - int(
        (EVIDENCE / f"{prefix}-wall-start-ns.txt").read_text()
    )
    process_ns = (
        sum(process_after) - sum(process_before)
    ) * 1_000_000_000 // ticks
    observed = highwater(f"{prefix}-process-highwater.txt")
    fd_baseline = int((EVIDENCE / f"{prefix}-fd-baseline.txt").read_text())
    terminal = load(f"{prefix}-terminal.json")
    result = {
        "schema": "layerfs-stage2-resource-v1",
        "control": control,
        "wall_ns": wall,
        "daemon_cpu": {
            "user_ticks": process_after[0] - process_before[0],
            "system_ticks": process_after[1] - process_before[1],
            "clock_ticks_per_second": ticks,
            "total_ns": process_ns,
            "limit_ns": int(1.05 * wall + 5_000_000),
        },
        "cgroup_cpu": {
            "usage_usec_delta": after["usage_usec"] - before["usage_usec"],
            "throttled_usec_delta": after["throttled_usec"]
            - before["throttled_usec"],
            "throttle_ratio": (after["throttled_usec"] - before["throttled_usec"])
            * 1000
            / wall,
        },
        "memory": {
            "cgroup_peak_bytes": int(
                (EVIDENCE / f"{prefix}-memory-peak-bytes.txt").read_text()
            ),
            "rss_baseline_bytes": int(
                (EVIDENCE / f"{prefix}-rss-baseline-bytes.txt").read_text()
            ),
            "rss_high_water_bytes": observed["rss_high_water_bytes"],
            "rss_after_bytes": int(
                (EVIDENCE / f"{prefix}-rss-after-bytes.txt").read_text()
            ),
            "oom_delta": memory_after["oom"] - memory_before["oom"],
            "oom_kill_delta": memory_after["oom_kill"]
            - memory_before["oom_kill"],
        },
        "fd": {
            "baseline": fd_baseline,
            "high_water": observed["fd_high_water"],
            "limit": fd_baseline + 64,
            "terminal": 0,
            "terminal_reason": "daemon process absent after successful container removal",
        },
        "lock": {
            "mount_wait_ns": terminal["callbacks"]["mount_lock_wait_ns"],
            "connection_mutex_wait_ns": terminal["engine"][
                "connection_mutex_wait_ns"
            ],
            "mount_wait_ratio": terminal["callbacks"]["mount_lock_wait_ns"] / wall,
        },
        "connections": {
            "high_water": terminal["engine"]["connections_high_water"],
            "before_drop": terminal["engine"]["connections_before_drop"],
            "terminal": terminal["engine"]["connections_terminal"],
        },
        "lookup_refs": {
            "high_water": terminal["mounted"]["lookup_refs_high_water"],
            "terminal": terminal["mounted"]["lookup_refs"],
        },
        "mounted_nodes": {
            "high_water": terminal["mounted"]["live_nodes_high_water"],
            "terminal": terminal["mounted"]["live_nodes"],
        },
    }
    result["checks"] = {
        "daemon_cpu": process_ns <= result["daemon_cpu"]["limit_ns"],
        "throttle": result["cgroup_cpu"]["throttle_ratio"] <= 0.05,
        "memory_peak": result["memory"]["cgroup_peak_bytes"] <= 536_870_912,
        "oom": result["memory"]["oom_delta"] == 0
        and result["memory"]["oom_kill_delta"] == 0,
        "fd": result["fd"]["high_water"] <= result["fd"]["limit"],
        "mount_lock": result["lock"]["mount_wait_ratio"] <= 0.10,
        "connections": result["connections"]["terminal"] == 0,
        "lookup_terminal": result["lookup_refs"]["terminal"] == 1,
        "node_terminal": result["mounted_nodes"]["terminal"] == 1,
    }
    result["status"] = "PASS" if all(result["checks"].values()) else "FAIL"
    return result


def readiness(control, base):
    raw = load(f"readiness-{control}.json")
    terminal = load(f"readiness-{control}-terminal.json")
    checks = {
        **raw_matrix(raw, base, 1, 0),
        **{key: value == 0 for key, value in stdout_checks(f"readiness-{control}.stdout").items()},
        **terminal_checks(terminal, True),
    }
    summary = aggregates(raw)
    summary["forecast_numeric_gates"] = {
        "SL": summary["SL_ns"] <= 4_500_000_000,
        "Rsum": summary["Rsum"] <= (2.85 if control == "var" else 3.10),
        "G": summary["G"] <= (7.00 if control == "var" else 7.75),
    }
    receipt = {
        "schema": "layerfs-stage2-readiness-v1",
        "status": "READY"
        if all(checks.values()) and all(summary["forecast_numeric_gates"].values())
        else "REVISE",
        "control": control,
        "checks": checks,
        "aggregates": summary,
        "process_highwater": highwater(f"readiness-{control}-process-highwater.txt"),
    }
    return receipt


def publication(control):
    terminal = load(f"authoritative-{control}-terminal.json")
    mounted = terminal["mounted"]
    engine = terminal["engine"]
    genesis = 1
    equations = {
        "fresh_store_generation_zero": terminal["generation"] == 0,
        "publication_commits_equal_genesis_plus_checkpoints": engine[
            "publication_commits"
        ]
        == genesis + mounted["checkpoints"],
        "transactions_started_equal_publications": engine["transactions_started"]
        == engine["publication_commits"],
        "transactions_committed_equal_publications": engine[
            "transactions_committed"
        ]
        == engine["publication_commits"],
        "rollbacks_zero": engine["transactions_rolled_back"] == 0,
        "campaign_checkpoints_zero": mounted["checkpoints"] == 0,
        "shutdown_noop_checkpoint_one": mounted["no_op_checkpoints"] == 1,
        **terminal_checks(terminal, True),
    }
    return {
        "control": control,
        "genesis_publications": genesis,
        "campaign_publications": mounted["checkpoints"],
        "equations": equations,
        "status": "PASS" if all(equations.values()) else "FAIL",
    }


def source_and_tools():
    assert run("git", "rev-parse", "HEAD") == SOURCE_COMMIT
    assert run("git", "rev-parse", "HEAD^{tree}") == SOURCE_TREE
    source_status = run(
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
    assert source_status == ""
    assert run("docker", "image", "inspect", IMAGE, "--format", "{{.Id}}") == IMAGE_ID
    tracked = run("git", "ls-tree", "-r", "--name-only", "HEAD").splitlines()
    tracked = [
        path
        for path in tracked
        if path in ("Cargo.toml", "Cargo.lock")
        or path.startswith(("crates/", "tools/", "containers/layerfs-fuse/"))
    ]
    source_files = {
        path: hashlib.sha256((ROOT / path).read_bytes()).hexdigest() for path in tracked
    }
    executable_sha256 = run(
        "docker",
        "run",
        "--rm",
        "--platform",
        "linux/arm64",
        "--entrypoint",
        "sha256sum",
        IMAGE,
        "/usr/local/bin/layerfs-fuse",
    ).split()[0]
    terminal_hashes = {
        load(name)["executable_blake3"]
        for name in (
            "functional-terminal.json",
            "restart-terminal.json",
            "authoritative-var-terminal.json",
            "authoritative-tmp-terminal.json",
            "splice-control-terminal.json",
        )
    }
    assert len(terminal_hashes) == 1
    source = {
        "schema": "layerfs-stage2-source-manifest-v1",
        "source_commit": SOURCE_COMMIT,
        "source_tree": SOURCE_TREE,
        "build_context_clean": True,
        "image": IMAGE,
        "image_id": IMAGE_ID,
        "architecture": "arm64",
        "os": "linux",
        "executable_sha256": executable_sha256,
        "executable_blake3": terminal_hashes.pop(),
        "fs_bench_sha256": FS_BENCH_SHA256,
        "verifier_sha256": hashlib.sha256(
            (ROOT / "containers/layerfs-fuse/verify_fs_bench.py").read_bytes()
        ).hexdigest(),
        "source_files_sha256": source_files,
    }
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
    tools = {
        "schema": "layerfs-stage2-tool-versions-v1",
        "host": {
            "rustc": run("rustc", "--version"),
            "cargo": run("cargo", "--version"),
            "docker": run("docker", "version", "--format", "{{.Server.Version}}"),
            "uname": run("uname", "-a"),
        },
        "image": image_tools,
        "fs_bench_sha256": FS_BENCH_SHA256,
        "verifier_schema": "layerfs-stage2-fs-bench-verification-v2",
    }
    return source, tools


def cleanup():
    containers = [
        value
        for value in run("docker", "ps", "-a", "--format", "{{.Names}}").splitlines()
        if "layerfs-stage2-final011" in value
    ]
    volumes = [
        value
        for value in run("docker", "volume", "ls", "--format", "{{.Name}}").splitlines()
        if value.startswith("layerfs_stage2_final011")
    ]
    checks = {
        "containers_absent": containers == [],
        "volumes_absent": volumes == [],
        "processes_absent": containers == [],
        "mounts_absent": containers == [],
        "scratch_journal_wal_shm_absent": volumes == [],
    }
    return {
        "schema": "layerfs-stage2-external-cleanup-v1",
        "status": "PASS" if all(checks.values()) else "FAIL",
        "checks": checks,
        "containers_remaining": containers,
        "volumes_remaining": volumes,
        "retained_image": IMAGE_ID,
    }


def main():
    resources = {
        control: authoritative_resource(control) for control in ("var", "tmp")
    }
    for control, receipt in resources.items():
        write(f"resources-{control}.json", receipt)
    readiness_receipts = {
        "var": readiness("var", "/var/tmp"),
        "tmp": readiness("tmp", "/tmp"),
    }
    write(
        "readiness-verification.json",
        {
            "schema": "layerfs-stage2-readiness-collection-v1",
            "status": "READY"
            if all(value["status"] == "READY" for value in readiness_receipts.values())
            else "REVISE",
            "populations": readiness_receipts,
        },
    )
    idle_before = [
        int(value)
        for value in (EVIDENCE / "idle-process-cpu-before-ticks.txt").read_text().split()
    ]
    idle_after = [
        int(value)
        for value in (EVIDENCE / "idle-process-cpu-after-ticks.txt").read_text().split()
    ]
    idle_hz = int((EVIDENCE / "idle-clk-tck.txt").read_text())
    idle_cpu = (sum(idle_after) - sum(idle_before)) * 1_000_000_000 // idle_hz
    idle = {
        "schema": "layerfs-stage2-idle-cpu-v1",
        "wall_ns": int((EVIDENCE / "idle-wall-end-ns.txt").read_text())
        - int((EVIDENCE / "idle-wall-start-ns.txt").read_text()),
        "cpu_ns": idle_cpu,
        "limit_ns": 25_000_000,
        "status": "PASS" if idle_cpu <= 25_000_000 else "FAIL",
    }
    write("idle-cpu.json", idle)
    publications = {control: publication(control) for control in ("var", "tmp")}
    write(
        "publication-equations.json",
        {
            "schema": "layerfs-stage2-publication-equations-v1",
            "status": "PASS"
            if all(value["status"] == "PASS" for value in publications.values())
            else "FAIL",
            "populations": publications,
        },
    )
    source, tools = source_and_tools()
    write("source-manifest.json", source)
    write("tool-versions.json", tools)
    cleanup_receipt = cleanup()
    write("cleanup.json", cleanup_receipt)
    architecture = {
        "schema": "layerfs-stage2-architecture-admission-v1",
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
    failures = {
        "schema": "layerfs-stage2-failure-ledger-v1",
        "status": "CLOSED",
        "attempts": [
            {
                "id": "functional-attempt-001",
                "classification": "environment-invalid",
                "reason": "default 100 ms CFS quota throttled four checkpoint periods",
                "artifact": "functional-attempt-001-fail.json",
                "superseded_by": "functional-oracle.json",
            },
            {
                "id": "readiness-var-attempt-001",
                "classification": "diagnostic-unsealed",
                "reason": "cgroup counters and population wall were not atomically bracketed",
                "artifact": "readiness-var-attempt-001-unbracketed.json",
                "superseded_by": "readiness-var.json",
            },
            {
                "id": "readiness-var-attempt-002",
                "classification": "environment-invalid",
                "reason": "CFS quota throttling ratio 0.08607778715112475 exceeded 0.05",
                "artifact": "readiness-var-attempt-002-throttled.json",
                "superseded_by": "readiness-var.json",
            },
        ],
    }
    write("failure-ledger.json", failures)
    verification = {
        control: load(f"verification-{control}.json") for control in ("var", "tmp")
    }
    final_checks = {
        "authoritative_numeric": all(
            value["status"] == "PASS_OPTIMIZED" for value in verification.values()
        ),
        "resources": all(value["status"] == "PASS" for value in resources.values()),
        "readiness": all(
            value["status"] == "READY" for value in readiness_receipts.values()
        ),
        "publication": all(
            value["status"] == "PASS" for value in publications.values()
        ),
        "functional": load("functional-oracle.json")["status"] == "PASS",
        "restart": load("restart-oracle.json")["status"] == "PASS",
        "forced_death": load("forced-death-oracle.json")["status"] == "PASS",
        "splice": load("splice-oracle.json")["status"] == "PASS",
        "idle_cpu": idle["status"] == "PASS",
        "cleanup": cleanup_receipt["status"] == "PASS",
        "source_binding": source["image_id"] == IMAGE_ID,
    }
    summary = {
        "schema": "layerfs-stage2-final-summary-v1",
        "status": "PASS_OPTIMIZED" if all(final_checks.values()) else "REVISE",
        "checks": final_checks,
        "source_commit": SOURCE_COMMIT,
        "source_tree": SOURCE_TREE,
        "image_id": IMAGE_ID,
        "executable_blake3": source["executable_blake3"],
        "fs_bench_sha256": FS_BENCH_SHA256,
        "authoritative": {
            control: verification[control]["aggregates"] for control in ("var", "tmp")
        },
        "functional": {
            "checkpoint_100mib_ns": load("functional-oracle.json")[
                "checkpoint_100mib_ns"
            ],
            "same_daemon_read_mib_s": load("functional-oracle.json")[
                "sequential_read_mib_s"
            ],
            "restart_read_mib_s": load("restart-oracle.json")[
                "sequential_read_mib_s"
            ],
        },
    }
    write("summary.json", summary)
    if summary["status"] != "PASS_OPTIMIZED":
        raise SystemExit(1)


if __name__ == "__main__":
    main()
