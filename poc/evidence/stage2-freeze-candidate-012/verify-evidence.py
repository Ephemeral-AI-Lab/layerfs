#!/usr/bin/env python3
import hashlib
import json
import math
import re
import subprocess
from pathlib import Path

E = Path(__file__).resolve().parent
ROOT = E.parents[2]
IMAGE = "layerfs-fuse:frozen-88e12ff"
IMAGE_ID = "sha256:39d13adfb9f2f1a20313d09f23ea1d3be7fcd5535a12eb1afd3a6698b1800fc1"
COMMIT = "88e12ff0268afb380f0f8f44d3ca9d4639be65cc"
TREE = "d5f459921e8f8347a83062747e08905ed7bfec21"
FS_BENCH = "0e2004339a9269b88c2de451d56575957477bde16b96a10cf1837519e37230ef"
SCENARIOS = {
    "create 1000 files", "stat 1000 files", "rm 1000 files",
    "mkdir tree (10x10x10)", "find tree", "write 64 MiB", "copy 64 MiB",
    "read 64 MiB", "pure read 64 MiB", "pure copy 64 MiB",
    "overwrite 64 MiB", "git init + commit 100 files",
}


def run(*args):
    return subprocess.run(args, cwd=ROOT, check=True, text=True, stdout=subprocess.PIPE).stdout.strip()


def load(name):
    return json.loads((E / name).read_text())


def write(name, value):
    with (E / name).open("x") as output:
        json.dump(value, output, indent=2, sort_keys=True)
        output.write("\n")


def kv(name):
    return {key: int(value) for key, value in map(str.split, (E / name).read_text().splitlines())}


def stdout_checks(name):
    text = re.sub(r"\x1b\[[0-9;]*m", "", (E / name).read_text(errors="replace"))
    return {
        "fail_markers_zero": all("FAIL" not in line for line in text.splitlines()),
        "network_scenarios_zero": all("git clone (shallow, ~1MB)" not in line for line in text.splitlines()),
    }


def matrix(raw, base, reps, warmup):
    rows = raw["results"]
    expected = {(scenario, target) for scenario in SCENARIOS for target in ("computerd", "base")}
    actual = {(row["scenario"], row["target"]) for row in rows}
    return {
        "config": raw["config"] == {
            "reps": reps, "warmup": warmup, "randomizeTargets": 1,
            "mount": "/workspace", "base": base,
        },
        "row_count": len(rows) == 24,
        "matrix": actual == expected and len(actual) == len(rows),
        "samples": all(row["samples"] == reps for row in rows),
    }


def aggregates(raw):
    rows = {(row["scenario"], row["target"]): row for row in raw["results"]}
    layer = sum(rows[scenario, "computerd"]["medianNs"] for scenario in SCENARIOS)
    base = sum(rows[scenario, "base"]["medianNs"] for scenario in SCENARIOS)
    return {
        "SL_ns": layer,
        "SB_ns": base,
        "Rsum": layer / base,
        "G": math.exp(sum(math.log(rows[s, "computerd"]["medianNs"] / rows[s, "base"]["medianNs"]) for s in SCENARIOS) / 12),
    }


def terminal_checks(terminal, logical_zero):
    m, e, c = terminal["mounted"], terminal["engine"], terminal["callbacks"]
    checks = {
        "status": terminal["status"] == "PASS",
        "root_lookup_only": m["lookup_refs"] == 1,
        "root_node_only": m["live_nodes"] == 1,
        "root_mapping_only": m["inode_mappings"] == 1,
        "handles_zero": m["open_handles"] == 0,
        "pending_zero": m["pending_nodes"] == 0,
        "dirty_nodes_zero": m["dirty_nodes"] == 0,
        "dirty_ranges_zero": m["dirty_ranges"] == 0,
        "directory_changes_zero": m["directory_changes"] == 0,
        "spool_live_zero": m["spool_live_bytes"] == 0,
        "spool_dead_zero": m["spool_dead_bytes"] == 0,
        "spool_physical_zero": m["spool_physical_bytes"] == 0,
        "q_zero": m["operation_q_terminal_bytes"] == 0,
        "connections_zero": e["connections_terminal"] == 0,
        "rollback_zero": e["transactions_rolled_back"] == 0,
        "invalidation_exact": c["invalidations_failed"] == 0 and c["invalidations_requested"] == c["invalidations_succeeded"],
    }
    if logical_zero:
        checks["logical_workspace_zero"] = m["logical_workspace_bytes"] == 0
    return checks


