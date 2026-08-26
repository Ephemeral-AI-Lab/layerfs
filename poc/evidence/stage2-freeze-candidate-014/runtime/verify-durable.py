#!/usr/bin/env python3
"""Independent verifier for the candidate-014 persistence-inclusive campaign."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import statistics
import subprocess


E = Path(__file__).resolve().parents[1]
COMMIT = "292be840c31052d85ab6e9441706298af3cd3d15"
TREE = "e3055bcd7a41921879fa149c11918891517e4522"
IMAGE_ID = "sha256:62b459af3f03dc8bbe97419b8522ed3599ab6d562b12ebe8b8ed5efb7f22f5fc"
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
GENERATION_DELTA = {
    "create 1000 files": 1,
    "stat 1000 files": 1,
    "rm 1000 files": 0,
    "mkdir tree (10x10x10)": 1,
    "find tree": 1,
    "write 64 MiB": 1,
    "copy 64 MiB": 1,
    "read 64 MiB": 1,
    "pure read 64 MiB": 0,
    "pure copy 64 MiB": 1,
    "overwrite 64 MiB": 1,
    "git init + commit 100 files": 1,
}
TERMINAL_ZERO = (
    "dirty_nodes",
    "dirty_ranges",
    "pending_nodes",
    "directory_changes",
    "open_handles",
    "logical_workspace_bytes",
    "spool_live_bytes",
    "spool_dead_bytes",
    "spool_physical_bytes",
    "operation_q_terminal_bytes",
)


def load(path: Path) -> dict:
    return json.loads(path.read_text())


def terminal_clean(terminal: dict) -> bool:
    mounted = terminal["mounted"]
    return (
        terminal["status"] == "PASS"
        and terminal["source_commit"] == COMMIT
        and terminal["source_tree"] == TREE
        and terminal["fs_bench_sha256"] == FS_BENCH
        and all(mounted[field] == 0 for field in TERMINAL_ZERO)
        and mounted["lookup_refs"]
        == mounted["live_nodes"]
        == mounted["inode_mappings"]
        == 1
        and terminal["engine"]["connections_terminal"] == 0
    )


def launch_checks(sample: Path) -> bool:
    plans = sorted(sample.glob("launch-*.plan.json"))
    starts = sorted(sample.glob("launch-*.startup.json"))
    cpu = sorted(sample.glob("launch-*.cpu.max.txt"))
    mounts = sorted(sample.glob("launch-*.mountinfo.txt"))
    inspections = sorted(sample.glob("launch-*.inspect.json"))
    if not all(len(paths) == 3 for paths in (plans, starts, cpu, mounts, inspections)):
        return False
    for start in starts:
        receipt = load(start)
        if not (
            receipt["source_commit"] == COMMIT
            and receipt["source_tree"] == TREE
            and receipt["fs_bench_sha256"] == FS_BENCH
            and receipt["integrity"] == "Verified"
        ):
            return False
    if not all(path.read_text().strip() == "100000 100000" for path in cpu):
        return False
    if not all(" /workspace " in path.read_text() and " - fuse layerfs " in path.read_text() for path in mounts):
        return False
    for path in inspections:
        inspection = load(path)
        host = inspection["HostConfig"]
        if not (
            inspection["Image"] == IMAGE_ID
            and host["NanoCpus"] == 1_000_000_000
            and host["Memory"] == 3 * 1024 * 1024 * 1024
            and host["PidsLimit"] == 512
            and host["NetworkMode"] == "none"
            and host["Privileged"] is False
            and all(mount["Destination"] != "/workspace" for mount in inspection["Mounts"])
        ):
            return False
    return True


def sample_checks(path: Path, receipt: dict) -> dict[str, bool]:
    prepared = load(path / "state-01-prepared.json")
    prepared_reopen = load(path / "state-02-prepared-reopen.json")
    accepted = load(path / "state-03-acknowledged.json")
    accepted_reopen = load(path / "state-04-acknowledged-reopen.json")
    final = load(path / "state-05-clean-final.json")
    terminal = load(path / "terminal.json")
    delta = accepted["ref"]["generation"] - prepared["ref"]["generation"]
    expected_delta = GENERATION_DELTA[receipt["scenario"]]
    crashes = all(
        (path / f"crash-{number}-{label}.exit").read_text().strip() == "137"
        and load(path / f"crash-{number}-{label}.stopped-inspect.json")["State"]["OOMKilled"] is False
        and not (path / f"crash-{number}-{label}.unexpected-terminal.json").exists()
        for number, label in (("01", "prepared"), ("02", "acknowledged"))
    )
    timing = load(path / "timing.json")
    return {
        "receipt_pass": receipt["status"] == "PASS",
        "timing_positive": all(
            isinstance(timing[field], int) and timing[field] > 0
            for field in ("T_live_ns", "T_checkpoint_ns", "T_to_durable_ns")
        ),
        "timing_equation_exact": timing["T_to_durable_ns"]
        == timing["T_live_ns"] + timing["T_checkpoint_ns"],
        "receipt_timing_exact": all(
            receipt[field] == timing[field]
            for field in ("T_live_ns", "T_checkpoint_ns", "T_to_durable_ns")
        ),
        "prepared_crash_reopen_exact": prepared == prepared_reopen,
        "accepted_crash_reopen_exact": accepted == accepted_reopen,
        "generation_delta_exact": delta == expected_delta,
        "rm_scope_exact": receipt["scenario"] != "rm 1000 files"
        or receipt["timed_durability_scope"]
        == "FINAL_STATE_ONLY_OPERATION_HISTORY_NOT_CLAIMED",
        "clean_final_inventory": final["snapshot"]["descendant_count"] == 0,
        "terminal_clean": terminal_clean(terminal)
        and terminal["generation"] == final["ref"]["generation"]
        and terminal["root"] == final["ref"]["root"],
        "two_sigkills_exact": crashes,
        "launches_exact": launch_checks(path),
        "resource_cleanup": load(path / "resource-cleanup.json")["status"] == "PASS",
    }


def verify(run_id: str) -> dict:
    root = E / "durable" / run_id
    campaign = load(root / "summary.json")
    binding = load(root / "binding.json")
    paths = sorted((root / "samples").glob("*/receipt.json"))
    rows = []
    for receipt_path in paths:
        receipt = load(receipt_path)
        checks = sample_checks(receipt_path.parent, receipt)
        rows.append(
            {
                "artifact": str(receipt_path.relative_to(root)),
                "scenario": receipt["scenario"],
                "warmup": receipt["warmup"],
                "repetition": receipt["repetition"],
                "status": "PASS" if all(checks.values()) else "FAIL",
                "checks": checks,
                "T_live_ns": receipt["T_live_ns"],
                "T_checkpoint_ns": receipt["T_checkpoint_ns"],
                "T_to_durable_ns": receipt["T_to_durable_ns"],
                "generation_delta": receipt["acknowledged_ref"]["generation"]
                - receipt["prepared_ref"]["generation"],
            }
        )
    measured = [row for row in rows if not row["warmup"]]
    timings = {}
    for scenario in SCENARIOS:
        selected = [row for row in measured if row["scenario"] == scenario]
        timing = {}
        for field in ("T_live_ns", "T_checkpoint_ns", "T_to_durable_ns"):
            samples = [row[field] for row in selected]
            timing[field] = {
                "samples_execution_order": samples,
                "samples_sorted": sorted(samples),
                "median": int(statistics.median(samples)) if samples else None,
                "minimum": min(samples) if samples else None,
                "maximum": max(samples) if samples else None,
                "mean": sum(samples) // len(samples) if samples else None,
            }
        timings[scenario] = timing
    aggregate = {
        "sum_live_medians_ns": sum(value["T_live_ns"]["median"] for value in timings.values()),
        "sum_checkpoint_medians_ns": sum(
            value["T_checkpoint_ns"]["median"] for value in timings.values()
        ),
        "sum_to_durable_medians_ns": sum(
            value["T_to_durable_ns"]["median"] for value in timings.values()
        ),
    }
    history = load(root / "history-oracle/receipt.json")
    history_checks = {
        "receipt_pass": history["status"] == "PASS" and all(history["checks"].values()),
        "create_reopen_exact": load(root / "history-oracle/state-01-created.json")
        == load(root / "history-oracle/state-02-created-reopen.json"),
        "delete_reopen_exact": load(root / "history-oracle/state-03-deleted.json")
        == load(root / "history-oracle/state-04-deleted-reopen.json"),
        "terminal_clean": terminal_clean(load(root / "history-oracle/terminal.json")),
        "resource_cleanup": load(root / "history-oracle/resource-cleanup.json")["status"]
        == "PASS",
    }
    collection_checks = {
        "campaign_pass": campaign["status"] == "PASS",
        "binding_exact": binding["status"] == "PASS"
        and binding["image_id"] == IMAGE_ID
        and binding["source_commit"] == COMMIT
        and binding["source_tree"] == TREE
        and binding["fs_bench_sha256"] == FS_BENCH,
        "sample_count_48": len(rows) == 48,
        "measured_count_36": len(measured) == 36,
        "scenario_set_exact": {row["scenario"] for row in rows} == set(SCENARIOS),
        "one_warmup_three_measured_each": all(
            sum(row["scenario"] == scenario and row["warmup"] for row in rows) == 1
            and sum(row["scenario"] == scenario and not row["warmup"] for row in rows) == 3
            for scenario in SCENARIOS
        ),
        "all_samples_pass": len(rows) == 48 and all(row["status"] == "PASS" for row in rows),
        "history_pass": all(history_checks.values()),
        "failure_artifacts_zero": not list(root.rglob("failure.json")),
        "latency_gate_not_fabricated": load(E / "durable-preregistration.json")["acceptance"][
            "numeric_latency_gate"
        ]
        is None,
    }
    containers = subprocess.run(
        ["docker", "ps", "-a", "--format", "{{.Names}}"],
        check=True,
        text=True,
        stdout=subprocess.PIPE,
    ).stdout
    volumes = subprocess.run(
        ["docker", "volume", "ls", "--format", "{{.Name}}"],
        check=True,
        text=True,
        stdout=subprocess.PIPE,
    ).stdout
    collection_checks["owned_runtime_absent"] = (
        "layerfs-stage2-final014-dur-" not in containers
        and "layerfs_stage2_final014_dur_" not in volumes
    )
    return {
        "schema": "layerfs-stage2-014-durable-verification-v1",
        "status": "PASS_DURABLE" if all(collection_checks.values()) else "REVISE",
        "classification": "COLD_FRESH_STORE_RESTART_DURABLE",
        "checks": collection_checks,
        "history": {"checks": history_checks, "receipt": history},
        "aggregate": aggregate,
        "timings": timings,
        "rows": rows,
        "numeric_latency_gate": None,
        "numeric_latency_gate_reason": "No deployed restart-durable Cloudflare population exists; live thresholds are not reused.",
        "cloudflare_full_product_comparison": "UNAVAILABLE",
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--run-id", default="durable02")
    parser.add_argument("--output", type=Path, default=E / "durable/verification.json")
    arguments = parser.parse_args()
    receipt = verify(arguments.run_id)
    arguments.output.parent.mkdir(exist_ok=True)
    with arguments.output.open("x") as output:
        json.dump(receipt, output, indent=2, sort_keys=True)
        output.write("\n")
    print(json.dumps(receipt, indent=2, sort_keys=True))
    if receipt["status"] != "PASS_DURABLE":
        raise SystemExit(1)


if __name__ == "__main__":
    main()
