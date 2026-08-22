#!/usr/bin/env python3
"""Finalize and seal a completed G3-v13 campaign after parent static closure."""

import argparse
import csv
import hashlib
import json
import re
import tempfile
from pathlib import Path, PurePosixPath

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[4]
TARGET = REPO / "target/phase4-g3-incremental-materialization-20260822-v13"
RESULTS = TARGET / "results-v13"
LOCK = REPO / "target/phase4-g3-incremental-materialization-20260822-v13.lock"
MANIFEST = RESULTS / "PAYLOAD-MANIFEST-v13.tsv"
TERMINAL = RESULTS / "TERMINAL-v13.json"
VERIFICATION = RESULTS / "TERMINAL-VERIFICATION-v13.txt"
STATIC = RESULTS / "STATIC-CLOSURE-v13.json"
DRY_RUN = HERE / "DRY-RUN-v13.json"
PREMEASUREMENT_FREEZE = HERE.parent / "PREMEASUREMENT-FREEZE-v13.json"
REQUIRED_STATIC_LABELS = [
    "focused-g3-tests", "workspace-tests", "workspace-clippy",
    "workspace-fmt-check", "git-diff-check", "custody-review",
]
FOCUSED_G3_TEST_RESULT = b"test result: ok. 15 passed; 0 failed"
FOCUSED_G3_TEST_NAMES = {
    "phase4_g3_materialization::tests::fclonefileat_clones_an_unlinked_read_only_seed_fd",
    "phase4_g3_materialization::tests::g3_rows_cover_qualified_fallback_rejection_and_fault_routes",
    "phase4_g3_materialization::tests::missing_destination_and_seed_are_complete_fallback_misses",
    "phase4_g3_materialization::tests::symlink_preflight_precedes_invalid_authority_for_every_scenario",
    "phase4_g3_materialization::tests::permit_rechecks_retained_directory_identity",
    "phase4_g3_materialization::tests::patch_retry_resets_target_and_proves_one_exact_range",
    "phase4_g3_materialization::tests::canonical_range_proof_rejects_underdeclared_range_and_digest_replay",
    "phase4_g3_materialization::tests::stream_root_dfs_q_decharges_after_success_and_writer_error",
    "phase4_g3_materialization::tests::clone_miss_falls_back_without_consuming_single_use_permit",
    "phase4_g3_materialization::tests::reconciliation_rejects_identity_change_during_complete_compare",
    "phase4_g3_materialization::tests::rename_error_cleans_target_temp_and_preserves_prior_failure",
    "phase4_g3_materialization::tests::reconciliation_q_charges_fixed_comparison_buffer_exactly",
    "phase4_g3_materialization::tests::temp_counter_failure_leaves_no_named_residue",
    "phase4_g3_materialization::tests::seed_post_create_failure_leaves_no_named_residue",
    "phase4_g3_materialization::tests::publication_error_dominates_cleanup_error_with_both_provenances",
}
ROW_LABELS = ["01-qualified-noop", "02-qualified-one-byte", "03-qualified-one-mib", "04-invalid-authority", "05-external-mutation", "06-symlink-substitution", "07-count-change", "08-before-publication-fault", "09-lost-ack"]
SOURCE_PATHS = [
    "Cargo.lock", "crates/layerfs-engine/Cargo.toml",
    "crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs",
    "crates/layerfs-engine/src/bin/phase4_g3_materialization.rs",
]
METHOD_NAMES = [
    "PROSPECTIVE-G3-INCREMENTAL-MATERIALIZATION-v13.md",
    "COUNTER-DICTIONARY-v13.md", "run_g3_v13.py", "analyze_g3_v13.py",
    "recompute_g3_v13.py", "finalize_g3_v13.py",
]
BUILD_COMMAND = ["cargo", "build", "--release", "-p", "layerfs-engine", "--bin", "phase4_create_edit_benchmark", "--offline"]
CONTROLLED_ENVIRONMENT = {
    "LANG", "LC_ALL", "PATH", "SHELL", "TZ", "RUST_BACKTRACE",
    "G3_SOURCE_SET_SHA256", "G3_METHODOLOGY_SET_SHA256", "G3_EXECUTABLE_SHA256",
}
STATIC_COMMANDS = [
    ["cargo", "test", "-p", "layerfs-engine", "--bin", "phase4_create_edit_benchmark", "phase4_g3_materialization::tests", "--offline", "--", "--test-threads=8"],
    ["cargo", "test", "--workspace", "--offline", "--all-targets"],
    ["cargo", "clippy", "--workspace", "--offline", "--all-targets", "--", "-D", "warnings"],
    ["cargo", "fmt", "--all", "--", "--check"],
    ["git", "diff", "--check"],
    ["python3", str(REPO / "implementation-detail/phase-4/experiments/g3-incremental-materialization/v13/finalize_g3_v13.py"), "--self-check"],
]
G2_ROOT = REPO / "target/phase4-g2-materialization-decomposition-20260822-v5/results-v5"
G2 = {
    "payload_manifest_sha256": (G2_ROOT / "PAYLOAD-MANIFEST-v5.tsv", "12f74b88188c1a22babe129c4b1d5d0e1889ba55d2cf0046ae55af6803709399"),
    "terminal_sha256": (G2_ROOT / "TERMINAL-v5.json", "09a5948a2c6a31c55811d50459c24cf72c4d2e3ff61ea5773754bf5c6c1a60a2"),
    "terminal_verification_sha256": (G2_ROOT / "TERMINAL-VERIFICATION-v5.txt", "41447453a34b1933850e6e090a2bc59628d58f7d585e7c394e937cfe03250af0"),
    "raw_sha256": (G2_ROOT / "rows-v5/G2-V5-RAW.jsonl", "c64a4f7b4d1a831fd7406251f0de2ab44cfbf390d07188d55298fdbbfefb0eeb"),
    "primary_sha256": (G2_ROOT / "G2-V5-ANALYSIS.json", "432f903ecebe3afc6370e422c559e346f71abd71ba16f328d35e169e28732803"),
    "independent_sha256": (G2_ROOT / "G2-V5-INDEPENDENT-RECOMPUTATION.json", "86ab101df69f82ec548d8baa223ea4a6fde13646660969f6478a4e73fe08df5e"),
}
G2_LEDGER_SHA256 = "5de0586cdcb80932b503458c0b74e1983b3b2b5179adc6ba5ed4480aa7af33b9"
EXCLUDED = {"PAYLOAD-MANIFEST-v13.tsv", "TERMINAL-v13.json", "TERMINAL-VERIFICATION-v13.txt"}


