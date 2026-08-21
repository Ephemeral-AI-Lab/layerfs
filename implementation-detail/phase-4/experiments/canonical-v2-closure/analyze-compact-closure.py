#!/usr/bin/env python3
"""Fail-closed analyzer for the single compact canonical-v2 closure loop."""

import csv
import hashlib
import json
import statistics
import sys
from pathlib import Path

CONTROL_PROFILE = "b0ebb845409ef995a5fa454bb23d10a80c6ecf44deb7832ca2ce1213eb0f4ba1"
CANDIDATE_PROFILE = "94a03ba7b6c97b5ff37c0ec62ef1d801b9896494b45456bd3df23e2cb278d13b"
SOURCE = {
    1_048_576: ("f79de600cf44b20c4443e06d2e2b9e8819e956ba5a7bcc9cab4ffd8a08059cf8", 53),
    10_485_760: ("e40db05d7407b92253e56099df402f03b399990014b2d1397e422ca305472449", 531),
    104_857_600: ("bb883eecf4ea85d80432953791dcc352243da94175e7503e2c476afe9bd0bab7", 5284),
}
FULL_CLOSURE = {
    "A": {
        1_048_576: "f9c0e593b97e0430ec81e9ef763fa005715b465ca99001835f2acba0794a7ee2",
        10_485_760: "535d3cd52a0d6a2a25bce7e00ac19632211fd96fce49dd51f802fd5679223a59",
        104_857_600: "d6aac6e40cc851dd6295dbeec6488f1c5ebefa7520f86b0cd12bdcdce1f0d54a",
    },
    "B": {
        1_048_576: "7e806f7023c3e33914c59d2b0d0d84bca8859fdbd7663b55f5f5c99313252d42",
        10_485_760: "35282fcfecc493c025a3bc4a7567efc12562fc8a4d863c88e07617fb5e97d1c9",
        104_857_600: "29233d6018b031f6035c8e2c8175f1ab86fb721808f21a8858bc965567e9c0c1",
    },
}


def expected_schedule():
    rows = []

    def add(label, kind, size, operation, order, pair="-", comparable=True):
        arms = list(order) if comparable else ["B"]
        for arm in arms:
            rows.append({
                "sequence": len(rows) + 1,
                "label": f"{label}-{arm}",
                "kind": kind,
                "size": size,
                "operation": operation,
                "arm": arm,
                "pair": str(pair),
                "order": order,
                "comparable": comparable,
            })

    add("warm-full-100", "warmup", 104_857_600, "full", "AB")
    add("scale-full-1", "scaling", 1_048_576, "full", "AB")
    add("scale-full-10", "scaling", 10_485_760, "full", "BA")
    add("primary-full-100-p0", "primary", 104_857_600, "full", "AB", 0)
    add("primary-full-100-p1", "primary", 104_857_600, "full", "BA", 1)
    for label, operation, order in [
        ("guard-same", "same-middle", "AB"),
        ("guard-plus1-early", "plus1-early", "BA"),
        ("guard-plus1-middle", "plus1-middle", "AB"),
        ("guard-materialize-warm", "materialize-warm", "BA"),
        ("guard-materialize-fresh", "materialize-fresh", "AB"),
        ("guard-reopen", "reopen", "BA"),
        ("guard-range1m", "read-range-1m", "AB"),
    ]:
        add(label, "guard", 104_857_600, operation, order)
    for label, operation in [
        ("guard-one-byte-early", "one-byte-early"),
        ("guard-one-byte-middle", "one-byte-middle"),
        ("guard-one-byte-late", "one-byte-late"),
        ("guard-first-edit", "first-edit-after-reopen"),
        ("guard-scrub", "scrub-only"),
    ]:
        add(label, "candidate-only", 104_857_600, operation, "B", comparable=False)
    return rows


