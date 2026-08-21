#!/usr/bin/env python3
"""Fail-closed analyzer for the compact publication-repair screen."""

import csv
import json
import statistics
import sys
from pathlib import Path

CONTROL_PROFILE = "b0ebb845409ef995a5fa454bb23d10a80c6ecf44deb7832ca2ce1213eb0f4ba1"
CANDIDATE_PROFILE = "94a03ba7b6c97b5ff37c0ec62ef1d801b9896494b45456bd3df23e2cb278d13b"
CONTROL_SHA = "9cda87ee7fd92784281a6ec7ee3045eb661681d8b7b930dd36546119ae4749d7"
SOURCE = "bb883eecf4ea85d80432953791dcc352243da94175e7503e2c476afe9bd0bab7"
FIXTURE = "63b3695b8c117b5bc39885e0df0dcd0af1d49e575482bab16577d84b4f40eff4"
FULL_CLOSURE = {
    "A": "d6aac6e40cc851dd6295dbeec6488f1c5ebefa7520f86b0cd12bdcdce1f0d54a",
    "B": "29233d6018b031f6035c8e2c8175f1ab86fb721808f21a8858bc965567e9c0c1",
}
GUARD_CLOSURE = {
    "same-middle": "d7614133f35f1a254d0d2222815cdbcbdcd69915baf30c3a801831e6497b1683",
    "one-byte-middle": "b71da56600ce3c2011cdca037771c9050fbf5f16df2a2297b19e4af11173878e",
    "plus1-middle": "4cdcd09b47447c6673d391bdbece5eb239bd26bb9320061223f44d22e56d104c",
}
def expected_schedule():
    return [
        ("warm-full-100-A", "warmup", "full", "A", "-", "AB", True),
        ("warm-full-100-B", "warmup", "full", "B", "-", "AB", True),
        ("primary-full-100-p0-A", "primary", "full", "A", "0", "AB", True),
        ("primary-full-100-p0-B", "primary", "full", "B", "0", "AB", True),
        ("primary-full-100-p1-B", "primary", "full", "B", "1", "BA", True),
        ("primary-full-100-p1-A", "primary", "full", "A", "1", "BA", True),
        ("guard-same-middle-B", "candidate-only", "same-middle", "B", "-", "B", False),
        ("guard-one-byte-middle-B", "candidate-only", "one-byte-middle", "B", "-", "B", False),
        ("guard-plus1-middle-B", "candidate-only", "plus1-middle", "B", "-", "B", False),
    ]


def expected_schedule_rows():
    keys = ("label", "kind", "operation", "arm", "pair", "order", "comparable")
    return [{"sequence": str(index), "size": "104857600", **{key: str(value) for key, value in zip(keys, row)}} for index, row in enumerate(expected_schedule(), 1)]


def phase(row, name):
    matches = [item for item in row.get("phase_counters", []) if item.get("phase") == name]
    if len(matches) != 1:
        raise ValueError(f"{row.get('operation')}: expected one {name} phase, got {len(matches)}")
    return matches[0]


def positive_int(value):
    return isinstance(value, int) and not isinstance(value, bool) and value > 0


def load_external(root, reasons):
    result = {}
    with (root / "EXTERNAL-TIME-v1.tsv").open() as handle:
        for row in csv.DictReader(handle, delimiter="\t"):
            label = row["label"]
            parts = label.split("-", 2)
            if len(parts) != 3 or parts[0] != "row" or not parts[1].isdigit():
                reasons.append(f"external-label:{label}")
                continue
            label = parts[2]
            if label in result:
                reasons.append(f"external-duplicate:{label}")
            result[label] = row
    return result


