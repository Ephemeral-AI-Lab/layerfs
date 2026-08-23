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
import stat
import subprocess
import sys
import time


HERE = pathlib.Path(__file__).resolve().parent
REPO = HERE.parents[4]
METHOD = HERE / "method"
SCHEDULE = METHOD / "SCHEDULE-v6.tsv"
EXPECTED = METHOD / "EXPECTED-OUTCOMES-v6.tsv"
INPUT_MANIFEST = METHOD / "INPUT-MANIFEST-v6.tsv"
METHOD_MANIFEST = METHOD / "METHOD-MANIFEST-v6.tsv"
SOURCE_FREEZE = METHOD / "SOURCE-FREEZE-v6.json"
LIMITATIONS = HERE / "LIMITATIONS-v6.json"
STATIC_CLOSURE = HERE / "STATIC-CLOSURE-v6.json"
DRY_RUN = HERE / "DRY-RUN-v6.json"
PRIMARY = HERE / "analyzers/primary.py"
INDEPENDENT = HERE / "analyzers/independent.py"

CHECKPOINT = "d58c5a1307253dfc221fe50de996c183deb9458a"
BRANCH = "codex/empty-worktree"
DATE = "20260823"
LOCK = REPO / "target/BENCHMARK_LOCK"
INPUT_ROOT = REPO / f"target/phase4-g5-trusted-reopen-edit-inputs-{DATE}-v6"
SCREEN_RESULT = REPO / f"target/phase4-g5-trusted-reopen-edit-{DATE}-v6-screen"
GATE_RESULT = REPO / f"target/phase4-g5-trusted-reopen-edit-{DATE}-v6"

# Frozen v6 Rust transport.
G5_CHILD_BINARY = HERE / "g5-benchmark/target/release/layerfs-g5-trusted-child-v6"
FIXTURE_FLAG = "--g5-fixture"
PREPARE_FLAG = "--g5-prepare"
CHILD_FLAG = "--g5-child"
SEMANTIC_FLAG = "--g5-semantic"
CHILD_READY_SCHEMA = "phase4-g5-trusted-child-ready-v6"
CHILD_ENVELOPE_SCHEMA = "phase4-g5-trusted-child-row-v6"
CHILD_TERMINAL_SCHEMA = "phase4-g5-trusted-child-terminal-v6"
FIXTURE_SCHEMA = "phase4-g5-trusted-fixture-v6"
PREPARE_SCHEMA = "phase4-g5-trusted-prepare-v6"
SEMANTIC_SCHEMA = "phase4-g5-trusted-semantic-v6"
SEMANTIC_TERMINAL_SCHEMA = "phase4-g5-trusted-semantic-terminal-v6"
SENTINEL_SCHEMA = "phase4-g5-1-protected-sentinel-v6"
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
    "operand_and_isolated_base_preparation": 20_000_000_000,
    "lock_custody_and_preflight": 5_000_000_000,
    "analyzers_cleanup_manifests_and_terminal": 20_000_000_000,
}
CALIBRATION_SIZE = 104_857_600
HASH_CALIBRATION_DIVISOR = 2
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