def load_rows(root):
    schedule = list(csv.DictReader((root / "SCHEDULE-v1.tsv").open(), delimiter="\t"))
    raw = [json.loads(line) for line in (root / "RAW-v1.jsonl").read_text().splitlines() if line]
    expected = expected_schedule()
    projected = [{k: str(row[k]) for k in ("sequence", "label", "kind", "size", "operation", "arm", "pair", "order", "comparable")} for row in expected]
    actual = [{k: row[k] for k in projected[0]} for row in schedule] if schedule else []
    return expected, projected, actual, raw


def int_nonnegative(row, key):
    return isinstance(row.get(key), int) and not isinstance(row.get(key), bool) and row[key] >= 0


def analyze(root):
    reasons = []
    expected, projected, schedule, rows = load_rows(root)
    if schedule != projected:
        reasons.append("schedule")
    if len(rows) != len(expected):
        reasons.append("row-count")
    control_sha = (root / "CONTROL-SHA256-v1.txt").read_text().strip()
    candidate_sha = (root / "CANDIDATE-SHA256-v1.txt").read_text().strip()
    joined = list(zip(expected, rows)) if len(rows) == len(expected) else []
    one_byte_offsets = []
    external = {}
    time_path = root / "EXTERNAL-TIME-v1.tsv"
    if time_path.is_file():
        external = {row["label"]: row for row in csv.DictReader(time_path.open(), delimiter="\t")}

    mutation = {"full", "same-middle", "plus1-early", "plus1-middle", "one-byte-early", "one-byte-middle", "one-byte-late", "first-edit-after-reopen"}
    reads = {"materialize-warm", "materialize-fresh", "reopen", "read-range-1m", "scrub-only"}
    for spec, row in joined:
        tag = spec["label"]
        arm = spec["arm"]
        size = spec["size"]
        operation = spec["operation"]
        expected_profile = CONTROL_PROFILE if arm == "A" else CANDIDATE_PROFILE
        expected_executable = control_sha if arm == "A" else candidate_sha
        if row.get("status") != "PASS" or row.get("error") is not None:
            reasons.append(f"{tag}:status")
        if row.get("profile_id") != expected_profile or row.get("executable_sha256") != expected_executable:
            reasons.append(f"{tag}:operand")
        if row.get("size_bytes") != size or row.get("operation") != operation:
            reasons.append(f"{tag}:identity")
        source, base_references = SOURCE[size]
        if row.get("source_fingerprint") != source:
            reasons.append(f"{tag}:source")
        if operation == "full" and (row.get("expected_cdc_references"), row.get("actual_cdc_references")) != (base_references, base_references):
            reasons.append(f"{tag}:full-references")
        if operation == "full" and row.get("ordered_closure_digest") != FULL_CLOSURE[arm][size]:
            reasons.append(f"{tag}:full-closure")
        expected_transactions = 1 if operation in mutation else 0
        if (row.get("transactions"), row.get("commits")) != (expected_transactions, expected_transactions):
            reasons.append(f"{tag}:transaction")
        if operation in mutation and (row.get("commit_dispatches"), row.get("commit_returns"), row.get("commit_return_successes"), row.get("commit_return_errors")) != (1, 1, 1, 0):
            reasons.append(f"{tag}:commit")
        if row.get("sqlite_runtime_journal_mode") != "delete" or row.get("sqlite_runtime_synchronous") != 2:
            reasons.append(f"{tag}:durability")
        q_ceiling = 131_072 if operation == "full" else 4_194_304
        if row.get("q_current") != 0 or not int_nonnegative(row, "q_high_water") or row.get("q_high_water", 10**9) > q_ceiling:
            reasons.append(f"{tag}:q")
        if row.get("physical_journal_apparent_bytes") != 0 or row.get("q_fixed_envelope_removed") is not True:
            reasons.append(f"{tag}:cleanup")
        if row.get("base_preparation_in_measured_interval") is not False:
            reasons.append(f"{tag}:preparation")
        if operation in mutation and (row.get("durable_phase_sum_matches") is not True or row.get("commit_timer_equation_matches") is not True):
            reasons.append(f"{tag}:timer")
        if operation in reads and row.get("complete_lifecycle_total_wall_ns", 0) <= 0:
            reasons.append(f"{tag}:read-timer")
        for key in ("sql_calls", "row_blob_reads", "row_blob_writes", "w_bytes", "payload_io_bytes"):
            if not int_nonnegative(row, key):
                reasons.append(f"{tag}:{key}")
        if tag not in external:
            reasons.append(f"{tag}:external-time")

        if operation.startswith("one-byte-") or operation == "first-edit-after-reopen":
            old = row.get("edit_removed_hex")
            new = row.get("edit_inserted_hex")
            before = row.get("edit_reference_count_before")
            after = row.get("edit_reference_count_after")
            classification = "same-count" if before == after else "count-changing"
            if not (isinstance(old, str) and isinstance(new, str) and len(old) == len(new) == 2):
                reasons.append(f"{tag}:one-byte")
            else:
                if int(new, 16) != (int(old, 16) ^ 0x5A):
                    reasons.append(f"{tag}:replacement")
            if row.get("edit_count_classification") != classification or not isinstance(row.get("edit_offset"), int):
                reasons.append(f"{tag}:classification")
            if operation.startswith("one-byte-"):
                one_byte_offsets.append(row.get("edit_offset"))
            if row.get("rejoin_scan_bytes", 0) <= 0 or row.get("suffix_references", 0) <= 0:
                reasons.append(f"{tag}:bounded-rejoin")

        if operation == "first-edit-after-reopen":
            reopen = row.get("fresh_reopen_head_wall_ns")
            authority = row.get("same_open_authority_establishment_wall_ns")
            publication = row.get("durable_capture_total_wall_ns")
            total = row.get("complete_lifecycle_total_wall_ns")
            if not all(isinstance(value, int) and value > 0 for value in (reopen, authority, publication, total)) or total != reopen + authority + publication:
                reasons.append(f"{tag}:three-phase")

    if len(one_byte_offsets) != 3 or not all(isinstance(value, int) for value in one_byte_offsets) or one_byte_offsets != sorted(set(one_byte_offsets)):
        reasons.append("one-byte-offset-order")

    by_label = {spec["label"]: row for spec, row in joined}
    pairs = []
    for pair in (0, 1):
        prefix = f"primary-full-100-p{pair}"
        a = by_label.get(prefix + "-A", {})
        b = by_label.get(prefix + "-B", {})
        a_ns = a.get("durable_capture_total_wall_ns")
        b_ns = b.get("durable_capture_total_wall_ns")
        if not all(isinstance(value, int) and value > 0 for value in (a_ns, b_ns)):
            reasons.append(f"primary-pair-{pair}:timer")
            continue
        pairs.append({
            "pair": pair,
            "order": "AB" if pair == 0 else "BA",
            "control_ns": a_ns,
            "candidate_ns": b_ns,
            "delta_ns": b_ns - a_ns,
            "improvement_percent": (a_ns - b_ns) * 100.0 / a_ns,
        })
        if b_ns >= a_ns:
            reasons.append(f"primary-pair-{pair}:candidate-not-faster")

    guard_results = []
    for base in ["guard-same", "guard-plus1-early", "guard-plus1-middle", "guard-materialize-warm", "guard-materialize-fresh", "guard-reopen", "guard-range1m"]:
        a = by_label.get(base + "-A", {})
        b = by_label.get(base + "-B", {})
        a_ns = a.get("elapsed_wall_ns")
        b_ns = b.get("elapsed_wall_ns")
        if all(isinstance(value, int) and value > 0 for value in (a_ns, b_ns)):
            delta = b_ns - a_ns
            material = delta > 20_000_000 and b_ns > a_ns * 1.5
            guard_results.append({"operation": base.removeprefix("guard-"), "control_ns": a_ns, "candidate_ns": b_ns, "delta_ns": delta, "material_regression": material})
            if material:
                reasons.append(f"{base}:material-regression")
        else:
            reasons.append(f"{base}:timer")

    residue = [str(path.relative_to(root)) for path in root.rglob("*") if path.is_file() and (path.name.endswith("-journal") or path.name.endswith("-wal") or path.name.endswith("-shm")) and path.stat().st_size]
    if residue:
        reasons.append("residue")
    candidate_values = [item["candidate_ns"] for item in pairs]
    control_values = [item["control_ns"] for item in pairs]
    center = {
        "control_ns": statistics.mean(control_values) if len(control_values) == 2 else None,
        "candidate_ns": statistics.mean(candidate_values) if len(candidate_values) == 2 else None,
    }
    center["improvement_percent"] = ((center["control_ns"] - center["candidate_ns"]) * 100.0 / center["control_ns"]) if center["control_ns"] else None
    return {
        "status": "PASS" if not reasons else "REVISE",
        "disposition": "CANONICAL-V2 PASS" if not reasons else "CANONICAL-V2 REVISE",
        "reasons": reasons,
        "row_count": len(rows),
        "primary_pairs": pairs,
        "position_balanced_center": center,
        "guards": guard_results,
        "candidate_only": ["one-byte-early", "one-byte-middle", "one-byte-late", "first-edit-after-reopen", "scrub-only"],
        "residue": residue,
        "limitations": [
            "1/10 MiB are single-pair scaling checks",
            "lifecycle guards are descriptive single comparisons",
            "new aliases and scrub-only are candidate-only",
            "fresh-process cache state is warm-or-unknown",
            "instructions/cycles and physical I/O are unavailable",
        ],
    }