def validate_row_chronology(root, expected, reasons):
    starts = list(csv.DictReader((root / "ROW-STARTS-v1.tsv").open(), delimiter="\t"))
    wanted_starts = []
    for spec in expected:
        for event in ("started", "completed"):
            wanted_starts.append({key: value for key, value in (
                ("sequence", spec["sequence"]),
                ("event", event),
                ("label", spec["label"]),
                ("arm", spec["arm"]),
                ("operation", spec["operation"]),
            )})
    projected = [{key: row.get(key) for key in wanted_starts[0]} for row in starts]
    if projected != wanted_starts:
        reasons.append("row-starts-chronology")
    try:
        times = [int(row["monotonic_ns"]) for row in starts]
        if len(times) != 18 or any(after <= before for before, after in zip(times, times[1:])):
            reasons.append("row-starts-time-order")
    except (KeyError, TypeError, ValueError):
        reasons.append("row-starts-time-format")

    invocations = list(csv.DictReader((root / "ACTUAL-INVOCATIONS-v1.tsv").open(), delimiter="\t"))
    rows = [row for row in invocations if row.get("label", "").startswith("row-")]
    if len(rows) != 18:
        reasons.append("row-invocation-count")
        return
    invocation_numbers = []
    invocation_times = []
    for index, spec in enumerate(expected):
        started, completed = rows[index * 2:index * 2 + 2]
        label = f"row-{int(spec['sequence']):02d}-{spec['label']}"
        if (started.get("event"), completed.get("event"), started.get("label"), completed.get("label"), started.get("exit"), completed.get("exit")) != ("started", "completed", label, label, "-", "0"):
            reasons.append(f"{spec['label']}:row-invocation")
        if started.get("sequence") != completed.get("sequence"):
            reasons.append(f"{spec['label']}:row-invocation-pair")
        try:
            invocation_numbers.append(int(started["sequence"]))
            invocation_times.extend((int(started["time_ns"]), int(completed["time_ns"])))
        except (KeyError, TypeError, ValueError):
            reasons.append(f"{spec['label']}:row-invocation-format")
    if len(invocation_numbers) != 9 or any(after <= before for before, after in zip(invocation_numbers, invocation_numbers[1:])):
        reasons.append("row-invocation-order")
    if len(invocation_times) != 18 or any(after <= before for before, after in zip(invocation_times, invocation_times[1:])):
        reasons.append("row-invocation-time-order")