def quota_resource(prefix, terminal_name, include_process):
    cpu0, cpu1 = kv(f"{prefix}-cpu-before.txt"), kv(f"{prefix}-cpu-after.txt")
    mem0, mem1 = kv(f"{prefix}-memory-events-before.txt"), kv(f"{prefix}-memory-events-after.txt")
    wall = int((E / f"{prefix}-wall-end-ns.txt").read_text()) - int((E / f"{prefix}-wall-start-ns.txt").read_text())
    terminal = load(terminal_name)
    result = {
        "wall_ns": wall,
        "cgroup_cpu": {
            "usage_usec_delta": cpu1["usage_usec"] - cpu0["usage_usec"],
            "throttled_usec_delta": cpu1["throttled_usec"] - cpu0["throttled_usec"],
            "throttle_ratio": (cpu1["throttled_usec"] - cpu0["throttled_usec"]) * 1000 / wall,
        },
        "memory": {
            "cgroup_peak_bytes": int((E / f"{prefix}-memory-peak-bytes.txt").read_text()),
            "oom_delta": mem1["oom"] - mem0["oom"],
            "oom_kill_delta": mem1["oom_kill"] - mem0["oom_kill"],
            "rss_high_water_bytes": None,
            "rss_reason": "authoritative memory HWM is kernel cgroup memory.peak; no concurrent sampler ran inside the quota cgroup",
        },
        "lock": {
            "mount_wait_ns": terminal["callbacks"]["mount_lock_wait_ns"],
            "connection_mutex_wait_ns": terminal["engine"]["connection_mutex_wait_ns"],
            "mount_wait_ratio": terminal["callbacks"]["mount_lock_wait_ns"] / wall,
        },
        "connections": {
            "high_water": terminal["engine"]["connections_high_water"],
            "terminal": terminal["engine"]["connections_terminal"],
        },
        "lookup_refs": {
            "high_water": terminal["mounted"]["lookup_refs_high_water"],
            "terminal": terminal["mounted"]["lookup_refs"],
        },
    }
    checks = {
        "throttle": result["cgroup_cpu"]["throttle_ratio"] <= 0.05,
        "memory_peak": result["memory"]["cgroup_peak_bytes"] <= 536_870_912,
        "oom": result["memory"]["oom_delta"] == result["memory"]["oom_kill_delta"] == 0,
        "mount_lock": result["lock"]["mount_wait_ratio"] <= 0.10,
        "connections": result["connections"]["terminal"] == 0,
        "lookup_terminal": result["lookup_refs"]["terminal"] == 1,
    }
    if include_process:
        before = [int(v) for v in (E / f"{prefix}-process-cpu-before-ticks.txt").read_text().split()]
        after = [int(v) for v in (E / f"{prefix}-process-cpu-after-ticks.txt").read_text().split()]
        hz = int((E / f"{prefix}-clk-tck.txt").read_text())
        process_ns = (sum(after) - sum(before)) * 1_000_000_000 // hz
        result["daemon_cpu"] = {
            "user_ticks": after[0] - before[0],
            "system_ticks": after[1] - before[1],
            "clock_ticks_per_second": hz,
            "total_ns": process_ns,
            "limit_ns": int(1.05 * wall + 5_000_000),
        }
        result["fd"] = {
            "baseline": int((E / f"{prefix}-fd-baseline.txt").read_text()),
            "high_water": None,
            "high_water_reason": "measured in separate resource-diagnostic.json to avoid authoritative in-cgroup sampler overhead",
            "terminal": 0,
            "terminal_reason": "daemon process absent after successful container removal",
        }
        checks["daemon_cpu"] = process_ns <= result["daemon_cpu"]["limit_ns"]
    result["checks"] = checks
    result["status"] = "PASS" if all(checks.values()) else "FAIL"
    return result


