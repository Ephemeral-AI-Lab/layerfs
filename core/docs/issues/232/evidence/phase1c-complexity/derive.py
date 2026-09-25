#!/usr/bin/env python3
"""Reproduce Phase 1C dispositions from retained one-shot raw files."""

import hashlib
import json
from pathlib import Path


HERE = Path(__file__).resolve().parent
PRE = json.loads((HERE / "PRE_RUN.json").read_text())
REGISTRY_PATH = (HERE / "../../../../../benchmark/fs-bench-pro/registry/workspace-exec-complexity-v1.json").resolve()
assert hashlib.sha256(REGISTRY_PATH.read_bytes()).hexdigest() == PRE["registry_sha256"]
REGISTRY = json.loads(REGISTRY_PATH.read_text())


def fields(line):
    return {key: int(value) if value.isdecimal() else value
            for key, value in (item.split("=", 1) for item in line.split()[1:])}


def row(case):
    label = case["label"]
    folder = HERE / "attempt-01" / label
    for line in (folder / "SHA256SUMS").read_text().splitlines():
        sha, name = line.split("  ", 1)
        assert hashlib.sha256((folder / name).read_bytes()).hexdigest() == sha
    receipt = json.loads((folder / "receipt.json").read_text())
    verifier = json.loads((folder / "verification.json").read_text())
    driver = json.loads((folder / "driver-receipt.json").read_text())
    post = json.loads((folder / "POST_RUN.json").read_text())
    assert receipt["sample_count"] == 1
    assert receipt["source_commit"] == "66c702a08fd916f08a1839bbd1bc8256a4e42903"
    assert receipt["product_seal"] == PRE["product_seal"]
    assert receipt["harness_seal"] == PRE["harness_seal"]
    assert receipt["registry_sha256"] == PRE["registry_sha256"]
    assert receipt["image_id"] == PRE["image"]["image_id"]
    assert receipt["cache"]["linux-fuse-backing"]["status"] == "INELIGIBLE"
    lines = (folder / "driver.raw.stderr").read_text().splitlines()
    named = {}
    for kind in ("LFS_PIECE_COUNT", "LFS_PIECE_PAGES", "LFS_PIECE_LOWER", "LFS_C1_EDIT_COUNT"):
        named[kind] = [fields(line) for line in lines if line.startswith(kind + " ")]
    pieces = named["LFS_PIECE_COUNT"]
    pages = named["LFS_PIECE_PAGES"]
    lower = named["LFS_PIECE_LOWER"]
    c1 = named["LFS_C1_EDIT_COUNT"]
    assert len(pieces) == len(pages) == case["edit_count"]
    assert [item["new"] for item in pieces] == [item["pieces"] for item in pages]
    visits = [sum(item[key] for key in ("prefix_visits", "suffix_visits",
                                        "replacement_visits", "summary_visits")) for item in pieces]
    lft = [json.loads(line[5:]) for line in lines if line.startswith("LFT1 ")]
    roots = [item for item in lft if item.get("kind") == "operation"
             and item.get("namespace") == 7 and item.get("role") == 1
             and item.get("key") == 232000]
    assert len(roots) == 1
    root = roots[0]
    children = {item["name"]: item["elapsed_ns"] for item in root["timing"]["children"]}
    if receipt["status"] == "INELIGIBLE":
        assert len(lower) == len(c1) == 1 and root["success"]
        assert verifier["status"] == "PASS" and verifier["wall_ns"] < 10_000_000_000
        assert driver["unmount_ok"] and driver["sandbox_delete_ok"]
        assert receipt["route_ok"] and receipt["cache"]["macos-store"]["status"] == "PASS"
        assert verifier["child"]["content_match"] and verifier["child"]["cutoff_match"]
    else:
        assert label == "repeated-128" and receipt["status"] == "FAIL"
        assert not lower and not c1 and not root["success"]
        assert verifier["status"] == "NOT_RUN"
    return {
        "status": receipt["status"], "sample_count": 1,
        "complete_command_ns": receipt["complete_command_wall_ns"],
        "driver_status": driver["status"], "driver_detail": driver["detail"],
        "verifier_status": verifier["status"], "verifier_ns": verifier.get("wall_ns"),
        "unmount_ok": driver["unmount_ok"], "sandbox_delete_ok": driver["sandbox_delete_ok"],
        "lft_root_ns": root["timing"]["elapsed_ns"], "lft_root_success": root["success"],
        "lft_children_ns": children, "lft_samples": root["samples"],
        "lft_resource_status": root["resource_status"],
        "lft_cpu_shared_ns": root["cpu_shared_ns"],
        "lft_sampled_max_rss": root["sampled_max_rss"],
        "lft_scope_first_ns": root["first_ns"], "lft_scope_opened_ns": root["opened_ns"],
        "lft_scope_last_ns": root["last_ns"], "lft_scope_closed_ns": root["closed_ns"],
        "piece_count_after_each": [item["new"] for item in pieces],
        "piece_visits_per_edit": visits, "piece_visits_total": sum(visits),
        "piece_pages_per_edit": [item["written"] for item in pages],
        "piece_pages_total": sum(item["written"] for item in pages),
        "lower_piece_visits": lower[0]["pieces"] if lower else None,
        "c1_counts": c1[0] if c1 else None,
        "callbacks": driver["projection_counts"],
        "accepted_bytes": driver["range_accepted_payload_bytes"],
        "shifted_suffix_bytes": driver["range_shifted_suffix_bytes"],
        "canonical_root": (verifier.get("child") or {}).get("canonical_root"),
        "canonical_extents": (verifier.get("child") or {}).get("extent_count"),
        "chunked": (verifier.get("child") or {}).get("chunked"),
        "post_run_store_pages": post["files"]["store.sqlite"]["page_count"],
    }


if __name__ == "__main__":
    result = {case["label"]: row(case) for case in REGISTRY["cases"]}
    print(json.dumps({"schema": "issue232-phase1c-derived-v1", "rows": result},
                     indent=2, sort_keys=True))
