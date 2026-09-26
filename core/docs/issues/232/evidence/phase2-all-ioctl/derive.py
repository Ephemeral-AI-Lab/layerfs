#!/usr/bin/env python3
"""Recheck every immutable #232 v3 raw receipt without taking a new sample."""

import hashlib
import json
from pathlib import Path


HERE = Path(__file__).resolve().parent
PRE = json.loads((HERE / "PRE_RUN.json").read_text())
REGISTRY_PATH = (HERE / "../../../../../benchmark/fs-bench-pro/registry/workspace-exec-edit-v3.json").resolve()
assert hashlib.sha256(REGISTRY_PATH.read_bytes()).hexdigest() == PRE["registry_sha256"]
REGISTRY = json.loads(REGISTRY_PATH.read_text())


def child(parent, name):
    found = [item for item in parent["children"] if item["name"] == name]
    assert len(found) == 1, (parent["name"], name)
    return found[0]


def result(case):
    scenario = case["scenario_id"]
    folder = HERE / "attempt-01" / scenario
    for line in (folder / "SHA256SUMS").read_text().splitlines():
        sha, name = line.split("  ", 1)
        assert hashlib.sha256((folder / name).read_bytes()).hexdigest() == sha
    receipt = json.loads((folder / "receipt.json").read_text())
    driver = json.loads((folder / "driver-receipt.json").read_text())
    verifier = json.loads((folder / "verification.json").read_text())
    assert receipt["scenario_id"] == scenario and receipt["sample_count"] == 1
    assert receipt["source_commit"] == "46cf957abf97ad5c340c8faf0dfab7e9fb426658"
    assert receipt["product_seal"] == PRE["product_seal"]
    assert receipt["harness_seal"] == PRE["harness_seal"]
    assert receipt["registry_sha256"] == PRE["registry_sha256"]
    assert receipt["image_id"] == PRE["image"]["image_id"]
    assert receipt["status"] == "INELIGIBLE" and not receipt["admission_eligible"]
    assert receipt["complete_command_wall_ns"] <= PRE["complete_command_budget_ns"]
    assert receipt["cache"]["macos-store"]["status"] == "PASS"
    assert receipt["cache"]["macos-store"]["resident_pages"] == 0
    assert receipt["cache"]["linux-fuse-backing"]["status"] == "INELIGIBLE"
    assert receipt["route_ok"] and driver["status"] == "COMPLETE"
    assert driver["unmount_ok"] and driver["sandbox_delete_ok"]
    assert driver["range_accepted_payload_bytes"] == case["expected_accepted_literal_bytes"]
    assert driver["range_shifted_suffix_bytes"] == 0
    callbacks = dict(item.split("=", 1) for item in driver["projection_counts"].split(","))
    assert int(callbacks["range_state"]) == 2
    assert int(callbacks["range_edit"]) == case["expected_callback_total"] - 2
    assert int(callbacks["write"]) == 0
    assert verifier["status"] == "PASS" and verifier["wall_ns"] < PRE["verifier_budget_ns"]
    assert verifier["child"]["content_match"]
    assert verifier["child"]["historical_root_match"]
    assert verifier["child"]["published_metadata_match"]
    assert verifier["child"]["historical_metadata_match"]
    assert verifier["child"]["canonical_root_match"]
    assert verifier["child"]["canonical_count_match"]
    lines = (folder / "driver.raw.stderr").read_text().splitlines()
    records = [json.loads(line[5:]) for line in lines if line.startswith("LFT1 ")]
    roots = [item for item in records if item.get("namespace") == 7
             and item.get("role") == 1 and item.get("kind") == "operation"
             and item.get("key") == 232000]
    assert len(roots) == 1 and roots[0]["success"]
    root = roots[0]
    assert root["timing"]["name"] == "sdk.edit_commit.fuse"
    edit, commit = (child(root["timing"], name) for name in ("edit", "commit"))
    edits = [item for item in records if item.get("namespace") == 7
             and item.get("role") == 1 and item.get("kind") == "operation"
             and item.get("timing", {}).get("name") == "EditFile"]
    assert len(edits) == 1 and edits[0]["success"]
    finish = child(edits[0]["timing"], "service.finish")
    drain = child(finish, "storage.finish.drain")
    owner = child(finish, "storage.finish.owner")
    aim = case["raw_engineering_aim_ms"]
    return {
        "family_id": case["family_id"], "fixture_bytes": case["fixture_bytes"],
        "status": receipt["status"], "sample_count": 1,
        "complete_command_ns": receipt["complete_command_wall_ns"],
        "verifier_ns": verifier["wall_ns"], "verifier_status": verifier["status"],
        "cleanup": "PASS", "cache": "INELIGIBLE",
        "root_ns": root["timing"]["elapsed_ns"], "edit_ns": edit["elapsed_ns"],
        "commit_ns": commit["elapsed_ns"],
        "root_samples": root["samples"],
        "root_cpu_shared_ns": root["cpu_shared_ns"],
        "root_sampled_max_rss": root["sampled_max_rss"],
        "root_first_ns": root["first_ns"], "root_opened_ns": root["opened_ns"],
        "root_last_ns": root["last_ns"], "root_closed_ns": root["closed_ns"],
        "service_finish_ns": finish["elapsed_ns"],
        "storage_drain_ns": drain["elapsed_ns"],
        "storage_owner_ns": owner["elapsed_ns"],
        "raw_engineering_aim_ms": aim,
        "raw_aim_met": None if aim is None else root["timing"]["elapsed_ns"] <= aim * 1_000_000,
        "expected_callbacks": case["expected_callbacks"],
        "observed_projection": callbacks,
        "accepted_literal_bytes": driver["range_accepted_payload_bytes"],
        "shifted_suffix_bytes": driver["range_shifted_suffix_bytes"],
        "canonical_root": verifier["child"]["canonical_root"],
        "canonical_extents": verifier["child"]["extent_count"],
    }


if __name__ == "__main__":
    rows = {case["scenario_id"]: result(case) for case in REGISTRY["cases"]}
    assert len(rows) == 56
    summary = {
        "status_counts": {"INELIGIBLE": 56},
        "family_counts": {name: sum(row["family_id"] == name for row in rows.values())
                          for name in sorted({row["family_id"] for row in rows.values()})},
        "max_complete_command_ns": max(row["complete_command_ns"] for row in rows.values()),
        "max_verifier_ns": max(row["verifier_ns"] for row in rows.values()),
        "raw_aim_met": sum(row["raw_aim_met"] is True for row in rows.values()),
        "raw_aim_missed": sum(row["raw_aim_met"] is False for row in rows.values()),
        "numeric_target_unset": sum(row["raw_aim_met"] is None for row in rows.values()),
    }
    print(json.dumps({"schema": "issue232-all-ioctl-derived-v3", "summary": summary,
                      "rows": rows}, indent=2, sort_keys=True))