def sha256(path):
    result = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            result.update(block)
    return result.hexdigest()


def canonical_hash(value):
    return hashlib.sha256(json.dumps(value, separators=(",", ":"), sort_keys=True).encode()).hexdigest()


def mode(path):
    return f"{path.stat().st_mode & 0o7777:04o}"


def read_json(path):
    value = json.loads(path.read_text())
    if type(value) is not dict:
        raise RuntimeError(f"not a JSON object: {path}")
    return value


def safe_result_file(relative):
    pure = PurePosixPath(relative)
    if pure.is_absolute() or not pure.parts or any(part in ("", ".", "..") for part in pure.parts):
        raise RuntimeError(f"unsafe static artifact path: {relative}")
    path = RESULTS.joinpath(*pure.parts)
    if path.is_symlink() or not path.is_file() or path.resolve().parent != RESULTS.resolve() and RESULTS.resolve() not in path.resolve().parents:
        raise RuntimeError(f"invalid static artifact: {relative}")
    return path


def exact_source_copy(row, result_root=RESULTS):
    relative = row.get("copy_path")
    pure = PurePosixPath(relative) if isinstance(relative, str) else None
    expected = f"source-custody-v13/{row.get('path')}"
    if pure is None or pure.is_absolute() or any(part in ("", ".", "..") for part in pure.parts) or relative != expected:
        raise RuntimeError(f"unsafe or inexact source copy path: {row.get('path')}")
    copy = result_root.joinpath(*pure.parts)
    original = REPO / row["path"]
    custody_root = (result_root / "source-custody-v13").resolve()
    if custody_root not in copy.resolve().parents or copy.resolve() == original.resolve() or copy.is_symlink() or not copy.is_file() or (copy.stat().st_dev, copy.stat().st_ino) == (original.stat().st_dev, original.stat().st_ino):
        raise RuntimeError(f"source copy containment/distinctness mismatch: {row['path']}")
    if row.get("source_mode") != "0644" or row.get("copy_mode") != "0400" or row.get("copy_size_bytes") != row.get("size_bytes") or row.get("copy_sha256") != row.get("sha256") or mode(original) != "0644" or sha256(original) != row.get("sha256") or original.stat().st_size != row.get("size_bytes") or mode(copy) != "0400" or sha256(copy) != row.get("copy_sha256") or copy.stat().st_size != row.get("copy_size_bytes"):
        raise RuntimeError(f"source copy bytes/size/mode mismatch: {row['path']}")
    return original, copy


def verify_g2():
    observed = {}
    for name, (path, expected) in G2.items():
        if not path.is_file() or sha256(path) != expected:
            raise RuntimeError(f"G2-v5 custody mismatch: {name}")
        observed[name] = expected
    if read_json(G2["terminal_sha256"][0]).get("status") != "PASS":
        raise RuntimeError("G2-v5 terminal is not PASS")
    primary = read_json(G2_ROOT / "G2-V5-ANALYSIS.json")
    independent = read_json(G2_ROOT / "G2-V5-INDEPENDENT-RECOMPUTATION.json")
    if canonical_hash(primary.get("normalized_ledger")) != G2_LEDGER_SHA256 or canonical_hash(independent.get("normalized_ledger")) != G2_LEDGER_SHA256 or primary.get("normalized_ledger") != independent.get("normalized_ledger"):
        raise RuntimeError("G2-v5 normalized ledger custody mismatch")
    observed["normalized_ledger_sha256"] = G2_LEDGER_SHA256
    return observed


def expected_campaign_commands(operand, environment):
    binary = RESULTS / operand["copy_path"]
    methods = RESULTS / "methodology-v13"
    work = RESULTS / "work-v13"
    python = environment["python_executable"]
    result = [{"kind": "build", "label": "release-build", "timeout_seconds": 30, "command": BUILD_COMMAND}]
    result += [
        {"kind": "analyzer-self-check", "label": "primary-self-check", "timeout_seconds": 5, "command": [python, str(methods / "analyze_g3_v13.py"), "--self-check"]},
        {"kind": "analyzer-self-check", "label": "independent-self-check", "timeout_seconds": 5, "command": [python, str(methods / "recompute_g3_v13.py"), "--self-check"]},
    ]
    schedule = [(1, "qualified-noop", 10 * 1024 * 1024), (2, "qualified-one-byte", 100 * 1024 * 1024), (3, "qualified-one-mib", 10 * 1024 * 1024), (4, "invalid-authority", 1024 * 1024), (5, "external-mutation", 1024 * 1024), (6, "symlink-substitution", 1024 * 1024), (7, "count-change", 1024 * 1024), (8, "before-publication-fault", 1024 * 1024), (9, "lost-ack", 1024 * 1024)]
    for sequence, scenario, size in schedule:
        label = f"{sequence:02d}-{scenario}"
        result.append({"kind": "measured-row", "label": label, "timeout_seconds": 15 if scenario == "qualified-one-byte" else 5, "command": ["/usr/bin/time", "-l", str(binary), "--g3-row", str(work / label), str(size), scenario]})
    result += [
        {"kind": "analyzer", "label": "primary-analysis", "timeout_seconds": 5, "command": [python, str(methods / "analyze_g3_v13.py"), str(RESULTS)]},
        {"kind": "analyzer", "label": "independent-recomputation", "timeout_seconds": 5, "command": [python, str(methods / "recompute_g3_v13.py"), str(RESULTS)]},
    ]
    return result