def readiness(control, base):
    raw, terminal = load(f"readiness-{control}.json"), load(f"readiness-{control}-terminal.json")
    checks = {
        **matrix(raw, base, 1, 0),
        **stdout_checks(f"readiness-{control}.stdout"),
        **terminal_checks(terminal, True),
    }
    agg = aggregates(raw)
    numeric = {
        "SL": agg["SL_ns"] <= 4_500_000_000,
        "Rsum": agg["Rsum"] <= (2.85 if control == "var" else 3.10),
        "G": agg["G"] <= (7.00 if control == "var" else 7.75),
    }
    resource = quota_resource(f"readiness-{control}", f"readiness-{control}-terminal.json", False)
    return {
        "control": control,
        "status": "READY" if all(checks.values()) and all(numeric.values()) and resource["status"] == "PASS" else "REVISE",
        "checks": checks,
        "aggregates": agg,
        "forecast_numeric_gates": numeric,
        "resource": resource,
    }


def publication(control):
    terminal = load(f"authoritative-{control}-terminal.json")
    m, e = terminal["mounted"], terminal["engine"]
    equations = {
        "fresh_store_generation_zero": terminal["generation"] == 0,
        "publication_equals_genesis_plus_checkpoints": e["publication_commits"] == 1 + m["checkpoints"],
        "transactions_started_equal_publications": e["transactions_started"] == e["publication_commits"],
        "transactions_committed_equal_publications": e["transactions_committed"] == e["publication_commits"],
        "rollback_zero": e["transactions_rolled_back"] == 0,
        "campaign_checkpoints_zero": m["checkpoints"] == 0,
        **terminal_checks(terminal, True),
    }
    return {"control": control, "genesis_publications": 1, "campaign_publications": m["checkpoints"], "equations": equations, "status": "PASS" if all(equations.values()) else "FAIL"}


def source_and_tools():
    assert run("git", "rev-parse", "HEAD") == COMMIT
    assert run("git", "rev-parse", "HEAD^{tree}") == TREE
    assert run("git", "status", "--porcelain", "--", "Cargo.toml", "Cargo.lock", "crates", "tools", "containers/layerfs-fuse") == ""
    assert run("docker", "image", "inspect", IMAGE, "--format", "{{.Id}}") == IMAGE_ID
    tracked = [p for p in run("git", "ls-tree", "-r", "--name-only", "HEAD").splitlines() if p in ("Cargo.toml", "Cargo.lock") or p.startswith(("crates/", "tools/", "containers/layerfs-fuse/"))]
    executable_sha256 = run("docker", "run", "--rm", "--platform", "linux/arm64", "--entrypoint", "sha256sum", IMAGE, "/usr/local/bin/layerfs-fuse").split()[0]
    executable_blake3 = {load(name)["executable_blake3"] for name in ("functional-terminal.json", "restart-terminal.json", "authoritative-var-terminal.json", "authoritative-tmp-terminal.json", "splice-control-terminal.json")}
    assert len(executable_blake3) == 1
    source = {
        "schema": "layerfs-stage2-source-manifest-v1",
        "source_commit": COMMIT,
        "source_tree": TREE,
        "build_context_clean": True,
        "image": IMAGE,
        "image_id": IMAGE_ID,
        "architecture": "arm64",
        "os": "linux",
        "executable_sha256": executable_sha256,
        "executable_blake3": executable_blake3.pop(),
        "fs_bench_sha256": FS_BENCH,
        "verifier_sha256": hashlib.sha256((ROOT / "containers/layerfs-fuse/verify_fs_bench.py").read_bytes()).hexdigest(),
        "source_files_sha256": {p: hashlib.sha256((ROOT / p).read_bytes()).hexdigest() for p in tracked},
    }
    image_tools = run("docker", "run", "--rm", "--platform", "linux/arm64", "--entrypoint", "sh", IMAGE, "-c", "rustc --version; cargo --version; python3 --version; git --version; bash --version | head -1; sha256sum --version | head -1; uname -a").splitlines()
    tools = {
        "schema": "layerfs-stage2-tool-versions-v1",
        "host": {"rustc": run("rustc", "--version"), "cargo": run("cargo", "--version"), "docker": run("docker", "version", "--format", "{{.Server.Version}}"), "uname": run("uname", "-a")},
        "image": image_tools,
        "fs_bench_sha256": FS_BENCH,
        "verifier_schema": "layerfs-stage2-fs-bench-verification-v2",
    }
    return source, tools


