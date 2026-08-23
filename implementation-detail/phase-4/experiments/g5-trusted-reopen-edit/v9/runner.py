#!/usr/bin/env python3
import csv
import ctypes
import datetime
import hashlib
import json
import os
import pathlib
import platform
import shutil
import sqlite3
import stat
import subprocess
import sys
import time


HERE = pathlib.Path(__file__).resolve().parent
REPO = HERE.parents[4]
METHOD = HERE / "method"
SCHEDULE = METHOD / "SCHEDULE-v9.tsv"
EXPECTED = METHOD / "EXPECTED-OUTCOMES-v9.tsv"
INPUT_MANIFEST = METHOD / "INPUT-MANIFEST-v9.tsv"
METHOD_MANIFEST = METHOD / "METHOD-MANIFEST-v9.tsv"
SOURCE_FREEZE = METHOD / "SOURCE-FREEZE-v9.json"
LIMITATIONS = HERE / "LIMITATIONS-v9.json"
STATIC_CLOSURE = HERE / "STATIC-CLOSURE-v9.json"
DRY_RUN = HERE / "DRY-RUN-v9.json"
DRY_RUN_INTENT = HERE / "DRY-RUN-INTENT-v9.json"
DRY_RUN_CALIBRATION_STDOUT = HERE / "DRY-RUN-CALIBRATION-v9.stdout"
DRY_RUN_CALIBRATION_STDERR = HERE / "DRY-RUN-CALIBRATION-v9.stderr"
DRY_RUN_CALIBRATION_TERMINAL = HERE / "DRY-RUN-CALIBRATION-TERMINAL-v9.json"
DRY_RUN_DISPOSITION = HERE / "DRY-RUN-DISPOSITION-v9.json"
DRY_RUN_FAILED = HERE / "DRY-RUN-FAILED-v9.json"
PREMEASUREMENT_REVISE = HERE / "PREMEASUREMENT-REVISE-v9.json"
PRIMARY = HERE / "analyzers/primary.py"
INDEPENDENT = HERE / "analyzers/independent.py"

CHECKPOINT = "d58c5a1307253dfc221fe50de996c183deb9458a"
BRANCH = "codex/empty-worktree"
DATE = "20260823"
LOCK = REPO / "target/BENCHMARK_LOCK"
INPUT_ROOT = REPO / f"target/phase4-g5-trusted-reopen-edit-inputs-{DATE}-v9"
SCREEN_RESULT = REPO / f"target/phase4-g5-trusted-reopen-edit-{DATE}-v9-screen"
GATE_RESULT = REPO / f"target/phase4-g5-trusted-reopen-edit-{DATE}-v9"

# Frozen v9 Rust transport.
G5_CHILD_BINARY = HERE / "g5-benchmark/target/release/layerfs-g5-trusted-child-v9"
FIXTURE_FLAG = "--g5-fixture"
PREPARE_FLAG = "--g5-prepare"
CHILD_FLAG = "--g5-child"
SEMANTIC_FLAG = "--g5-semantic"
CHILD_READY_SCHEMA = "phase4-g5-trusted-child-ready-v9"
CHILD_ENVELOPE_SCHEMA = "phase4-g5-trusted-child-row-v9"
CHILD_TERMINAL_SCHEMA = "phase4-g5-trusted-child-terminal-v9"
FIXTURE_SCHEMA = "phase4-g5-trusted-fixture-v9"
PREPARE_SCHEMA = "phase4-g5-trusted-prepare-v9"
SEMANTIC_SCHEMA = "phase4-g5-trusted-semantic-v9"
SEMANTIC_TERMINAL_SCHEMA = "phase4-g5-trusted-semantic-terminal-v9"
SENTINEL_SCHEMA = "phase4-g5-1-protected-sentinel-v9"
REQUEST_FIELDS = (
    "id", "root", "iteration", "warmup", "validation",
)
TIMER_FIELDS = (
    "store_preflight_ns",
    "sqlite_open_and_profile_ns",
    "visible_head_and_transition_ns",
    "edit_base_scope_ns",
    "mapping_and_construction_ns",
    "proof_ns",
    "publication_commit_ns",
    "reconciliation_ns",
)

G4_EXECUTABLE = (
    REPO
    / "target/phase4-g4-materialization-acceptance-20260822-v12/results-v12"
    / "operands-v1/phase4_create_edit_benchmark-g4"
)
G4_EXECUTABLE_SHA256 = "e72988fc25e96f608d0d405e157ea8e837029595ace916f066932082a736db33"
G4_FINAL_MANIFEST = G4_EXECUTABLE.parents[1] / "FINAL-ARTIFACT-HASHES-v1.tsv"
G4_FINAL_MANIFEST_SHA256 = "585be251a1bd1a260a12415790a0e8f4cd59271217c8533639971a11a4c0b012"
V7_PREMEASUREMENT = HERE.parent / "v7/PREMEASUREMENT-REVISE-v7.json"
V7_PREMEASUREMENT_SHA256 = "fce57857882471bc06f327b8c2b0e5ec07443662fc2986e5c77c5f0ce1a6f01d"
V7_PREPARATION_AUDIT = HERE.parent / "v7/PREPARATION-FAILURE-AUDIT-v7.json"
V7_PREPARATION_AUDIT_SHA256 = "3ae6e471bf5d4c3e7a522b3cb19cacb5f0a94429f86ab3c21373d7889c4b24fa"
V7_PARTIAL_INPUT_ROOT = REPO / "target/phase4-g5-trusted-reopen-edit-inputs-20260823-v7"
V7_PARTIAL_TREE_SHA256 = "69645cc89c3b815f07df67d55ccaaf82c9035d92bc5e9f7f916c1112f19825b2"
V8_PREMEASUREMENT = HERE.parent / "v8/PREMEASUREMENT-REVISE-v8.json"
V8_PREMEASUREMENT_SHA256 = "9f155086b9f31246d9430076e521b29a001db01f49fde692f6bf8f862c0d4a09"

CONTROLLING_HASHES = {
    "implementation-detail/phase-4/g5/implementation-verification-plan.md": "7a7092424d7bd7f55f8479791d04d4411b4cd9a1a7a5618355f5015cb7ee0acd",
    "research/phase-4/g5-round-0/benchmark-contracts/g5-fast-iteration-contract.md": "36495a4640e1d20591ece55f7f2ce35bd8b6ed76ccae41e43c288fa01f0635ba",
    "implementation-detail/phase-4/baseline/current-benchmark-scoreboard.md": "aae8a7abe2a13c3dfdf4adc006b31bc08a18fc05d02f7b7b06489d7ed0910b77",
    "implementation-detail/phase-4/experiments/g4-materialization-acceptance/G4-STAGE-TERMINAL-v1.json": "0297ca2e3b49ddb7d8d2d435713450dcc336397b53cbaaaee9647a46eebcede8",
    "implementation-detail/phase-4/experiments/g5-foundation-h11/v9/G5-0-TERMINAL-AUDIT-v9.json": "baef3615ab28c5b56d5714e86f870845d16a02bad688ad270892f7395ce18e26",
}

LIMIT_NS = {"screen": 20_000_000_000, "gate": 120_000_000_000}
RSS_LIMIT = 20_971_520
SUPPORTED_CHILD_OPERATIONS = {
    "first-edit-after-reopen", "same-middle", "one-byte-early", "one-byte-middle",
    "one-byte-late", "plus1-early", "plus1-middle",
}
SCREEN_NATIVE_DISPATCH = {
    "S02": ("semantic", "touched-corruption"),
    "S03": ("semantic", "unrelated-corruption"),
    "S04": ("semantic", "trusted-verified-reopen", "reconciliation"),
    "S07": ("frozen-g4-protected", "full-create", "range"),
}
GATE_ARM_OBSERVATIONS = 200
BASE_FORECAST_COMPONENTS_NS = {
    "two_hundred_arms_at_250ms_each": GATE_ARM_OBSERVATIONS * 250_000_000,
    "nonhash_clone_fsync_and_operand_preparation": 5_000_000_000,
    "lock_custody_and_preflight": 5_000_000_000,
    "analyzers_cleanup_manifests_and_terminal": 10_000_000_000,
}
CALIBRATION_SIZE = 104_857_600
HASH_CALIBRATION_DIVISOR = 2
FORECAST_MODEL_VERSION = "phase4-g5-1-v9-fast-law-forecast-v1"
SECONDARY_BA_OPERATIONS = {"one-byte-early", "one-byte-late", "plus1-middle"}
COMMON_PARITY_FIELDS = (
    "canonical_bytes_authenticated", "objects_authenticated",
    "canonical_authentication_hash_bytes", "canonical_authentication_hashes",
    "reused_object_id_authentications", "reused_object_id_authentication_bytes",
    "statement_cache_acquisitions", "sql_calls", "sql_rows_returned",
    "sql_query_calls", "sql_execute_calls", "sql_rows_changed",
    "row_blob_reads", "row_blob_writes", "row_blob_copy_bytes",
    "borrowed_row_blob_reads", "borrowed_row_blob_bytes",
    "blob_opens", "blob_reads", "blob_writes",
)
MUTATION_WORK_FIELDS = (
    "canonical_new_write_bytes",
    "canonical_bytes_written",
    "mapping_bytes_rewritten",
    "objects_created",
    "objects_reused",
    "transactions",
    "commits",
    "publication_status",
)
S07_FIXTURE_SHA256 = "4a3acf60f044bbae8ed0d0a8aa8fabd8b4cee74216dbccc36255b9c6fbe50a2a"
S07_COMMON = {
    "source_fingerprint": "f79de600cf44b20c4443e06d2e2b9e8819e956ba5a7bcc9cab4ffd8a08059cf8",
    "expected_cdc_references": 53,
    "actual_cdc_references": 53,
    "expected_cdc_sequence_fingerprint": "6a1d02f70694a50859c88c0080f0e2cc046c8b0d9e21f474c58dab66a895f1c1",
    "root_id": "84abbaa054ec67a8411674f5125b5969d0a3b12869b0ac08a1f65f39008b4026",
    "transition_id": "e923b65ef4041952bb0c92b1b375bf29d7619f7e673454f0711cd7b5a138b90c",
    "ordered_closure_digest": "f9c0e593b97e0430ec81e9ef763fa005715b465ca99001835f2acba0794a7ee2",
    "q_current": 0,
}
S07_FULL = {
    **S07_COMMON,
    "operation": "full",
    "canonical_bytes_written": 1_053_105,
    "canonical_new_write_bytes": 1_053_105,
    "canonical_bytes_authenticated": 1_053_105,
    "objects_created": 57,
    "objects_authenticated": 57,
    "objects_reused": 0,
    "mapping_bytes_rewritten": 3_840,
    "source_bytes_read": 1_048_576,
    "raw_bytes_hashed": 1_048_576,
    "payload_io_bytes": 1_048_576,
    "d_bytes": 0,
    "sqlite_pre_logical_database_bytes": 20_480,
    "sqlite_post_logical_database_bytes": 1_105_920,
    "transactions": 1,
    "commits": 1,
    "commit_dispatches": 1,
    "commit_returns": 1,
    "commit_return_successes": 1,
    "commit_return_errors": 0,
    "commit_reconciliation_calls": 0,
    "publication_status": "Committed",
}
S07_RANGE_MEASUREMENT = {
    "label": "sequential-1m",
    "start": 0,
    "end": 1_048_576,
    "returned_bytes": 1_048_576,
    "canonical_bytes_authenticated": 1_052_986,
    "objects_authenticated": 55,
}
S07_RANGE = {
    **S07_COMMON,
    "operation": "read-range-1m",
    "canonical_bytes_authenticated": 1_053_129,
    "objects_authenticated": 57,
    "canonical_bytes_written": 0,
    "canonical_new_write_bytes": 0,
    "objects_created": 0,
    "objects_reused": 0,
    "mapping_bytes_rewritten": 0,
    "payload_io_bytes": 1_048_576,
    "d_bytes": 1_048_576,
    "sqlite_pre_logical_database_bytes": 1_105_920,
    "sqlite_post_logical_database_bytes": 1_105_920,
    "transactions": 0,
    "commits": 0,
    "commit_dispatches": 0,
    "commit_returns": 0,
    "commit_return_successes": 0,
    "commit_return_errors": 0,
    "commit_reconciliation_calls": 0,
    "publication_status": "Unavailable",
}
VERIFIED_INPUT_CUSTODY = None
VERIFIED_INPUT_MANIFEST_SHA256 = None

CLONE_RECEIPT_SCHEMA = "g5-v9-native-clone-receipt-v1"
CLONE_COPY_CONTENT = "NotRehashedPerFastLaw"
CLONE_CUSTODY_PROOF = "preverified-sealed-master-plus-native-clone-receipt"
LOGICAL_CATALOG_SCHEMA = "g5-v9-ordered-logical-catalog-v1"
LOGICAL_CATALOG_HASH_SEMANTICS = (
    "ordered-logical-catalog-content-address-digest-not-physical-sqlite-bytes"
)


def compact(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":"))


def sha256(path):
    digest = hashlib.sha256()
    with pathlib.Path(path).open("rb") as handle:
        for block in iter(lambda: handle.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def sha256_bytes(value):
    return hashlib.sha256(value).hexdigest()


def fsync_dir(path):
    descriptor = os.open(path, os.O_RDONLY | os.O_DIRECTORY)
    try:
        os.fsync(descriptor)
    finally:
        os.close(descriptor)


def fsync_file(path):
    descriptor = os.open(path, os.O_RDONLY | os.O_NOFOLLOW)
    try:
        os.fsync(descriptor)
    finally:
        os.close(descriptor)


def write_text(path, value):
    path = pathlib.Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("x", encoding="utf-8") as handle:
        handle.write(value)
        handle.flush()
        os.fsync(handle.fileno())
    fsync_dir(path.parent)


def write_bytes(path, value):
    path = pathlib.Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
    try:
        view = memoryview(value)
        while view:
            written = os.write(descriptor, view)
            if written <= 0:
                raise OSError("short preparation evidence write")
            view = view[written:]
        os.fsync(descriptor)
    finally:
        os.close(descriptor)
    fsync_dir(path.parent)


def write_json(path, value):
    write_text(path, compact(value) + "\n")


def append_text(path, value):
    path = pathlib.Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    flags = os.O_WRONLY | os.O_APPEND | os.O_CREAT | os.O_NOFOLLOW
    descriptor = os.open(path, flags, 0o600)
    try:
        os.write(descriptor, value.encode())
        os.fsync(descriptor)
    finally:
        os.close(descriptor)
    fsync_dir(path.parent)


def read_tsv(path):
    with pathlib.Path(path).open(newline="", encoding="utf-8") as handle:
        return list(csv.DictReader(handle, delimiter="\t"))


def input_manifest_index():
    return {
        row["input_relative_path"]: {"bytes": int(row["bytes"]), "sha256": row["sha256"]}
        for row in read_tsv(INPUT_MANIFEST)
    }


def exact_inventory(root):
    root = pathlib.Path(root)
    values = []
    for path in sorted(root.rglob("*")):
        if path.is_symlink() or not (path.is_dir() or path.is_file()):
            raise RuntimeError(f"unsupported operand inventory entry: {path}")
        values.append(
            {
                "path": str(path.relative_to(root)),
                "kind": "directory" if path.is_dir() else "file",
                "bytes": None if path.is_dir() else path.stat().st_size,
            }
        )
    return values


def path_kind_size_mode_sha256_tree(root):
    root = pathlib.Path(root)
    digest = hashlib.sha256()
    for path in sorted(root.rglob("*")):
        if path.is_symlink() or not (path.is_dir() or path.is_file()):
            raise RuntimeError(f"unsupported preserved-tree entry: {path}")
        row = (
            str(path.relative_to(root)),
            "directory" if path.is_dir() else "file",
            0 if path.is_dir() else path.stat().st_size,
            stat.filemode(path.stat().st_mode),
            "-" if path.is_dir() else sha256(path),
        )
        digest.update(("\0".join(map(str, row)) + "\n").encode())
    return digest.hexdigest()


def clonefile(source, destination):
    function = ctypes.CDLL(None, use_errno=True).clonefile
    function.argtypes = (ctypes.c_char_p, ctypes.c_char_p, ctypes.c_int)
    function.restype = ctypes.c_int
    if function(os.fsencode(source), os.fsencode(destination), 0) != 0:
        error = ctypes.get_errno()
        raise OSError(error, os.strerror(error), str(destination))


def verify_file(path, expected, size=None):
    path = pathlib.Path(path)
    if not path.is_file() or (size is not None and path.stat().st_size != int(size)) or sha256(path) != expected:
        raise RuntimeError(f"custody mismatch: {path}")


def tracked_diff_hash():
    return sha256_bytes(subprocess.check_output(["git", "diff", "--binary"], cwd=REPO))


def status_bytes():
    return subprocess.check_output(
        ["git", "status", "--porcelain=v2", "--untracked-files=normal", "-z"], cwd=REPO
    )


def hash_explicit_sources(paths):
    digest = hashlib.sha256()
    for name in sorted(paths):
        path = REPO / name
        digest.update(name.encode())
        digest.update(b"\0")
        digest.update(str(path.stat().st_size).encode())
        digest.update(b"\0")
        digest.update(bytes.fromhex(sha256(path)))
    return digest.hexdigest()


def verify_repository_identity():
    head = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=REPO, text=True).strip()
    branch = subprocess.check_output(["git", "branch", "--show-current"], cwd=REPO, text=True).strip()
    if (head, branch) != (CHECKPOINT, BRANCH):
        raise RuntimeError(f"repository identity mismatch: {branch} {head}")
    for relative, expected in CONTROLLING_HASHES.items():
        verify_file(REPO / relative, expected)
    verify_file(G4_EXECUTABLE, G4_EXECUTABLE_SHA256)
    verify_file(G4_FINAL_MANIFEST, G4_FINAL_MANIFEST_SHA256)
    verify_file(V7_PREMEASUREMENT, V7_PREMEASUREMENT_SHA256)
    verify_file(V7_PREPARATION_AUDIT, V7_PREPARATION_AUDIT_SHA256)
    if path_kind_size_mode_sha256_tree(V7_PARTIAL_INPUT_ROOT) != V7_PARTIAL_TREE_SHA256:
        raise RuntimeError("preserved v7 partial input custody mismatch")
    verify_file(V8_PREMEASUREMENT, V8_PREMEASUREMENT_SHA256)


