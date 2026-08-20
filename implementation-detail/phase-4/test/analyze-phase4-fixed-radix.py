#!/usr/bin/env python3
"""Validate and summarize the compact K64/F64 WP4-M JSONL."""

import hashlib
import json
import re
import sys

SIZES = (1_048_576, 10_485_760, 104_857_600)
OPS = ("write", "edit-same", "edit-plus1-early", "edit-plus1-middle")
ARMS = tuple((size, "write") for size in SIZES) + tuple((SIZES[-1], op) for op in OPS[1:])
ENGINE_OPS = dict(zip(OPS, ("full", "same-middle", "plus1-early", "plus1-middle")))
PROFILE = "b0ebb845409ef995a5fa454bb23d10a80c6ecf44deb7832ca2ce1213eb0f4ba1"
HEX64 = re.compile(r"[0-9a-f]{64}\Z")
APPROVED_100_GIB_MIDDLE_BUDGET = {
    "approved": True,
    "old_reference_count": 5_410_816,
    "insertion_ordinal": 2_705_408,
    "rebuilt_reference_occurrences": 2_705_409,
    "changed_leaves": 42_273,
    "changed_branches": 673,
    "mapping_objects": 42_947,
    "canonical_mapping_bytes": 186_891_342,
    "latency_projection": False,
}