def write_outputs(root, result):
    (root / "ANALYSIS-v1.json").write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
    center = result["position_balanced_center"]
    report = [
        "# Canonical-v2 compact closure\n",
        f"Disposition: **{result['disposition']}**\n",
        f"Rows: {result['row_count']} (candidate-only guards: {', '.join(result['candidate_only'])}).\n",
    ]
    if center["control_ns"]:
        report.append(f"100-MiB full-create position-balanced center: control {center['control_ns']/1e6:.3f} ms; canonical-v2 {center['candidate_ns']/1e6:.3f} ms; improvement {center['improvement_percent']:.3f}%.\n")
    for pair in result["primary_pairs"]:
        report.append(f"Pair {pair['pair']} {pair['order']}: A {pair['control_ns']/1e6:.3f} ms; B {pair['candidate_ns']/1e6:.3f} ms; improvement {pair['improvement_percent']:.3f}%.\n")
    report.append("\nHard-gate reasons: " + (", ".join(result["reasons"]) if result["reasons"] else "none") + ".\n")
    report.append("\nLimitations:\n" + "".join(f"- {item}\n" for item in result["limitations"]))
    (root / "REPORT-v1.md").write_text("\n".join(report))
    disposition = result["disposition"] + "\n\n"
    disposition += "CP-0009 remains the accepted control.\n" if result["reasons"] else "Fresh-store canonical-v2 is eligible for promotion; automatic v1 migration remains deferred.\n"
    disposition += "Reasons: " + (", ".join(result["reasons"]) if result["reasons"] else "none") + "\n"
    disposition += "No later optimization or commit is authorized by this result.\n"
    (root / "DISPOSITION-v1.txt").write_text(disposition)


def main():
    if len(sys.argv) != 2:
        raise SystemExit("usage: analyze-compact-closure.py RESULT_ROOT")
    root = Path(sys.argv[1]).resolve()
    try:
        result = analyze(root)
    except Exception as error:
        result = {"status": "REVISE", "disposition": "CANONICAL-V2 REVISE", "reasons": [f"analyzer:{type(error).__name__}:{error}"], "row_count": 0, "primary_pairs": [], "position_balanced_center": {"control_ns": None, "candidate_ns": None, "improvement_percent": None}, "guards": [], "candidate_only": [], "residue": [], "limitations": []}
    write_outputs(root, result)
    print(json.dumps(result, sort_keys=True))
    return 0 if result["status"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