def schedule_rows(campaign=None):
    rows = read_tsv(SCHEDULE)
    if len(rows) != 21 or [int(row["ordinal"]) for row in rows] != list(range(1, 22)):
        raise RuntimeError("schedule ordinal/count mismatch")
    screen = [row for row in rows if row["campaign"] == "screen"]
    gate = [row for row in rows if row["campaign"] == "gate"]
    if len(screen) != 7 or len(gate) != 14:
        raise RuntimeError("screen/gate schedule count mismatch")
    undispatched = [
        row["sequence_id"]
        for row in screen
        if row["operation"] not in SUPPORTED_CHILD_OPERATIONS
        and row["sequence_id"] not in SCREEN_NATIVE_DISPATCH
    ]
    if undispatched or set(SCREEN_NATIVE_DISPATCH) != {"S02", "S03", "S04", "S07"}:
        raise RuntimeError(f"screen dispatch coverage mismatch: {undispatched}")
    for comparison in ("g4-verified-vs-g5-verified", "g5-verified-vs-g5-trusted"):
        selected = [row for row in gate if row["comparison"] == comparison]
        if len(selected) != 7 or sum(int(row["pairs"]) for row in selected) != 50:
            raise RuntimeError(f"comparison pair law mismatch: {comparison}")
        primary = next(row for row in selected if row["operation"] == "first-edit-after-reopen")
        if int(primary["pairs"]) != 20 or any(
            int(row["pairs"]) != 5 for row in selected if row is not primary
        ):
            raise RuntimeError(f"primary/secondary pair mismatch: {comparison}")
    expectations = read_tsv(EXPECTED)
    expectation_ids = {row["expectation_id"] for row in expectations}
    if not all(row["expectation_id"] in expectation_ids for row in rows):
        raise RuntimeError("schedule references an unknown expectation")
    return rows if campaign is None else [row for row in rows if row["campaign"] == campaign]


def manifest_text(root, key="result_relative_path", excluded=()):
    root = pathlib.Path(root)
    files = sorted(
        path for path in root.rglob("*") if path.is_file() and path.name not in set(excluded)
    )
    return (
        f"{key}\tbytes\tsha256\n"
        + "".join(
            f"{path.relative_to(root)}\t{path.stat().st_size}\t{sha256(path)}\n"
            for path in files
        )
    )


def verify_manifest(root, path, key):
    rows = read_tsv(path)
    names = [row[key] for row in rows]
    if names != sorted(names) or len(names) != len(set(names)):
        raise RuntimeError(f"manifest ordering/uniqueness mismatch: {path}")
    for row in rows:
        verify_file(pathlib.Path(root) / row[key], row["sha256"], row["bytes"])
    reconstructed = f"{key}\tbytes\tsha256\n" + "".join(
        f"{row[key]}\t{row['bytes']}\t{row['sha256']}\n" for row in rows
    )
    if reconstructed.encode() != pathlib.Path(path).read_bytes():
        raise RuntimeError(f"manifest byte reconstruction mismatch: {path}")
    return len(rows)


def verify_sealed_input_manifest():
    rows = read_tsv(INPUT_MANIFEST)
    expected = [row["input_relative_path"] for row in rows]
    actual = []
    root_mode = stat.S_IMODE(INPUT_ROOT.stat(follow_symlinks=False).st_mode)
    if INPUT_ROOT.is_symlink() or not INPUT_ROOT.is_dir() or root_mode != 0o555:
        raise RuntimeError("sealed input root kind/mode mismatch")
    for path in sorted(INPUT_ROOT.rglob("*")):
        metadata = path.stat(follow_symlinks=False)
        if path.is_symlink():
            raise RuntimeError(f"sealed input symlink is forbidden: {path}")
        if stat.S_ISDIR(metadata.st_mode):
            if stat.S_IMODE(metadata.st_mode) != 0o555:
                raise RuntimeError(f"sealed input directory mode mismatch: {path}")
        elif stat.S_ISREG(metadata.st_mode):
            if stat.S_IMODE(metadata.st_mode) != 0o444:
                raise RuntimeError(f"sealed input file mode mismatch: {path}")
            actual.append(str(path.relative_to(INPUT_ROOT)))
        else:
            raise RuntimeError(f"sealed input kind mismatch: {path}")
    if actual != expected:
        raise RuntimeError("sealed input exact inventory mismatch")
    return verify_manifest(INPUT_ROOT, INPUT_MANIFEST, "input_relative_path")


def method_source_names():
    fixed = {
        str(path.relative_to(REPO))
        for path in (
            HERE / "PREREGISTRATION-v9.md",
            HERE / "REVIEW-SYNTHESIS-v9.md",
            HERE / "SAMPLE-COUNT-INTERPRETATION-ADDENDUM-v9.md",
            HERE / "V8-SUPERSESSION-v9.json",
            HERE / "V8-FORECAST-FAILURE-BINDING-v9.json",
            HERE / "FOCUSED-TEST-ATTEMPTS-v9.json",
            HERE / "OVERLAY-TEST-ATTEMPTS-v9.json",
            HERE / "SOURCE-INTEGRATION-AUDIT-v9.json",
            HERE / "NATIVE-CLONE-MODE-DIAGNOSTIC-v9.json",
            LIMITATIONS,
            HERE / "runner.py",
            PRIMARY,
            INDEPENDENT,
            SCHEDULE,
            EXPECTED,
            INPUT_MANIFEST,
            G4_EXECUTABLE,
            G4_FINAL_MANIFEST,
            V7_PREMEASUREMENT,
            V7_PREPARATION_AUDIT,
            V8_PREMEASUREMENT,
        )
    }
    fixed.update(CONTROLLING_HASHES)
    fixed.update(
        str(path.relative_to(REPO))
        for path in (HERE / "g5-benchmark").rglob("*")
        if path.is_file() and "target" not in path.parts
    )
    tracked = subprocess.check_output(
        [
            "git", "ls-files", "Cargo.toml", "Cargo.lock", "crates/layerfs-core",
            "crates/layerfs-engine",
        ],
        cwd=REPO,
        text=True,
    ).splitlines()
    fixed.update(tracked)
    return sorted(fixed)


def write_input_and_method_manifests():
    input_text = manifest_text(INPUT_ROOT, key="input_relative_path")
    write_text(INPUT_MANIFEST, input_text)
    sources = method_source_names()
    method_text = "repo_relative_path\tbytes\tsha256\n" + "".join(
        f"{name}\t{(REPO / name).stat().st_size}\t{sha256(REPO / name)}\n" for name in sources
    )
    write_text(METHOD_MANIFEST, method_text)
    freeze = {
        "schema": "phase4-g5-1-source-freeze-v9",
        "status": "FROZEN_BEFORE_DRY_RUN",
        "branch": BRANCH,
        "checkpoint": CHECKPOINT,
        "git_status_sha256": sha256_bytes(status_bytes()),
        "tracked_diff_sha256": tracked_diff_hash(),
        "explicit_sources": sources,
        "explicit_sources_sha256": hash_explicit_sources(sources),
        "method_manifest_sha256": sha256(METHOD_MANIFEST),
        "input_manifest_sha256": sha256(INPUT_MANIFEST),
        "g4_verified_executable_sha256": G4_EXECUTABLE_SHA256,
        "g5_executable_sha256": sha256(G5_CHILD_BINARY),
        "schedule_sha256": sha256(SCHEDULE),
        "expectations_sha256": sha256(EXPECTED),
        "limitations_sha256": sha256(LIMITATIONS),
        "interface": {
            "prepare_flag": PREPARE_FLAG,
            "fixture_flag": FIXTURE_FLAG,
            "child_flag": CHILD_FLAG,
            "semantic_flag": SEMANTIC_FLAG,
            "child_ready_schema": CHILD_READY_SCHEMA,
            "child_envelope_schema": CHILD_ENVELOPE_SCHEMA,
            "child_terminal_schema": CHILD_TERMINAL_SCHEMA,
            "request_fields": REQUEST_FIELDS,
        },
        "forecast_model": FORECAST_MODEL_VERSION,
        "base_forecast_components_ns": BASE_FORECAST_COMPONENTS_NS,
        "full_wrapper_limit_ns": LIMIT_NS["gate"],
        "frozen_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
    }
    write_json(SOURCE_FREEZE, freeze)


def strict_native_envelope(stdout, expected_schema):
    if not stdout.endswith(b"\n") or stdout.count(b"\n") != 1 or b"\r" in stdout:
        raise RuntimeError("preparation stdout must be exactly one newline-terminated JSON line")
    try:
        line = stdout[:-1].decode("utf-8", errors="strict")
    except UnicodeDecodeError as error:
        raise RuntimeError("preparation stdout is not UTF-8") from error
    try:
        value = json.loads(line)
    except json.JSONDecodeError as error:
        raise RuntimeError("preparation stdout is not native JSON") from error
    if type(value) is not dict or value.get("schema") != expected_schema:
        raise RuntimeError("preparation native JSON schema mismatch")
    return value


def run_preparation_command(
    ordinal, label, command, target_root, expected_schema, expected_fields, evidence_root
):
    command = list(map(str, command))
    prefix = f"{ordinal:03d}-{label}"
    chronology = evidence_root / "CHRONOLOGY-v9.jsonl"
    intent = evidence_root / f"{prefix}.intent.json"
    stdout_path = evidence_root / f"{prefix}.stdout"
    stderr_path = evidence_root / f"{prefix}.stderr"
    terminal_path = evidence_root / f"{prefix}.terminal.json"
    started_utc = datetime.datetime.now(datetime.timezone.utc).isoformat()
    started_ns = time.monotonic_ns()
    write_json(
        intent,
        {
            "schema": "phase4-g5-1-preparation-command-intent-v9",
            "ordinal": ordinal,
            "label": label,
            "argv": command,
            "executable_sha256": sha256(command[0]),
            "target_root": str(target_root),
            "target_preinventory": exact_inventory(target_root),
            "expected_stdout_schema": expected_schema,
            "expected_envelope_fields": expected_fields,
            "expected_stderr_bytes": 0,
            "started_utc": started_utc,
            "started_monotonic_ns": started_ns,
        },
    )
    append_text(
        chronology,
        compact(
            {
                "event": "command-started",
                "ordinal": ordinal,
                "label": label,
                "intent_sha256": sha256(intent),
                "monotonic_ns": started_ns,
            }
        )
        + "\n",
    )
    completed = subprocess.run(command, cwd=REPO, text=False, capture_output=True)
    ended_ns = time.monotonic_ns()
    write_bytes(stdout_path, completed.stdout)
    write_bytes(stderr_path, completed.stderr)
    terminal = {
        "schema": "phase4-g5-1-preparation-command-terminal-v9",
        "ordinal": ordinal,
        "label": label,
        "return_code": completed.returncode,
        "started_monotonic_ns": started_ns,
        "ended_monotonic_ns": ended_ns,
        "elapsed_ns": ended_ns - started_ns,
        "stdout_relative_path": str(stdout_path.relative_to(INPUT_ROOT)),
        "stdout_bytes": len(completed.stdout),
        "stdout_sha256": sha256(stdout_path),
        "stderr_relative_path": str(stderr_path.relative_to(INPUT_ROOT)),
        "stderr_bytes": len(completed.stderr),
        "stderr_sha256": sha256(stderr_path),
        "target_postinventory": exact_inventory(target_root),
        "executable_sha256": sha256(command[0]),
    }
    write_json(terminal_path, terminal)
    append_text(
        chronology,
        compact(
            {
                "event": "command-returned",
                "ordinal": ordinal,
                "label": label,
                "return_code": completed.returncode,
                "terminal_sha256": sha256(terminal_path),
                "monotonic_ns": ended_ns,
            }
        )
        + "\n",
    )
    try:
        if completed.returncode != 0:
            raise RuntimeError(f"preparation command returned {completed.returncode}")
        if completed.stderr != b"":
            raise RuntimeError("preparation command emitted stderr")
        envelope = strict_native_envelope(completed.stdout, expected_schema)
        mismatches = {
            name: {"expected": expected, "actual": envelope.get(name)}
            for name, expected in expected_fields.items()
            if envelope.get(name) != expected
        }
        if mismatches:
            raise RuntimeError(f"preparation native envelope field mismatch: {mismatches}")
    except Exception as error:
        append_text(
            chronology,
            compact(
                {
                    "event": "command-rejected",
                    "ordinal": ordinal,
                    "label": label,
                    "error": str(error),
                    "terminal_sha256": sha256(terminal_path),
                    "monotonic_ns": time.monotonic_ns(),
                }
            )
            + "\n",
        )
        raise
    append_text(
        chronology,
        compact(
            {
                "event": "command-accepted",
                "ordinal": ordinal,
                "label": label,
                "terminal_sha256": sha256(terminal_path),
                "monotonic_ns": time.monotonic_ns(),
            }
        )
        + "\n",
    )
    return {
        "ordinal": ordinal,
        "label": label,
        "argv": command,
        "intent_sha256": sha256(intent),
        "terminal_sha256": sha256(terminal_path),
        "stdout_sha256": terminal["stdout_sha256"],
        "stderr_sha256": terminal["stderr_sha256"],
        "envelope": envelope,
    }


