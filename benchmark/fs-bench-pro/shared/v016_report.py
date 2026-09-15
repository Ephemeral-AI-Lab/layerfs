#!/usr/bin/env python3
"""Render one rollout tag's receipts into the committed evidence matrix.

Reads the driver's ``rollout-matrix.json`` and each retained receipt, and emits a
machine-readable matrix plus the per-case rows the #154/#122 reports quote. It
never invents a number: a field the receipt does not carry stays null with its
scope recorded.
"""

import argparse
import json
from pathlib import Path
import re

REPO = Path(__file__).resolve().parents[3]


def load_records(path):
    if not path.is_file():
        return []
    rows = []
    for line in path.open():
        line = line.strip()
        if line.startswith("{"):
            try:
                rows.append(json.loads(line))
            except json.JSONDecodeError:
                pass
    return rows


def phase_records(sample):
    """Every per-phase/operation record in workload order."""
    return [rec for rec in sample.get("records", []) if rec.get("kind") == "phase"]


def commit_outcomes(sample):
    """Created / UpToDate / Busy / HeadMoved, counted separately."""
    counts = {"Created": 0, "UpToDate": 0, "Busy": 0, "HeadMoved": 0, "presentation_failures": 0}
    latencies = []
    for rec in sample.get("records", []):
        if rec.get("kind") != "phase" or rec.get("phase") != "commit":
            continue
        outcome = rec.get("outcome", "")
        if "Created" in outcome:
            counts["Created"] += 1
        elif "UpToDate" in outcome:
            counts["UpToDate"] += 1
        elif "Busy" in outcome:
            counts["Busy"] += 1
        elif "HeadMoved" in outcome:
            counts["HeadMoved"] += 1
        if "presentation_failed: true" in outcome:
            counts["presentation_failures"] += 1
        if isinstance(rec.get("elapsed_ns"), int):
            latencies.append(rec["elapsed_ns"])
    return counts, latencies