def exact_environment(environment, source_set, method_set, executable):
    build = environment.get("child_environments", {}).get("build", {})
    runtime = environment.get("child_environments", {}).get("runtime", {})
    build_selected = build.get("selected_environment", {})
    runtime_selected = runtime.get("selected_environment", {})
    common = {"LANG": "C", "LC_ALL": "C", "TZ": "UTC", "RUST_BACKTRACE": "0", "G3_SOURCE_SET_SHA256": source_set, "G3_METHODOLOGY_SET_SHA256": method_set}
    return (
        environment.get("schema") == "phase4-g3-v13-environment-v1"
        and environment.get("status") == "PASS"
        and environment.get("source_set_sha256") == source_set
        and environment.get("methodology_set_sha256") == method_set
        and environment.get("executable_sha256") == executable
        and environment.get("child_environment_roles") == {"build": "build", "analyzer-self-check": "runtime", "measured-row": "runtime", "analyzer": "runtime"}
        and set(build_selected) == set(runtime_selected) == CONTROLLED_ENVIRONMENT
        and all(build_selected.get(key) == value and runtime_selected.get(key) == value for key, value in common.items())
        and build_selected.get("G3_EXECUTABLE_SHA256") is None
        and runtime_selected.get("G3_EXECUTABLE_SHA256") == executable
        and build.get("selected_environment_sha256") == canonical_hash(build_selected)
        and runtime.get("selected_environment_sha256") == canonical_hash(runtime_selected)
        and build.get("executable_sha256_applicable") is False
        and runtime.get("executable_sha256_applicable") is True
        and isinstance(environment.get("python_executable"), str) and bool(environment.get("python_executable"))
    )


def exact_row_custody(rows, operand, methods, environment_hash, commands):
    expected_rows = commands[3:12]
    return len(rows) == len(expected_rows) == 9 and all(
        row.get("executable_sha256") == operand.get("sha256")
        and row.get("methodology_set_sha256") == methods.get("methodology_set_sha256")
        and row.get("environment_sha256") == environment_hash
        and row.get("command") == planned.get("command")
        for row, planned in zip(rows, expected_rows)
    )


def focused_test_names_exact(output):
    observed = [name.decode() for name in re.findall(rb"^test ([^ ]+) \.\.\. ok$", output, re.MULTILINE)]
    return FOCUSED_G3_TEST_RESULT in output and len(observed) == 15 and set(observed) == FOCUSED_G3_TEST_NAMES


def verify_static(source_set):
    closure = read_json(STATIC)
    commands = closure.get("commands", [])
    if closure.get("schema") != "phase4-g3-v13-static-closure-v1" or closure.get("status") != "PASS" or closure.get("source_set_sha256") != source_set or closure.get("candidate_retained") is not True or [row.get("sequence") for row in commands] != list(range(1, 7)) or [row.get("label") for row in commands] != REQUIRED_STATIC_LABELS or [row.get("command") for row in commands] != STATIC_COMMANDS:
        raise RuntimeError("static closure shape/status/source mismatch")
    for row in commands:
        if row.get("exit_code") != 0 or not isinstance(row.get("command"), list) or not row["command"]:
            raise RuntimeError(f"static command failed or missing: {row.get('label')}")
        for stream in ("stdout", "stderr"):
            path = safe_result_file(row.get(f"{stream}_path", ""))
            if sha256(path) != row.get(f"{stream}_sha256") or path.stat().st_size != row.get(f"{stream}_size_bytes"):
                raise RuntimeError(f"static {stream} custody mismatch: {row['label']}")
    focused_stdout = safe_result_file(commands[0]["stdout_path"])
    focused_bytes = focused_stdout.read_bytes()
    if not focused_test_names_exact(focused_bytes):
        raise RuntimeError("focused G3 static closure did not prove the exact 15 named tests")
    return closure


