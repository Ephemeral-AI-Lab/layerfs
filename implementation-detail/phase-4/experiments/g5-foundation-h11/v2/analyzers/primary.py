#!/usr/bin/env python3
import importlib.util
import json
import pathlib
import sys


def load_module(name, source):
    spec = importlib.util.spec_from_file_location(name, source)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def repair(result, raw_path):
    rows = [json.loads(line) for line in raw_path.read_text(encoding="utf-8").splitlines() if line]
    index = {(row["history_revisions"], row["sample"]): row for row in rows}
    failures = {item for item in result["normalized"]["hard_failures"] if not item.startswith("work_parity:first_edit_after_reopen:") and item != "latency_material:first_edit_after_reopen" and not (item.startswith("storage_pair:") and item.endswith(":terminal_allocated_store_bytes"))}
    fields = ("sql_queries", "sql_rows", "row_blob_reads", "row_blob_writes", "canonical_authenticated", "mapping_rewritten", "transactions", "commits", "q_current")
    for field in fields:
        if len({index[(1, sample)]["first_edit_after_reopen"][field] for sample in (1, 2)}) != 1:
            failures.add(f"genesis_work_pair:first_edit_after_reopen:{field}")
        if len({index[(history, sample)]["first_edit_after_reopen"][field] for history in (10, 100, 1000) for sample in (1, 2)}) != 1:
            failures.add(f"nongenenesis_work_parity:first_edit_after_reopen:{field}")
    control = [index[(10, sample)]["first_edit_after_reopen"]["wall_ns"] for sample in (1, 2)]
    candidate = [index[(1000, sample)]["first_edit_after_reopen"]["wall_ns"] for sample in (1, 2)]
    control_sum, candidate_sum = sum(control), sum(candidate)
    relative = candidate_sum * 100 > control_sum * 105
    absolute = candidate_sum - control_sum >= 2_000_000
    result["normalized"]["latency"]["first_edit_after_reopen"] = {"control_raw_ns": control, "candidate_raw_ns": candidate, "control_sum_ns": control_sum, "candidate_sum_ns": candidate_sum, "control_mean_ns": control_sum / 2, "candidate_mean_ns": candidate_sum / 2, "ratio": candidate_sum / control_sum, "absolute_delta_mean_ns": (candidate_sum - control_sum) / 2, "relative_branch_failed": relative, "absolute_branch_failed": absolute, "product_material_regression": relative and absolute}
    if relative and absolute:
        failures.add("latency_material:first_edit_after_reopen")
    allocated = {history: [index[(history, sample)]["terminal_allocated_store_bytes"] for sample in (1, 2)] for history in (1, 10, 100, 1000)}
    for history, values in allocated.items():
        if max(values) - min(values) > 1_048_576:
            failures.add(f"allocated_pair_spread:{history}")
    storage = result["normalized"]["storage"]
    storage["allocated_raw"] = allocated
    storage["control"]["terminal_allocated_store_bytes"] = min(allocated[1])
    storage["candidate"]["terminal_allocated_store_bytes"] = max(allocated[1000])
    storage["allocated_store_bytes_per_revision"] = (max(allocated[1000]) - min(allocated[1])) / 999
    if storage["allocated_store_bytes_per_revision"] > 131_072:
        failures.add("storage_slope:allocated")
    result["normalized"]["hard_failures"] = sorted(failures)
    result["schema"] = "phase4-g5-h11-primary-analysis-v2"
    result["status"] = "PASS" if not failures else "REVISE"
    return result


if __name__ == "__main__":
    raw, expected, output = map(pathlib.Path, sys.argv[1:4])
    v1 = load_module("h11_v1_primary", pathlib.Path(__file__).parents[2] / "v1/analyzers/primary.py")
    result = repair(v1.analyze(raw, expected), raw)
    output.write_text(json.dumps(result, sort_keys=True, separators=(",", ":")) + "\n", encoding="utf-8")
    raise SystemExit(0 if result["status"] == "PASS" else 1)