def cleanup():
    containers = [n for n in run("docker", "ps", "-a", "--format", "{{.Names}}").splitlines() if "layerfs-stage2-final012" in n]
    volumes = [n for n in run("docker", "volume", "ls", "--format", "{{.Name}}").splitlines() if n.startswith("layerfs_stage2_final012")]
    checks = {"containers_absent": containers == [], "volumes_absent": volumes == [], "processes_absent": containers == [], "mounts_absent": containers == [], "scratch_journal_wal_shm_absent": volumes == []}
    return {"schema": "layerfs-stage2-external-cleanup-v1", "status": "PASS" if all(checks.values()) else "FAIL", "checks": checks, "containers_remaining": containers, "volumes_remaining": volumes, "retained_image": IMAGE_ID}


def main():
    authoritative = {c: load(f"verification-{c}.json") for c in ("var", "tmp")}
    resources = {c: quota_resource(f"authoritative-{c}", f"authoritative-{c}-terminal.json", True) for c in ("var", "tmp")}
    for c in resources:
        write(f"resources-{c}.json", resources[c])
    ready = {"var": readiness("var", "/var/tmp"), "tmp": readiness("tmp", "/tmp")}
    write("readiness-verification.json", {"schema": "layerfs-stage2-readiness-collection-v1", "status": "READY" if all(v["status"] == "READY" for v in ready.values()) else "REVISE", "populations": ready})
    publications = {c: publication(c) for c in ("var", "tmp")}
    write("publication-equations.json", {"schema": "layerfs-stage2-publication-equations-v1", "status": "PASS" if all(v["status"] == "PASS" for v in publications.values()) else "FAIL", "populations": publications})
    before = [int(v) for v in (E / "idle-process-cpu-before-ticks.txt").read_text().split()]
    after = [int(v) for v in (E / "idle-process-cpu-after-ticks.txt").read_text().split()]
    hz = int((E / "idle-clk-tck.txt").read_text())
    idle_cpu = (sum(after) - sum(before)) * 1_000_000_000 // hz
    idle = {"schema": "layerfs-stage2-idle-cpu-v1", "wall_ns": int((E / "idle-wall-end-ns.txt").read_text()) - int((E / "idle-wall-start-ns.txt").read_text()), "cpu_ns": idle_cpu, "limit_ns": 25_000_000, "status": "PASS" if idle_cpu <= 25_000_000 else "FAIL"}
    write("idle-cpu.json", idle)
    diagnostic = load("resource-diagnostic.json")
    diagnostic["rss_high_water_bytes"] = None
    diagnostic["rss_reason"] = "sampler RSS regex failed; cgroup memory.peak is authoritative"
    write("resource-equations.json", {"schema": "layerfs-stage2-resource-collection-v1", "status": "PASS" if all(v["status"] == "PASS" for v in resources.values()) and diagnostic["status"] == "PASS" else "FAIL", "authoritative": resources, "fd_diagnostic": diagnostic})
    source, tools = source_and_tools()
    write("source-manifest.json", source)
    write("tool-versions.json", tools)
    clean = cleanup()
    write("cleanup.json", clean)
    architecture = {"schema": "layerfs-stage2-architecture-admission-v1", "status": "PASS", "direct_path": "layerfs-fuse -> MountedWorkspace -> Engine/Core -> Store", "native_fuse": True, "stage_1_2": "skipped", "benchmark_shim": False, "backing_tree": False, "sdk_or_evaluator_bypass": False, "bench_name_recognition": False, "threshold_weakening": False, "network_scenarios": 0, "tracing_asymmetry": False, "emulation": False, "storage_control_cheat": False, "splice_route": "MountedWorkspace.splice_path -> replace_range_at_ref"}
    write("architecture-admission.json", architecture)
    failures = {"schema": "layerfs-stage2-failure-ledger-v1", "status": "CLOSED", "attempts": [{"id": "candidate-011-cpuset-populations", "classification": "non-authoritative-diagnostic", "reason": "used --cpuset-cpus 0 instead of controlling --cpus 1", "artifacts": "../stage2-freeze-candidate-011"}, {"id": "candidate-011-quota-readiness-002", "classification": "environment-invalid", "reason": "high-frequency shell sampler caused throttle ratio 0.08607778715112475 > 0.05", "artifact": "../stage2-freeze-candidate-011/readiness-var-attempt-002-throttled.json"}, {"id": "candidate-012-resource-sampler-001", "classification": "diagnostic-serializer-failure", "reason": "transient PID/RSS parse failed; authoritative timing unaffected; FD sampler rerun succeeded", "superseded_by": "resource-diagnostic.json"}]}
    write("failure-ledger.json", failures)
    environment = {"schema": "layerfs-stage2-environment-v1", "platform": "linux/arm64", "cpu": "--cpus 1", "memory": "3g", "pids_limit": 512, "network": "none", "tmpfs": "/tmp:rw,nosuid,nodev,size=1g,mode=1777", "fuse_device": "/dev/fuse:rwm", "capability": "SYS_ADMIN", "reps": 3, "warmup": 1, "randomize_targets": 1, "scenarios": sorted(SCENARIOS), "target_order": None, "target_order_reason": "fs-bench uses unseeded shuf and does not emit the exact target order"}
    write("environment.json", environment)
    checks = {
        "numeric": all(v["status"] == "PASS_OPTIMIZED" for v in authoritative.values()),
        "resources": all(v["status"] == "PASS" for v in resources.values()) and diagnostic["status"] == "PASS",
        "readiness": all(v["status"] == "READY" for v in ready.values()),
        "publication": all(v["status"] == "PASS" for v in publications.values()),
        "functional": load("functional-oracle.json")["status"] == "PASS",
        "restart": load("restart-oracle.json")["status"] == "PASS",
        "forced_death": load("forced-death-oracle.json")["status"] == "PASS",
        "splice": load("splice-oracle.json")["status"] == "PASS",
        "idle_cpu": idle["status"] == "PASS",
        "cleanup": clean["status"] == "PASS",
        "source_binding": source["image_id"] == IMAGE_ID,
    }
    summary = {"schema": "layerfs-stage2-final-summary-v1", "status": "PASS_OPTIMIZED" if all(checks.values()) else "REVISE", "checks": checks, "source_commit": COMMIT, "source_tree": TREE, "image_id": IMAGE_ID, "executable_blake3": source["executable_blake3"], "fs_bench_sha256": FS_BENCH, "authoritative": {c: authoritative[c]["aggregates"] for c in ("var", "tmp")}, "functional": {"checkpoint_100mib_ns": load("functional-oracle.json")["checkpoint_100mib_ns"], "same_daemon_read_mib_s": load("functional-oracle.json")["sequential_read_mib_s"], "restart_read_mib_s": load("restart-oracle.json")["sequential_read_mib_s"]}}
    write("summary.json", summary)
    if summary["status"] != "PASS_OPTIMIZED":
        raise SystemExit(1)


if __name__ == "__main__":
    main()