def verify_row_cleanup(cleanup, records, raw_rows, results=None):
    expected = [(sequence, label, event) for sequence, label in enumerate(ROW_LABELS, 1) for event in ("PREPARE", "COMPLETE")]
    if [(row.get("sequence"), row.get("label"), row.get("event")) for row in records] != expected or len(raw_rows) != 9:
        raise RuntimeError("row cleanup count/order mismatch")
    dimensions = ("logical_bytes", "apparent_bytes", "allocated_bytes"); prepares, completes = records[::2], records[1::2]
    method = "descriptor-relative-openat-fstatat-unlinkat-rmdir-no-follow-exact-inventory-v1"
    for sequence, (prepare, complete, raw) in enumerate(zip(prepares, completes, raw_rows), 1):
        entries = prepare.get("inventory", []); paths = [entry.get("path") for entry in entries]
        safe = isinstance(entries, list) and entries == sorted(entries, key=lambda entry: entry.get("path", "")) and paths == sorted(set(paths)) and all(isinstance(path, str) and path and not PurePosixPath(path).is_absolute() and all(part not in ("", ".", "..") for part in PurePosixPath(path).parts) and entry.get("kind") in ("regular", "directory", "symlink", "other") and all(type(entry.get(key)) is int for key in ("device", "inode", "mode", "nlink", "size_bytes", "mtime_ns", "ctime_ns", "allocated_bytes")) for path, entry in zip(paths, entries))
        row_identity = prepare.get("row_identity", {}); safe_row_identity = row_identity.get("kind") == "directory" and all(type(row_identity.get(key)) is int for key in ("device", "inode", "mode", "nlink", "size_bytes", "mtime_ns", "ctime_ns", "allocated_bytes"))
        exactness = {key: raw.get(key) for key in ("byte_exact", "mode_exact", "temp_residue_count", "seed_residue_count", "old_or_new", "output_digest", "expected_output_digest")}
        before, work_before = prepare.get("pre_delete_row", {}), prepare.get("pre_delete_work", {})
        valid_prepare = prepare.get("schema") == "phase4-g3-v13-row-cleanup-v1" and prepare.get("row_root") == f"work-v13/{ROW_LABELS[sequence - 1]}" and safe and safe_row_identity and prepare.get("inventory_count") == len(entries) and prepare.get("inventory_sha256") == canonical_hash(entries) and prepare.get("deletion_method") == method and prepare.get("anchored_work_dirfd") is True and prepare.get("anchored_row_dirfd") is True and prepare.get("row_fd_retained_prepare_through_delete") is True and prepare.get("enumeration_followed_symlinks") is False and prepare.get("private_namespace_process_custody") is True and prepare.get("candidate_exactness") == exactness and all(type(before.get(key)) is int and 0 <= before[key] <= 512 * 1024 * 1024 and work_before.get(key) == before[key] for key in dimensions)
        valid_complete = complete.get("schema") == "phase4-g3-v13-row-cleanup-v1" and complete.get("row_root") == prepare.get("row_root") and complete.get("prepare_sha256") == canonical_hash(prepare) and complete.get("inventory_count") == len(entries) and complete.get("inventory_sha256") == canonical_hash(entries) and complete.get("deleted_count") == len(entries) and complete.get("deleted_sha256") == canonical_hash(entries) and complete.get("deletion_method") == method and complete.get("row_root_absent") is True and all(complete.get("post_delete_work", {}).get(key) == 0 for key in (*dimensions, "files", "directories", "symlinks"))
        if not valid_prepare or not valid_complete:
            raise RuntimeError(f"row cleanup record mismatch: {sequence}")
        if results is not None and (results / prepare["row_root"]).exists():
            raise RuntimeError(f"retired row root exists: {sequence}")
    peaks = {key: max(row["pre_delete_row"][key] for row in prepares) for key in dimensions}; cumulative = {key: sum(row["pre_delete_row"][key] for row in prepares) for key in dimensions}
    expected_cleanup = cleanup.get("status") == "PASS" and cleanup.get("declared_root") == "work-v13" and cleanup.get("deletion_method") == method and cleanup.get("prepare_records") == cleanup.get("complete_records") == 9 and cleanup.get("row_cleanup_records") == 18 and cleanup.get("row_cleanup_labels") == ROW_LABELS and cleanup.get("all_row_roots_absent") is True and cleanup.get("work_root_absent") is True and cleanup.get("broad_deletion") is False and cleanup.get("durable_prepare_complete") is True and cleanup.get("peak_equation") == "max_individual_PREPARE_pre_delete_row_not_cumulative_sum" and all(cleanup.get(f"peak_{key}") == peaks[key] and cleanup.get(f"cumulative_{key}") == cumulative[key] for key in dimensions) and max(peaks.values()) <= 512 * 1024 * 1024
    if not expected_cleanup or (results is not None and (cleanup.get("row_cleanup_sha256") != sha256(results / "ROW-CLEANUP-v13.jsonl") or (results / "work-v13").exists())):
        raise RuntimeError("final cleanup peak/equation/artifact mismatch")