def method_source_names():
    fixed = {
        str(path.relative_to(REPO))
        for path in (
            HERE / "PREREGISTRATION-v6.md",
            HERE / "REVIEW-SYNTHESIS-v6.md",
            HERE / "SAMPLE-COUNT-INTERPRETATION-ADDENDUM-v6.md",
            HERE / "V5-SUPERSESSION-v6.json",
            LIMITATIONS,
            HERE / "runner.py",
            PRIMARY,
            INDEPENDENT,
            SCHEDULE,
            EXPECTED,
            INPUT_MANIFEST,
            G4_EXECUTABLE,
            G4_FINAL_MANIFEST,
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
        "schema": "phase4-g5-1-source-freeze-v6",
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
        "forecast_model": "dry-run-calibrated-v6-python-and-external-shasum",
        "base_forecast_components_ns": BASE_FORECAST_COMPONENTS_NS,
        "full_wrapper_limit_ns": LIMIT_NS["gate"],
        "frozen_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
    }
    write_json(SOURCE_FREEZE, freeze)


def seal_input_tree():
    for path in sorted(INPUT_ROOT.rglob("*"), reverse=True):
        path.chmod(0o555 if path.is_dir() else 0o444)
    INPUT_ROOT.chmod(0o555)
    fsync_dir(INPUT_ROOT)
    fsync_dir(INPUT_ROOT.parent)


def prepare_inputs():
    verify_repository_identity()
    schedule_rows()
    if LOCK.exists() or SCREEN_RESULT.exists() or GATE_RESULT.exists() or INPUT_ROOT.exists():
        raise RuntimeError("prepare-inputs requires absent lock, inputs, and result roots")
    if any(path.exists() for path in (INPUT_MANIFEST, METHOD_MANIFEST, SOURCE_FREEZE, DRY_RUN, STATIC_CLOSURE)):
        raise RuntimeError("v6 method/freeze evidence already exists")
    if not G5_CHILD_BINARY.is_file() or not os.access(G5_CHILD_BINARY, os.X_OK):
        raise RuntimeError(f"pending Rust child interface: {G5_CHILD_BINARY}")
    INPUT_ROOT.mkdir(mode=0o700)
    fsync_dir(INPUT_ROOT.parent)
    records = []
    fixture_roots = {}
    for size in (1_048_576, 10_485_760, 104_857_600):
        root = INPUT_ROOT / "fixtures" / str(size)
        root.mkdir(parents=True)
        command = [str(G5_CHILD_BINARY), FIXTURE_FLAG, str(root), str(size)]
        completed = subprocess.run(command, cwd=REPO, text=True, capture_output=True)
        lines = [json.loads(line) for line in completed.stdout.splitlines() if line]
        if (
            completed.returncode != 0
            or len(lines) != 1
            or lines[0].get("schema") != FIXTURE_SCHEMA
            or lines[0].get("status") != "PASS"
            or lines[0].get("size_bytes") != size
            or lines[0].get("q_current") != 0
        ):
            raise RuntimeError(f"fixture preparation failed: {size}: {completed.stderr.strip()}")
        records.append({"kind": "fixture", "command": command, "stdout": lines[0], "stderr": completed.stderr})
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
        clone_master(fixture_roots[size], root)
        command = [str(G5_CHILD_BINARY), PREPARE_FLAG, str(root), str(size), operation, "0"]
        completed = subprocess.run(command, cwd=REPO, text=True, capture_output=True)
        lines = [json.loads(line) for line in completed.stdout.splitlines() if line]
        if (
            completed.returncode != 0
            or len(lines) != 1
            or lines[0].get("schema") != PREPARE_SCHEMA
            or lines[0].get("status") != "PASS"
            or lines[0].get("size_bytes") != size
            or lines[0].get("operation") != operation
            or lines[0].get("iteration") != 0
            or lines[0].get("q_current") != 0
        ):
            raise RuntimeError(f"row preparation failed: {size}/{operation}: {completed.stderr.strip()}")
        records.append({"kind": "prepared-row", "command": command, "stdout": lines[0], "stderr": completed.stderr})
    write_json(
        INPUT_ROOT / "PREPARATION-CUSTODY-v6.json",
        {
            "schema": "phase4-g5-1-input-preparation-v6",
            "status": "PASS",
            "executable_sha256": sha256(G5_CHILD_BINARY),
            "fixture_sizes": sorted(fixture_roots),
            "prepared_masters": [{"size_bytes": size, "operation": operation} for size, operation in masters],
            "records": records,
        },
    )
    write_input_and_method_manifests()
    seal_input_tree()
    print(compact({"status": "PASS", "input_root": str(INPUT_ROOT), "source_freeze": str(SOURCE_FREEZE)}))
    return 0


def verify_dry_run(freeze):
    value = json.loads(DRY_RUN.read_text(encoding="utf-8"))
    required = {
        "schema": "phase4-g5-1-dry-run-v6",
        "status": "PASS",
        "measured_rows": 0,
        "benchmark_child_processes_started": 0,
        "stores_opened": 0,
        "base_copies_created": 0,
        "measurement_timers_started": 0,
        "gate_arm_observations": GATE_ARM_OBSERVATIONS,
        "source_freeze_sha256": sha256(SOURCE_FREEZE),
        "method_manifest_sha256": freeze["method_manifest_sha256"],
        "input_manifest_sha256": freeze["input_manifest_sha256"],
        "full_wrapper_limit_ns": LIMIT_NS["gate"],
        "full_wrapper_forecast_status": "PASS",
    }
    if any(value.get(key) != expected for key, expected in required.items()):
        raise RuntimeError("dry-run custody/zero-row mismatch")
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
    global VERIFIED_INPUT_CUSTODY
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
    verify_manifest(INPUT_ROOT, INPUT_MANIFEST, "input_relative_path")
    VERIFIED_INPUT_CUSTODY = input_manifest_index()
    if require_dry:
        verify_dry_run(freeze)
    if require_static:
        static = json.loads(STATIC_CLOSURE.read_text(encoding="utf-8"))
        screen_terminal = SCREEN_RESULT / "TERMINAL-VERIFICATION-v6.json"
        screen_final_manifest = SCREEN_RESULT / "FINAL-ARTIFACT-HASHES-v6.tsv"
        screen_final_verification = SCREEN_RESULT / "FINAL-READONLY-VERIFICATION-v6.json"
        screen_complete_wall = SCREEN_RESULT / "COMPLETE-WALL-v6.json"
        required = {
            "schema": "phase4-g5-1-static-closure-v6",
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
    direct_freeze_bytes = METHOD_MANIFEST.stat().st_size + INPUT_MANIFEST.stat().st_size + G5_CHILD_BINARY.stat().st_size

    def row_custody_bytes(master):
        prefix = f"{pathlib.Path(master).relative_to(INPUT_ROOT)}/"
        selected = [
            item["bytes"] for name, item in VERIFIED_INPUT_CUSTODY.items()
            if name.startswith(prefix)
            and (name.endswith(".sqlite") or name.endswith(".sqlite.authority") or name.endswith(".sqlite.expectations"))
        ]
        if len(selected) != 3:
            raise RuntimeError(f"gate hash custody shape mismatch: {master}: {len(selected)}")
        return sum(selected)

    observations = expanded_observations("gate")
    post_row_bytes = sum(row_custody_bytes(master_path(row)) for row in observations)
    prepared_bytes = sum(row_custody_bytes(path) for path in {master_path(row) for row in observations})
    operand_recheck_bytes = 3 * (G4_EXECUTABLE.stat().st_size + G5_CHILD_BINARY.stat().st_size)
    components = {
        "repository_identity": repository_identity_bytes,
        "explicit_method_sources_twice": explicit_bytes * 2,
        "direct_freeze_files": direct_freeze_bytes,
        "input_manifest_preverification": input_bytes,
        "unique_prepared_custody": prepared_bytes,
        "two_hundred_post_row_custody": post_row_bytes,
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
        text=True,
        capture_output=True,
    )
    external_ns = max(1, time.monotonic_ns() - external_started)
    external_parts = completed.stdout.split()
    external_digest = external_parts[0] if len(external_parts) == 2 else None
    if completed.returncode != 0 or python_digest != expected["sha256"] or external_digest != python_digest:
        raise RuntimeError("dry-run Python/external SHA-256 calibration mismatch")
    python_bps = CALIBRATION_SIZE * 1_000_000_000 // python_ns
    external_bps = CALIBRATION_SIZE * 1_000_000_000 // external_ns
    floor_bps = min(python_bps, external_bps) // HASH_CALIBRATION_DIVISOR
    if floor_bps <= 0 or floor_bps * HASH_CALIBRATION_DIVISOR > min(python_bps, external_bps):
        raise RuntimeError("invalid conservative SHA-256 throughput floor")
    return {
        "schema": "phase4-g5-1-hash-calibration-v6",
        "classification": "zero-row-nonbenchmark-read-only-hash-calibration",
        "source": str(source.relative_to(INPUT_ROOT)),
        "bytes_per_pass": CALIBRATION_SIZE,
        "python": {"elapsed_ns": python_ns, "bytes_per_second": python_bps, "sha256": python_digest},
        "external_shasum": {"elapsed_ns": external_ns, "bytes_per_second": external_bps, "sha256": external_digest},
        "conservative_floor_bytes_per_second": floor_bps,
        "floor_divisor": HASH_CALIBRATION_DIVISOR,
    }


def dry_run():
    freeze = verify_freeze()
    if LOCK.exists() or SCREEN_RESULT.exists() or GATE_RESULT.exists():
        raise RuntimeError("dry-run requires absent lock and result roots")
    rows = schedule_rows()
    calibration = hash_calibration()
    hash_components, expected_hash_bytes = gate_hash_bytes()
    floor = calibration["conservative_floor_bytes_per_second"]
    hash_forecast_ns = (expected_hash_bytes * 1_000_000_000 + floor - 1) // floor
    forecast_components = {
        **BASE_FORECAST_COMPONENTS_NS,
        "exact_gate_hash_bytes_at_calibrated_floor": hash_forecast_ns,
    }
    full_wrapper_forecast_ns = sum(forecast_components.values())
    if full_wrapper_forecast_ns > LIMIT_NS["gate"]:
        raise RuntimeError("v6 PREMEASUREMENT_REVISE: conservative full-wrapper forecast exceeds 120 seconds")
    generated_residue = sorted(
        str(path.relative_to(REPO)) for path in HERE.rglob("__pycache__") if path.is_dir()
    )
    value = {
        "schema": "phase4-g5-1-dry-run-v6",
        "status": "PASS",
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
        "gate_arm_observations": GATE_ARM_OBSERVATIONS,
        "sample_count_interpretation": "deliberately-stricter-v6-choice-not-unambiguous-user-minimum",
        "hash_calibration": calibration,
        "expected_gate_hash_components_bytes": hash_components,
        "expected_gate_hash_bytes": expected_hash_bytes,
        "full_wrapper_forecast_components_ns": forecast_components,
        "full_wrapper_forecast_ns": full_wrapper_forecast_ns,
        "full_wrapper_limit_ns": LIMIT_NS["gate"],
        "full_wrapper_forecast_status": "PASS",
        "generated_non_authoritative_residue": generated_residue,
        "generated_residue_policy": "__pycache__ is generated non-authoritative residue; preserve rather than delete history",
        "source_freeze_sha256": sha256(SOURCE_FREEZE),
        "method_manifest_sha256": freeze["method_manifest_sha256"],
        "input_manifest_sha256": freeze["input_manifest_sha256"],
    }
    write_json(DRY_RUN, value)
    print(compact(value))
    return 0


def acquire_lock():
    started = time.monotonic_ns()
    descriptor = os.open(LOCK, os.O_RDWR | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
    token = os.urandom(32).hex()
    content = (
        compact(
            {
                "schema": "phase4-g5-1-lock-v6",
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
        attestation = result / "BENCHMARK-LOCK-RELEASE-ATTESTATION-v6.json"
        if attestation.exists():
            raise RuntimeError("lock release attestation exists")
        payload = (
            compact(
                {
                    "schema": "phase4-g5-1-lock-v6",
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
            "schema": "phase4-g5-1-lock-release-v6",
            "status": "PASS" if state == "release" else "REVISE",
            "state": state,
            "device": lock["device"],
            "inode": lock["inode"],
            "token_sha256": sha256_bytes(lock["token"].encode()),
            "attestation_sha256": sha256(attestation),
            "terminal_verification_sha256": sha256(terminal_verification) if terminal_verification else None,
            "lock_absent": True,
        }
        write_json(result / "LOCK-RELEASE-v6.json", value)
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


def clone_master(master, destination):
    if destination.exists():
        raise RuntimeError(f"isolated destination exists: {destination}")
    destination.mkdir(parents=True)
    custody = []
    for source in sorted(path for path in master.rglob("*") if path.is_file()):
        relative = source.relative_to(master)
        copied = destination / relative
        copied.parent.mkdir(parents=True, exist_ok=True)
        expected = None
        if VERIFIED_INPUT_CUSTODY is not None and source.is_relative_to(INPUT_ROOT):
            expected = VERIFIED_INPUT_CUSTODY.get(str(source.relative_to(INPUT_ROOT)))
            if expected is None or source.stat().st_size != expected["bytes"]:
                raise RuntimeError(f"frozen input manifest lookup mismatch: {source}")
        clonefile(source, copied)
        fsync_file(copied)
        expected_size = expected["bytes"] if expected else source.stat().st_size
        if source.stat().st_ino == copied.stat().st_ino or copied.stat().st_size != expected_size:
            raise RuntimeError(f"isolated clone custody mismatch: {copied}")
        item = {"path": str(relative), "bytes": expected_size, "clonefile": "success", "distinct_inode": True}
        if expected:
            item["manifest_sha256"] = expected["sha256"]
        custody.append(item)
    for directory in sorted((path for path in destination.rglob("*") if path.is_dir()), reverse=True):
        fsync_dir(directory)
    fsync_dir(destination)
    fsync_dir(destination.parent)
    return custody


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
    stdout = result / f"children-v6/{label}.stdout"
    stderr = result / f"children-v6/{label}.stderr"
    sidecar = result / f"time-v6/{label}.txt"
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
    terminal["external_time"] = parse_time(sidecar)
    terminal["role"] = f"semantic_{case.replace('-', '_')}"
    return values, terminal, command


def timed_external_command(command, result, label, env=None):
    stdout = result / f"children-v6/{label}.stdout"
    stderr = result / f"children-v6/{label}.stderr"
    sidecar = result / f"time-v6/{label}.txt"
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
    if (
        len(frozen_sources) != 1
        or len(probe_sources) != 1
        or sha256(frozen_sources[0]) != S07_FIXTURE_SHA256
        or sha256(probe_sources[0]) != S07_FIXTURE_SHA256
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
        base_custody = clone_master(frozen_fixture, root)
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
        state = post_row_state(root, product, allowed_inventory)
        if state["post_authority_sha256"] != custody["authority_sha256"]:
            raise RuntimeError(f"S07 {route} authority changed")
        if state["post_expectations_sha256"] != custody["expectations_sha256"]:
            raise RuntimeError(f"S07 {route} expectations changed")
        if route == "range" and state["post_database_sha256"] != custody["database_sha256"]:
            raise RuntimeError("S07 range changed database bytes")
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
                "frozen_fixture_sha256": sha256(frozen_sources[0]),
                "probe_fixture_sha256": sha256(probe_sources[0]),
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
        "q_high_water",
        "q_current",
        "q_report_output_bytes",
        "max_single_buffer_bytes",
        "buffer_evidence_complete",
        "full_file_buffer_bytes",
        *COMMON_PARITY_FIELDS,
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
        raise RuntimeError("child Q/buffer evidence mismatch")
    return {
        "schema": "phase4-g5-1-operation-v6",
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
        "schema": "phase4-g5-1-operation-v6",
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
    if unexpected or missing_inventory:
        raise RuntimeError(
            f"post-row exact inventory mismatch: unexpected={unexpected} missing={missing_inventory}"
        )
    return {
        "post_database_sha256": sha256(database),
        "post_database_bytes": database.stat().st_size,
        "post_database_hash_semantics": "physical-byte-parity-only-not-logical-digest",
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
        return [str(executable), "--fixed-radix-acceptance-row", *common, mapped, iteration, "false", "capture-only"]
    mapped = {
        "first-edit-after-reopen": "first-edit-after-reopen",
        "one-byte-early": "edit-one-byte-early",
        "one-byte-middle": "edit-one-byte-middle",
        "one-byte-late": "edit-one-byte-late",
    }.get(operation)
    if mapped is None:
        raise RuntimeError(f"operation has no frozen G4 command: {operation}")
    return [str(executable), "--fast-row", *common, mapped, iteration, "false", "capture-only"]


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
        self.stdout_path = result / f"children-v6/{label}.stdout"
        self.stderr_path = result / f"children-v6/{label}.stderr"
        self.time_path = result / f"time-v6/{label}.txt"
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


def run_oneshot(executable, request, result, label, g4=False):
    stdout = result / f"children-v6/{label}.stdout"
    stderr = result / f"children-v6/{label}.stderr"
    sidecar = result / f"time-v6/{label}.txt"
    command = g4_command(executable, request) if g4 else [
        str(executable), ONESHOT_FLAG, request["mode"], request["root"],
        str(request["size_bytes"]), request["operation"], str(request["iteration"]),
        request["expectation_id"],
    ]
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
        raise RuntimeError(f"one-shot child failed: {label}: {completed.stderr.strip()}")
    lines = [json.loads(line) for line in completed.stdout.splitlines() if line]
    if len(lines) != 1:
        raise RuntimeError(f"one-shot child row count mismatch: {label}")
    row = normalize_g4_row(lines[0]) if g4 else validate_child_row(lines[0])
    row["external_time"] = parse_time(sidecar)
    row["command"] = command
    return row


def expanded_observations(campaign):
    observations = []
    ordinal = 0
    for sequence in schedule_rows(campaign):
        if sequence["operation"] not in SUPPORTED_CHILD_OPERATIONS:
            continue
        comparison = sequence["comparison"]
        for pair in range(1, int(sequence["pairs"]) + 1):
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
                observations.append(
                    {
                        **sequence,
                        "ordinal": ordinal,
                        "pair": pair,
                        "role": role,
                        "mode": "trusted-local-dev" if role == "g5_trusted" else "verified",
                        "iteration": 0,
                    }
                )
    if campaign == "gate" and len(observations) != 200:
        raise RuntimeError(f"gate arm observation mismatch: {len(observations)}")
    if campaign == "gate":
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
        (PRIMARY, "PRIMARY-ANALYSIS-v6.json"),
        (INDEPENDENT, "INDEPENDENT-RECOMPUTATION-v6.json"),
    ):
        output = result / name
        completed = subprocess.run(
            [
                sys.executable, str(analyzer), str(result / "RAW-v6.jsonl"),
                str(result / "TIMINGS-v6.tsv"), str(SCHEDULE), str(EXPECTED), str(output),
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
        result / "ANALYZER-AGREEMENT-v6.json",
        {
            "schema": "phase4-g5-1-analyzer-agreement-v6",
            "status": "PASS" if agreement else "REVISE",
            "exact_normalized_agreement": agreement,
        },
    )
    if not agreement or any(output.get("status") != "PASS" for output in outputs):
        raise RuntimeError("analysis disposition REVISE")


def ladder_prelock(campaign):
    paths = [DRY_RUN]
    if campaign == "gate":
        paths.extend(
            (
                SCREEN_RESULT / "TERMINAL-VERIFICATION-v6.json",
                SCREEN_RESULT / "FINAL-ARTIFACT-HASHES-v6.tsv",
                SCREEN_RESULT / "FINAL-READONLY-VERIFICATION-v6.json",
                SCREEN_RESULT / "COMPLETE-WALL-v6.json",
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
        work = result.parent / f"{result.name}-work-v6"
        work.mkdir(mode=0o700)
        for name in ("operands-v6", "children-v6", "time-v6"):
            (result / name).mkdir()
        fsync_dir(result)
        fsync_dir(result.parent)

        g4_copy = result / "operands-v6/frozen-g4-verified"
        g5_copy = result / "operands-v6/g5-verified-trusted"
        exclusive_operand_copy(G4_EXECUTABLE, g4_copy, G4_EXECUTABLE_SHA256)
        exclusive_operand_copy(G5_CHILD_BINARY, g5_copy, freeze["g5_executable_sha256"])
        write_json(
            result / "OPERAND-CUSTODY-v6.json",
            {
                "schema": "phase4-g5-1-operand-custody-v6",
                "g4_verified": {"path": str(g4_copy), "sha256": sha256(g4_copy), "mode": "0500"},
                "g5_verified_trusted": {"path": str(g5_copy), "sha256": sha256(g5_copy), "mode": "0500"},
                "same_g5_bytes": True,
            },
        )
        write_json(
            result / "PREFLIGHT-v6.json",
            {
                "schema": "phase4-g5-1-preflight-v6",
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
            result / "ENVIRONMENT-v6.json",
            {
                "schema": "phase4-g5-1-environment-v6",
                "platform": platform.platform(),
                "machine": platform.machine(),
                "python": platform.python_version(),
                "rustc": subprocess.check_output(["rustc", "--version"], text=True).strip(),
                "controlled_cold": "Unavailable",
                "physical_io_bytes": "Unavailable",
            },
        )
        shutil.copyfile(INPUT_MANIFEST, result / "INPUT-CUSTODY-v6.tsv")
        with (result / "INPUT-CUSTODY-v6.tsv").open("rb") as handle:
            os.fsync(handle.fileno())

        child_counts = {}
        for observation in observations:
            if observation["role"].startswith("g5_"):
                key = (observation["mode"], int(observation["size_bytes"]), observation["operation"])
                child_counts[key] = child_counts.get(key, 0) + 1
        masters = {
            master_path({"operation": key[2], "size_bytes": key[1]}) for key in child_counts
        }
        master_custody = {path: prepared_master_custody(path) for path in masters}
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
                custody = clone_master(master_path(observation), row_root)
                allowed_inventory = exact_inventory(row_root)
                request = {
                    **observation,
                    "root": str(row_root),
                    "warmup": "false",
                    "validation": "capture-only",
                }
                label = f"{observation['ordinal']:03d}-{observation['sequence_id']}-{observation['role']}"
                request["id"] = label
                if observation["role"] == "g4_verified":
                    value = run_oneshot(g4_copy, request, result, label, g4=True)
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
                        "base_custody": custody,
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
                    result / "CHRONOLOGY-v6.jsonl",
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
                            "expectation_id": "native-semantic-v6",
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

        write_text(result / "RAW-v6.jsonl", "".join(compact(row) + "\n" for row in raw))
        timing_fields = ("ordinal", "sequence_id", "comparison", "pair", "role", "operation", *TIMER_FIELDS, "total_ns", "decision_ns")
        write_text(
            result / "TIMINGS-v6.tsv",
            "\t".join(timing_fields) + "\n"
            + "".join("\t".join(str(row[name]) for name in timing_fields) + "\n" for row in timings),
        )
        write_text(
            result / "SEMANTIC-FAULT-RESULTS-v6.tsv",
            "ordinal\tsequence_id\trole\texpectation_id\tstatus\terror\n"
            + "".join(
                f"{row['ordinal']}\t{row['sequence_id']}\t{row['role']}\t{row['expectation_id']}\t{row['status']}\t{row['error']}\n"
                for row in semantics
            ),
        )
        write_json(result / "COMMANDS-v6.json", {"schema": "phase4-g5-1-commands-v6", "commands": commands})
        analyze(result)

        if work.parent != result.parent or not work.name.endswith("-work-v6"):
            raise RuntimeError("refusing unsafe work cleanup")
        shutil.rmtree(work)
        fsync_dir(work.parent)
        residue = [str(path) for path in result.parent.glob(f"{result.name}-work-v6")]
        write_json(
            result / "CLEANUP-v6.json",
            {
                "schema": "phase4-g5-1-cleanup-v6",
                "status": "PASS" if not residue else "REVISE",
                "work_residue": residue,
                "lock_owned": verify_owned_lock(lock),
            },
        )
        if residue:
            raise RuntimeError("work residue")

        payload_excluded = {
            "PAYLOAD-MANIFEST-v6.tsv", "MEASURED-TERMINAL-v6.json",
            "TERMINAL-VERIFICATION-v6.json", "COMPLETE-WALL-v6.json",
            "BENCHMARK-LOCK-RELEASE-ATTESTATION-v6.json", "LOCK-RELEASE-v6.json",
            "FINAL-ARTIFACT-HASHES-v6.tsv", "FINAL-READONLY-VERIFICATION-v6.json",
        }
        payload = result / "PAYLOAD-MANIFEST-v6.tsv"
        write_text(payload, manifest_text(result, excluded=payload_excluded))
        payload_count = verify_manifest(result, payload, "result_relative_path")
        terminal = result / "MEASURED-TERMINAL-v6.json"
        write_json(
            terminal,
            {
                "schema": "phase4-g5-1-measured-terminal-v6",
                "status": "PASS",
                "campaign": campaign,
                "rows": len(timings),
                "payload_files": payload_count,
                "payload_manifest_sha256": sha256(payload),
                "elapsed_before_terminal_verification_ns": time.monotonic_ns() - started,
            },
        )
        verification = result / "TERMINAL-VERIFICATION-v6.json"
        write_json(
            verification,
            {
                "schema": "phase4-g5-1-terminal-verification-v6",
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
        final = result / "FINAL-ARTIFACT-HASHES-v6.tsv"
        write_text(
            final,
            manifest_text(
                result,
                excluded={final.name, "FINAL-READONLY-VERIFICATION-v6.json", "COMPLETE-WALL-v6.json"},
            ),
        )
        final_count = verify_manifest(result, final, "result_relative_path")
        write_json(
            result / "FINAL-READONLY-VERIFICATION-v6.json",
            {
                "schema": "phase4-g5-1-final-readonly-verification-v6",
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
            result / "COMPLETE-WALL-v6.json",
            {
                "schema": "phase4-g5-1-complete-wall-v6",
                "status": "PASS",
                "campaign": campaign,
                "complete_wall_ns": complete_ns,
                "limit_ns": LIMIT_NS[campaign],
                "from": "fail-fast global lock acquisition",
                "through": "final manifest and read-only verification fsync",
                "terminal_self_exclusion": "COMPLETE-WALL-v6.json follows the verified final manifest",
            },
        )
        fsync_dir(result)
        print(compact({"status": "PASS", "campaign": campaign, "result": str(result), "complete_wall_ns": complete_ns}))
        return 0
    except Exception as error:
        if result.exists() and not (result / "FAILED-v6.json").exists():
            write_json(
                result / "FAILED-v6.json",
                {
                    "schema": "phase4-g5-1-failure-v6",
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