def analyze(root):
    reasons = []
    expected = expected_schedule_rows()
    schedule = list(csv.DictReader((root / "SCHEDULE-v1.tsv").open(), delimiter="\t"))
    projected = [{key: row[key] for key in expected[0]} for row in schedule] if schedule else []
    if projected != expected:
        reasons.append("schedule")
    validate_row_chronology(root, expected, reasons)
    rows = [json.loads(line) for line in (root / "RAW-v1.jsonl").read_text().splitlines() if line]
    if len(rows) != 9:
        reasons.append("row-count")
    control_sha = (root / "CONTROL-SHA256-v1.txt").read_text().strip()
    candidate_sha = (root / "CANDIDATE-SHA256-v1.txt").read_text().strip()
    if control_sha != CONTROL_SHA or len(candidate_sha) != 64 or any(char not in "0123456789abcdef" for char in candidate_sha):
        reasons.append("operand-hash-custody")
    external = load_external(root, reasons)
    if set(external) != {row["label"] for row in expected}:
        reasons.append("external-coverage")

    custody = list(csv.DictReader((root / "INPUT-CUSTODY-v1.tsv").open(), delimiter="\t"))
    if len(custody) != 9 or [row.get("label") for row in custody] != [row["label"] for row in expected]:
        reasons.append("input-custody-order")
    if any(row.get("fixture_sha256") != FIXTURE for row in custody):
        reasons.append("fixture-custody")
    if len({row.get("database_path") for row in custody}) != 9 or len({(row.get("database_device"), row.get("database_inode")) for row in custody}) != 9:
        reasons.append("copied-base-distinctness")
    for arm, executable in (("A", control_sha), ("B", candidate_sha)):
        selected = [row for row in custody if row.get("arm") == arm]
        if any(row.get("executable_sha256") != executable for row in selected):
            reasons.append(f"{arm}:executable-custody")
        full = [row for row in selected if "full-100" in row.get("label", "")]
        for key in ("database_sha256", "authority_sha256", "expectations_sha256"):
            if len({row.get(key) for row in full}) != 1:
                reasons.append(f"{arm}:full-{key}-stability")

    by_label = {}
    joined = list(zip(expected, rows)) if len(rows) == 9 else []
    for spec, row in joined:
        tag, arm, operation = spec["label"], spec["arm"], spec["operation"]
        by_label[tag] = row
        expected_profile = CONTROL_PROFILE if arm == "A" else CANDIDATE_PROFILE
        expected_executable = control_sha if arm == "A" else candidate_sha
        if row.get("status") != "PASS" or row.get("error") is not None:
            reasons.append(f"{tag}:status")
        if row.get("profile_id") != expected_profile or row.get("executable_sha256") != expected_executable:
            reasons.append(f"{tag}:operand")
        if row.get("operation") != operation or row.get("size_bytes") != 104_857_600 or row.get("source_fingerprint") != SOURCE:
            reasons.append(f"{tag}:identity")
        references = 5_284 if operation != "plus1-middle" else 5_285
        if (row.get("expected_cdc_references"), row.get("actual_cdc_references")) != (references, references):
            reasons.append(f"{tag}:references")
        closure = FULL_CLOSURE[arm] if operation == "full" else GUARD_CLOSURE[operation]
        if row.get("ordered_closure_digest") != closure:
            reasons.append(f"{tag}:closure")
        if (row.get("transactions"), row.get("commits"), row.get("commit_dispatches"), row.get("commit_returns"), row.get("commit_return_successes"), row.get("commit_return_errors")) != (1, 1, 1, 1, 1, 0):
            reasons.append(f"{tag}:one-commit")
        if row.get("publication_status") != "Committed" or row.get("sqlite_runtime_journal_mode") != "delete" or row.get("sqlite_runtime_synchronous") != 2 or row.get("sqlite_runtime_temp_store") != 1 or row.get("sqlite_runtime_mmap_size") != 0:
            reasons.append(f"{tag}:durability")
        q_ceiling = 131_072 if operation == "full" else 4_194_304
        if row.get("q_current") != 0 or not isinstance(row.get("q_high_water"), int) or row.get("q_high_water", q_ceiling + 1) > q_ceiling:
            reasons.append(f"{tag}:q")
        if row.get("physical_journal_apparent_bytes") != 0 or row.get("q_fixed_envelope_removed") is not True:
            reasons.append(f"{tag}:cleanup")
        if row.get("base_preparation_in_measured_interval") is not False or row.get("durable_phase_sum_matches") is not True or row.get("commit_timer_equation_matches") is not True:
            reasons.append(f"{tag}:timer-or-preparation")
        if not positive_int(row.get("commit_dispatch_to_return_wall_ns")) or not isinstance(row.get("commit_pre_and_post_dispatch_wall_ns"), int) or row.get("commit_pre_and_post_dispatch_wall_ns") < 0:
            reasons.append(f"{tag}:commit-timer-separation")

        if arm == "B":
            commit = phase(row, "sqlite_commit")
            exact_zero = (
                "identity_bytes_hashed", "canonical_bytes_authenticated",
                "canonical_authenticated_nonnew_bytes", "canonical_authentication_hashes",
                "objects_authenticated", "statement_cache_acquisitions",
                "borrowed_row_blob_reads", "borrowed_row_blob_bytes",
            )
            if any(commit.get(key) != 0 for key in exact_zero):
                reasons.append(f"{tag}:publication-graph-work")
            expected_reads = 0 if operation == "full" else 4
            expected_returned = 0 if operation == "full" else 1
            if (commit.get("sql_query_calls"), commit.get("sql_execute_calls"), commit.get("sql_rows_returned"), commit.get("row_blob_reads"), commit.get("row_blob_writes"), commit.get("commits")) != (1, 2, expected_returned, expected_reads, 4, 1):
                reasons.append(f"{tag}:publication-sql-work")
            if operation == "full" and (row.get("sql_query_calls"), row.get("row_blob_reads")) != (4, 4):
                reasons.append(f"{tag}:full-qualified-work")
        if operation == "one-byte-middle":
            old, new = row.get("edit_removed_hex"), row.get("edit_inserted_hex")
            if row.get("edit_count_classification") != "same-count" or row.get("edit_reference_count_before") != 5_284 or row.get("edit_reference_count_after") != 5_284:
                reasons.append(f"{tag}:same-count")
            if not (isinstance(old, str) and isinstance(new, str) and len(old) == len(new) == 2 and int(new, 16) == (int(old, 16) ^ 0x5A)):
                reasons.append(f"{tag}:replacement")
            closure_phase = phase(row, "precommit_closure")
            if (closure_phase.get("incremental_qualification_calls"), closure_phase.get("sql_query_calls"), closure_phase.get("row_blob_reads"), closure_phase.get("borrowed_row_blob_reads"), closure_phase.get("objects_authenticated")) != (1, 25, 28, 5, 24):
                reasons.append(f"{tag}:changed-spine-work")

    pairs = []
    for pair, order in ((0, "AB"), (1, "BA")):
        prefix = f"primary-full-100-p{pair}"
        control = by_label.get(prefix + "-A", {}).get("durable_capture_total_wall_ns")
        candidate = by_label.get(prefix + "-B", {}).get("durable_capture_total_wall_ns")
        if not positive_int(control) or not positive_int(candidate):
            reasons.append(f"pair-{pair}:timer")
            continue
        pairs.append({"pair": pair, "order": order, "control_ns": control, "candidate_ns": candidate, "delta_ns": candidate - control, "improvement_percent": (control - candidate) * 100.0 / control})
        if candidate >= control:
            reasons.append(f"pair-{pair}:candidate-not-faster")

    residue = [str(path.relative_to(root)) for path in root.rglob("*") if path.is_file() and path.name.endswith(("-journal", "-wal", "-shm"))]
    if residue:
        reasons.append("residue")
    controls = [pair["control_ns"] for pair in pairs]
    candidates = [pair["candidate_ns"] for pair in pairs]
    control_center = statistics.mean(controls) if len(controls) == 2 else None
    candidate_center = statistics.mean(candidates) if len(candidates) == 2 else None
    primary_phases = []
    for pair, order in ((0, "AB"), (1, "BA")):
        for arm in order:
            label = f"primary-full-100-p{pair}-{arm}"
            row = by_label.get(label, {})
            primary_phases.append({
                "label": label,
                "arm": arm,
                "durable_capture_total_wall_ns": row.get("durable_capture_total_wall_ns"),
                "canonical_cas_mapping_stage_wall_ns": row.get("canonical_cas_mapping_stage_wall_ns"),
                "precommit_closure_validation_wall_ns": row.get("precommit_closure_validation_wall_ns"),
                "commit_dispatch_to_return_wall_ns": row.get("commit_dispatch_to_return_wall_ns"),
                "commit_pre_and_post_dispatch_wall_ns": row.get("commit_pre_and_post_dispatch_wall_ns"),
            })
    guards = []
    for operation in ("same-middle", "one-byte-middle", "plus1-middle"):
        row = by_label.get(f"guard-{operation}-B", {})
        guards.append({
            "operation": operation,
            "durable_capture_total_wall_ns": row.get("durable_capture_total_wall_ns"),
            "commit_dispatch_to_return_wall_ns": row.get("commit_dispatch_to_return_wall_ns"),
            "commit_pre_and_post_dispatch_wall_ns": row.get("commit_pre_and_post_dispatch_wall_ns"),
        })
    return {
        "status": "PASS" if not reasons else "REVISE",
        "disposition": "CANONICAL-V2 PUBLICATION-REPAIR PASS" if not reasons else "CANONICAL-V2 PUBLICATION-REPAIR REVISE",
        "reasons": reasons,
        "row_count": len(rows),
        "exact_schedule": [row["label"] for row in expected],
        "primary_pairs": pairs,
        "primary_phase_separation": primary_phases,
        "position_balanced_center": {
            "control_ns": control_center,
            "candidate_ns": candidate_center,
            "improvement_percent": ((control_center - candidate_center) * 100.0 / control_center) if control_center else None,
        },
        "candidate_guards": guards,
        "external_time_rows_joined": len(external),
        "residue": residue,
        "limitations": [
            "This compact screen is not a promotion campaign.",
            "Candidate-only guard timings make no control-speed claim.",
            "OS/filesystem cache state is warm-or-unknown.",
            "Instructions, cycles, and physical I/O are unavailable.",
        ],
    }