def sample_row(path):
    rows = load_records(path)
    header = next((row for row in rows if row.get("kind") == "header"), None)
    sample = next((row for row in rows if row.get("kind") == "sample"), None)
    if not sample:
        return None
    summary = next((row for row in rows if row.get("kind") == "summary"), None)
    records = sample.get("records", [])
    complete = next((rec for rec in records if rec.get("kind") == "sample-complete"), {})
    counts, latencies = commit_outcomes(sample)
    phases = phase_records(sample)
    resources = sample.get("resources", {}) or {}
    setup = sample.get("setup", {}) or {}
    return {
        "status": sample.get("status"),
        "phase": sample.get("phase"),
        "error": sample.get("error"),
        "timer_ns": complete.get("pure_call_sum_ns"),
        "command_wall_ns": sample.get("command_wall_ns"),
        "preparation_wall_ns": sample.get("preparation_wall_ns"),
        "cleanup": sample.get("cleanup"),
        "created_commit_count": complete.get("created_commit_count"),
        "commit_outcomes": counts,
        "commit_latency_ns": {
            "count": len(latencies),
            "max": max(latencies) if latencies else None,
            "min": min(latencies) if latencies else None,
            "median": sorted(latencies)[len(latencies) // 2] if latencies else None,
            "sum": sum(latencies) if latencies else None,
        },
        "phase_ns": {rec.get("phase"): rec.get("elapsed_ns") for rec in phases if rec.get("elapsed_ns")},
        "phase_count": len(phases),
        "setup": {key: setup.get(key) for key in (
            "setup_mode", "clone_method", "master_unchanged", "cache_hit",
            "sample_store_sha256", "manifest_sha256")},
        "resources": resources,
        "production_records": {
            rec.get("kind"): {key: value for key, value in rec.items()
                              if key not in ("kind", "details") and not isinstance(value, (dict, list))}
            for rec in records
            if rec.get("kind") in ("store-observation", "workspace-physical-spool", "sample-complete",
                                   "workspace-spool-observation", "host-resources", "host-rss-samples",
                                   "runtime-observation-window", "graph-proof", "worker-intervals")
        },
        "identities": (header or {}).get("identities"),
        "summary": {key: summary.get(key) for key in (
            "status", "attempted", "completed", "valid", "timer", "min_ns", "median_ns", "max_ns",
            "admission_eligible", "verification_status")} if summary else None,
    }


def _native_receipt(text):
    """The independent oracle's own key=value receipt, verbatim."""
    values = {}
    for line in str(text).splitlines():
        if "=" in line:
            key, _, value = line.partition("=")
            values[key.strip()] = value.strip()
    return values


def _canonical_receipt(text):
    try:
        return json.loads(text)
    except (TypeError, ValueError):
        return {}


def verification_row(path):
    path = path if path.is_file() else path / "verification.json"
    if not path.is_file():
        return None
    receipt = json.loads(path.read_text())
    checks = receipt.get("checks", [])
    kinds = [check.get("kind") for check in checks if isinstance(check, dict)]
    native = next((check for check in checks if check.get("kind") == "native-verification"), None)
    canonical = next((check for check in checks if check.get("kind") == "canonical-verification"), None)
    split = next((check for check in checks if check.get("kind") == "v016-alias-inode-classes"), None)
    published = [check for check in checks if check.get("kind") == "published-root"]
    steps = next((check for check in checks if check.get("kind") == "v016-alias-posix-steps"), None)
    canonical_values = _canonical_receipt((canonical or {}).get("receipt")) if canonical else {}
    native_values = _native_receipt((native or {}).get("receipt")) if native else {}
    return {
        "status": receipt.get("status"),
        "error": receipt.get("error"),
        "wall_seconds": receipt.get("wall_seconds"),
        "work_deadline_seconds": receipt.get("work_deadline_seconds"),
        "hard_limit_seconds": receipt.get("hard_limit_seconds"),
        "policy": receipt.get("verification_policy"),
        "cleanup": receipt.get("cleanup"),
        "omissions": receipt.get("omissions"),
        "reused_proof_identities": receipt.get("reused_proof_identities"),
        "evidence_path": receipt.get("evidence_path"),
        "sampled_paths_or_ranges": receipt.get("sampled_paths_or_ranges"),
        "check_kinds": kinds,
        "coverage": {
            "canonical_full_state_verified": canonical is not None,
            "canonical_payload_objects": canonical_values.get("canonical_Chunk_objects"),
            "canonical_payload_bytes": canonical_values.get("canonical_Chunk_bytes"),
            "declared_set_verified_paths": native_values.get("verified_paths"),
            "declared_set_verified_regular_paths": native_values.get("verified_regular_paths"),
            "declared_set_independent_content_paths": native_values.get("independent_content_paths"),
            "declared_set_logical_bytes": native_values.get("logical_bytes"),
            "oracle_scope": native_values.get("oracle_scope"),
            "oracle_identity": native_values.get("oracle_identity"),
            "retained_published_roots": len(published),
            "alias_inode_class_split": (split or {}).get("inode_class_split"),
            "alias_inode_separated": (split or {}).get("pre_replacement_inode") != (split or {}).get("alias_inode") if split else None,
            "posix_steps_replayed": len((steps or {}).get("steps", [])) if steps else None,
        },
    }


CASES = json.loads((REPO / "docs/roadmap/0.1/0.1.6/cases.json").read_text())["cases"]


def case_commits(case_id):
    case = next((row for row in CASES if row["id"] == case_id), None)
    if not case:
        return None
    return {
        "new_commits_total": case.get("new_commits_total"),
        "branch_local_new_commits": case.get("branch_local_new_commits"),
        "longest_ancestry_commits": case.get("longest_ancestry_commits"),
        "max_live_workspaces": case.get("max_live_workspaces"),
        "lane": case.get("lane"),
        "modes": case.get("modes"),
    }


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--tag", required=True)
    parser.add_argument("--out", required=True)
    args = parser.parse_args()
    results = REPO / "benchmark-results" / "v016" / args.tag
    matrix = json.loads((results / "rollout-matrix.json").read_text())
    out = {"tag": args.tag, "image": matrix.get("image"), "rows": []}
    for row in sorted(matrix["rows"], key=lambda item: (item["case"], item["seed"])):
        entry = {key: row.get(key) for key in ("case", "family", "seed", "image", "limit_seconds")}
        identities = (row.get("identities") or {})
        perf_receipt = REPO / row["performance"]["receipt"] if row.get("performance", {}).get("receipt") else None
        entry["performance"] = {
            "driver": row.get("performance"),
            "receipt": sample_row(perf_receipt / "perf.jsonl") if perf_receipt else None,
        }
        verify_receipt = REPO / row["verification"]["receipt"] if row.get("verification", {}).get("receipt") else None
        entry["verification"] = {
            "driver": row.get("verification"),
            "receipt": verification_row(verify_receipt) if verify_receipt else None,
        }
        entry["requested"] = {
            "fixture_files": (entry["performance"]["receipt"] or {}).get("identities", {}).get("fixture_files"),
            "fixture_bytes": (entry["performance"]["receipt"] or {}).get("identities", {}).get("fixture_bytes"),
            "fixture_profile": (entry["performance"]["receipt"] or {}).get("identities", {}).get("fixture_profile"),
            "requested_new_commits": case_commits(row["case"]),
            "identities": identities,
        }
        out["rows"].append(entry)
    target = REPO / args.out
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(json.dumps(out, indent=1, sort_keys=True) + "\n")
    print(f"wrote {target} ({len(out['rows'])} rows)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