def seal_input_tree():
    verify_manifest(INPUT_ROOT, INPUT_MANIFEST, "input_relative_path")
    for path in sorted(INPUT_ROOT.rglob("*"), reverse=True):
        if path.is_file():
            fsync_file(path)
            path.chmod(0o444)
        else:
            path.chmod(0o555)
            fsync_dir(path)
    INPUT_ROOT.chmod(0o555)
    fsync_dir(INPUT_ROOT)
    fsync_dir(INPUT_ROOT.parent)
    verify_sealed_input_manifest()
    if any(path.stat().st_mode & 0o222 for path in INPUT_ROOT.rglob("*")):
        raise RuntimeError("sealed input tree remains writable")


def prepare_inputs():
    verify_repository_identity()
    schedule_rows()
    if LOCK.exists() or SCREEN_RESULT.exists() or GATE_RESULT.exists() or INPUT_ROOT.exists():
        raise RuntimeError("prepare-inputs requires absent lock, inputs, and result roots")
    if any(
        path.exists()
        for path in (
            INPUT_MANIFEST,
            METHOD_MANIFEST,
            SOURCE_FREEZE,
            DRY_RUN,
            DRY_RUN_INTENT,
            DRY_RUN_CALIBRATION_STDOUT,
            DRY_RUN_CALIBRATION_STDERR,
            DRY_RUN_CALIBRATION_TERMINAL,
            DRY_RUN_DISPOSITION,
            DRY_RUN_FAILED,
            PREMEASUREMENT_REVISE,
            STATIC_CLOSURE,
        )
    ):
        raise RuntimeError("v9 method/freeze evidence already exists")
    if not G5_CHILD_BINARY.is_file() or not os.access(G5_CHILD_BINARY, os.X_OK):
        raise RuntimeError(f"pending Rust child interface: {G5_CHILD_BINARY}")
    INPUT_ROOT.mkdir(mode=0o700)
    fsync_dir(INPUT_ROOT.parent)
    evidence_root = INPUT_ROOT / "preparation-evidence-v9"
    evidence_root.mkdir(mode=0o700)
    fsync_dir(INPUT_ROOT)
    records = []
    fixture_roots = {}
    ordinal = 0
    try:
        for size in (1_048_576, 10_485_760, 104_857_600):
            root = INPUT_ROOT / "fixtures" / str(size)
            root.mkdir(parents=True)
            fsync_dir(root)
            fsync_dir(root.parent)
            ordinal += 1
            record = run_preparation_command(
                ordinal,
                f"fixture-{size}",
                [str(G5_CHILD_BINARY), FIXTURE_FLAG, str(root), str(size)],
                root,
                FIXTURE_SCHEMA,
                {"status": "PASS", "size_bytes": size, "q_current": 0},
                evidence_root,
            )
            records.append({"kind": "fixture", **record})
            fixture_roots[size] = root
        masters = sorted(
            {
                (int(row["size_bytes"]), row["operation"])
                for row in schedule_rows()
                if row["operation"] in SUPPORTED_CHILD_OPERATIONS
            }
        )
        for size, operation in masters:
            root = INPUT_ROOT / "bases" / f"{operation}-{size}"
            clone_fixture_for_preparation(fixture_roots[size], root)
            ordinal += 1
            record = run_preparation_command(
                ordinal,
                f"prepare-{size}-{operation}",
                [str(G5_CHILD_BINARY), PREPARE_FLAG, str(root), str(size), operation, "0"],
                root,
                PREPARE_SCHEMA,
                {
                    "status": "PASS",
                    "size_bytes": size,
                    "operation": operation,
                    "iteration": 0,
                    "q_current": 0,
                },
                evidence_root,
            )
            records.append({"kind": "prepared-row", **record})
    except Exception as error:
        append_text(
            evidence_root / "CHRONOLOGY-v9.jsonl",
            compact(
                {
                    "event": "preparation-failed",
                    "ordinal": ordinal,
                    "error": str(error),
                    "monotonic_ns": time.monotonic_ns(),
                }
            )
            + "\n",
        )
        write_json(
            INPUT_ROOT / "PREPARATION-FAILED-v9.json",
            {
                "schema": "phase4-g5-1-input-preparation-failure-v9",
                "status": "REVISE",
                "first_failing_ordinal": ordinal,
                "error": str(error),
                "partial_inventory_before_failure_record": exact_inventory(INPUT_ROOT),
                "partial_tree_digest_before_failure_record": path_kind_size_mode_sha256_tree(INPUT_ROOT),
                "commands_accepted": len(records),
                "no_cleanup_or_retry": True,
                "method_manifest_created": False,
                "source_freeze_created": False,
                "input_tree_sealed": False,
            },
        )
        raise
    preparation_manifest = evidence_root / "PREPARATION-EVIDENCE-MANIFEST-v9.tsv"
    write_text(
        preparation_manifest,
        manifest_text(
            evidence_root,
            key="evidence_relative_path",
            excluded={preparation_manifest.name},
        ),
    )
    evidence_files = verify_manifest(
        evidence_root, preparation_manifest, "evidence_relative_path"
    )
    write_json(
        INPUT_ROOT / "PREPARATION-CUSTODY-v9.json",
        {
            "schema": "phase4-g5-1-input-preparation-v9",
            "status": "PASS",
            "executable_sha256": sha256(G5_CHILD_BINARY),
            "fixture_sizes": sorted(fixture_roots),
            "prepared_masters": [{"size_bytes": size, "operation": operation} for size, operation in masters],
            "command_count": len(records),
            "evidence_files": evidence_files,
            "preparation_evidence_manifest_sha256": sha256(preparation_manifest),
            "records": records,
        },
    )
    write_input_and_method_manifests()
    seal_input_tree()
    print(compact({"status": "PASS", "input_root": str(INPUT_ROOT), "source_freeze": str(SOURCE_FREEZE)}))
    return 0


def verify_dry_run(freeze):
    value = json.loads(DRY_RUN.read_text(encoding="utf-8"))
    intent = json.loads(DRY_RUN_INTENT.read_text(encoding="utf-8"))
    disposition = json.loads(DRY_RUN_DISPOSITION.read_text(encoding="utf-8"))
    calibration_source = INPUT_ROOT / "fixtures" / str(CALIBRATION_SIZE) / "S1-100.source"
    calibration_manifest = manifest_entry(calibration_source)
    required = {
        "schema": "phase4-g5-1-dry-run-v9",
        "status": "PASS",
        "measured_rows": 0,
        "benchmark_child_processes_started": 0,
        "stores_opened": 0,
        "base_copies_created": 0,
        "measurement_timers_started": 0,
        "gate_arm_observations": GATE_ARM_OBSERVATIONS,
        "fixed_complete_roundtrip_arms": 56,
        "source_freeze_sha256": sha256(SOURCE_FREEZE),
        "method_manifest_sha256": freeze["method_manifest_sha256"],
        "input_manifest_sha256": freeze["input_manifest_sha256"],
        "full_wrapper_limit_ns": LIMIT_NS["gate"],
        "full_wrapper_forecast_status": "PASS",
        "full_wrapper_forecast_overrun_ns": 0,
    }
    if any(value.get(key) != expected for key, expected in required.items()):
        raise RuntimeError("dry-run custody/zero-row mismatch")
    intent_required = {
        "schema": "phase4-g5-1-dry-run-intent-v9",
        "status": "STARTED",
        "branch": BRANCH,
        "head": CHECKPOINT,
        "git_status_sha256": freeze["git_status_sha256"],
        "tracked_diff_sha256": freeze["tracked_diff_sha256"],
        "source_freeze_sha256": sha256(SOURCE_FREEZE),
        "method_manifest_sha256": freeze["method_manifest_sha256"],
        "input_manifest_sha256": freeze["input_manifest_sha256"],
        "schedule_sha256": freeze["schedule_sha256"],
        "gate_arm_observations": GATE_ARM_OBSERVATIONS,
        "fixed_complete_roundtrip_arms": 56,
        "measured_rows": 0,
        "benchmark_child_processes_started": 0,
        "calibration_processes_started": 0,
        "stores_opened": 0,
        "base_copies_created": 0,
        "measurement_timers_started": 0,
        "global_lock_absent": True,
        "global_lock_acquired": False,
        "result_roots_absent": True,
        "calibration_source": str(calibration_source),
        "calibration_source_bytes": CALIBRATION_SIZE,
        "calibration_source_manifest_sha256": calibration_manifest["sha256"],
        "calibration_external_argv": [
            "/usr/bin/shasum", "-a", "256", str(calibration_source),
        ],
        "forecast_model_version": FORECAST_MODEL_VERSION,
        "full_wrapper_limit_ns": LIMIT_NS["gate"],
    }
    if any(intent.get(key) != expected for key, expected in intent_required.items()):
        raise RuntimeError("dry-run intent custody mismatch")
    if (
        DRY_RUN_FAILED.exists()
        or PREMEASUREMENT_REVISE.exists()
        or disposition.get("schema") != "phase4-g5-1-dry-run-disposition-v9"
        or disposition.get("status") != "PASS"
        or disposition.get("dry_run_sha256") != sha256(DRY_RUN)
        or disposition.get("intent_sha256") != sha256(DRY_RUN_INTENT)
        or disposition.get("calibration_stdout_sha256")
        != sha256(DRY_RUN_CALIBRATION_STDOUT)
        or disposition.get("calibration_stderr_sha256")
        != sha256(DRY_RUN_CALIBRATION_STDERR)
        or disposition.get("calibration_terminal_sha256")
        != sha256(DRY_RUN_CALIBRATION_TERMINAL)
        or disposition.get("premeasurement_revise_sha256") is not None
    ):
        raise RuntimeError("dry-run disposition custody mismatch")
    components = value.get("full_wrapper_forecast_components_ns")
    calibration = value.get("hash_calibration", {})
    observed = [
        calibration.get("python", {}).get("bytes_per_second"),
        calibration.get("external_shasum", {}).get("bytes_per_second"),
    ]
    floor = calibration.get("conservative_floor_bytes_per_second")
    if (
        not isinstance(components, dict)
        or not components
        or any(type(number) is not int or number < 0 for number in components.values())
        or sum(components.values()) != value.get("full_wrapper_forecast_ns")
        or value["full_wrapper_forecast_ns"] > LIMIT_NS["gate"]
        or type(value.get("full_wrapper_forecast_reserve_ns")) is not int
        or value["full_wrapper_forecast_reserve_ns"] < 0
        or value["full_wrapper_forecast_ns"]
        + value["full_wrapper_forecast_reserve_ns"]
        != LIMIT_NS["gate"]
        or any(type(number) is not int or number <= 0 for number in observed)
        or type(floor) is not int
        or floor <= 0
        or floor * HASH_CALIBRATION_DIVISOR > min(observed)
        or type(value.get("expected_gate_hash_bytes")) is not int
        or value["expected_gate_hash_bytes"] <= 0
    ):
        raise RuntimeError("dry-run calibrated forecast mismatch")
    return value


def verify_freeze(require_static=False, require_dry=False):
    global VERIFIED_INPUT_CUSTODY, VERIFIED_INPUT_MANIFEST_SHA256
    verify_repository_identity()
    schedule_rows()
    freeze = json.loads(SOURCE_FREEZE.read_text(encoding="utf-8"))
    if freeze.get("status") != "FROZEN_BEFORE_DRY_RUN":
        raise RuntimeError("source freeze status mismatch")
    if tracked_diff_hash() != freeze["tracked_diff_sha256"]:
        raise RuntimeError("tracked diff custody mismatch")
    if sha256_bytes(status_bytes()) != freeze["git_status_sha256"]:
        raise RuntimeError("git status custody mismatch")
    if hash_explicit_sources(freeze["explicit_sources"]) != freeze["explicit_sources_sha256"]:
        raise RuntimeError("explicit source custody mismatch")
    verify_file(METHOD_MANIFEST, freeze["method_manifest_sha256"])
    verify_file(INPUT_MANIFEST, freeze["input_manifest_sha256"])
    verify_file(G5_CHILD_BINARY, freeze["g5_executable_sha256"])
    verify_manifest(REPO, METHOD_MANIFEST, "repo_relative_path")
    verify_sealed_input_manifest()
    VERIFIED_INPUT_CUSTODY = input_manifest_index()
    VERIFIED_INPUT_MANIFEST_SHA256 = freeze["input_manifest_sha256"]
    if require_dry:
        verify_dry_run(freeze)
    if require_static:
        static = json.loads(STATIC_CLOSURE.read_text(encoding="utf-8"))
        screen_terminal = SCREEN_RESULT / "TERMINAL-VERIFICATION-v9.json"
        screen_final_manifest = SCREEN_RESULT / "FINAL-ARTIFACT-HASHES-v9.tsv"
        screen_final_verification = SCREEN_RESULT / "FINAL-READONLY-VERIFICATION-v9.json"
        screen_complete_wall = SCREEN_RESULT / "COMPLETE-WALL-v9.json"
        required = {
            "schema": "phase4-g5-1-static-closure-v9",
            "status": "PASS",
            "source_freeze_sha256": sha256(SOURCE_FREEZE),
            "tracked_diff_sha256": freeze["tracked_diff_sha256"],
            "g5_executable_sha256": freeze["g5_executable_sha256"],
            "screen_terminal_verification_sha256": sha256(screen_terminal),
            "screen_final_artifact_hashes_sha256": sha256(screen_final_manifest),
            "screen_final_readonly_verification_sha256": sha256(screen_final_verification),
            "screen_complete_wall_sha256": sha256(screen_complete_wall),
        }
        final_value = json.loads(screen_final_verification.read_text(encoding="utf-8"))
        wall_value = json.loads(screen_complete_wall.read_text(encoding="utf-8"))
        if (
            any(static.get(key) != value for key, value in required.items())
            or final_value.get("status") != "PASS"
            or final_value.get("lock_absent") is not True
            or wall_value.get("status") != "PASS"
            or wall_value.get("campaign") != "screen"
            or wall_value.get("complete_wall_ns", LIMIT_NS["screen"] + 1) > LIMIT_NS["screen"]
            or verify_manifest(SCREEN_RESULT, screen_final_manifest, "result_relative_path")
            != final_value.get("files_verified")
            or STATIC_CLOSURE.stat().st_mtime_ns
            <= max(
                screen_terminal.stat().st_mtime_ns,
                screen_final_manifest.stat().st_mtime_ns,
                screen_final_verification.stat().st_mtime_ns,
                screen_complete_wall.stat().st_mtime_ns,
            )
        ):
            raise RuntimeError("static closure custody mismatch")
    return freeze


def gate_hash_bytes():
    if VERIFIED_INPUT_CUSTODY is None:
        raise RuntimeError("input manifest has not been preverified")
    input_bytes = sum(item["bytes"] for item in VERIFIED_INPUT_CUSTODY.values())
    method_rows = read_tsv(METHOD_MANIFEST)
    explicit_bytes = sum(int(row["bytes"]) for row in method_rows)
    repository_identity_bytes = sum((REPO / name).stat().st_size for name in CONTROLLING_HASHES)
    repository_identity_bytes += G4_EXECUTABLE.stat().st_size + G4_FINAL_MANIFEST.stat().st_size
    preserved_history_bytes = (
        V7_PREMEASUREMENT.stat().st_size
        + V7_PREPARATION_AUDIT.stat().st_size
        + V8_PREMEASUREMENT.stat().st_size
    )
    preserved_history_bytes += sum(
        path.stat().st_size for path in V7_PARTIAL_INPUT_ROOT.rglob("*") if path.is_file()
    )
    direct_freeze_bytes = METHOD_MANIFEST.stat().st_size + INPUT_MANIFEST.stat().st_size + G5_CHILD_BINARY.stat().st_size
    observations = expanded_observations("gate")
    if len(observations) != GATE_ARM_OBSERVATIONS:
        raise RuntimeError("gate hash forecast arm count mismatch")
    operand_recheck_bytes = 3 * (G4_EXECUTABLE.stat().st_size + G5_CHILD_BINARY.stat().st_size)
    components = {
        "repository_identity": repository_identity_bytes,
        "preserved_v7_v8_failure_evidence": preserved_history_bytes,
        "explicit_method_sources_twice": explicit_bytes * 2,
        "direct_freeze_files": direct_freeze_bytes,
        "sealed_input_manifest_one_preflight_pass": input_bytes,
        "operand_copy_custody_and_terminal_rechecks": operand_recheck_bytes,
    }
    return components, sum(components.values())


