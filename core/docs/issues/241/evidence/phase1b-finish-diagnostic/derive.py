#!/usr/bin/env python3
"""Derive the Phase 1B decision from the two immutable diagnostic receipts."""

import json
from pathlib import Path


ROOT = Path(__file__).resolve().parent
PRE = json.loads((ROOT / "PRE_RUN.json").read_text())
EXPECTED_CHILDREN = (
    "storage.finish.pack_seal",
    "storage.finish.transaction_begin",
    "storage.finish.candidate_flush",
    "storage.finish.ownership_publish",
    "storage.finish.ordinal_watermark",
    "storage.finish.sqlite_commit",
    "storage.finish.pool_clone",
    "storage.finish.candidates_clone",
)


def child(parent, name):
    matches = [item for item in parent["children"] if item["name"] == name]
    assert len(matches) == 1, (parent["name"], name, len(matches))
    return matches[0]


def row(label):
    base = ROOT / "attempt-01" / label
    read = lambda name: json.loads((base / name).read_text())
    receipt, run, verifier, driver, post = (
        read(name)
        for name in (
            "receipt.json", "run.json", "verification.json",
            "driver-receipt.json", "POST_RUN.json",
        )
    )
    declared = next(case for case in PRE["cases"] if case["label"] == label)
    assert run["identity"]["source_commit"] == "09a2084175d2f9c6ad2afc18b49a418743ef6627"
    assert run["identity"]["product_seal"] == PRE["product_seal"]
    assert run["identity"]["harness_seal"] == PRE["harness_seal"]
    assert receipt["scenario_id"] == declared["scenario_id"]
    assert receipt["sample_count"] == 1 and receipt["status"] == "INELIGIBLE"
    assert receipt["complete_command_status"] == "PASS"
    assert receipt["complete_command_wall_ns"] <= PRE["complete_command_budget_ns"]
    assert receipt["cache"]["linux-fuse-backing"]["status"] == "INELIGIBLE"
    assert receipt["cache"]["macos-store"]["resident_pages"] == 0
    assert verifier["status"] == "PASS" and verifier["identity_match"]
    assert verifier["wall_ns"] < PRE["independent_verifier_strictly_less_than_ns"]
    assert driver["status"] == "COMPLETE" and driver["unmount_ok"] and driver["sandbox_delete_ok"]
    assert driver["range_accepted_payload_bytes"] == 4096
    assert driver["range_shifted_suffix_bytes"] == 0
    assert "range_state=2,range_edit=1" in driver["projection_counts"]
    lines = (base / "driver.raw.stderr").read_text().splitlines()
    op = [json.loads(line[5:]) for line in lines if line.startswith("LFT1 ")]
    edit = [item for item in op if item.get("namespace") == 7 and item.get("role") == 1
            and item.get("kind") == "operation" and item.get("key") == 8
            and item["timing"]["name"] == "EditFile"]
    assert len(edit) == 1
    finish = child(edit[0]["timing"], "service.finish")
    drain, owner = (child(finish, "storage.finish." + name) for name in ("drain", "owner"))
    assert len(finish["children"]) == 2 and len(owner["children"]) == len(EXPECTED_CHILDREN)
    parts = {name.rsplit(".", 1)[-1]: child(owner, name)["elapsed_ns"]
             for name in EXPECTED_CHILDREN}
    finish_residual = finish["elapsed_ns"] - drain["elapsed_ns"] - owner["elapsed_ns"]
    owner_residual = owner["elapsed_ns"] - sum(parts.values())
    assert finish_residual >= 0 and owner_residual >= 0
    counts = [line for line in lines if line.startswith("LFS_FINISH_COUNT v=1 request=8 ")]
    assert len(counts) == 1
    values = dict(token.split("=", 1) for token in counts[0].split()[1:])
    counts = {key: int(value) if value.isdecimal() else value for key, value in values.items()}
    assert counts["phase_pages"] == counts["phase_physical_read_bytes"] == "UNAVAILABLE"
    return {
        "scenario_id": receipt["scenario_id"], "status": receipt["status"],
        "complete_command_wall_ns": receipt["complete_command_wall_ns"],
        "raw_edit_commit_ns": receipt["raw_edit_commit_ns"],
        "verifier_wall_ns": verifier["wall_ns"], "verifier_status": verifier["status"],
        "cleanup": "PASS", "macos_store_pre_sample_resident_pages": 0,
        "linux_fuse_backing": "INELIGIBLE", "edit_lft1_ns": edit[0]["timing"]["elapsed_ns"],
        "edit_lft1_resource_status": edit[0]["resource_status"],
        "edit_lft1_samples": edit[0]["samples"],
        "edit_lft1_cpu_shared_ns": edit[0]["cpu_shared_ns"],
        "finish_ns": finish["elapsed_ns"], "drain_ns": drain["elapsed_ns"],
        "owner_ns": owner["elapsed_ns"], "finish_residual_ns": finish_residual,
        "owner_parts_ns": parts, "owner_residual_ns": owner_residual,
        "counts": counts,
        "process_wide_disk_read_delta_bytes": driver["process_disk_read_bytes_delta"],
        "post_run_store_page_count": post["files"]["store.sqlite"]["page_count"],
        "post_run_store_page_size": post["files"]["store.sqlite"]["page_size"],
    }


if __name__ == "__main__":
    small, large = (row(label) for label in ("1mib", "500mib"))
    growth = {key + "_ns": large[key + "_ns"] - small[key + "_ns"]
              for key in ("finish", "drain", "owner")}
    growth["drain_share_of_finish_growth"] = growth["drain_ns"] / growth["finish_ns"]
    print(json.dumps({"schema": "issue241-phase1b-derived-v1", "1mib": small,
                      "500mib": large, "growth": growth}, indent=2, sort_keys=True))