def verify_campaign_inputs():
    if not RESULTS.is_dir() or LOCK.exists() or any(path.is_symlink() for path in RESULTS.rglob("*")):
        raise RuntimeError("result root absent, lock present, or retained symlink present")
    if any(path.exists() for path in (MANIFEST, TERMINAL, VERIFICATION)):
        raise RuntimeError("v13 final artifacts already exist")
    if (RESULTS / "FAILURE-v13.json").exists():
        raise RuntimeError("failed/revised campaign cannot be finalized")
    campaign = read_json(RESULTS / "CAMPAIGN-v13.json")
    primary = read_json(RESULTS / "G3-PRIMARY-ANALYSIS-v13.json")
    independent = read_json(RESULTS / "G3-INDEPENDENT-RECOMPUTATION-v13.json")
    source = read_json(RESULTS / "SOURCE-CUSTODY-v13.json")
    operand = read_json(RESULTS / "OPERAND-CUSTODY-v13.json")
    methods = read_json(RESULTS / "METHODOLOGY-CUSTODY-v13.json")
    environment_path = RESULTS / "ENVIRONMENT-v13.json"
    environment = read_json(environment_path)
    cleanup = read_json(RESULTS / "CLEANUP-v13.json")
    raw = RESULTS / "rows-v13/G3-V13-RAW.jsonl"
    raw_rows = [json.loads(line) for line in raw.read_text().splitlines() if line]
    row_cleanup_path = RESULTS / "ROW-CLEANUP-v13.jsonl"
    row_cleanups = [json.loads(line) for line in row_cleanup_path.read_text().splitlines() if line]
    commands = json.loads((RESULTS / "COMMANDS-v13.json").read_text())
    chronology = [json.loads(line) for line in (RESULTS / "CHRONOLOGY-v13.jsonl").read_text().splitlines() if line]
    starts = [row for row in chronology if row.get("event") == "child-start"]
    completes = [row for row in chronology if row.get("event") == "child-complete"]
    if not isinstance(commands, list) or len(commands) != 14 or sum(row.get("kind") == "build" for row in commands) != 1 or sum(row.get("kind") == "measured-row" for row in commands) != 9 or [row.get("label") for row in starts] != [row.get("label") for row in commands] or [row.get("label") for row in completes] != [row.get("label") for row in commands] or [row.get("command") for row in starts] != [row.get("command") for row in commands] or [row.get("command") for row in completes] != [row.get("command") for row in commands] or any(row.get("exit_code") != 0 for row in completes):
        raise RuntimeError("command/chronology/build-once custody mismatch")
    if len([line for line in raw.read_text().splitlines() if line]) != 9:
        raise RuntimeError("raw row count mismatch")
    verify_row_cleanup(cleanup, row_cleanups, raw_rows, RESULTS)
    if campaign.get("status") != "PASS" or campaign.get("rows") != 9 or campaign.get("rows_rerun") != 0 or campaign.get("build_invocations") != 1 or campaign.get("global_elapsed_ns", 59_000_000_000) >= 59_000_000_000:
        raise RuntimeError("campaign is not a complete PASS")
    if primary.get("status") != "PASS" or independent.get("status") != "PASS" or primary.get("normalized_ledger") != independent.get("normalized_ledger") or primary.get("normalized_ledger_sha256") != independent.get("normalized_ledger_sha256"):
        raise RuntimeError("analysis PASS/agreement mismatch")
    ledger = primary["normalized_ledger"]
    if canonical_hash(ledger) != primary["normalized_ledger_sha256"] or ledger.get("failures") or not all(ledger.get("gates", {}).values()):
        raise RuntimeError("normalized ledger is not an exact PASS")
    identity = [{key: row.get(key) for key in ("path", "sha256", "size_bytes", "source_mode")} for row in source.get("sources", [])]
    if source.get("status") != "PASS" or [row.get("path") for row in source.get("sources", [])] != SOURCE_PATHS or source.get("source_set_sha256") != canonical_hash(identity):
        raise RuntimeError("source custody mismatch")
    for row in source["sources"]:
        original, copy = exact_source_copy(row)
        if original.resolve() == copy.resolve():
            raise RuntimeError(f"source copy is not distinct: {row['path']}")
    binary = RESULTS / operand.get("copy_path", "")
    if operand.get("status") != "PASS" or operand.get("source_set_sha256") != source["source_set_sha256"] or operand.get("build_invocations") != 1 or operand.get("build_command") != BUILD_COMMAND or operand.get("copy_path") != "operands-v13/phase4_create_edit_benchmark" or not binary.is_file() or binary.is_symlink() or sha256(binary) != operand.get("sha256") or mode(binary) != "0500":
        raise RuntimeError("binary custody mismatch")
    method_rows = methods.get("methods", [])
    if methods.get("status") != "PASS" or [row.get("path") for row in method_rows] != METHOD_NAMES or methods.get("methodology_set_sha256") != canonical_hash([{key: row[key] for key in ("path", "sha256", "size_bytes")} for row in method_rows]):
        raise RuntimeError("method custody mismatch")
    for row in method_rows:
        if sha256(HERE / row["path"]) != row["sha256"] or sha256(RESULTS / "methodology-v13" / row["path"]) != row["sha256"]:
            raise RuntimeError(f"method changed: {row['path']}")
    freeze_copy = RESULTS / methods.get("premeasurement_freeze_path", "")
    if sha256(DRY_RUN) != methods.get("dry_run_sha256") or sha256(RESULTS / "methodology-v13/DRY-RUN-v13.json") != methods.get("dry_run_sha256") or methods.get("premeasurement_freeze_path") != "PREMEASUREMENT-FREEZE-v13.json" or methods.get("premeasurement_freeze_copy_mode") != "0400" or PREMEASUREMENT_FREEZE.is_symlink() or not PREMEASUREMENT_FREEZE.is_file() or sha256(PREMEASUREMENT_FREEZE) != methods.get("premeasurement_freeze_sha256") or freeze_copy.is_symlink() or not freeze_copy.is_file() or mode(freeze_copy) != "0400" or sha256(freeze_copy) != methods.get("premeasurement_freeze_sha256"):
        raise RuntimeError("dry-run or premeasurement-freeze custody mismatch")
    expected_commands = expected_campaign_commands(operand, environment)
    if commands != expected_commands or not exact_environment(environment, source["source_set_sha256"], methods["methodology_set_sha256"], operand["sha256"]) or not exact_row_custody(raw_rows, operand, methods, sha256(environment_path), commands):
        raise RuntimeError("exact command/operand/method/environment row custody mismatch")
    build_environment_hash = environment["child_environments"]["build"]["selected_environment_sha256"]
    runtime_environment_hash = environment["child_environments"]["runtime"]["selected_environment_sha256"]
    if [row.get("environment_role") for row in starts] != ["build", *("runtime" for _ in range(13))] or [row.get("selected_environment_sha256") for row in starts] != [build_environment_hash, *(runtime_environment_hash for _ in range(13))] or [row.get("environment_role") for row in completes] != ["build", *("runtime" for _ in range(13))] or [row.get("selected_environment_sha256") for row in completes] != [build_environment_hash, *(runtime_environment_hash for _ in range(13))]:
        raise RuntimeError("chronology child environment identity mismatch")
    if cleanup.get("status") != "PASS" or cleanup.get("declared_root") != "work-v13" or cleanup.get("work_root_absent") is not True or cleanup.get("broad_deletion") is not False or (RESULTS / "work-v13").exists():
        raise RuntimeError("cleanup mismatch")
    hashes = {"raw_sha256": raw, "row_cleanup_sha256": row_cleanup_path, "primary_sha256": RESULTS / "G3-PRIMARY-ANALYSIS-v13.json", "independent_sha256": RESULTS / "G3-INDEPENDENT-RECOMPUTATION-v13.json", "cleanup_sha256": RESULTS / "CLEANUP-v13.json", "source_custody_sha256": RESULTS / "SOURCE-CUSTODY-v13.json", "binary_custody_sha256": RESULTS / "OPERAND-CUSTODY-v13.json"}
    for name, path in hashes.items():
        if sha256(path) != campaign.get(name):
            raise RuntimeError(f"campaign hash mismatch: {name}")
    if campaign.get("source_set_sha256") != source["source_set_sha256"] or campaign.get("executable_sha256") != operand["sha256"] or campaign.get("methodology_set_sha256") != methods["methodology_set_sha256"] or campaign.get("environment_sha256") != sha256(environment_path) or campaign.get("premeasurement_freeze_sha256") != methods["premeasurement_freeze_sha256"] or campaign.get("normalized_ledger_sha256") != primary["normalized_ledger_sha256"]:
        raise RuntimeError("campaign identity mismatch")
    static = verify_static(source["source_set_sha256"])
    return campaign, primary, source, operand, methods, cleanup, static