def hash_calibration():
    sources = sorted((INPUT_ROOT / "fixtures" / str(CALIBRATION_SIZE)).glob("*.source"))
    if len(sources) != 1:
        raise RuntimeError("dry-run hash calibration requires one frozen 100-MiB source")
    source = sources[0]
    expected = VERIFIED_INPUT_CUSTODY.get(str(source.relative_to(INPUT_ROOT)))
    if expected is None or expected["bytes"] != CALIBRATION_SIZE:
        raise RuntimeError("dry-run hash calibration manifest mismatch")
    python_started = time.monotonic_ns()
    python_digest = sha256(source)
    python_ns = max(1, time.monotonic_ns() - python_started)
    external_started = time.monotonic_ns()
    completed = subprocess.run(
        ["/usr/bin/shasum", "-a", "256", str(source)],
        cwd=REPO,
        capture_output=True,
    )
    external_ns = max(1, time.monotonic_ns() - external_started)
    write_bytes(DRY_RUN_CALIBRATION_STDOUT, completed.stdout)
    write_bytes(DRY_RUN_CALIBRATION_STDERR, completed.stderr)
    write_json(
        DRY_RUN_CALIBRATION_TERMINAL,
        {
            "schema": "phase4-g5-1-dry-run-calibration-terminal-v9",
            "status": "RETURNED",
            "return_code": completed.returncode,
            "source": str(source.relative_to(INPUT_ROOT)),
            "source_bytes": CALIBRATION_SIZE,
            "python_elapsed_ns": python_ns,
            "python_sha256": python_digest,
            "external_elapsed_ns": external_ns,
            "stdout_bytes": len(completed.stdout),
            "stdout_sha256": sha256(DRY_RUN_CALIBRATION_STDOUT),
            "stderr_bytes": len(completed.stderr),
            "stderr_sha256": sha256(DRY_RUN_CALIBRATION_STDERR),
        },
    )
    try:
        external_text = completed.stdout.decode("utf-8", errors="strict")
    except UnicodeDecodeError as error:
        raise RuntimeError("dry-run external shasum stdout is not UTF-8") from error
    external_parts = external_text.split()
    external_digest = external_parts[0] if len(external_parts) == 2 else None
    if (
        completed.returncode != 0
        or completed.stderr != b""
        or python_digest != expected["sha256"]
        or external_digest != python_digest
    ):
        raise RuntimeError("dry-run Python/external SHA-256 calibration mismatch")
    python_bps = CALIBRATION_SIZE * 1_000_000_000 // python_ns
    external_bps = CALIBRATION_SIZE * 1_000_000_000 // external_ns
    floor_bps = min(python_bps, external_bps) // HASH_CALIBRATION_DIVISOR
    if floor_bps <= 0 or floor_bps * HASH_CALIBRATION_DIVISOR > min(python_bps, external_bps):
        raise RuntimeError("invalid conservative SHA-256 throughput floor")
    return {
        "schema": "phase4-g5-1-hash-calibration-v9",
        "classification": "zero-row-nonbenchmark-read-only-hash-calibration",
        "source": str(source.relative_to(INPUT_ROOT)),
        "bytes_per_pass": CALIBRATION_SIZE,
        "python": {"elapsed_ns": python_ns, "bytes_per_second": python_bps, "sha256": python_digest},
        "external_shasum": {"elapsed_ns": external_ns, "bytes_per_second": external_bps, "sha256": external_digest},
        "raw_stdout_sha256": sha256(DRY_RUN_CALIBRATION_STDOUT),
        "raw_stderr_sha256": sha256(DRY_RUN_CALIBRATION_STDERR),
        "terminal_sha256": sha256(DRY_RUN_CALIBRATION_TERMINAL),
        "conservative_floor_bytes_per_second": floor_bps,
        "floor_divisor": HASH_CALIBRATION_DIVISOR,
    }


def dry_run():
    evidence_paths = (
        DRY_RUN,
        DRY_RUN_INTENT,
        DRY_RUN_CALIBRATION_STDOUT,
        DRY_RUN_CALIBRATION_STDERR,
        DRY_RUN_CALIBRATION_TERMINAL,
        DRY_RUN_DISPOSITION,
        DRY_RUN_FAILED,
        PREMEASUREMENT_REVISE,
    )
    if any(path.exists() for path in evidence_paths):
        raise RuntimeError("v9 dry-run evidence already exists")
    calibration_source = INPUT_ROOT / "fixtures" / str(CALIBRATION_SIZE) / "S1-100.source"
    calibration_external_argv = [
        "/usr/bin/shasum", "-a", "256", str(calibration_source),
    ]
    observations = expanded_observations("gate")
    fixed_checkpoints = sum(row["fixed_checkpoint"] for row in observations)
    calibration_manifest = input_manifest_index().get(str(calibration_source.relative_to(INPUT_ROOT)))
    if calibration_manifest is None:
        raise RuntimeError("dry-run calibration source is absent from the input manifest")
    intent = {
        "schema": "phase4-g5-1-dry-run-intent-v9",
        "status": "STARTED",
        "started_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "branch": subprocess.check_output(
            ["git", "branch", "--show-current"], cwd=REPO, text=True
        ).strip(),
        "head": subprocess.check_output(
            ["git", "rev-parse", "HEAD"], cwd=REPO, text=True
        ).strip(),
        "git_status_sha256": sha256_bytes(status_bytes()),
        "tracked_diff_sha256": tracked_diff_hash(),
        "source_freeze_sha256": sha256(SOURCE_FREEZE),
        "method_manifest_sha256": sha256(METHOD_MANIFEST),
        "input_manifest_sha256": sha256(INPUT_MANIFEST),
        "schedule_sha256": sha256(SCHEDULE),
        "gate_arm_observations": len(observations),
        "fixed_complete_roundtrip_arms": fixed_checkpoints,
        "measured_rows": 0,
        "benchmark_child_processes_started": 0,
        "calibration_processes_started": 0,
        "stores_opened": 0,
        "base_copies_created": 0,
        "measurement_timers_started": 0,
        "global_lock_absent": not LOCK.exists(),
        "global_lock_acquired": False,
        "result_roots_absent": not SCREEN_RESULT.exists() and not GATE_RESULT.exists(),
        "calibration_source": str(calibration_source),
        "calibration_source_bytes": CALIBRATION_SIZE,
        "calibration_source_manifest_sha256": calibration_manifest["sha256"],
        "calibration_external_argv": calibration_external_argv,
        "forecast_model_version": FORECAST_MODEL_VERSION,
        "full_wrapper_limit_ns": LIMIT_NS["gate"],
    }
    write_json(DRY_RUN_INTENT, intent)
    collected = {"intent": intent}
    try:
        freeze = verify_freeze()
        collected["freeze"] = freeze
        if LOCK.exists() or SCREEN_RESULT.exists() or GATE_RESULT.exists():
            raise RuntimeError("dry-run requires absent lock and result roots")
        rows = schedule_rows()
        calibration = hash_calibration()
        collected["hash_calibration"] = calibration
        hash_components, expected_hash_bytes = gate_hash_bytes()
        collected["expected_gate_hash_components_bytes"] = hash_components
        collected["expected_gate_hash_bytes"] = expected_hash_bytes
        floor = calibration["conservative_floor_bytes_per_second"]
        hash_forecast_ns = (expected_hash_bytes * 1_000_000_000 + floor - 1) // floor
        forecast_components = {
            **BASE_FORECAST_COMPONENTS_NS,
            "external_bulk_hash_bytes_at_calibrated_floor": hash_forecast_ns,
        }
        full_wrapper_forecast_ns = sum(forecast_components.values())
        forecast_overrun_ns = max(0, full_wrapper_forecast_ns - LIMIT_NS["gate"])
        forecast_reserve_ns = max(0, LIMIT_NS["gate"] - full_wrapper_forecast_ns)
        status = "PASS" if forecast_overrun_ns == 0 else "REVISE"
        collected["full_wrapper_forecast_components_ns"] = forecast_components
        collected["full_wrapper_forecast_ns"] = full_wrapper_forecast_ns
        collected["full_wrapper_forecast_overrun_ns"] = forecast_overrun_ns
        collected["full_wrapper_forecast_reserve_ns"] = forecast_reserve_ns
        generated_residue = sorted(
            str(path.relative_to(REPO)) for path in HERE.rglob("__pycache__") if path.is_dir()
        )
        value = {
            "schema": "phase4-g5-1-dry-run-v9",
            "status": status,
            "measured_rows": 0,
            "benchmark_child_processes_started": 0,
            "calibration_processes_started": 1,
            "stores_opened": 0,
            "base_copies_created": 0,
            "measurement_timers_started": 0,
            "result_roots_absent": True,
            "global_lock_absent": True,
            "schedule_rows": len(rows),
            "screen_sequences": 7,
            "gate_sequences": 14,
            "gate_arm_observations": len(observations),
            "fixed_complete_roundtrip_arms": sum(
                row["fixed_checkpoint"] for row in observations
            ),
            "sample_count_interpretation": "deliberately-stricter-v9-choice-not-unambiguous-user-minimum",
            "hash_calibration": calibration,
            "expected_gate_hash_components_bytes": hash_components,
            "expected_gate_hash_bytes": expected_hash_bytes,
            "gate_hash_scope": (
                "external predictable bulk hashes only; frozen-G4 in-child hashes and "
                "logical-catalog work are inside the 250ms per-arm allowance"
            ),
            "full_wrapper_forecast_components_ns": forecast_components,
            "full_wrapper_forecast_ns": full_wrapper_forecast_ns,
            "full_wrapper_limit_ns": LIMIT_NS["gate"],
            "full_wrapper_forecast_status": status,
            "full_wrapper_forecast_overrun_ns": forecast_overrun_ns,
            "full_wrapper_forecast_reserve_ns": forecast_reserve_ns,
            "forecast_model_version": FORECAST_MODEL_VERSION,
            "generated_non_authoritative_residue": generated_residue,
            "generated_residue_policy": "__pycache__ is generated non-authoritative residue; preserve rather than delete history",
            "source_freeze_sha256": sha256(SOURCE_FREEZE),
            "method_manifest_sha256": freeze["method_manifest_sha256"],
            "input_manifest_sha256": freeze["input_manifest_sha256"],
        }
        collected["dry_run"] = value
        write_json(DRY_RUN, value)
        revise_sha256 = None
        if status == "REVISE":
            write_json(
                PREMEASUREMENT_REVISE,
                {
                    "schema": "phase4-g5-1-premeasurement-revise-v9",
                    "status": "REVISE",
                    "classification": "CALIBRATED_COMPLETE_WALL_FORECAST_EXCEEDS_LIMIT",
                    "intent_sha256": sha256(DRY_RUN_INTENT),
                    "dry_run_sha256": sha256(DRY_RUN),
                    "full_wrapper_forecast_ns": full_wrapper_forecast_ns,
                    "full_wrapper_limit_ns": LIMIT_NS["gate"],
                    "full_wrapper_forecast_overrun_ns": forecast_overrun_ns,
                    "full_wrapper_forecast_reserve_ns": forecast_reserve_ns,
                    "measured_rows": 0,
                    "global_lock_acquired": False,
                },
            )
            revise_sha256 = sha256(PREMEASUREMENT_REVISE)
        write_json(
            DRY_RUN_DISPOSITION,
            {
                "schema": "phase4-g5-1-dry-run-disposition-v9",
                "status": status,
                "intent_sha256": sha256(DRY_RUN_INTENT),
                "calibration_stdout_sha256": sha256(DRY_RUN_CALIBRATION_STDOUT),
                "calibration_stderr_sha256": sha256(DRY_RUN_CALIBRATION_STDERR),
                "calibration_terminal_sha256": sha256(DRY_RUN_CALIBRATION_TERMINAL),
                "dry_run_sha256": sha256(DRY_RUN),
                "premeasurement_revise_sha256": revise_sha256,
                "measured_rows": 0,
                "global_lock_acquired": False,
            },
        )
        print(compact(value))
        return 0 if status == "PASS" else 1
    except Exception as error:
        if not DRY_RUN_FAILED.exists():
            write_json(
                DRY_RUN_FAILED,
                {
                    "schema": "phase4-g5-1-dry-run-failed-v9",
                    "status": "REVISE",
                    "classification": "UNEXPECTED_DRY_RUN_FAILURE",
                    "error_type": type(error).__name__,
                    "error": str(error),
                    "intent_sha256": sha256(DRY_RUN_INTENT),
                    "collected": collected,
                    "artifacts": {
                        str(path): {
                            "present": path.is_file(),
                            "bytes": path.stat().st_size if path.is_file() else None,
                            "sha256": sha256(path) if path.is_file() else None,
                        }
                        for path in (
                            DRY_RUN_CALIBRATION_STDOUT,
                            DRY_RUN_CALIBRATION_STDERR,
                            DRY_RUN_CALIBRATION_TERMINAL,
                            DRY_RUN,
                            PREMEASUREMENT_REVISE,
                        )
                    },
                    "measured_rows": 0,
                    "global_lock_acquired": False,
                },
            )
        if not DRY_RUN_DISPOSITION.exists():
            write_json(
                DRY_RUN_DISPOSITION,
                {
                    "schema": "phase4-g5-1-dry-run-disposition-v9",
                    "status": "REVISE",
                    "classification": "UNEXPECTED_DRY_RUN_FAILURE",
                    "intent_sha256": sha256(DRY_RUN_INTENT),
                    "calibration_stdout_sha256": (
                        sha256(DRY_RUN_CALIBRATION_STDOUT)
                        if DRY_RUN_CALIBRATION_STDOUT.is_file()
                        else None
                    ),
                    "calibration_stderr_sha256": (
                        sha256(DRY_RUN_CALIBRATION_STDERR)
                        if DRY_RUN_CALIBRATION_STDERR.is_file()
                        else None
                    ),
                    "calibration_terminal_sha256": (
                        sha256(DRY_RUN_CALIBRATION_TERMINAL)
                        if DRY_RUN_CALIBRATION_TERMINAL.is_file()
                        else None
                    ),
                    "dry_run_failed_sha256": sha256(DRY_RUN_FAILED),
                    "dry_run_sha256": sha256(DRY_RUN) if DRY_RUN.is_file() else None,
                    "premeasurement_revise_sha256": (
                        sha256(PREMEASUREMENT_REVISE)
                        if PREMEASUREMENT_REVISE.is_file()
                        else None
                    ),
                    "measured_rows": 0,
                    "global_lock_acquired": False,
                },
            )
        raise