def stat(values):
    ordered = sorted(values)
    return {
        "count": len(ordered),
        "median_ns": ordered[len(ordered) // 2],
        "min_ns": ordered[0],
        "max_ns": ordered[-1],
        "spread_ns": ordered[-1] - ordered[0],
    }


def ratio(numerator, denominator):
    return {
        "numerator": numerator,
        "denominator": denominator,
        "decimal": f"{numerator / denominator:.6f}" if denominator else "Unavailable",
    }


def expected_changed_counts(old_references, insertion_ordinal):
    new_leaves = (old_references + 64) // 64
    first_leaf = insertion_ordinal // 64
    leaves = new_leaves - first_leaf
    branches = 0
    total, first = new_leaves, first_leaf
    while total > 64:
        total = (total + 63) // 64
        first //= 64
        branches += total - first
    return leaves, branches


def analyze(rows, raw_sha256):
    reasons = []
    fail = lambda condition, reason: reasons.append(reason) if condition else None
    campaign = [row for row in rows if row.get("row_kind") in ("warmup", "measured")]
    roundtrips = [row for row in rows if row.get("row_kind") == "roundtrip-check"]
    unknown = [row for row in rows if row.get("row_kind") not in ("warmup", "measured", "roundtrip-check")]
    fail(len(campaign) != 24, f"campaign row count {len(campaign)} != 24")
    fail(sum(row.get("row_kind") == "warmup" for row in campaign) != 6, "warmup row count != 6")
    fail(sum(row.get("row_kind") == "measured" for row in campaign) != 18, "measured row count != 18")
    fail(len(roundtrips) != 3, f"roundtrip row count {len(roundtrips)} != 3")
    fail(bool(unknown), f"unknown row_kind count {len(unknown)}")

    expected = {(size, op, kind, sample) for size, op in ARMS
                for kind, samples in (("warmup", (0,)), ("measured", range(1, 4))) for sample in samples}
    actual = [(row.get("size_bytes"), row.get("operation"), row.get("row_kind"), row.get("sample_index")) for row in campaign]
    fail(set(actual) != expected or len(actual) != len(set(actual)), "campaign schedule is not exactly 6 arms x (1+3)")
    expected_roundtrips = {(size, "write", "roundtrip-check", None) for size in SIZES}
    actual_roundtrips = {(row.get("size_bytes"), row.get("operation"), row.get("row_kind"), row.get("sample_index")) for row in roundtrips}
    fail(actual_roundtrips != expected_roundtrips, "roundtrip schedule is not one write per size")

    required_hashes = ("fixture_sha256", "source_fingerprint", "executable_sha256", "runner_sha256")
    for index, row in enumerate(rows):
        prefix = f"row {index}"
        fail(row.get("schema") != "wp4m-fixed-radix-acceptance-row-v1", f"{prefix}: schema")
        fail(row.get("purpose") != "fixed_radix_acceptance", f"{prefix}: purpose")
        fail(row.get("milestone") != "WP4-M-FIXED-RADIX", f"{prefix}: milestone")
        fail(row.get("status") != "PASS", f"{prefix}: status")
        fail(row.get("candidate") != "K64-F64" or row.get("profile_id") != PROFILE, f"{prefix}: profile")
        fail(row.get("qualification") is not False or row.get("promotion") is not False, f"{prefix}: qualification/promotion")
        fail(row.get("engine_operation") != ENGINE_OPS.get(row.get("operation")), f"{prefix}: operation mapping")
        fail(row.get("validation_scope") != ("complete-roundtrip" if row.get("row_kind") == "roundtrip-check" else "capture-only"), f"{prefix}: validation scope")
        fail(any(not HEX64.fullmatch(str(row.get(field, ""))) for field in required_hashes), f"{prefix}: custody hash")
        fail(any(not HEX64.fullmatch(str(row.get(field, ""))) for field in ("root_id", "transition_id", "ordered_closure_digest")), f"{prefix}: result identity")
        fail(row.get("actual_cdc_references") != row.get("expected_cdc_references"), f"{prefix}: CDC count")
        fail(row.get("runner_wall_ceiling_seconds") != 120 or row.get("runner_command_ceiling_seconds") != 60,
             f"{prefix}: runner ceiling")
        fail(any(row.get(field) != value for field, value in (
            ("transactions", 1), ("commits", 1), ("commit_dispatches", 1),
            ("commit_returns", 1), ("commit_return_successes", 1),
            ("commit_return_errors", 0), ("q_current", 0))), f"{prefix}: transaction/COMMIT/Q")
        fail(not row.get("commit_timer_equation_matches", False), f"{prefix}: COMMIT timer equation")
        fail(not row.get("durable_phase_sum_matches", False), f"{prefix}: durable timer equation")
        fail(not isinstance(row.get("capture_publish_wall_ns"), int) or row.get("capture_publish_wall_ns", 0) <= 0, f"{prefix}: publish timer")
        fail(not isinstance(row.get("complete_lifecycle_total_wall_ns"), int) or row.get("complete_lifecycle_total_wall_ns", 0) <= 0, f"{prefix}: lifecycle timer")
        if row.get("operation") != "write":
            fail(any(not HEX64.fullmatch(str(row.get(field, ""))) for field in
                     ("pre_edit_database_sha256", "pre_edit_authority_sha256", "pre_edit_expectations_sha256")), f"{prefix}: pre-edit custody")

    fail(len({row.get("executable_sha256") for row in rows}) != 1, "multiple executable identities")
    fail(len({row.get("runner_sha256") for row in rows}) != 1, "multiple runner identities")
    for size in SIZES:
        sized = [row for row in rows if row.get("size_bytes") == size]
        fail(len({row.get("source_fingerprint") for row in sized}) != 1, f"{size}: multiple source identities")
        fail(len({(row.get("fixture"), row.get("fixture_sha256")) for row in sized}) != 1, f"{size}: multiple fixture identities")
        for op in [arm_op for arm_size, arm_op in ARMS if arm_size == size]:
            arm = [row for row in campaign if row.get("size_bytes") == size and row.get("operation") == op]
            fail(len({(row.get("root_id"), row.get("transition_id"), row.get("ordered_closure_digest")) for row in arm}) != 1, f"{size}/{op}: unstable result identities")
            if op != "write":
                fail(len({(row.get("pre_edit_database_sha256"), row.get("pre_edit_authority_sha256"), row.get("pre_edit_expectations_sha256")) for row in arm}) != 1, f"{size}/{op}: unstable pre-edit custody")
        write_identity = {(row.get("root_id"), row.get("transition_id"), row.get("ordered_closure_digest")) for row in campaign if row.get("size_bytes") == size and row.get("operation") == "write"}
        rt_identity = {(row.get("root_id"), row.get("transition_id"), row.get("ordered_closure_digest")) for row in roundtrips if row.get("size_bytes") == size}
        fail(write_identity != rt_identity, f"{size}: roundtrip identity differs from write")

    suffix_fields = ("suffix_references", "suffix_bytes", "suffix_objects", "pages", "branches", "mapping_bytes_rewritten")
    suffix_summary = []
    for size in (SIZES[-1],):
        for op in OPS[2:]:
            arm = [row for row in campaign if row.get("size_bytes") == size and row.get("operation") == op]
            signatures = {tuple(row.get(field) for field in suffix_fields) for row in arm}
            fail(len(signatures) != 1, f"{size}/{op}: unstable suffix counters")
            if not arm:
                continue
            row = arm[0]
            model = row.get("suffix_model", {})
            old = model.get("old_references")
            position = 0 if op.endswith("early") else (old // 2 if isinstance(old, int) else -1)
            leaves, branches = expected_changed_counts(old, position) if isinstance(old, int) and old >= 0 else (-1, -1)
            expected_model = {
                "kind": "ordinal-fixed-radix-suffix-linear-v1",
                "old_references": old,
                "insertion_ordinal": position,
                "rewritten_references": old - position if isinstance(old, int) else -1,
                "rewritten_raw_bytes": row.get("suffix_bytes"),
                "authenticated_objects": row.get("suffix_objects"),
                "rewritten_pages": row.get("pages"),
                "rewritten_branches": row.get("branches"),
                "rewritten_mapping_bytes": row.get("mapping_bytes_rewritten"),
            }
            fail(model != expected_model, f"{size}/{op}: suffix model fields")
            fail(row.get("suffix_references") != expected_model["rewritten_references"], f"{size}/{op}: suffix reference equation")
            fail(row.get("pages") != leaves or row.get("branches") != branches, f"{size}/{op}: fixed-radix topology equation")
            fail(any(other.get("suffix_model") != model for other in arm), f"{size}/{op}: unstable suffix model")
            suffix_summary.append({
                "size_bytes": size, "operation": op,
                "source_suffix_references": row.get("suffix_references"),
                "rebuilt_reference_occurrences": row.get("suffix_references", -1) + 1,
                "rewritten_raw_bytes": row.get("suffix_bytes"),
                "authenticated_objects": row.get("suffix_objects"),
                "changed_leaves": row.get("pages"), "changed_branches": row.get("branches"),
                "mapping_objects": row.get("pages", -1) + row.get("branches", -1) + 1,
                "canonical_mapping_bytes": row.get("mapping_bytes_rewritten"),
            })

    arms = []
    arm_index = {}
    for size, op in ARMS:
        measured = [row for row in campaign if row.get("size_bytes") == size and row.get("operation") == op and row.get("row_kind") == "measured"]
        if len(measured) != 3:
            continue
        item = {
            "size_bytes": size, "operation": op,
            "publish_wall": stat([row["capture_publish_wall_ns"] for row in measured]),
            "complete_wall": stat([row["complete_lifecycle_total_wall_ns"] for row in measured]),
            "mapping_bytes_rewritten": measured[0].get("mapping_bytes_rewritten"),
        }
        arms.append(item)
        arm_index[(size, op)] = item
    slopes = []
    for op in ("write",):
        for small_size, large_size in ((SIZES[0], SIZES[1]), (SIZES[1], SIZES[2]), (SIZES[0], SIZES[2])):
            small, large = arm_index.get((small_size, op)), arm_index.get((large_size, op))
            if small and large:
                slopes.append({
                    "operation": op, "from_size_bytes": small_size, "to_size_bytes": large_size,
                    "publish_wall": ratio(large["publish_wall"]["median_ns"], small["publish_wall"]["median_ns"]),
                    "complete_wall": ratio(large["complete_wall"]["median_ns"], small["complete_wall"]["median_ns"]),
                    "mapping_bytes": ratio(large["mapping_bytes_rewritten"], small["mapping_bytes_rewritten"]),
                })
    alarms = []
    for size in (SIZES[-1],):
        full = arm_index.get((size, "write"))
        if full:
            for op in OPS[2:]:
                edit = arm_index.get((size, op))
                if edit:
                    alarms.append({"size_bytes": size, "operation": op,
                                   "publish_to_write_percent": ratio(edit["publish_wall"]["median_ns"] * 100, full["publish_wall"]["median_ns"])["decimal"],
                                   "binding": False})

    return {
        "schema": "wp4m-fixed-radix-analysis-v1",
        "status": "FAIL" if reasons else "PASS",
        "reasons": sorted(set(reasons)),
        "row_counts": {"campaign": len(campaign), "warmup": sum(row.get("row_kind") == "warmup" for row in campaign), "measured": sum(row.get("row_kind") == "measured" for row in campaign), "roundtrip": len(roundtrips)},
        "custody": {"raw_jsonl_sha256": raw_sha256,
                    "executable_sha256": next(iter({row.get("executable_sha256") for row in rows}), None),
                    "runner_sha256": next(iter({row.get("runner_sha256") for row in rows}), None)},
        "arms": arms,
        "slopes": slopes,
        "suffix_models": suffix_summary,
        "local_five_percent_alarm": alarms,
        "approved_100_gib_middle_budget": APPROVED_100_GIB_MIDDLE_BUDGET,
        "routine_contract": {"sizes_bytes": list(SIZES), "capture_rows": 24,
                             "roundtrip_rows": 3, "runner_ceiling_seconds": 120,
                             "command_ceiling_seconds": 60,
                             "size_512_mib_closes_wp4m": False},
        "disposition": {"qualification": False, "promotion": False, "directory_default": "DIR256K-unmeasured-fallback"},
    }


def self_test_rows():
    h = lambda value: hashlib.sha256(value.encode()).hexdigest()
    rows = []
    refs = {SIZES[0]: 53, SIZES[1]: 531, SIZES[2]: 5_284}
    for size in SIZES:
        identities = {}
        for op in [arm_op for arm_size, arm_op in ARMS if arm_size == size]:
            identities[op] = (h(f"root-{size}-{op}"), h(f"transition-{size}-{op}"), h(f"closure-{size}-{op}"))
            for kind, samples in (("warmup", (0,)), ("measured", range(1, 4))):
                for sample in samples:
                    old = refs[size]
                    position = 0 if op.endswith("early") else old // 2
                    suffix = old - position if op.startswith("edit-plus1") else 0
                    leaves, branches = expected_changed_counts(old, position) if suffix else (1, 1)
                    row = {
                        "schema": "wp4m-fixed-radix-acceptance-row-v1", "purpose": "fixed_radix_acceptance", "milestone": "WP4-M-FIXED-RADIX",
                        "status": "PASS", "candidate": "K64-F64", "profile_id": PROFILE, "qualification": False, "promotion": False,
                        "size_bytes": size, "operation": op, "engine_operation": ENGINE_OPS[op], "row_kind": kind, "sample_index": sample,
                        "validation_scope": "capture-only", "fixture": f"S-{size}", "fixture_sha256": h(f"fixture-{size}"),
                        "source_fingerprint": h(f"source-{size}"), "executable_sha256": h("exe"), "runner_sha256": h("runner"),
                        "pre_edit_database_sha256": h(f"db-{size}-{op}"), "pre_edit_authority_sha256": h(f"authority-{size}-{op}"),
                        "pre_edit_expectations_sha256": h(f"expectations-{size}-{op}"), "root_id": identities[op][0],
                        "transition_id": identities[op][1], "ordered_closure_digest": identities[op][2],
                        "actual_cdc_references": old + (1 if op.startswith("edit-plus1") else 0),
                        "expected_cdc_references": old + (1 if op.startswith("edit-plus1") else 0),
                        "runner_wall_ceiling_seconds": 120, "runner_command_ceiling_seconds": 60,
                        "transactions": 1, "commits": 1, "commit_dispatches": 1, "commit_returns": 1,
                        "commit_return_successes": 1, "commit_return_errors": 0, "q_current": 0,
                        "commit_timer_equation_matches": True, "durable_phase_sum_matches": True,
                        "capture_publish_wall_ns": size + sample * 1000 + OPS.index(op) * 100,
                        "complete_lifecycle_total_wall_ns": size * 2 + sample * 1000 + OPS.index(op) * 100,
                        "suffix_references": suffix, "suffix_bytes": suffix * 20_000, "suffix_objects": leaves * 2 + branches,
                        "pages": leaves, "branches": branches, "mapping_bytes_rewritten": (suffix + 1) * 68 + leaves * 28 + branches * 69 + 49,
                    }
                    if suffix:
                        row["suffix_model"] = {"kind": "ordinal-fixed-radix-suffix-linear-v1", "old_references": old,
                            "insertion_ordinal": position, "rewritten_references": suffix, "rewritten_raw_bytes": row["suffix_bytes"],
                            "authenticated_objects": row["suffix_objects"], "rewritten_pages": leaves, "rewritten_branches": branches,
                            "rewritten_mapping_bytes": row["mapping_bytes_rewritten"]}
                    rows.append(row)
        base = next(row for row in rows if row["size_bytes"] == size and row["operation"] == "write")
        rt = dict(base, row_kind="roundtrip-check", sample_index=None, validation_scope="complete-roundtrip",
                  capture_publish_wall_ns=size, complete_lifecycle_total_wall_ns=size * 3)
        rows.append(rt)
    return rows


def main():
    if sys.argv[1:] == ["--self-test"]:
        rows = self_test_rows()
        assert analyze(rows, h := hashlib.sha256(b"self-test").hexdigest())["status"] == "PASS"
        broken = [dict(row) for row in rows]
        broken[0]["commits"] = 2
        assert analyze(broken, h)["status"] == "FAIL"
        print("PASS")
        return 0
    if len(sys.argv) != 2:
        print(f"usage: {sys.argv[0]} RAW.jsonl | --self-test", file=sys.stderr)
        return 2
    try:
        raw = open(sys.argv[1], "rb").read()
        rows = [json.loads(line) for line in raw.decode().splitlines() if line.strip()]
        result = analyze(rows, hashlib.sha256(raw).hexdigest())
    except Exception as error:
        result = {"schema": "wp4m-fixed-radix-analysis-v1", "status": "FAIL", "reasons": [f"input: {error}"]}
    print(json.dumps(result, sort_keys=True, separators=(",", ":")))
    return result["status"] != "PASS"


if __name__ == "__main__":
    raise SystemExit(main())
