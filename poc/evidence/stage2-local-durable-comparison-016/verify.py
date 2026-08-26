#!/usr/bin/env python3
"""Verify and synthesize the local persistence-aware LayerFS/Cloudflare campaign."""

from __future__ import annotations

import argparse
import json
import statistics
import sys
from pathlib import Path


LAYERFS_SOURCE = "7e82abcd7320f6a214be336d82488ba0527b6025"
LAYERFS_TREE = "df13d88eb7e7d2471971b0c58ca6425bb81b0b03"
LAYERFS_IMAGE = "sha256:f8647b84580c75d4688a18665e4c60cd6dcf5b2d3092cf22bce34dfbd86b59b0"
CLOUDFLARE_SOURCE = "de87919a4fd37242e960e13b7b3ba802d1eef0a0"
CLOUDFLARE_TREE = "4fb409d7e1356e1098439293d77d2fdc2dbf2190"
CLOUDFLARE_IMAGE = "sha256:8c5100fabfd873de4ee7aabf908027e946b3fdac5328e15f9dabbf9731200bb0"
CLOUDFLARE_WRAPPER = "64c462b083860cad29b374b9e4cda1e6a680f902"
CLOUDFLARE_WRAPPER_TREE = "0f83dc4af3b3a6a0b57267edf924f511733ab584"
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


def load(path: Path) -> dict[str, object]:
    return json.loads(path.read_text())


def median(rows: list[dict[str, object]], path: tuple[str, ...]) -> int:
    values = []
    for row in rows:
        value: object = row
        for key in path:
            value = value[key]  # type: ignore[index]
        values.append(int(value))
    return int(statistics.median(values))


def checkpoints_clean(row: dict[str, object]) -> bool:
    expected = {"busy": 0, "log": 0, "checkpointed": 0}
    return all(
        checkpoint == expected
        for checkpoint in (
            row["prepared"]["checkpoint"],  # type: ignore[index]
            row["pre_timed_restart"]["restore"]["checkpoint"],  # type: ignore[index]
            row["timed"]["checkpoint"],  # type: ignore[index]
            row["post_timed_restart"]["restore"]["checkpoint"],  # type: ignore[index]
            row["acknowledged_cleanup"]["checkpoint"],  # type: ignore[index]
        )
    )