def payload_files():
    files = []
    for path in RESULTS.rglob("*"):
        if path.is_symlink():
            raise RuntimeError(f"retained symlink: {path}")
        if path.is_file() and str(path.relative_to(RESULTS)) not in EXCLUDED:
            files.append(path)
    return sorted(files, key=lambda path: str(path.relative_to(RESULTS)))


def write_manifest(files):
    with MANIFEST.open("x", newline="") as handle:
        writer = csv.writer(handle, delimiter="\t", lineterminator="\n")
        writer.writerow(("path", "sha256", "size_bytes"))
        for path in files:
            writer.writerow((str(path.relative_to(RESULTS)), sha256(path), path.stat().st_size))


def verify_manifest():
    rows = list(csv.DictReader(MANIFEST.open(), delimiter="\t"))
    if not rows or len({row["path"] for row in rows}) != len(rows):
        raise RuntimeError("empty or duplicate payload manifest")
    mismatches = []
    for row in rows:
        path = safe_result_file(row["path"])
        if row["path"] in EXCLUDED or sha256(path) != row["sha256"] or path.stat().st_size != int(row["size_bytes"]):
            mismatches.append(row["path"])
    actual = {str(path.relative_to(RESULTS)) for path in payload_files()}
    listed = {row["path"] for row in rows}
    if mismatches or actual != listed:
        raise RuntimeError("payload manifest mismatch or incomplete closure")
    return rows


def seal_existing():
    for path in (entry for entry in RESULTS.rglob("*") if entry.is_file() and not entry.is_symlink()):
        path.chmod(0o444)
    for path in sorted((entry for entry in RESULTS.rglob("*") if entry.is_dir()), key=lambda entry: len(entry.parts), reverse=True):
        path.chmod(0o555)


def final_verification(expected_entries, g2, terminal_hash, manifest_hash, source_set, check_result_root):
    rows = list(csv.DictReader(MANIFEST.open(), delimiter="\t"))
    mismatches = []
    for row in rows:
        path = RESULTS / row["path"]
        if not path.is_file() or path.is_symlink() or sha256(path) != row["sha256"] or path.stat().st_size != int(row["size_bytes"]) or mode(path) != "0444":
            mismatches.append(row["path"])
    payload_names = {row["path"] for row in rows}
    actual_payload = {str(path.relative_to(RESULTS)) for path in RESULTS.rglob("*") if path.is_file() and str(path.relative_to(RESULTS)) not in EXCLUDED}
    checked_directories = [entry for entry in RESULTS.rglob("*") if entry.is_dir()]
    if check_result_root:
        checked_directories.insert(0, RESULTS)
    directory_mode_mismatches = [str(path.relative_to(TARGET)) for path in checked_directories if mode(path) != "0555"]
    file_mode_mismatches = [str(path.relative_to(RESULTS)) for path in RESULTS.rglob("*") if path.is_file() and mode(path) != "0444"]
    symlinks = [str(path.relative_to(RESULTS)) for path in RESULTS.rglob("*") if path.is_symlink()]
    expected_g2 = {name: expected for name, (_, expected) in G2.items()} | {"normalized_ledger_sha256": G2_LEDGER_SHA256}
    if len(rows) != expected_entries or mismatches or payload_names != actual_payload or directory_mode_mismatches or file_mode_mismatches or symlinks or sha256(TERMINAL) != terminal_hash or sha256(MANIFEST) != manifest_hash or LOCK.exists() or g2 != expected_g2:
        raise RuntimeError("sealed terminal verification failed")
    return {"payload_manifest_entries": len(rows), "payload_mismatches": 0, "manifest_closure_exact": True, "file_mode_mismatches": 0, "directory_mode_mismatches": 0, "symlinks": 0, "lock_absent": True, "source_set_sha256": source_set, "terminal_sha256": terminal_hash, "payload_manifest_sha256": manifest_hash, "g2_hashes": g2}


