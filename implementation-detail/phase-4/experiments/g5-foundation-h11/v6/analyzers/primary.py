#!/usr/bin/env python3
import importlib.util
import json
import pathlib
import sys

HERE = pathlib.Path(__file__).resolve().parent
V1 = HERE.parents[1] / "v1/analyzers/primary.py"
V2 = HERE.parents[1] / "v2/analyzers/primary.py"


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def expected_rows(path):
    rows = {}
    for line in path.read_text(encoding="utf-8").splitlines()[1:]:
        fields = line.split("\t")
        rows[int(fields[0])] = {
            "revision": int(fields[0]),
            "root": fields[1],
            "transition": fields[2],
            "output_digest": fields[4],
        }
    return rows


def extras(rows, expected):
    failures = set()
    q = {}
    historical = {}
    reopen = {}
    for row in rows:
        key = f"h{row['history_revisions']}s{row['sample']}"
        marker = row.get("q_terminal", {})
        q[key] = {
            "high_water": row.get("q_high_water"),
            "terminal": marker.get("q_current"),
            "terminal_high_water": marker.get("q_high_water"),
        }
        if (
            row.get("q_current") != 0
            or row.get("q_high_water", 0) < 691_675
            or marker.get("status") != "PASS"
            or marker.get("q_current") != 0
            or marker.get("q_high_water") != row.get("q_high_water")
            or row.get("reachability_entry_q_bytes") != 64
        ):
            failures.add(f"{key}:whole_harness_q")
        selected = sorted(
            {1, row["history_revisions"], row["history_revisions"] // 2}
            | {value for value in (1, 10, 100, 1000) if value <= row["history_revisions"]}
        )
        selected = [value for value in selected if value]
        tuples = row.get("historical_tuples", [])
        wanted = [expected[value] for value in selected]
        historical[key] = tuples
        if tuples != wanted:
            failures.add(f"{key}:historical_tuples")
        phases = row.get("reopen_phases", {})
        phase_sum = sum(
            phases.get(name, -1)
            for name in ("preflight_ns", "sqlite_open_profile_ns", "cache_profile_ns", "head_lookup_ns")
        )
        reopen[key] = phases
        if (
            phase_sum != phases.get("sum_ns")
            or phase_sum != row.get("reopen_head", {}).get("wall_ns")
            or not str(phases.get("sql_counter_scope", "")).startswith("partial-logical")
        ):
            failures.add(f"{key}:reopen_phases")
        n = row["history_revisions"]
        history_checks = {
            "history_objects_created": 6 * (n - 1),
            "history_objects_reused": 0,
            "history_canonical_new_bytes": 23_030 * (n - 1),
            "history_mapping_rewritten": 2_309 * (n - 1),
            "history_transactions": n - 1,
            "history_commits": n - 1,
        }
        for name, value in history_checks.items():
            if row.get(name) != value:
                failures.add(f"{key}:{name}")
    return failures, {
        "whole_harness_q": q,
        "historical_tuples": historical,
        "reopen_phases": reopen,
        "reachability_entry_q_bytes": 64,
    }


def main(raw_path, expected_path, output_path):
    rows = [json.loads(line) for line in raw_path.read_text(encoding="utf-8").splitlines() if line]
    v1 = load("h11_v1_primary_for_v6", V1)
    v2 = load("h11_v2_primary_for_v6", V2)
    result = v2.repair(v1.analyze(raw_path, expected_path), raw_path)
    failures, additions = extras(rows, expected_rows(expected_path))
    failures.update(result["normalized"]["hard_failures"])
    result["normalized"].update(additions)
    result["normalized"]["hard_failures"] = sorted(failures)
    result["schema"] = "phase4-g5-h11-primary-analysis-v6"
    result["status"] = "PASS" if not failures else "REVISE"
    output_path.write_text(json.dumps(result, sort_keys=True, separators=(",", ":")) + "\n", encoding="utf-8")
    return result["status"]


if __name__ == "__main__":
    if len(sys.argv) != 4:
        raise SystemExit("usage: primary.py RAW EXPECTED OUTPUT")
    raise SystemExit(0 if main(*map(pathlib.Path, sys.argv[1:])) == "PASS" else 1)