def acquire_lock():
    started = time.monotonic_ns()
    descriptor = os.open(LOCK, os.O_RDWR | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
    token = os.urandom(32).hex()
    content = (
        compact(
            {
                "schema": "phase4-g5-1-lock-v9",
                "state": "held",
                "pid": os.getpid(),
                "token": token,
                "acquired_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
            }
        )
        + "\n"
    ).encode()
    os.write(descriptor, content)
    os.fsync(descriptor)
    fsync_dir(LOCK.parent)
    metadata = os.fstat(descriptor)
    return started, {
        "fd": descriptor,
        "token": token,
        "content": content,
        "device": metadata.st_dev,
        "inode": metadata.st_ino,
    }


def verify_owned_lock(lock):
    named = os.stat(LOCK, follow_symlinks=False)
    held = os.fstat(lock["fd"])
    content = os.pread(lock["fd"], len(lock["content"]), 0)
    return (
        (named.st_dev, named.st_ino) == (held.st_dev, held.st_ino) == (lock["device"], lock["inode"])
        and held.st_size == len(lock["content"])
        and content == lock["content"]
    )


def release_lock(lock, result, terminal_verification=None, state="release"):
    if lock.get("fd") is None:
        return None
    try:
        if not verify_owned_lock(lock):
            raise RuntimeError("lock identity/token mismatch before release")
        attestation = result / "BENCHMARK-LOCK-RELEASE-ATTESTATION-v9.json"
        if attestation.exists():
            raise RuntimeError("lock release attestation exists")
        payload = (
            compact(
                {
                    "schema": "phase4-g5-1-lock-v9",
                    "state": state,
                    "pid": os.getpid(),
                    "token": lock["token"],
                    "device": lock["device"],
                    "inode": lock["inode"],
                }
            )
            + "\n"
        ).encode()
        os.pwrite(lock["fd"], payload, 0)
        os.ftruncate(lock["fd"], len(payload))
        os.fsync(lock["fd"])
        lock["content"] = payload
        if not verify_owned_lock(lock):
            raise RuntimeError("lock identity/token mismatch after rewrite")
        os.rename(LOCK, attestation)
        fsync_dir(LOCK.parent)
        fsync_dir(attestation.parent)
        renamed = os.stat(attestation, follow_symlinks=False)
        if LOCK.exists() or (renamed.st_dev, renamed.st_ino) != (lock["device"], lock["inode"]):
            raise RuntimeError("lock release reconciliation mismatch")
        value = {
            "schema": "phase4-g5-1-lock-release-v9",
            "status": "PASS" if state == "release" else "REVISE",
            "state": state,
            "device": lock["device"],
            "inode": lock["inode"],
            "token_sha256": sha256_bytes(lock["token"].encode()),
            "attestation_sha256": sha256(attestation),
            "terminal_verification_sha256": sha256(terminal_verification) if terminal_verification else None,
            "lock_absent": True,
        }
        write_json(result / "LOCK-RELEASE-v9.json", value)
        return value
    finally:
        os.close(lock["fd"])
        lock["fd"] = None


def exclusive_operand_copy(source, destination, expected):
    destination.parent.mkdir(parents=True, exist_ok=True)
    source_fd = os.open(source, os.O_RDONLY | os.O_NOFOLLOW)
    destination_fd = os.open(destination, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o500)
    try:
        while block := os.read(source_fd, 1 << 20):
            view = memoryview(block)
            while view:
                written = os.write(destination_fd, view)
                if written <= 0:
                    raise OSError("short operand write")
                view = view[written:]
        os.fchmod(destination_fd, 0o500)
        os.fsync(destination_fd)
    finally:
        os.close(source_fd)
        os.close(destination_fd)
    fsync_dir(destination.parent)
    verify_file(destination, expected)


def clone_fixture_for_preparation(source_root, destination):
    if destination.exists():
        raise RuntimeError(f"preparation destination exists: {destination}")
    source_root = pathlib.Path(source_root)
    destination.mkdir(parents=True)
    for source in sorted(path for path in source_root.rglob("*") if path.is_file()):
        copied = destination / source.relative_to(source_root)
        copied.parent.mkdir(parents=True, exist_ok=True)
        clonefile(source, copied)
        fsync_file(copied)
        if source.stat().st_ino == copied.stat().st_ino or source.stat().st_size != copied.stat().st_size:
            raise RuntimeError(f"preparation native clone mismatch: {copied}")
    for directory in sorted((path for path in destination.rglob("*") if path.is_dir()), reverse=True):
        fsync_dir(directory)
    fsync_dir(destination)
    fsync_dir(destination.parent)


def manifest_entry(path):
    if VERIFIED_INPUT_CUSTODY is None:
        raise RuntimeError("input manifest has not been preverified")
    path = pathlib.Path(path)
    if not path.is_relative_to(INPUT_ROOT):
        raise RuntimeError(f"manifest path is outside the sealed input root: {path}")
    relative = str(path.relative_to(INPUT_ROOT))
    expected = VERIFIED_INPUT_CUSTODY.get(relative)
    if expected is None:
        raise RuntimeError(f"sealed input manifest entry missing: {relative}")
    return expected


def manifest_master_custody(root):
    root = pathlib.Path(root)
    databases = sorted(path for path in root.rglob("*.sqlite") if path.is_file())
    if len(databases) != 1:
        raise RuntimeError(f"expected one prepared master database, found {len(databases)}: {root}")
    database = databases[0]
    paths = {
        "database_sha256": database,
        "authority_sha256": pathlib.Path(f"{database}.authority"),
        "expectations_sha256": pathlib.Path(f"{database}.expectations"),
    }
    values = {}
    for field, path in paths.items():
        expected = manifest_entry(path)
        metadata = path.stat(follow_symlinks=False)
        if not stat.S_ISREG(metadata.st_mode) or metadata.st_size != expected["bytes"]:
            raise RuntimeError(f"sealed master stat mismatch: {path}")
        values[field] = expected["sha256"]
    values["proof"] = CLONE_CUSTODY_PROOF
    return values


def clone_master_attested(master, destination):
    if VERIFIED_INPUT_MANIFEST_SHA256 is None:
        raise RuntimeError("input manifest digest has not been preverified")
    master = pathlib.Path(master)
    destination = pathlib.Path(destination)
    if destination.exists():
        raise RuntimeError(f"isolated destination exists: {destination}")
    source_inventory = exact_inventory(master)
    destination.mkdir(parents=True)
    entries = []
    for source in sorted(path for path in master.rglob("*") if path.is_file()):
        relative = source.relative_to(master)
        copied = destination / relative
        copied.parent.mkdir(parents=True, exist_ok=True)
        expected = manifest_entry(source)
        before = source.stat(follow_symlinks=False)
        if (
            not stat.S_ISREG(before.st_mode)
            or source.is_symlink()
            or before.st_size != expected["bytes"]
        ):
            raise RuntimeError(f"sealed master file stat mismatch: {source}")
        clonefile(source, copied)
        clone_stat = copied.stat(follow_symlinks=False)
        if (
            not stat.S_ISREG(clone_stat.st_mode)
            or before.st_dev != clone_stat.st_dev
            or before.st_ino == clone_stat.st_ino
            or before.st_size != clone_stat.st_size
            or stat.S_IMODE(before.st_mode) != 0o444
            or stat.S_IMODE(clone_stat.st_mode) != 0o444
        ):
            raise RuntimeError(f"native sealed clone receipt mismatch: {copied}")
        copied.chmod(0o600)
        fsync_file(copied)
        after = source.stat(follow_symlinks=False)
        copied_stat = copied.stat(follow_symlinks=False)
        source_unchanged = (
            before.st_dev,
            before.st_ino,
            before.st_mode,
            before.st_size,
            before.st_mtime_ns,
        ) == (
            after.st_dev,
            after.st_ino,
            after.st_mode,
            after.st_size,
            after.st_mtime_ns,
        )
        same_device = before.st_dev == copied_stat.st_dev
        distinct_inode = before.st_ino != copied_stat.st_ino
        size_equal = before.st_size == copied_stat.st_size == expected["bytes"]
        if (
            not stat.S_ISREG(copied_stat.st_mode)
            or not source_unchanged
            or not same_device
            or not distinct_inode
            or not size_equal
            or stat.S_IMODE(copied_stat.st_mode) != 0o600
        ):
            raise RuntimeError(f"native clone receipt mismatch: {copied}")
        entries.append(
            {
                "path": str(relative),
                "bytes": expected["bytes"],
                "master_manifest_sha256": expected["sha256"],
                "clonefile_success": True,
                "source_device": before.st_dev,
                "source_inode": before.st_ino,
                "source_mode": stat.filemode(before.st_mode),
                "destination_device": copied_stat.st_dev,
                "destination_inode": copied_stat.st_ino,
                "clone_destination_mode": stat.filemode(clone_stat.st_mode),
                "dispatch_mode": stat.filemode(copied_stat.st_mode),
                "mode_transition": "sealed-0444-to-private-0600",
                "same_device": same_device,
                "distinct_inode": distinct_inode,
                "size_equal": size_equal,
                "source_unchanged": source_unchanged,
            }
        )
    for directory in sorted((path for path in destination.rglob("*") if path.is_dir()), reverse=True):
        directory.chmod(0o700)
        fsync_dir(directory)
    destination.chmod(0o700)
    fsync_dir(destination)
    fsync_dir(destination.parent)
    destination_inventory = exact_inventory(destination)
    inventory_equal = source_inventory == destination_inventory
    dispatch_modes_exact = all(
        entry["source_mode"] == "-r--r--r--"
        and entry["clone_destination_mode"] == "-r--r--r--"
        and entry["dispatch_mode"] == "-rw-------"
        for entry in entries
    ) and all(
        stat.S_IMODE(path.stat(follow_symlinks=False).st_mode) == 0o700
        for path in (destination, *(path for path in destination.rglob("*") if path.is_dir()))
    )
    if not entries or not inventory_equal or not dispatch_modes_exact:
        raise RuntimeError(f"native clone inventory mismatch: {destination}")
    return {
        "schema": CLONE_RECEIPT_SCHEMA,
        "method": "darwin-clonefile",
        "copy_content": CLONE_COPY_CONTENT,
        "sealed_input_manifest_sha256": VERIFIED_INPUT_MANIFEST_SHA256,
        "inventory_equal": inventory_equal,
        "dispatch_modes_exact": dispatch_modes_exact,
        "entries": entries,
    }


def parse_time(path):
    text = pathlib.Path(path).read_text(encoding="utf-8")
    import re

    first = re.search(r"([0-9.]+)\s+real\s+([0-9.]+)\s+user\s+([0-9.]+)\s+sys", text)
    rss = re.search(r"^\s*(\d+)\s+maximum resident set size\s*$", text, re.MULTILINE)
    if not first or not rss:
        raise RuntimeError(f"unparsed time sidecar: {path}")
    return {
        "real_seconds": float(first.group(1)),
        "user_seconds": float(first.group(2)),
        "system_seconds": float(first.group(3)),
        "maximum_resident_set_size": int(rss.group(1)),
    }


def run_semantic(executable, case, root, result, label):
    stdout = result / f"children-v9/{label}.stdout"
    stderr = result / f"children-v9/{label}.stderr"
    sidecar = result / f"time-v9/{label}.txt"
    command = [str(executable), SEMANTIC_FLAG, case, str(root)]
    completed = subprocess.run(
        ["/usr/bin/time", "-l", "-o", str(sidecar), *command],
        cwd=REPO,
        text=True,
        capture_output=True,
    )
    write_text(stdout, completed.stdout)
    write_text(stderr, completed.stderr)
    fsync_file(sidecar)
    fsync_dir(sidecar.parent)
    if completed.returncode != 0:
        raise RuntimeError(f"semantic child failed: {case}: {completed.stderr.strip()}")
    values = [json.loads(line) for line in completed.stdout.splitlines() if line]
    if not values or values[-1].get("schema") != SEMANTIC_TERMINAL_SCHEMA:
        raise RuntimeError(f"semantic terminal missing: {case}")
    terminal = values.pop()
    if terminal.get("status") != "PASS" or terminal.get("case") != case or terminal.get("q_current") != 0:
        raise RuntimeError(f"semantic terminal mismatch: {case}")
    required = {
        "status", "schema", "case", "integrity_mode", "error", "later_snapshot_error",
        "publication_status", "reconciliation", "before_generation", "after_generation",
        "before_root", "after_root", "head_unchanged", "transactions", "commits",
        "edit_base_complete_scrub_calls", "edit_base_complete_scrub_canonical_bytes",
        "verified_reopen_complete_scrub_calls", "verified_reopen_complete_scrub_canonical_bytes",
        "trusted_assumed_equal_edges", "trusted_assumed_prior_references",
        "trusted_assumed_prior_raw_bytes", "verified_carry_forward", "cleanup_ok",
        "residue", "q_high_water", "q_current",
    }
    for value in values:
        if (
            value.get("schema") != SEMANTIC_SCHEMA
            or value.get("status") != "PASS"
            or value.get("case") != case
            or required - value.keys()
            or value.get("cleanup_ok") is not True
            or value.get("residue") is not False
            or value.get("q_current") != 0
        ):
            raise RuntimeError(f"semantic record mismatch: {case}")
        value["wrapper"] = {"campaign": "screen", "category": "fault", "semantic_case": case}
    if case == "reconciliation":
        expected = {
            "rollback": "NotAttempted",
            "prior": "PriorVisible",
            "requested": "RequestedVisible",
            "different": "DifferentHead",
            "ambiguous": "Ambiguous",
        }
        observed = {value.get("integrity_mode"): value for value in values}
        if (
            set(observed) != set(expected)
            or len(values) != len(expected)
            or any(
                observed[label].get("reconciliation") != reconciliation
                or observed[label].get("verified_carry_forward") is not False
                for label, reconciliation in expected.items()
            )
        ):
            raise RuntimeError("semantic reconciliation label/outcome mismatch")
    terminal["external_time"] = parse_time(sidecar)
    terminal["role"] = f"semantic_{case.replace('-', '_')}"
    return values, terminal, command


def timed_external_command(command, result, label, env=None):
    stdout = result / f"children-v9/{label}.stdout"
    stderr = result / f"children-v9/{label}.stderr"
    sidecar = result / f"time-v9/{label}.txt"
    completed = subprocess.run(
        ["/usr/bin/time", "-l", "-o", str(sidecar), *map(str, command)],
        cwd=REPO,
        text=True,
        capture_output=True,
        env=env,
    )
    write_text(stdout, completed.stdout)
    write_text(stderr, completed.stderr)
    fsync_file(sidecar)
    fsync_dir(sidecar.parent)
    if completed.returncode != 0:
        raise RuntimeError(f"S07 command failed: {label}: {completed.stderr.strip()}")
    return completed, parse_time(sidecar)


def run_s07(g4_executable, work, result):
    size = 1_048_576
    frozen_fixture = INPUT_ROOT / "fixtures" / str(size)
    if not frozen_fixture.is_dir():
        raise RuntimeError("S07 frozen 1-MiB fixture missing")
    commands, records = [], []
    probe = work / "s07-fixture-probe"
    probe.mkdir()
    command = [g4_executable, "--fast-fixture", probe, str(size)]
    _, external = timed_external_command(command, result, "s07-01-fixture")
    commands.append({"label": "s07-01-fixture", "command": list(map(str, command)), "env": {}, "external": external})
    frozen_sources = sorted(frozen_fixture.glob("*.source"))
    probe_sources = sorted(probe.glob("*.source"))
    frozen_fixture_sha256 = (
        manifest_entry(frozen_sources[0])["sha256"] if len(frozen_sources) == 1 else None
    )
    probe_fixture_sha256 = sha256(probe_sources[0]) if len(probe_sources) == 1 else None
    if (
        len(frozen_sources) != 1
        or len(probe_sources) != 1
        or frozen_fixture_sha256 != S07_FIXTURE_SHA256
        or probe_fixture_sha256 != S07_FIXTURE_SHA256
    ):
        raise RuntimeError("S07 frozen/G4 fixture equivalence mismatch")

    for index, (route, prepare_operation, row_operation, expected_transactions, expected_commits) in enumerate(
        (
            ("full-create", "write", "write", 1, 1),
            ("range", "read-range-1m", "read-range-1m", 0, 0),
        ),
        start=2,
    ):
        root = work / f"s07-{route}"
        base_custody = clone_master_attested(frozen_fixture, root)
        prepare_index = 2 + (index - 2) * 2
        row_index = prepare_index + 1
        prepare = [g4_executable, "--fast-prepare", root, str(size), prepare_operation, "0"]
        _, prepare_external = timed_external_command(prepare, result, f"s07-{prepare_index:02d}-{route}-prepare")
        commands.append({"label": f"s07-{prepare_index:02d}-{route}-prepare", "command": list(map(str, prepare)), "env": {}, "external": prepare_external})
        custody = prepared_master_custody(root)
        allowed_inventory = exact_inventory(root)
        row_env_values = {
            "LAYERFS_FAST_LANE": "1",
            "WP4M_EXECUTABLE_SHA256": G4_EXECUTABLE_SHA256,
            "WP4M_BASE_COPY_METHOD": "fast-lane-isolated-prepared-row",
            "WP4M_BASE_DATABASE_SHA256": custody["database_sha256"],
            "WP4M_BASE_AUTHORITY_SHA256": custody["authority_sha256"],
            "WP4M_BASE_EXPECTATIONS_SHA256": custody["expectations_sha256"],
        }
        row_env = os.environ.copy()
        row_env.update(row_env_values)
        row_command = [
            g4_executable, "--fast-row", root, str(size), row_operation, "0", "false",
            "complete-roundtrip",
        ]
        completed, row_external = timed_external_command(
            row_command, result, f"s07-{row_index:02d}-{route}-row", row_env
        )
        commands.append({"label": f"s07-{row_index:02d}-{route}-row", "command": list(map(str, row_command)), "env": row_env_values, "external": row_external})
        values = [json.loads(line) for line in completed.stdout.splitlines() if line]
        if len(values) != 1:
            raise RuntimeError(f"S07 {route} row count mismatch")
        product = values[0]
        validate_product_resource_evidence(product)
        expected_tuple = S07_FULL if route == "full-create" else S07_RANGE
        tuple_mismatches = {
            key: {"expected": expected, "actual": product.get(key)}
            for key, expected in expected_tuple.items()
            if product.get(key) != expected
        }
        if tuple_mismatches:
            raise RuntimeError(f"S07 {route} deterministic tuple mismatch: {tuple_mismatches}")
        if (
            product.get("status") != "PASS"
            or product.get("error") is not None
            or product.get("transactions") != expected_transactions
            or product.get("commits") != expected_commits
            or product.get("executable_sha256") != G4_EXECUTABLE_SHA256
            or product.get("base_copy_method") != "fast-lane-isolated-prepared-row"
            or product.get("pre_edit_database_sha256") != custody["database_sha256"]
            or product.get("pre_edit_authority_sha256") != custody["authority_sha256"]
            or product.get("pre_edit_expectations_sha256") != custody["expectations_sha256"]
        ):
            raise RuntimeError(f"S07 {route} semantic/work mismatch")
        if route == "range":
            ranges = product.get("range_measurements")
            if not isinstance(ranges, list) or len(ranges) != 1 or any(
                ranges[0].get(key) != expected for key, expected in S07_RANGE_MEASUREMENT.items()
            ):
                raise RuntimeError(f"S07 range counters mismatch: {ranges}")
        elif product.get("range_measurements") != []:
            raise RuntimeError(f"S07 full-create emitted range measurements: {product.get('range_measurements')}")
        state = post_row_state(root, product, allowed_inventory)
        if state["post_authority_sha256"] != custody["authority_sha256"]:
            raise RuntimeError(f"S07 {route} authority changed")
        if state["post_expectations_sha256"] != custody["expectations_sha256"]:
            raise RuntimeError(f"S07 {route} expectations changed")
        pre_cleanup_residue = state["inventory_residue"]
        if pre_cleanup_residue:
            raise RuntimeError(f"S07 {route} pre-cleanup residue: {pre_cleanup_residue}")
        records.append(
            {
                "schema": SENTINEL_SCHEMA,
                "status": "PASS",
                "sequence_id": "S07",
                "route": route,
                "executable_sha256": sha256(g4_executable),
                "frozen_fixture_sha256": frozen_fixture_sha256,
                "probe_fixture_sha256": probe_fixture_sha256,
                "base_custody": base_custody,
                "prepared_custody": custody,
                "row_environment": row_env_values,
                "fixture_command": [str(g4_executable), "--fast-fixture", str(probe), str(size)],
                "prepare_command": list(map(str, prepare)),
                "row_command": list(map(str, row_command)),
                "command_external_times": {
                    "fixture": external,
                    "prepare": prepare_external,
                    "row": row_external,
                },
                "pre_cleanup_residue": pre_cleanup_residue,
                "deterministic_tuple": expected_tuple,
                "deterministic_range": S07_RANGE_MEASUREMENT if route == "range" else None,
                "external_time": row_external,
                "product": product,
                **state,
            }
        )
    return records, commands


def validate_product_resource_evidence(row):
    required = {
        "q_high_water", "q_current", "q_report_output_bytes", "max_single_buffer_bytes",
        "buffer_evidence_complete", "full_file_buffer_bytes", *COMMON_PARITY_FIELDS,
    }
    missing = sorted(required - row.keys())
    if missing:
        raise RuntimeError(f"row resource/interface fields missing: {missing}")
    if (
        type(row["q_high_water"]) is not int
        or row["q_high_water"] <= 0
        or row["q_current"] != 0
        or type(row["q_report_output_bytes"]) is not int
        or row["q_report_output_bytes"] <= 0
        or type(row["max_single_buffer_bytes"]) is not int
        or not 0 <= row["max_single_buffer_bytes"] <= 1_048_576
        or row["buffer_evidence_complete"] is not True
        or row["full_file_buffer_bytes"] != 0
    ):
        raise RuntimeError("row Q/buffer evidence mismatch")


def validate_child_row(envelope):
    if envelope.get("schema") != CHILD_ENVELOPE_SCHEMA or envelope.get("status") != "PASS":
        raise RuntimeError(f"child envelope schema mismatch: {envelope.get('schema')}")
    row = envelope.get("row")
    if not isinstance(row, dict):
        raise RuntimeError("child envelope omitted retained product row")
    required = {
        "store_preflight_wall_ns",
        "sqlite_open_and_profile_wall_ns",
        "visible_head_lookup_and_open_wrapper_wall_ns",
        "edit_base_transition_wall_ns",
        "edit_base_complete_scrub_wall_ns",
        "edit_base_scope_residual_wall_ns",
        "canonical_cas_mapping_stage_wall_ns",
        "precommit_closure_validation_wall_ns",
        "sqlite_commit_durability_wall_ns",
        "commit_reconciliation_wall_ns",
        "first_edit_component_sum_wall_ns",
        "first_edit_equation_total_wall_ns",
        "first_edit_timer_equation_matches",
        "reconciliation_nested_in_commit",
    }
    missing = sorted(required - row.keys())
    if missing:
        raise RuntimeError(f"pending Rust row timer interface: {missing}")
    reconciliation = row["commit_reconciliation_wall_ns"]
    commit = row["sqlite_commit_durability_wall_ns"]
    if row["reconciliation_nested_in_commit"] is not True or commit < reconciliation:
        raise RuntimeError("G5 reconciliation/COMMIT nesting mismatch")
    timers = {
        "store_preflight_ns": row["store_preflight_wall_ns"],
        "sqlite_open_and_profile_ns": row["sqlite_open_and_profile_wall_ns"],
        "visible_head_and_transition_ns": row["visible_head_lookup_and_open_wrapper_wall_ns"],
        "edit_base_scope_ns": row["edit_base_transition_wall_ns"]
        + row["edit_base_complete_scrub_wall_ns"]
        + row["edit_base_scope_residual_wall_ns"],
        "mapping_and_construction_ns": row["canonical_cas_mapping_stage_wall_ns"],
        "proof_ns": row["precommit_closure_validation_wall_ns"],
        "publication_commit_ns": commit - reconciliation,
        "reconciliation_ns": reconciliation,
    }
    if any(type(timers[name]) is not int or timers[name] < 0 for name in TIMER_FIELDS):
        raise RuntimeError("G5 timer value type/range mismatch")
    total = sum(timers[name] for name in TIMER_FIELDS)
    if (
        row["first_edit_timer_equation_matches"] is not True
        or row["first_edit_component_sum_wall_ns"] != total
        or row["first_edit_equation_total_wall_ns"] != total
    ):
        raise RuntimeError("child timer equation mismatch")
    validate_product_resource_evidence(row)
    trusted_fields = (
        "trusted_assumed_equal_edges", "trusted_assumed_prior_references",
        "trusted_assumed_prior_raw_bytes",
    )
    if "covered_equal_edges" not in row or any(name not in row for name in trusted_fields):
        raise RuntimeError("child trust-provenance counters missing")
    if envelope.get("integrity_mode") == "trusted-local-dev":
        if (
            row["covered_equal_edges"] != 0
            or any(type(row[name]) is not int or row[name] < 0 for name in trusted_fields)
            or sum(row[name] for name in trusted_fields) <= 0
        ):
            raise RuntimeError("trusted authority laundering/counter mismatch")
    elif any(row[name] != 0 for name in trusted_fields):
        raise RuntimeError("verified row reported trusted assumptions")
    return {
        "schema": "phase4-g5-1-operation-v9",
        "status": envelope["status"],
        "request_id": envelope.get("request_id"),
        "integrity_mode": envelope.get("integrity_mode"),
        "mode_provenance": envelope.get("mode_provenance"),
        "timers_ns": timers,
        "total_ns": total,
        "decision_ns": total,
        "product": row,
    }


def normalize_g4_row(row):
    validate_product_resource_evidence(row)
    reconciliation = int(row.get("commit_reconciliation_wall_ns", 0))
    commit = int(row["sqlite_commit_durability_wall_ns"])
    timers = {
        "store_preflight_ns": 0,
        "sqlite_open_and_profile_ns": 0,
        "visible_head_and_transition_ns": int(row["fresh_reopen_head_wall_ns"]),
        "edit_base_scope_ns": int(row["same_open_authority_establishment_wall_ns"])
        + int(row["fresh_full_scrub_wall_ns"]),
        "mapping_and_construction_ns": int(row["canonical_cas_mapping_stage_wall_ns"]),
        "proof_ns": int(row["precommit_closure_validation_wall_ns"]),
        "publication_commit_ns": commit - reconciliation,
        "reconciliation_ns": reconciliation,
    }
    if timers["publication_commit_ns"] < 0:
        raise RuntimeError("frozen G4 reconciliation exceeds commit wall")
    return {
        "schema": "phase4-g5-1-operation-v9",
        "status": row.get("status", "PASS"),
        "integrity_mode": "Verified",
        "mode_provenance": "frozen-g4-one-shot",
        "timer_availability": "G4 preflight/open split unavailable; common retained intervals only",
        "timers_ns": timers,
        "total_ns": sum(timers.values()),
        "decision_ns": sum(timers.values()),
        "product": row,
    }


def master_path(row):
    operation = row["operation"]
    size = row["size_bytes"]
    direct = INPUT_ROOT / "bases" / f"{operation}-{size}"
    semantic = INPUT_ROOT / "bases/semantic-small" / f"{operation}-{size}"
    path = direct if direct.is_dir() else semantic
    if not path.is_dir():
        raise RuntimeError(f"missing frozen input master: {path}")
    return path


def prepared_master_custody(root):
    databases = sorted(path for path in pathlib.Path(root).rglob("*.sqlite") if path.is_file())
    if len(databases) != 1:
        raise RuntimeError(f"expected one prepared master database, found {len(databases)}: {root}")
    database = databases[0]
    authority = pathlib.Path(f"{database}.authority")
    expectations = pathlib.Path(f"{database}.expectations")
    if not authority.is_file() or not expectations.is_file():
        raise RuntimeError(f"prepared master sidecar missing: {database}")
    return {
        "database_sha256": sha256(database),
        "authority_sha256": sha256(authority),
        "expectations_sha256": sha256(expectations),
    }


def catalog_value_bytes(value):
    if value is None:
        return b"n", b""
    if isinstance(value, bytes):
        return b"b", value
    if isinstance(value, str):
        return b"s", value.encode("utf-8")
    if type(value) is int:
        return b"i", str(value).encode("ascii")
    raise RuntimeError(f"unsupported logical catalog value: {type(value).__name__}")


def catalog_feed(digest, label, value):
    label = label.encode("ascii")
    kind, payload = catalog_value_bytes(value)
    digest.update(len(label).to_bytes(4, "big"))
    digest.update(label)
    digest.update(kind)
    digest.update(len(payload).to_bytes(8, "big"))
    digest.update(payload)


def catalog_digest(domain, rows):
    digest = hashlib.sha256()
    catalog_feed(digest, "domain", domain)
    for row_index, row in enumerate(rows):
        catalog_feed(digest, "row", row_index)
        for field_index, value in enumerate(row):
            catalog_feed(digest, f"field-{field_index}", value)
    return digest.hexdigest()


def fixed_blob(value, length, label):
    if not isinstance(value, bytes) or len(value) != length:
        raise RuntimeError(f"logical catalog {label} is not a {length}-byte BLOB")
    return value


def logical_catalog(database):
    database = pathlib.Path(database)
    uri = database.resolve().as_uri() + "?mode=ro&immutable=1"
    connection = sqlite3.connect(uri, uri=True, isolation_level=None)
    try:
        connection.execute("PRAGMA query_only=ON")
        query_only = connection.execute("PRAGMA query_only").fetchone() == (1,)
        autocommit = not connection.in_transaction
        if not query_only or not autocommit:
            raise RuntimeError("logical catalog connection is not query-only autocommit")
        sqlite_page_size = connection.execute("PRAGMA page_size").fetchone()[0]
        sqlite_page_count = connection.execute("PRAGMA page_count").fetchone()[0]
        sqlite_freelist_count = connection.execute("PRAGMA freelist_count").fetchone()[0]
        for value, label in (
            (sqlite_page_size, "page_size"),
            (sqlite_page_count, "page_count"),
            (sqlite_freelist_count, "freelist_count"),
        ):
            if type(value) is not int or value < 0:
                raise RuntimeError(f"invalid SQLite {label}")
        sqlite_logical_database_bytes = sqlite_page_size * sqlite_page_count

        schema_rows = connection.execute(
            "SELECT type, name, tbl_name, rootpage, sql FROM sqlite_schema "
            "WHERE name NOT LIKE 'sqlite_%' ORDER BY type, name"
        ).fetchall()
        sqlite_schema_sha256 = catalog_digest(
            "layerfs-g5-v9-sqlite-schema-v1", schema_rows
        )

        meta_rows = connection.execute(
            "SELECT id, profile_id, store_instance_id, validation_authority_id, "
            "validation_key, integrity_epoch, schema_version, journal_mode, synchronous, "
            "temp_store, mmap_size FROM wp4m_meta ORDER BY id"
        ).fetchall()
        if len(meta_rows) != 1 or meta_rows[0][0] != 1:
            raise RuntimeError("logical catalog requires exactly one metadata row")
        meta_sha256 = catalog_digest("layerfs-g5-v9-meta-v1", meta_rows)

        object_digest = hashlib.sha256()
        catalog_feed(object_digest, "domain", "layerfs-g5-v9-object-catalog-v1")
        object_count = 0
        canonical_length_sum = 0
        blob_length_sum = 0
        cursor = connection.execute(
            "SELECT object_id, kind, canonical_length, length(canonical_bytes) "
            "FROM wp4m_objects ORDER BY object_id"
        )
        for object_id, kind, canonical_length, blob_length in cursor:
            fixed_blob(object_id, 32, "object_id")
            if (
                type(kind) is not int
                or kind < 0
                or type(canonical_length) is not int
                or canonical_length < 0
                or type(blob_length) is not int
                or blob_length < 0
            ):
                raise RuntimeError("invalid logical object catalog row")
            catalog_feed(object_digest, "row", object_count)
            catalog_feed(object_digest, "object_id", object_id)
            catalog_feed(object_digest, "kind", kind)
            catalog_feed(object_digest, "canonical_length", canonical_length)
            catalog_feed(object_digest, "blob_length", blob_length)
            object_count += 1
            canonical_length_sum += canonical_length
            blob_length_sum += blob_length
        object_catalog_sha256 = object_digest.hexdigest()

        head_rows = connection.execute(
            "SELECT id, generation, child, transition, validation_receipt "
            "FROM wp4m_visible_head ORDER BY id"
        ).fetchall()
        if len(head_rows) != 1 or head_rows[0][0] != 1:
            raise RuntimeError("logical catalog requires exactly one visible head row")
        _, generation, root_id, transition_id, receipt = head_rows[0]
        generation = fixed_blob(generation, 8, "head_generation")
        root_id = fixed_blob(root_id, 32, "head_root")
        transition_id = fixed_blob(transition_id, 32, "head_transition")
        receipt = fixed_blob(receipt, 216, "head_receipt")
        head_receipt_sha256 = sha256_bytes(receipt)
        head_sha256 = catalog_digest("layerfs-g5-v9-visible-head-v1", head_rows)

        logical_catalog_sha256 = catalog_digest(
            "layerfs-g5-v9-logical-catalog-v1",
            [
                (
                    sqlite_schema_sha256,
                    meta_sha256,
                    object_catalog_sha256,
                    head_sha256,
                    object_count,
                    canonical_length_sum,
                    blob_length_sum,
                    sqlite_page_size,
                    sqlite_page_count,
                    sqlite_freelist_count,
                    sqlite_logical_database_bytes,
                )
            ],
        )
        return {
            "schema": LOGICAL_CATALOG_SCHEMA,
            "hash_semantics": LOGICAL_CATALOG_HASH_SEMANTICS,
            "logical_catalog_sha256": logical_catalog_sha256,
            "query_only": query_only,
            "autocommit": autocommit,
            "sqlite_page_size": sqlite_page_size,
            "sqlite_page_count": sqlite_page_count,
            "sqlite_freelist_count": sqlite_freelist_count,
            "sqlite_logical_database_bytes": sqlite_logical_database_bytes,
            "sqlite_schema_sha256": sqlite_schema_sha256,
            "meta_row_count": len(meta_rows),
            "meta_sha256": meta_sha256,
            "object_count": object_count,
            "canonical_length_sum": canonical_length_sum,
            "blob_length_sum": blob_length_sum,
            "object_catalog_sha256": object_catalog_sha256,
            "head_row_count": len(head_rows),
            "head_generation": int.from_bytes(generation, "big"),
            "head_root_id": root_id.hex(),
            "head_transition_id": transition_id.hex(),
            "head_receipt_bytes": len(receipt),
            "head_receipt_sha256": head_receipt_sha256,
        }
    finally:
        connection.close()


def post_row_state(root, product, allowed_inventory):
    databases = sorted(
        path
        for path in pathlib.Path(root).rglob("*.sqlite")
        if not path.name.endswith(("-journal", "-wal", "-shm"))
    )
    if len(databases) != 1:
        raise RuntimeError(f"expected one post-row SQLite database, found {len(databases)}: {root}")
    database = databases[0]
    authority = pathlib.Path(f"{database}.authority")
    expectations = pathlib.Path(f"{database}.expectations")
    if not authority.is_file() or not expectations.is_file():
        raise RuntimeError(f"post-row custody sidecar missing: {database}")
    missing = [name for name in MUTATION_WORK_FIELDS if name not in product]
    if missing:
        raise RuntimeError(f"post-row exact mutation work unavailable: {missing}")
    work = {name: product[name] for name in MUTATION_WORK_FIELDS}
    work.update(root_id=product.get("root_id"), transition_id=product.get("transition_id"))
    if work["root_id"] is None or work["transition_id"] is None:
        raise RuntimeError("post-row root/transition identity unavailable")
    inventory = exact_inventory(root)
    allowed_types = {(entry["path"], entry["kind"]) for entry in allowed_inventory}
    actual_types = {(entry["path"], entry["kind"]) for entry in inventory}
    unexpected = sorted(actual_types - allowed_types)
    missing_inventory = sorted(allowed_types - actual_types)
    allowed_by_path = {entry["path"]: entry for entry in allowed_inventory}
    immutable_size_mismatches = sorted(
        entry["path"]
        for entry in inventory
        if entry["kind"] == "file"
        and not entry["path"].endswith(".sqlite")
        and allowed_by_path[entry["path"]]["bytes"] != entry["bytes"]
    )
    if unexpected or missing_inventory or immutable_size_mismatches:
        raise RuntimeError(
            "post-row exact inventory mismatch: "
            f"unexpected={unexpected} missing={missing_inventory} immutable_sizes={immutable_size_mismatches}"
        )
    catalog = logical_catalog(database)
    if (
        catalog["head_root_id"] != product.get("root_id")
        or catalog["head_transition_id"] != product.get("transition_id")
    ):
        raise RuntimeError("logical catalog head does not match product row")
    state = {
        "post_database_bytes": database.stat().st_size,
        "post_database_hash_semantics": LOGICAL_CATALOG_HASH_SEMANTICS,
        "logical_catalog": catalog,
        "post_authority_sha256": sha256(authority),
        "post_authority_bytes": authority.stat().st_size,
        "post_expectations_sha256": sha256(expectations),
        "post_expectations_bytes": expectations.stat().st_size,
        "mutation_work": work,
        "mutation_work_sha256": sha256_bytes(compact(work).encode()),
        "allowed_inventory": allowed_inventory,
        "post_inventory": inventory,
        "inventory_residue": [],
    }
    return state


def g4_command(executable, request):
    operation = request["operation"]
    common = [str(request["root"]), str(request["size_bytes"])]
    iteration = str(request["iteration"])
    if operation in ("same-middle", "plus1-early", "plus1-middle"):
        mapped = {
            "same-middle": "edit-same",
            "plus1-early": "edit-plus1-early",
            "plus1-middle": "edit-plus1-middle",
        }[operation]
        return [
            str(executable), "--fixed-radix-acceptance-row", *common, mapped, iteration,
            "false", request["validation"],
        ]
    mapped = {
        "first-edit-after-reopen": "first-edit-after-reopen",
        "one-byte-early": "edit-one-byte-early",
        "one-byte-middle": "edit-one-byte-middle",
        "one-byte-late": "edit-one-byte-late",
    }.get(operation)
    if mapped is None:
        raise RuntimeError(f"operation has no frozen G4 command: {operation}")
    return [
        str(executable), "--fast-row", *common, mapped, iteration, "false",
        request["validation"],
    ]


class PersistentChild:
    def __init__(
        self,
        executable,
        mode,
        size_bytes,
        operation,
        expected_rows,
        result,
        custody,
        forecast_ns,
        executable_sha256,
        label=None,
    ):
        self.mode = mode
        self.size_bytes = size_bytes
        self.operation = operation
        label = label or f"g5-{mode}-{size_bytes}-{operation}"
        self.stdout_path = result / f"children-v9/{label}.stdout"
        self.stderr_path = result / f"children-v9/{label}.stderr"
        self.time_path = result / f"time-v9/{label}.txt"
        self.stdout_path.parent.mkdir(parents=True, exist_ok=True)
        self.time_path.parent.mkdir(parents=True, exist_ok=True)
        self.stderr_handle = self.stderr_path.open("x", encoding="utf-8")
        command = [
            "/usr/bin/time", "-l", "-o", str(self.time_path), str(executable), CHILD_FLAG,
            "trusted" if mode == "trusted-local-dev" else "verified",
            str(size_bytes), operation, str(expected_rows), str(forecast_ns),
            str(LIMIT_NS["gate"]), executable_sha256, custody["database_sha256"],
            custody["authority_sha256"], custody["expectations_sha256"],
        ]
        self.process = subprocess.Popen(
            command,
            cwd=REPO,
            text=True,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=self.stderr_handle,
            bufsize=1,
        )
        self.command = command
        if self.process.stdout is None:
            raise RuntimeError("persistent child stdout unavailable")
        ready_line = self.process.stdout.readline()
        if not ready_line:
            raise RuntimeError(f"persistent child omitted READY: {label}")
        append_text(self.stdout_path, ready_line)
        ready = json.loads(ready_line)
        if (
            ready.get("schema") != CHILD_READY_SCHEMA
            or ready.get("status") != "READY"
            or ready.get("expected_rows") != expected_rows
            or ready.get("full_wrapper_forecast_ns") != forecast_ns
            or ready.get("full_wrapper_limit_ns") != LIMIT_NS["gate"]
            or ready.get("size_bytes") != size_bytes
            or ready.get("operation") != operation
            or ready.get("custody") != "runner-preverified-borrowed"
        ):
            raise RuntimeError(f"persistent child READY mismatch: {label}")

    def request(self, request):
        if self.process.stdin is None or self.process.stdout is None:
            raise RuntimeError("persistent child pipes unavailable")
        self.process.stdin.write("\t".join(str(request[name]) for name in REQUEST_FIELDS) + "\n")
        self.process.stdin.flush()
        line = self.process.stdout.readline()
        if not line:
            raise RuntimeError(f"persistent child ended before response: {self.mode}")
        append_text(self.stdout_path, line)
        return validate_child_row(json.loads(line))

    def close(self):
        if self.process.stdin is not None:
            self.process.stdin.close()
        terminal_line = self.process.stdout.readline() if self.process.stdout is not None else ""
        if terminal_line:
            append_text(self.stdout_path, terminal_line)
        remainder = self.process.stdout.read() if self.process.stdout is not None else ""
        if remainder:
            append_text(self.stdout_path, remainder)
        returncode = self.process.wait()
        self.stderr_handle.flush()
        os.fsync(self.stderr_handle.fileno())
        self.stderr_handle.close()
        fsync_file(self.time_path)
        fsync_dir(self.stderr_path.parent)
        fsync_dir(self.time_path.parent)
        if returncode != 0 or not terminal_line:
            raise RuntimeError(f"persistent child failed: {self.mode}: {returncode}")
        terminal = json.loads(terminal_line)
        if terminal.get("schema") != CHILD_TERMINAL_SCHEMA or terminal.get("status") != "PASS":
            raise RuntimeError(f"persistent child terminal mismatch: {self.mode}")
        terminal["external_time"] = parse_time(self.time_path)
        terminal["role"] = f"g5_{self.mode.replace('-', '_')}"
        terminal["size_bytes"] = self.size_bytes
        terminal["operation"] = self.operation
        return terminal


def run_oneshot(executable, request, result, label, g4=False, custody=None):
    stdout = result / f"children-v9/{label}.stdout"
    stderr = result / f"children-v9/{label}.stderr"
    sidecar = result / f"time-v9/{label}.txt"
    command = g4_command(executable, request) if g4 else [
        str(executable), ONESHOT_FLAG, request["mode"], request["root"],
        str(request["size_bytes"]), request["operation"], str(request["iteration"]),
        request["expectation_id"],
    ]
    environment_values = {}
    environment = None
    if g4:
        if custody is None:
            raise RuntimeError("frozen G4 row requires exact pre-dispatch custody")
        fixed = request["operation"] in ("same-middle", "plus1-early", "plus1-middle")
        environment_values = {
            "LAYERFS_FIXED_RADIX_ACCEPTANCE" if fixed else "LAYERFS_FAST_LANE": "1",
            "WP4M_EXECUTABLE_SHA256": G4_EXECUTABLE_SHA256,
            "WP4M_BASE_COPY_METHOD": (
                "fixed-radix-acceptance-master-copy"
                if fixed else "fast-lane-isolated-prepared-row"
            ),
            "WP4M_BASE_DATABASE_SHA256": custody["database_sha256"],
            "WP4M_BASE_AUTHORITY_SHA256": custody["authority_sha256"],
            "WP4M_BASE_EXPECTATIONS_SHA256": custody["expectations_sha256"],
        }
        environment = os.environ.copy()
        environment.update(environment_values)
    completed = subprocess.run(
        ["/usr/bin/time", "-l", "-o", str(sidecar), *command],
        cwd=REPO,
        text=True,
        capture_output=True,
        env=environment,
    )
    write_text(stdout, completed.stdout)
    write_text(stderr, completed.stderr)
    fsync_file(sidecar)
    fsync_dir(sidecar.parent)
    if completed.returncode != 0:
        raise RuntimeError(f"one-shot child failed: {label}: {completed.stderr.strip()}")
    lines = [json.loads(line) for line in completed.stdout.splitlines() if line]
    if len(lines) != 1:
        raise RuntimeError(f"one-shot child row count mismatch: {label}")
    row = normalize_g4_row(lines[0]) if g4 else validate_child_row(lines[0])
    row["external_time"] = parse_time(sidecar)
    row["command"] = command
    row["command_environment"] = environment_values
    return row


def expanded_observations(campaign):
    observations = []
    ordinal = 0
    for sequence in schedule_rows(campaign):
        if sequence["operation"] not in SUPPORTED_CHILD_OPERATIONS:
            continue
        comparison = sequence["comparison"]
        final_pair = int(sequence["pairs"])
        for pair in range(1, final_pair + 1):
            if comparison == "g4-verified-vs-g5-verified":
                roles = ["g4_verified", "g5_verified"]
            elif comparison == "g5-verified-vs-g5-trusted":
                roles = ["g5_verified", "g5_trusted"]
            elif comparison == "g4-g5-triple":
                roles = ["g4_verified", "g5_verified", "g5_trusted"]
            elif comparison == "same-g5":
                roles = ["g5_verified", "g5_trusted"]
            else:
                roles = ["g5_verified"]
            secondary_flip = (
                int(sequence["pairs"]) == 5
                and sequence["operation"] in SECONDARY_BA_OPERATIONS
            )
            if (pair % 2 == 0) != secondary_flip:
                roles.reverse()
            for role in roles:
                ordinal += 1
                fixed_checkpoint = (
                    campaign == "screen"
                    or (campaign == "gate" and pair in (1, final_pair))
                )
                observations.append(
                    {
                        **sequence,
                        "ordinal": ordinal,
                        "pair": pair,
                        "role": role,
                        "mode": "trusted-local-dev" if role == "g5_trusted" else "verified",
                        "iteration": 0,
                        "fixed_checkpoint": fixed_checkpoint,
                        "validation": (
                            "complete-roundtrip" if fixed_checkpoint else "capture-only"
                        ),
                        "validation_scope": (
                            "CompleteRoundTrip" if fixed_checkpoint else "CaptureOnly"
                        ),
                    }
                )
    if campaign == "gate" and len(observations) != 200:
        raise RuntimeError(f"gate arm observation mismatch: {len(observations)}")
    if campaign == "gate":
        if sum(row["fixed_checkpoint"] for row in observations) != 56:
            raise RuntimeError("gate fixed checkpoint count mismatch")
        checkpoint_cells = {}
        for row in observations:
            key = (row["comparison"], row["operation"], row["role"])
            checkpoint_cells.setdefault(key, []).append(row)
        if any(
            [row["pair"] for row in rows if row["fixed_checkpoint"]]
            != [1, int(rows[0]["pairs"])]
            for rows in checkpoint_cells.values()
        ):
            raise RuntimeError("gate fixed checkpoint position mismatch")
        for comparison in ("g4-verified-vs-g5-verified", "g5-verified-vs-g5-trusted"):
            secondary = [
                row for row in observations
                if row["comparison"] == comparison and int(row["pairs"]) == 5
            ]
            first_roles = [
                row["role"] for index, row in enumerate(secondary)
                if index == 0 or row["pair"] != secondary[index - 1]["pair"]
                or row["sequence_id"] != secondary[index - 1]["sequence_id"]
            ]
            control = "g4_verified" if comparison.startswith("g4-") else "g5_verified"
            candidate = "g5_verified" if comparison.startswith("g4-") else "g5_trusted"
            if first_roles.count(control) != 15 or first_roles.count(candidate) != 15:
                raise RuntimeError(f"secondary aggregate order imbalance: {comparison}")
    return observations


def analyze(result):
    outputs = []
    for analyzer, name in (
        (PRIMARY, "PRIMARY-ANALYSIS-v9.json"),
        (INDEPENDENT, "INDEPENDENT-RECOMPUTATION-v9.json"),
    ):
        output = result / name
        completed = subprocess.run(
            [
                sys.executable, str(analyzer), str(result / "RAW-v9.jsonl"),
                str(result / "TIMINGS-v9.tsv"), str(SCHEDULE), str(EXPECTED), str(output),
            ],
            cwd=REPO,
            text=True,
            capture_output=True,
        )
        if completed.returncode not in (0, 1) or not output.is_file():
            raise RuntimeError(f"analyzer failed abnormally: {analyzer}: {completed.stderr.strip()}")
        with output.open("rb") as handle:
            os.fsync(handle.fileno())
        outputs.append(json.loads(output.read_text(encoding="utf-8")))
    agreement = outputs[0].get("normalized") == outputs[1].get("normalized")
    write_json(
        result / "ANALYZER-AGREEMENT-v9.json",
        {
            "schema": "phase4-g5-1-analyzer-agreement-v9",
            "status": "PASS" if agreement else "REVISE",
            "exact_normalized_agreement": agreement,
        },
    )
    if not agreement or any(output.get("status") != "PASS" for output in outputs):
        raise RuntimeError("analysis disposition REVISE")


def ladder_prelock(campaign):
    paths = [
        DRY_RUN,
        DRY_RUN_INTENT,
        DRY_RUN_CALIBRATION_STDOUT,
        DRY_RUN_CALIBRATION_STDERR,
        DRY_RUN_CALIBRATION_TERMINAL,
        DRY_RUN_DISPOSITION,
    ]
    if campaign == "gate":
        paths.extend(
            (
                SCREEN_RESULT / "TERMINAL-VERIFICATION-v9.json",
                SCREEN_RESULT / "FINAL-ARTIFACT-HASHES-v9.tsv",
                SCREEN_RESULT / "FINAL-READONLY-VERIFICATION-v9.json",
                SCREEN_RESULT / "COMPLETE-WALL-v9.json",
                STATIC_CLOSURE,
            )
        )
    if any(not path.is_file() for path in paths):
        raise RuntimeError(f"{campaign} ladder evidence is incomplete")
    return {
        str(path): {"sha256": sha256(path), "mtime_ns": path.stat().st_mtime_ns}
        for path in paths
    }


def run_campaign(campaign):
    result = SCREEN_RESULT if campaign == "screen" else GATE_RESULT
    if result.exists():
        raise RuntimeError(f"result root exists: {result}")
    observations = expanded_observations(campaign)
    prelock_ladder = ladder_prelock(campaign)
    started, lock = acquire_lock()
    try:
        freeze = verify_freeze(require_static=campaign == "gate", require_dry=True)
        if ladder_prelock(campaign) != prelock_ladder:
            raise RuntimeError("ladder evidence changed across lock acquisition")
        dry = verify_dry_run(freeze)
        forecast_ns = dry["full_wrapper_forecast_ns"]
        result.mkdir(mode=0o700)
        work = result.parent / f"{result.name}-work-v9"
        work.mkdir(mode=0o700)
        for name in ("operands-v9", "children-v9", "time-v9"):
            (result / name).mkdir()
        fsync_dir(result)
        fsync_dir(result.parent)

        g4_copy = result / "operands-v9/frozen-g4-verified"
        g5_copy = result / "operands-v9/g5-verified-trusted"
        exclusive_operand_copy(G4_EXECUTABLE, g4_copy, G4_EXECUTABLE_SHA256)
        exclusive_operand_copy(G5_CHILD_BINARY, g5_copy, freeze["g5_executable_sha256"])
        write_json(
            result / "OPERAND-CUSTODY-v9.json",
            {
                "schema": "phase4-g5-1-operand-custody-v9",
                "g4_verified": {"path": str(g4_copy), "sha256": sha256(g4_copy), "mode": "0500"},
                "g5_verified_trusted": {"path": str(g5_copy), "sha256": sha256(g5_copy), "mode": "0500"},
                "same_g5_bytes": True,
            },
        )
        write_json(
            result / "PREFLIGHT-v9.json",
            {
                "schema": "phase4-g5-1-preflight-v9",
                "status": "PASS",
                "campaign": campaign,
                "branch": BRANCH,
                "checkpoint": CHECKPOINT,
                "source_freeze_sha256": sha256(SOURCE_FREEZE),
                "method_manifest_sha256": freeze["method_manifest_sha256"],
                "input_manifest_sha256": freeze["input_manifest_sha256"],
                "schedule_sha256": freeze["schedule_sha256"],
                "g4_executable_sha256": G4_EXECUTABLE_SHA256,
                "g5_executable_sha256": freeze["g5_executable_sha256"],
            },
        )
        write_json(
            result / "ENVIRONMENT-v9.json",
            {
                "schema": "phase4-g5-1-environment-v9",
                "platform": platform.platform(),
                "machine": platform.machine(),
                "python": platform.python_version(),
                "rustc": subprocess.check_output(["rustc", "--version"], text=True).strip(),
                "controlled_cold": "Unavailable",
                "physical_io_bytes": "Unavailable",
            },
        )
        shutil.copyfile(INPUT_MANIFEST, result / "INPUT-CUSTODY-v9.tsv")
        with (result / "INPUT-CUSTODY-v9.tsv").open("rb") as handle:
            os.fsync(handle.fileno())

        child_counts = {}
        for observation in observations:
            if observation["role"].startswith("g5_"):
                key = (observation["mode"], int(observation["size_bytes"]), observation["operation"])
                child_counts[key] = child_counts.get(key, 0) + 1
        masters = {
            master_path({"operation": key[2], "size_bytes": key[1]}) for key in child_counts
        }
        master_custody = {path: manifest_master_custody(path) for path in masters}
        persistent = {
            key: PersistentChild(
                g5_copy,
                *key,
                expected_rows,
                result,
                master_custody[master_path({"operation": key[2], "size_bytes": key[1]})],
                forecast_ns,
                freeze["g5_executable_sha256"],
            )
            for key, expected_rows in child_counts.items()
        }
        raw, timings, commands, semantics = [], [], [], []
        semantic_terminals = []
        try:
            for observation in observations:
                row_root = work / f"{observation['ordinal']:03d}-{observation['sequence_id']}-{observation['role']}"
                clone_receipt = clone_master_attested(master_path(observation), row_root)
                expected_custody = master_custody[master_path(observation)]
                pre_dispatch_custody = dict(expected_custody)
                allowed_inventory = exact_inventory(row_root)
                request = {
                    **observation,
                    "root": str(row_root),
                    "warmup": "false",
                    "validation": observation["validation"],
                }
                label = f"{observation['ordinal']:03d}-{observation['sequence_id']}-{observation['role']}"
                request["id"] = label
                if observation["role"] == "g4_verified":
                    value = run_oneshot(
                        g4_copy, request, result, label, g4=True, custody=pre_dispatch_custody
                    )
                else:
                    key = (observation["mode"], int(observation["size_bytes"]), observation["operation"])
                    value = persistent[key].request(request)
                state = post_row_state(row_root, value["product"], allowed_inventory)
                value.update(
                    wrapper={
                        "ordinal": observation["ordinal"],
                        "campaign": campaign,
                        "sequence_id": observation["sequence_id"],
                        "category": observation["category"],
                        "comparison": observation["comparison"],
                        "pair": observation["pair"],
                        "role": observation["role"],
                        "mode": observation["mode"],
                        "size_bytes": int(observation["size_bytes"]),
                        "operation": observation["operation"],
                        "expectation_id": observation["expectation_id"],
                        "clone_receipt": clone_receipt,
                        "pre_dispatch_custody": pre_dispatch_custody,
                        "validation_scope": observation["validation_scope"],
                        "fixed_checkpoint": observation["fixed_checkpoint"],
                        **state,
                    }
                )
                raw.append(value)
                timers = value["timers_ns"]
                timings.append(
                    {
                        "ordinal": observation["ordinal"],
                        "sequence_id": observation["sequence_id"],
                        "comparison": observation["comparison"],
                        "pair": observation["pair"],
                        "role": observation["role"],
                        "operation": observation["operation"],
                        **{name: timers[name] for name in TIMER_FIELDS},
                        "total_ns": value["total_ns"],
                        "decision_ns": value["decision_ns"],
                    }
                )
                commands.append({"ordinal": observation["ordinal"], "label": label, "role": observation["role"]})
                if observation["category"] in ("semantic", "fault", "sentinel"):
                    semantics.append(
                        {
                            "ordinal": observation["ordinal"],
                            "sequence_id": observation["sequence_id"],
                            "role": observation["role"],
                            "expectation_id": observation["expectation_id"],
                            "status": value.get("status"),
                            "error": value.get("error"),
                        }
                    )
                append_text(
                    result / "CHRONOLOGY-v9.jsonl",
                    compact(
                        {
                            "event": "operation-complete",
                            "ordinal": observation["ordinal"],
                            "sequence_id": observation["sequence_id"],
                            "role": observation["role"],
                            "monotonic_ns": time.monotonic_ns(),
                        }
                    )
                    + "\n",
                )
            if campaign == "screen":
                for case in (
                    "touched-corruption",
                    "unrelated-corruption",
                    "trusted-verified-reopen",
                    "reconciliation",
                ):
                    label = f"semantic-{case}"
                    values, terminal, command = run_semantic(
                        g5_copy, case, work / label, result, label
                    )
                    raw.extend(values)
                    semantic_terminals.append(terminal)
                    commands.append({"label": label, "role": "semantic", "command": command})
                    semantics.extend(
                        {
                            "ordinal": "native",
                            "sequence_id": case,
                            "role": value["integrity_mode"],
                            "expectation_id": "native-semantic-v9",
                            "status": value["status"],
                            "error": value["error"],
                        }
                        for value in values
                    )
                sentinel_records, sentinel_commands = run_s07(g4_copy, work, result)
                raw.extend(sentinel_records)
                commands.extend(sentinel_commands)
                semantics.extend(
                    {
                        "ordinal": "native",
                        "sequence_id": "S07",
                        "role": "frozen-g4-protected",
                        "expectation_id": "E_PROTECTED_SENTINEL",
                        "status": value["status"],
                        "error": value["product"].get("error"),
                    }
                    for value in sentinel_records
                )
        finally:
            terminals = [child.close() for child in persistent.values()]
        raw.extend(terminals)
        raw.extend(semantic_terminals)

        write_text(result / "RAW-v9.jsonl", "".join(compact(row) + "\n" for row in raw))
        timing_fields = ("ordinal", "sequence_id", "comparison", "pair", "role", "operation", *TIMER_FIELDS, "total_ns", "decision_ns")
        write_text(
            result / "TIMINGS-v9.tsv",
            "\t".join(timing_fields) + "\n"
            + "".join("\t".join(str(row[name]) for name in timing_fields) + "\n" for row in timings),
        )
        write_text(
            result / "SEMANTIC-FAULT-RESULTS-v9.tsv",
            "ordinal\tsequence_id\trole\texpectation_id\tstatus\terror\n"
            + "".join(
                f"{row['ordinal']}\t{row['sequence_id']}\t{row['role']}\t{row['expectation_id']}\t{row['status']}\t{row['error']}\n"
                for row in semantics
            ),
        )
        write_json(result / "COMMANDS-v9.json", {"schema": "phase4-g5-1-commands-v9", "commands": commands})
        analyze(result)

        if work.parent != result.parent or not work.name.endswith("-work-v9"):
            raise RuntimeError("refusing unsafe work cleanup")
        shutil.rmtree(work)
        fsync_dir(work.parent)
        residue = [str(path) for path in result.parent.glob(f"{result.name}-work-v9")]
        write_json(
            result / "CLEANUP-v9.json",
            {
                "schema": "phase4-g5-1-cleanup-v9",
                "status": "PASS" if not residue else "REVISE",
                "work_residue": residue,
                "lock_owned": verify_owned_lock(lock),
            },
        )
        if residue:
            raise RuntimeError("work residue")

        payload_excluded = {
            "PAYLOAD-MANIFEST-v9.tsv", "MEASURED-TERMINAL-v9.json",
            "TERMINAL-VERIFICATION-v9.json", "COMPLETE-WALL-v9.json",
            "BENCHMARK-LOCK-RELEASE-ATTESTATION-v9.json", "LOCK-RELEASE-v9.json",
            "FINAL-ARTIFACT-HASHES-v9.tsv", "FINAL-READONLY-VERIFICATION-v9.json",
        }
        payload = result / "PAYLOAD-MANIFEST-v9.tsv"
        write_text(payload, manifest_text(result, excluded=payload_excluded))
        payload_count = verify_manifest(result, payload, "result_relative_path")
        terminal = result / "MEASURED-TERMINAL-v9.json"
        write_json(
            terminal,
            {
                "schema": "phase4-g5-1-measured-terminal-v9",
                "status": "PASS",
                "campaign": campaign,
                "rows": len(timings),
                "payload_files": payload_count,
                "payload_manifest_sha256": sha256(payload),
                "elapsed_before_terminal_verification_ns": time.monotonic_ns() - started,
            },
        )
        verification = result / "TERMINAL-VERIFICATION-v9.json"
        write_json(
            verification,
            {
                "schema": "phase4-g5-1-terminal-verification-v9",
                "status": "PASS",
                "terminal_sha256": sha256(terminal),
                "payload_manifest_sha256": sha256(payload),
                "payload_files_verified": verify_manifest(result, payload, "result_relative_path"),
                "source_freeze_sha256": sha256(SOURCE_FREEZE),
                "g4_executable_sha256": sha256(g4_copy),
                "g5_executable_sha256": sha256(g5_copy),
                "lock_owned_through_terminal_verification": verify_owned_lock(lock),
            },
        )
        release = release_lock(lock, result, verification)
        if not release or release["status"] != "PASS":
            raise RuntimeError("lock release failed")
        final = result / "FINAL-ARTIFACT-HASHES-v9.tsv"
        write_text(
            final,
            manifest_text(
                result,
                excluded={final.name, "FINAL-READONLY-VERIFICATION-v9.json", "COMPLETE-WALL-v9.json"},
            ),
        )
        final_count = verify_manifest(result, final, "result_relative_path")
        write_json(
            result / "FINAL-READONLY-VERIFICATION-v9.json",
            {
                "schema": "phase4-g5-1-final-readonly-verification-v9",
                "status": "PASS",
                "files_verified": final_count,
                "final_artifact_hashes_sha256": sha256(final),
                "lock_absent": not LOCK.exists(),
                "result_directory_fsynced": True,
                "complete_wall_terminal_follows": True,
            },
        )
        fsync_dir(result)
        complete_ns = time.monotonic_ns() - started
        if complete_ns > LIMIT_NS[campaign]:
            raise RuntimeError(f"complete wall exceeded {campaign} limit")
        write_json(
            result / "COMPLETE-WALL-v9.json",
            {
                "schema": "phase4-g5-1-complete-wall-v9",
                "status": "PASS",
                "campaign": campaign,
                "complete_wall_ns": complete_ns,
                "limit_ns": LIMIT_NS[campaign],
                "from": "fail-fast global lock acquisition",
                "through": "final manifest and read-only verification fsync",
                "terminal_self_exclusion": "COMPLETE-WALL-v9.json follows the verified final manifest",
            },
        )
        fsync_dir(result)
        print(compact({"status": "PASS", "campaign": campaign, "result": str(result), "complete_wall_ns": complete_ns}))
        return 0
    except Exception as error:
        if result.exists() and not (result / "FAILED-v9.json").exists():
            write_json(
                result / "FAILED-v9.json",
                {
                    "schema": "phase4-g5-1-failure-v9",
                    "status": "REVISE",
                    "error": str(error),
                    "elapsed_ns": time.monotonic_ns() - started,
                },
            )
        raise
    finally:
        if lock.get("fd") is not None:
            failure_root = result if result.exists() else REPO / "target"
            try:
                release_lock(lock, failure_root, state="failure")
            except Exception:
                pass


def main():
    if len(sys.argv) != 2 or sys.argv[1] not in ("--prepare-inputs", "--dry-run", "--screen", "--gate"):
        raise SystemExit("usage: runner.py --prepare-inputs|--dry-run|--screen|--gate")
    if sys.argv[1] == "--prepare-inputs":
        return prepare_inputs()
    if sys.argv[1] == "--dry-run":
        return dry_run()
    return run_campaign(sys.argv[1][2:])


if __name__ == "__main__":
    raise SystemExit(main())
