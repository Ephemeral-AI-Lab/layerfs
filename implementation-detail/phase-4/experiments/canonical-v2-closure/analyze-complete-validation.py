#!/usr/bin/env python3
"""Fail-closed additions to the existing compact Canonical-v2 analyzer."""

import csv
import importlib.util
import json
import sys
from pathlib import Path

sys.dont_write_bytecode = True

HERE = Path(__file__).resolve().parent
BASE_PATH = HERE / "analyze-compact-closure.py"
CANDIDATE_SHA = "f3dd4c9420cc7bb7e7390960db9bf6e4a4a44de3d15dc0573002d3172b570280"
SOURCE_SHA = "16e9beedd2fe49d6da65f89f53f488cffbfdcfc71f10477e854cd2d37d00e120"


def load(path, name):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


base = load(BASE_PATH, "canonical_v2_compact_analyzer")


def tsv(path):
    return list(csv.DictReader(path.open(), delimiter="\t"))


def phase(row, name):
    matches = [item for item in row.get("phase_counters", []) if item.get("phase") == name]
    return matches[0] if len(matches) == 1 else None


def analyze(root):
    result = base.analyze(root)
    reasons = [reason for reason in result["reasons"] if not reason.endswith(":external-time")]
    expected = base.expected_schedule()
    rows = [json.loads(line) for line in (root / "RAW-v1.jsonl").read_text().splitlines() if line]

    if (root / "CANDIDATE-SHA256-v1.txt").read_text().strip() != CANDIDATE_SHA:
        reasons.append("candidate-sha")
    source_custody = tsv(root / "SOURCE-BUILD-CUSTODY-v1.tsv")
    source = next((row for row in source_custody if row["path"] ==
                   "crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs"), None)
    if source is None or source.get("sha256") != SOURCE_SHA:
        reasons.append("candidate-source-sha")

    external = tsv(root / "EXTERNAL-TIME-v1.tsv")
    external_labels = [row["label"] for row in external]
    expected_external = [f"row-{spec['sequence']:02d}-{spec['label']}" for spec in expected]
    if external_labels != expected_external:
        reasons.append("external-time-chronology")
    for row in external:
        for key in ("real_seconds", "user_seconds", "system_seconds",
                    "maximum_resident_set_bytes", "peak_memory_footprint_bytes"):
            try:
                if float(row[key]) < 0:
                    raise ValueError
            except (KeyError, TypeError, ValueError):
                reasons.append(f"external-time:{row.get('label')}:{key}")

    starts = tsv(root / "ROW-STARTS-v1.tsv")
    expected_starts = []
    for spec in expected:
        for event in ("started", "completed"):
            expected_starts.append((str(spec["sequence"]), event, spec["label"],
                                    spec["arm"], spec["operation"]))
    actual_starts = [(row["sequence"], row["event"], row["label"], row["arm"],
                      row["operation"]) for row in starts]
    if actual_starts != expected_starts:
        reasons.append("row-start-chronology")

    invocations = tsv(root / "ACTUAL-INVOCATIONS-v1.tsv")
    row_invocations = [row for row in invocations if row["label"].startswith("row-")]
    expected_invocations = []
    for spec in expected:
        label = f"row-{spec['sequence']:02d}-{spec['label']}"
        expected_invocations.extend([(label, "started", "-"), (label, "completed", "0")])
    actual_invocations = [(row["label"], row["event"], row["exit"])
                          for row in row_invocations]
    if actual_invocations != expected_invocations:
        reasons.append("row-invocation-chronology")

    custody = tsv(root / "AUTHORITY-MODE-CUSTODY-v1.tsv")
    if len(custody) != len(expected):
        reasons.append("authority-custody-count")
    for row in custody:
        if (row.get("source_sha256") != row.get("target_sha256")
                or row.get("target_runtime_mode") != "0600"
                or row.get("distinct_file") != "true"):
            reasons.append(f"authority-custody:{row.get('label', '?')}")

    mutations = {"full", "same-middle", "plus1-early", "plus1-middle",
                 "one-byte-early", "one-byte-middle", "one-byte-late",
                 "first-edit-after-reopen"}
    for spec, row in zip(expected, rows):
        if spec["arm"] != "B" or spec["operation"] not in mutations:
            continue
        commit = phase(row, "sqlite_commit")
        if commit is None:
            reasons.append(f"{spec['label']}:commit-phase")
            continue
        expected_reads = 0 if spec["operation"] == "full" else 4
        expected_rows = 0 if spec["operation"] == "full" else 1
        exact = {
            "sql_query_calls": 1,
            "sql_execute_calls": 2,
            "sql_rows_changed": 1,
            "sql_rows_returned": expected_rows,
            "row_blob_reads": expected_reads,
            "row_blob_writes": 4,
            "commits": 1,
            "objects_authenticated": 0,
            "canonical_bytes_authenticated": 0,
            "canonical_authentication_hashes": 0,
            "construction_proof_consumptions": 0,
            "incremental_qualification_calls": 0,
        }
        if any(commit.get(key) != value for key, value in exact.items()):
            reasons.append(f"{spec['label']}:publication-rescan")

        if spec["operation"] == "full" and spec["size"] == 104_857_600:
            full = {
                "actual_cdc_references": 5_284,
                "objects_created": 5_372,
                "objects_reused": 0,
                "canonical_bytes_written": 105_122_466,
                "mapping_bytes_rewritten": 196_174,
                "sql_calls": 5_381,
                "row_blob_writes": 10_748,
            }
            if any(row.get(key) != value for key, value in full.items()):
                reasons.append(f"{spec['label']}:exact-full-work")

    residue = [str(path.relative_to(root)) for path in root.rglob("*")
               if path.name.endswith(("-journal", "-wal", "-shm"))]
    if residue and "residue" not in reasons:
        reasons.append("residue")

    reasons = list(dict.fromkeys(reasons))
    result.update({
        "status": "PASS" if not reasons else "REVISE",
        "disposition": ("CANONICAL-V2 COMPLETE VALIDATION PASS"
                        if not reasons else "CANONICAL-V2 COMPLETE VALIDATION REVISE"),
        "reasons": reasons,
        "residue": residue,
        "publication_rescan_gate": "exact-zero-graph-work-in-sqlite_commit",
        "baseline_eligible": not reasons,
        "promotion_scope": "fresh-store canonical-v2; automatic v1 migration deferred",
    })
    return result