def finalize():
    campaign, primary, source, operand, methods, cleanup, static = verify_campaign_inputs()
    g2 = verify_g2()
    files = payload_files()
    write_manifest(files)
    manifest_rows = verify_manifest()
    terminal = {
        "schema": "phase4-g3-v13-terminal-v1", "status": "PASS",
        "disposition": "G3 PASS / G4 READY", "g4_eligible": True,
        "source_set_sha256": source["source_set_sha256"],
        "executable_sha256": operand["sha256"],
        "methodology_set_sha256": methods["methodology_set_sha256"],
        "environment_sha256": campaign["environment_sha256"],
        "premeasurement_freeze_sha256": methods["premeasurement_freeze_sha256"],
        "raw_sha256": campaign["raw_sha256"], "primary_sha256": campaign["primary_sha256"],
        "row_cleanup_sha256": campaign["row_cleanup_sha256"],
        "independent_sha256": campaign["independent_sha256"],
        "normalized_ledger_sha256": primary["normalized_ledger_sha256"],
        "campaign_sha256": sha256(RESULTS / "CAMPAIGN-v13.json"),
        "cleanup_sha256": sha256(RESULTS / "CLEANUP-v13.json"),
        "static_closure_sha256": sha256(STATIC),
        "source_custody_sha256": sha256(RESULTS / "SOURCE-CUSTODY-v13.json"),
        "binary_custody_sha256": sha256(RESULTS / "OPERAND-CUSTODY-v13.json"),
        "methodology_custody_sha256": sha256(RESULTS / "METHODOLOGY-CUSTODY-v13.json"),
        "dry_run_sha256": methods["dry_run_sha256"],
        "payload_manifest_sha256": sha256(MANIFEST),
        "payload_manifest_entries": len(manifest_rows),
        "static_commands": REQUIRED_STATIC_LABELS,
        "static_command_argv": STATIC_COMMANDS,
        "focused_g3_test_names": sorted(FOCUSED_G3_TEST_NAMES),
        "g2_hashes": g2, "lock_absent": not LOCK.exists(), "symlinks": 0,
    }
    with TERMINAL.open("x") as handle:
        handle.write(json.dumps(terminal, indent=2, sort_keys=True) + "\n")
    terminal_hash, manifest_hash = sha256(TERMINAL), sha256(MANIFEST)
    seal_existing()
    snapshot = {"schema": "phase4-g3-v13-terminal-verification-v1", "status": "PASS", "disposition": terminal["disposition"], **final_verification(len(manifest_rows), g2, terminal_hash, manifest_hash, source["source_set_sha256"], False)}
    with VERIFICATION.open("x") as handle:
        handle.write(json.dumps(snapshot, indent=2, sort_keys=True) + "\n")
    VERIFICATION.chmod(0o444)
    RESULTS.chmod(0o555); TARGET.chmod(0o555)
    final = final_verification(len(manifest_rows), g2, terminal_hash, manifest_hash, source["source_set_sha256"], True)
    if read_json(VERIFICATION).get("status") != "PASS" or mode(VERIFICATION) != "0444" or mode(RESULTS) != "0555" or mode(TARGET) != "0555" or final != {key: snapshot[key] for key in final}:
        raise RuntimeError("post-write terminal verification mismatch")
    print(json.dumps({"status": "PASS", "disposition": terminal["disposition"], "terminal_sha256": terminal_hash, "terminal_verification_sha256": sha256(VERIFICATION), "payload_manifest_sha256": manifest_hash, "source_set_sha256": source["source_set_sha256"]}, sort_keys=True, separators=(",", ":")))