def write_outputs(root, result):
    (root / "ANALYSIS-v1.json").write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
    center = result["position_balanced_center"]
    report = [
        "# Canonical-v2 publication repair compact screen\n",
        f"Disposition: **{result['disposition']}**\n",
        f"Rows: {result['row_count']}; external-time joins: {result['external_time_rows_joined']}.\n",
    ]
    if center["control_ns"]:
        report.append(f"Position-balanced 100-MiB center: control {center['control_ns']/1e6:.3f} ms; candidate {center['candidate_ns']/1e6:.3f} ms; improvement {center['improvement_percent']:.3f}%.\n")
    for pair in result["primary_pairs"]:
        report.append(f"Pair {pair['pair']} {pair['order']}: control {pair['control_ns']/1e6:.3f} ms; candidate {pair['candidate_ns']/1e6:.3f} ms; improvement {pair['improvement_percent']:.3f}%.\n")
    report.append("\nMeasured full-create phase separation:\n")
    for item in result["primary_phase_separation"]:
        fields = [item[key] for key in ("durable_capture_total_wall_ns", "canonical_cas_mapping_stage_wall_ns", "precommit_closure_validation_wall_ns", "commit_dispatch_to_return_wall_ns", "commit_pre_and_post_dispatch_wall_ns")]
        if all(isinstance(value, int) for value in fields):
            report.append(f"- {item['label']}: durable {fields[0]/1e6:.3f} ms; construction/mapping {fields[1]/1e6:.3f} ms; proof {fields[2]/1e6:.3f} ms; actual COMMIT dispatch/return {fields[3]/1e6:.3f} ms; pre/post publication {fields[4]/1e6:.3f} ms.\n")
    report.append("\nCandidate-only guard phase separation:\n")
    for guard in result["candidate_guards"]:
        total = guard["durable_capture_total_wall_ns"]
        dispatch = guard["commit_dispatch_to_return_wall_ns"]
        outside = guard["commit_pre_and_post_dispatch_wall_ns"]
        if all(isinstance(value, int) for value in (total, dispatch, outside)):
            report.append(f"- {guard['operation']}: durable {total/1e6:.3f} ms; actual COMMIT dispatch/return {dispatch/1e6:.3f} ms; pre/post publication {outside/1e6:.3f} ms.\n")
    report.append("\nHard-gate reasons: " + (", ".join(result["reasons"]) if result["reasons"] else "none") + ".\n")
    report.append("\nLimitations:\n" + "".join(f"- {item}\n" for item in result["limitations"]))
    (root / "REPORT-v1.md").write_text("\n".join(report))
    text = result["disposition"] + "\n\nCP-0009 remains the accepted control.\n"
    text += "Reasons: " + (", ".join(result["reasons"]) if result["reasons"] else "none") + "\n"
    text += "No promotion, later optimization, integration, commit, or historical relabeling is authorized.\n"
    (root / "DISPOSITION-v1.txt").write_text(text)


def main():
    if len(sys.argv) != 2:
        raise SystemExit("usage: analyze-publication-repair.py RESULT_ROOT")
    root = Path(sys.argv[1]).resolve()
    try:
        result = analyze(root)
    except Exception as error:
        result = {
            "status": "REVISE",
            "disposition": "CANONICAL-V2 PUBLICATION-REPAIR REVISE",
            "reasons": [f"analyzer:{type(error).__name__}:{error}"],
            "row_count": 0,
            "exact_schedule": [],
            "primary_pairs": [],
            "primary_phase_separation": [],
            "position_balanced_center": {"control_ns": None, "candidate_ns": None, "improvement_percent": None},
            "candidate_guards": [],
            "external_time_rows_joined": 0,
            "residue": [],
            "limitations": [],
        }
    write_outputs(root, result)
    print(json.dumps(result, sort_keys=True))
    return 0 if result["status"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
