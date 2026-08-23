#!/usr/bin/env python3
import importlib.util
import json
import pathlib
import sys


def import_v1(source):
    spec = importlib.util.spec_from_file_location("h11_v1_independent", source)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def corrected(result, raw_file):
    records = [json.loads(value) for value in raw_file.read_text(encoding="utf-8").splitlines() if value]
    lookup = {(value["history_revisions"], value["sample"]): value for value in records}
    failed = {value for value in result["normalized"]["hard_failures"] if not value.startswith("work_parity:first_edit_after_reopen:") and value != "latency_material:first_edit_after_reopen" and not (value.startswith("storage_pair:") and value.endswith(":terminal_allocated_store_bytes"))}
    counters = ["sql_queries", "sql_rows", "row_blob_reads", "row_blob_writes", "canonical_authenticated", "mapping_rewritten", "transactions", "commits", "q_current"]
    for counter in counters:
        if len({lookup[(1, sample)]["first_edit_after_reopen"][counter] for sample in [1, 2]}) != 1:
            failed.add(f"genesis_work_pair:first_edit_after_reopen:{counter}")
        if len({lookup[(history, sample)]["first_edit_after_reopen"][counter] for history in [10, 100, 1000] for sample in [1, 2]}) != 1:
            failed.add(f"nongenenesis_work_parity:first_edit_after_reopen:{counter}")
    controls = [lookup[(10, sample)]["first_edit_after_reopen"]["wall_ns"] for sample in [1, 2]]
    candidates = [lookup[(1000, sample)]["first_edit_after_reopen"]["wall_ns"] for sample in [1, 2]]
    control_total, candidate_total = sum(controls), sum(candidates)
    relative_fail = candidate_total * 100 > control_total * 105
    absolute_fail = candidate_total - control_total >= 2_000_000
    result["normalized"]["latency"]["first_edit_after_reopen"] = {"control_raw_ns": controls, "candidate_raw_ns": candidates, "control_sum_ns": control_total, "candidate_sum_ns": candidate_total, "control_mean_ns": control_total / 2, "candidate_mean_ns": candidate_total / 2, "ratio": candidate_total / control_total, "absolute_delta_mean_ns": (candidate_total - control_total) / 2, "relative_branch_failed": relative_fail, "absolute_branch_failed": absolute_fail, "product_material_regression": relative_fail and absolute_fail}
    if relative_fail and absolute_fail:
        failed.add("latency_material:first_edit_after_reopen")
    allocation = {history: [lookup[(history, sample)]["terminal_allocated_store_bytes"] for sample in [1, 2]] for history in [1, 10, 100, 1000]}
    for history, samples in allocation.items():
        if max(samples) - min(samples) > 1_048_576:
            failed.add(f"allocated_pair_spread:{history}")
    storage = result["normalized"]["storage"]
    storage["allocated_raw"] = allocation
    storage["control"]["terminal_allocated_store_bytes"] = min(allocation[1])
    storage["candidate"]["terminal_allocated_store_bytes"] = max(allocation[1000])
    storage["allocated_store_bytes_per_revision"] = (max(allocation[1000]) - min(allocation[1])) / 999
    if storage["allocated_store_bytes_per_revision"] > 131_072:
        failed.add("storage_slope:allocated")
    result["normalized"]["hard_failures"] = sorted(failed)
    result["schema"] = "phase4-g5-h11-independent-recomputation-v2"
    result["status"] = "PASS" if not failed else "REVISE"
    return result


if __name__ == "__main__":
    raw_path, expected_path, output_path = map(pathlib.Path, sys.argv[1:4])
    base = import_v1(pathlib.Path(__file__).parents[2] / "v1/analyzers/independent.py")
    result = corrected(base.recompute(raw_path, expected_path), raw_path)
    output_path.write_text(json.dumps(result, sort_keys=True, separators=(",", ":")) + "\n", encoding="utf-8")
    raise SystemExit(0 if result["status"] == "PASS" else 1)