def write_outputs(root, result):
    (root / "ANALYSIS-v1.json").write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
    center = result["position_balanced_center"]
    lines = [
        "# Canonical-v2 complete validation v1\n",
        f"Disposition: **{result['disposition']}**\n",
        f"Rows: {result['row_count']}; hard-gate reasons: "
        + (", ".join(result["reasons"]) if result["reasons"] else "none") + ".\n",
    ]
    if center["control_ns"]:
        lines.append(
            f"100-MiB full-create balanced center: control {center['control_ns']/1e6:.3f} ms; "
            f"Canonical-v2 {center['candidate_ns']/1e6:.3f} ms; improvement "
            f"{center['improvement_percent']:.3f}%.\n")
    for pair in result["primary_pairs"]:
        lines.append(
            f"Pair {pair['pair']} {pair['order']}: control {pair['control_ns']/1e6:.3f} ms; "
            f"candidate {pair['candidate_ns']/1e6:.3f} ms; improvement "
            f"{pair['improvement_percent']:.3f}%.\n")
    lines.extend([
        "\nPublication: every candidate mutation has exact head-only SQLite COMMIT-phase "
        "accounting and zero graph authentication.\n",
        "\nLimitations: 1/10-MiB rows and lifecycle guards are single comparisons; "
        "candidate-only rows make no speed claim; OS/filesystem cache is warm-or-unknown; "
        "physical I/O, instructions, and cycles remain unavailable.\n",
    ])
    (root / "REPORT-v1.md").write_text("\n".join(lines))
    disposition = result["disposition"] + "\n"
    disposition += "Reasons: " + (", ".join(result["reasons"]) if result["reasons"] else "none") + "\n"
    disposition += ("Exact fresh-store Canonical-v2 baseline is eligible to freeze; automatic v1 migration remains deferred.\n"
                    if result["baseline_eligible"] else
                    "CP-0009 remains accepted; Canonical-v2 baseline is not eligible to freeze.\n")
    disposition += "No later optimization, production integration, commit, or migration is authorized.\n"
    (root / "DISPOSITION-v1.txt").write_text(disposition)


def main():
    if len(sys.argv) != 2:
        raise SystemExit("usage: analyze-complete-validation.py RESULT_ROOT")
    root = Path(sys.argv[1]).resolve()
    try:
        result = analyze(root)
    except Exception as error:
        result = {
            "status": "REVISE",
            "disposition": "CANONICAL-V2 COMPLETE VALIDATION REVISE",
            "reasons": [f"analyzer:{type(error).__name__}:{error}"],
            "row_count": 0,
            "primary_pairs": [],
            "position_balanced_center": {"control_ns": None, "candidate_ns": None,
                                         "improvement_percent": None},
            "guards": [], "candidate_only": [], "residue": [],
            "baseline_eligible": False,
        }
    write_outputs(root, result)
    print(json.dumps(result, sort_keys=True))
    return 0 if result["status"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