def self_check():
    assert HERE.parents[4] == REPO and EXCLUDED == {"PAYLOAD-MANIFEST-v13.tsv", "TERMINAL-v13.json", "TERMINAL-VERIFICATION-v13.txt"}
    assert REQUIRED_STATIC_LABELS[-1] == "custody-review" and len(REQUIRED_STATIC_LABELS) == 6
    assert FOCUSED_G3_TEST_RESULT == b"test result: ok. 15 passed; 0 failed"
    assert len(STATIC_COMMANDS) == 6 and len(FOCUSED_G3_TEST_NAMES) == 15
    focused_fixture = b"\n".join(f"test {name} ... ok".encode() for name in sorted(FOCUSED_G3_TEST_NAMES)) + b"\ntest result: ok. 15 passed; 0 failed\n"
    assert focused_test_names_exact(focused_fixture)
    assert not focused_test_names_exact(focused_fixture.replace(next(iter(FOCUSED_G3_TEST_NAMES)).encode(), b"wrong-test", 1))
    g2 = verify_g2()
    assert g2 == {name: expected for name, (_, expected) in G2.items()} | {"normalized_ledger_sha256": G2_LEDGER_SHA256}
    with tempfile.TemporaryDirectory() as directory:
        path = Path(directory) / "x"; path.write_bytes(b"x"); assert sha256(path) == "2d711642b726b04401627ca9fbac32f5c8530fb1903cc4db02258717921a4881"
        root = Path(directory) / "results-v13"; copied = root / "source-custody-v13/Cargo.lock"; copied.parent.mkdir(parents=True); copied.write_bytes((REPO / "Cargo.lock").read_bytes()); copied.chmod(0o400)
        source_hash = sha256(REPO / "Cargo.lock")
        record = {"path": "Cargo.lock", "sha256": source_hash, "size_bytes": (REPO / "Cargo.lock").stat().st_size, "source_mode": "0644", "copy_path": "source-custody-v13/Cargo.lock", "copy_sha256": source_hash, "copy_size_bytes": copied.stat().st_size, "copy_mode": "0400"}
        exact_source_copy(record, root)
        mutations = []
        missing = dict(record); missing.pop("copy_path"); mutations.append(missing)
        escaped = dict(record, copy_path="source-custody-v13/../../outside"); mutations.append(escaped)
        wrong_size = dict(record, copy_size_bytes=record["copy_size_bytes"] + 1); mutations.append(wrong_size)
        for mutation in mutations:
            try: exact_source_copy(mutation, root)
            except RuntimeError: continue
            raise AssertionError("source-copy mutation accepted")
    raw = [{"byte_exact": True, "mode_exact": True, "temp_residue_count": 0, "seed_residue_count": 0, "old_or_new": "new", "output_digest": "d", "expected_output_digest": "d"} for _ in ROW_LABELS]
    operand = {"sha256": "a" * 64}; methods = {"methodology_set_sha256": "b" * 64}; environment_hash = "c" * 64
    commands = [{"command": ["setup", str(index)]} for index in range(3)] + [{"command": ["row", str(index)]} for index in range(9)] + [{"command": ["analysis", str(index)]} for index in range(2)]
    custody_rows = [{"executable_sha256": operand["sha256"], "methodology_set_sha256": methods["methodology_set_sha256"], "environment_sha256": environment_hash, "command": commands[index + 3]["command"]} for index in range(9)]
    assert exact_row_custody(custody_rows, operand, methods, environment_hash, commands)
    custody_mutations = []
    for key, value in (("executable_sha256", "0" * 64), ("methodology_set_sha256", "0" * 64), ("environment_sha256", "0" * 64), ("command", ["wrong"])):
        changed = json.loads(json.dumps(custody_rows)); changed[0][key] = value; custody_mutations.append(changed)
    assert all(not exact_row_custody(changed, operand, methods, environment_hash, commands) for changed in custody_mutations)
    records = []
    for sequence, label in enumerate(ROW_LABELS, 1):
        usage = {"logical_bytes": sequence, "apparent_bytes": sequence + 10, "allocated_bytes": sequence + 20, "files": 1, "directories": 1, "symlinks": 0}; entries = [{"path": "artifact", "kind": "regular", "device": 1, "inode": sequence, "mode": 0o600, "nlink": 1, "size_bytes": sequence, "mtime_ns": 1, "ctime_ns": 1, "allocated_bytes": sequence + 20}]
        method = "descriptor-relative-openat-fstatat-unlinkat-rmdir-no-follow-exact-inventory-v1"
        prepare = {"schema": "phase4-g3-v13-row-cleanup-v1", "event": "PREPARE", "sequence": sequence, "label": label, "row_root": f"work-v13/{label}", "row_identity": {"kind": "directory", "device": 1, "inode": 100 + sequence, "mode": 0o700, "nlink": 2, "size_bytes": 0, "mtime_ns": 1, "ctime_ns": 1, "allocated_bytes": 0}, "inventory": entries, "inventory_count": 1, "inventory_sha256": canonical_hash(entries), "pre_delete_row": usage, "pre_delete_work": dict(usage), "deletion_method": method, "anchored_work_dirfd": True, "anchored_row_dirfd": True, "row_fd_retained_prepare_through_delete": True, "enumeration_followed_symlinks": False, "private_namespace_process_custody": True, "candidate_exactness": dict(raw[sequence - 1])}
        complete = {"schema": "phase4-g3-v13-row-cleanup-v1", "event": "COMPLETE", "sequence": sequence, "label": label, "row_root": prepare["row_root"], "prepare_sha256": canonical_hash(prepare), "inventory_count": 1, "inventory_sha256": canonical_hash(entries), "deleted_count": 1, "deleted_sha256": canonical_hash(entries), "deletion_method": method, "row_root_absent": True, "post_delete_work": {key: 0 for key in usage}}
        records.extend((prepare, complete))
    prepares = records[::2]; dims = ("logical_bytes", "apparent_bytes", "allocated_bytes"); cleanup = {"status": "PASS", "declared_root": "work-v13", "deletion_method": method, "prepare_records": 9, "complete_records": 9, "row_cleanup_records": 18, "row_cleanup_labels": ROW_LABELS, "all_row_roots_absent": True, "work_root_absent": True, "broad_deletion": False, "durable_prepare_complete": True, "peak_equation": "max_individual_PREPARE_pre_delete_row_not_cumulative_sum", **{f"peak_{key}": max(row["pre_delete_row"][key] for row in prepares) for key in dims}, **{f"cumulative_{key}": sum(row["pre_delete_row"][key] for row in prepares) for key in dims}}
    verify_row_cleanup(cleanup, records, raw)
    mutations = [records[:-1]]; wrong_hash = json.loads(json.dumps(records)); wrong_hash[0]["inventory_sha256"] = "0" * 64; mutations.append(wrong_hash)
    misordered = json.loads(json.dumps(records)); misordered[1], misordered[2] = misordered[2], misordered[1]; mutations.append(misordered)
    no_method = json.loads(json.dumps(records)); no_method[0].pop("deletion_method"); mutations.append(no_method)
    late = json.loads(json.dumps(records)); late[0]["inventory"].append({**late[0]["inventory"][0], "path": "late", "inode": 999}); late[0]["inventory_count"] = 2; late[0]["inventory_sha256"] = canonical_hash(late[0]["inventory"]); mutations.append(late)
    ancestor = json.loads(json.dumps(records)); ancestor[0]["row_fd_retained_prepare_through_delete"] = False; mutations.append(ancestor)
    for changed in mutations:
        try: verify_row_cleanup(cleanup, changed, raw)
        except RuntimeError: continue
        raise AssertionError("row cleanup mutation accepted")
    cumulative = dict(cleanup, peak_allocated_bytes=cleanup["cumulative_allocated_bytes"])
    try: verify_row_cleanup(cumulative, records, raw)
    except RuntimeError: pass
    else: raise AssertionError("cumulative peak accepted")
    print(json.dumps({"status": "PASS", "g2_root": str(G2_ROOT), "g2_hashes": g2, "static_commands": 6, "focused_test_names": 15, "source_copy_mutations_rejected": 3, "row_custody_mutations_rejected": 4, "row_cleanup_mutations_rejected": 7, "campaign_children_invoked": 0, "finalized": False}, sort_keys=True))


def main():
    parser = argparse.ArgumentParser(); parser.add_argument("--execute", action="store_true"); parser.add_argument("--self-check", action="store_true"); args = parser.parse_args()
    if args.self_check: self_check()
    elif args.execute: finalize()
    else: raise SystemExit("refusing: use --self-check or --execute after PASS STATIC-CLOSURE-v13.json")


if __name__ == "__main__": main()