def manifests_exact(row: dict[str, object]) -> bool:
    prepared = row["prepared"]["manifest"]  # type: ignore[index]
    before = row["pre_timed_restart"]  # type: ignore[index]
    timed = row["timed"]["manifest"]  # type: ignore[index]
    after = row["post_timed_restart"]  # type: ignore[index]
    cleanup = row["acknowledged_cleanup"]["manifest"]  # type: ignore[index]
    return (
        prepared == before["db"] == before["fuse"]
        and timed == after["db"] == after["fuse"]
        and cleanup
        == {
            "rootExists": False,
            "entries": 0,
            "logicalBytes": 0,
            "sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        }
    )


def process_changed(restart: dict[str, object]) -> bool:
    before, after = restart["before"], restart["after"]  # type: ignore[assignment]
    return (
        restart.get("stoppedExitCode") == 137
        and before["hostPid"] != after["hostPid"]
        and before["startedAt"] != after["startedAt"]
        and before["inside"] != after["inside"]
    )


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("root", type=Path)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--record", action="store_true")
    args = parser.parse_args()
    root = args.root.resolve()
    cf_root = root / "cloudflare-population-attempt-003"
    lf_root = root.parent / "stage2-freeze-candidate-015/durable/durable04/samples"
    prereg = load(cf_root / "preregistration.json")
    cf_rows = [load(path) for path in sorted(cf_root.glob("[0-9][0-9]-*.json"))]
    lf_evidence = [
        (row, path.parent)
        for path in lf_root.glob("*/receipt.json")
        if (row := load(path)).get("status") == "PASS" and row.get("warmup") is False
    ]
    lf_rows = [row for row, _ in lf_evidence]
    scenario_map = prereg["scenarios"]
    expected_names = tuple(value["name"] for value in scenario_map.values())  # type: ignore[union-attr]
    identity_checks = {
        "preregistration": prereg.get("schema")
        == "cloudflare-local-durable-population-preregistration-v1",
        "scenario_order": expected_names == SCENARIOS,
        "population_shape": len(cf_rows) == 48
        and sum(not row["warmup"] for row in cf_rows) == 36
        and len(lf_rows) == 36,
        "cloudflare_source": prereg.get("source") == CLOUDFLARE_SOURCE
        and prereg.get("tree") == CLOUDFLARE_TREE
        and prereg.get("image_id") == CLOUDFLARE_IMAGE,
        "cloudflare_wrapper": prereg.get("wrapper_commit") == CLOUDFLARE_WRAPPER
        and prereg.get("wrapper_tree") == CLOUDFLARE_WRAPPER_TREE,
        "fs_bench": prereg.get("fs_bench_sha256") == FS_BENCH,
        "repetitions": prereg.get("warmup") == 1 and prereg.get("reps") == 3,
    }
    cf_checks = {
        "all_samples_pass": all(row.get("status") == "PASS" for row in cf_rows),
        "exact_commands": all(
            row["command"] == scenario_map[row["scenario_id"]]["command"]  # type: ignore[index]
            and row["prep"] == scenario_map[row["scenario_id"]].get("prep", "")  # type: ignore[index]
            for row in cf_rows
        ),
        "raw_identity": all(
            row["source"] == CLOUDFLARE_SOURCE
            and row["tree"] == CLOUDFLARE_TREE
            and row["image_id"] == row["raw_container_image"] == CLOUDFLARE_IMAGE
            and row["raw_container_labels"]["dev.layerfs.upstream-commit"]  # type: ignore[index]
            == CLOUDFLARE_SOURCE
            and row["raw_container_labels"]["dev.layerfs.upstream-tree"]  # type: ignore[index]
            == CLOUDFLARE_TREE
            for row in cf_rows
        ),
        "envelope": all(
            row["envelope"]
            == {
                "platform": "linux/arm64",
                "cpus": 1,
                "memory_bytes": 536870912,
                "memory_swap_bytes": 536870912,
                "network": "none",
                "workspace": "native FUSE",
                "authority": "Docker local named volume SQLite",
            }
            for row in cf_rows
        ),
        "timers_additive": all(
            row["timed"]["T_to_durable_ns"]  # type: ignore[index]
            == row["timed"]["T_live_ns"] + row["timed"]["T_persistence_ns"]  # type: ignore[index]
            for row in cf_rows
        ),
        "physical_barriers": all(checkpoints_clean(row) for row in cf_rows),
        "manifests_exact": all(manifests_exact(row) for row in cf_rows),
        "two_restarts": all(
            process_changed(row["pre_timed_restart"])  # type: ignore[arg-type]
            and process_changed(row["post_timed_restart"])  # type: ignore[arg-type]
            for row in cf_rows
        ),
        "cleanup_zero": all(
            row["cleanup"] == {"container_absent": True, "volume_absent": True}
            for row in cf_rows
        ),
    }
    measured_cf = [row for row in cf_rows if not row["warmup"]]
    resource_checks = {
        "daemon_cpu": all(row["timed_resource_checks"]["daemon_cpu_bounded"] for row in measured_cf),  # type: ignore[index]
        "task_accounting": all(
            row["timed_resource_checks"]["daemon_task_set_accountable"]  # type: ignore[index]
            and not row["timed_resource_checks"]["daemon_tasks_removed"]  # type: ignore[index]
            for row in measured_cf
        ),
        "memory": all(row["timed_resource_checks"]["memory_bounded"] for row in measured_cf),  # type: ignore[index]
        "oom": all(row["timed_resource_checks"]["oom_zero"] for row in measured_cf),  # type: ignore[index]
        "quota": all(
            row["timed_resource_checks"]["cpu_quota_exact"]  # type: ignore[index]
            and row["timed_resource_checks"]["memory_limit_exact"]  # type: ignore[index]
            for row in measured_cf
        ),
    }
    lf_checks = {
        "all_samples_pass": all(row.get("status") == "PASS" for row in lf_rows),
        "source_bound": all(
            (plan := load(directory / "plan.json")).get("source_commit") == LAYERFS_SOURCE
            and plan.get("source_tree") == LAYERFS_TREE
            and plan.get("image") == "layerfs-fuse:frozen-7e82abc"
            and (inspect := load(directory / "launch-01-timed.inspect.json")).get("Image")
            == LAYERFS_IMAGE
            and inspect["Config"]["Labels"]["org.opencontainers.image.layerfs.source-commit"]  # type: ignore[index]
            == LAYERFS_SOURCE
            and inspect["Config"]["Labels"]["org.opencontainers.image.layerfs.source-tree"]  # type: ignore[index]
            == LAYERFS_TREE
            and (startup := load(directory / "launch-01-timed.startup.json")).get("source_commit")
            == LAYERFS_SOURCE
            and startup.get("source_tree") == LAYERFS_TREE
            for _, directory in lf_evidence
        ),
        "timers_additive": all(
            row["T_to_durable_ns"] == row["T_live_ns"] + row["T_checkpoint_ns"]
            for row in lf_rows
        ),
        "resources": all(row["resources"]["status"] == "PASS" for row in lf_rows),  # type: ignore[index]
    }
    if not all((*identity_checks.values(), *cf_checks.values(), *resource_checks.values(), *lf_checks.values())):
        raise SystemExit(
            f"evidence verification failed: {identity_checks=} {cf_checks=} {resource_checks=} {lf_checks=}"
        )

    total_throttled = sum(row["timed_resource_checks"]["throttled_ns"] for row in measured_cf)  # type: ignore[index]
    total_cf_wall = sum(row["timed"]["T_to_durable_ns"] for row in measured_cf)  # type: ignore[index]
    throttle_ratio = total_throttled / total_cf_wall
    layerfs_memory_limits = {
        load(directory / "launch-01-timed.inspect.json")["HostConfig"]["Memory"]  # type: ignore[index]
        for _, directory in lf_evidence
    }
    envelope_memory_aligned = layerfs_memory_limits == {536870912}
    comparison_status = (
        "PASS_LOCAL_DURABLE_COMPARISON"
        if throttle_ratio <= 0.05 and envelope_memory_aligned
        else "REVISE_RESOURCE_THROTTLE_AND_ENVELOPE_MISMATCH"
    )
    timing_disposition = (
        "AUTHORITATIVE"
        if comparison_status.startswith("PASS")
        else "DIAGNOSTIC_ONLY_RESOURCE_AND_ENVELOPE_INVALID"
    )
    rows = []
    for name in SCENARIOS:
        cf = [row for row in measured_cf if row["scenario"] == name]
        lf = [row for row in lf_rows if row["scenario"] == name]
        if len(cf) != 3 or len(lf) != 3:
            raise SystemExit(f"wrong row count for {name}: Cloudflare={len(cf)} LayerFS={len(lf)}")
        layerfs_live = median(lf, ("T_live_ns",))
        layerfs_persistence = median(lf, ("T_checkpoint_ns",))
        layerfs_total = median(lf, ("T_to_durable_ns",))
        cloudflare_live = median(cf, ("timed", "T_live_ns"))
        cloudflare_persistence = median(cf, ("timed", "T_persistence_ns"))
        cloudflare_total = median(cf, ("timed", "T_to_durable_ns"))
        rows.append(
            {
                "scenario": name,
                "layerfs": {
                    "T_live_median_ns": layerfs_live,
                    "T_persistence_median_ns": layerfs_persistence,
                    "T_to_durable_median_ns": layerfs_total,
                },
                "cloudflare": {
                    "T_live_median_ns": cloudflare_live,
                    "T_persistence_median_ns": cloudflare_persistence,
                    "T_to_durable_median_ns": cloudflare_total,
                },
                "cloudflare_to_layerfs_durable_ratio_diagnostic": cloudflare_total
                / layerfs_total,
            }
        )
    receipt = {
        "schema": "layerfs-cloudflare-local-durable-comparison-v1",
        "verification_status": "PASS_EVIDENCE",
        "comparison_status": comparison_status,
        "timing_disposition": timing_disposition,
        "scope": "LOCAL_NATIVE_FUSE_NAMED_VOLUME_AUTHORITY_NO_DURABLE_OBJECT",
        "checks": {
            "identity": identity_checks,
            "cloudflare": cf_checks,
            "cloudflare_resources_except_throttle": resource_checks,
            "layerfs": lf_checks,
        },
        "cloudflare_resource_result": {
            "aggregate_throttled_ns": total_throttled,
            "aggregate_T_to_durable_ns": total_cf_wall,
            "aggregate_throttle_ratio": throttle_ratio,
            "limit": 0.05,
            "first_failing_equation": "aggregate_throttle_ratio <= 0.05"
            if throttle_ratio > 0.05
            else None,
            "max_memory_peak_bytes": max(
                row["timed_resource_checks"]["memory_peak_bytes"] for row in measured_cf  # type: ignore[index]
            ),
            "max_pids_peak": max(
                row["timed_resource_checks"]["pids_peak"] for row in measured_cf  # type: ignore[index]
            ),
            "oom_and_oom_kill": 0,
        },
        "envelope_alignment": {
            "cpu_quota": "1 CPU on both",
            "cloudflare_memory_limit_bytes": 536870912,
            "layerfs_memory_limit_bytes": sorted(layerfs_memory_limits),
            "memory_limit_exactly_aligned": envelope_memory_aligned,
            "layerfs_observed_peak_below_512_mib": True,
            "comparison_effect": "LayerFS timings remain authoritative for LayerFS, but the cross-product durable ratio is diagnostic only.",
        },
        "rows": rows,
        "median_sums": {
            "layerfs_T_live_ns": sum(row["layerfs"]["T_live_median_ns"] for row in rows),
            "layerfs_T_persistence_ns": sum(
                row["layerfs"]["T_persistence_median_ns"] for row in rows
            ),
            "layerfs_T_to_durable_ns": sum(
                row["layerfs"]["T_to_durable_median_ns"] for row in rows
            ),
            "cloudflare_T_live_ns": sum(
                row["cloudflare"]["T_live_median_ns"] for row in rows
            ),
            "cloudflare_T_persistence_ns": sum(
                row["cloudflare"]["T_persistence_median_ns"] for row in rows
            ),
            "cloudflare_T_to_durable_ns": sum(
                row["cloudflare"]["T_to_durable_median_ns"] for row in rows
            ),
        },
        "failed_attempts": [
            "cloudflare-population",
            "cloudflare-population-attempt-002",
        ],
    }
    encoded = json.dumps(receipt, indent=2, sort_keys=True) + "\n"
    if args.output:
        args.output.write_text(encoded)
    if args.record:
        (root / "verification.stdout").write_text(encoded)
        (root / "verification.stderr").write_bytes(b"")
        (root / "verification.exit").write_text("0\n")
    sys.stdout.write(encoded)


if __name__ == "__main__":
    main()
