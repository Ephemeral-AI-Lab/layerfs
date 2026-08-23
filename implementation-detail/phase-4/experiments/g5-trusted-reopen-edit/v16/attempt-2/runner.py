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
import tempfile
import time


HERE = pathlib.Path(__file__).resolve().parent
REPO = HERE.parents[5]
HISTORY = HERE.parents[1]
METHOD = HERE.parent / "method"
SCHEDULE = METHOD / "SCHEDULE-v16.tsv"
EXPECTED = METHOD / "EXPECTED-OUTCOMES-v16.tsv"
INPUT_MANIFEST = METHOD / "INPUT-MANIFEST-v16.tsv"
METHOD_MANIFEST = METHOD / "METHOD-MANIFEST-v16.tsv"
SOURCE_FREEZE = METHOD / "SOURCE-FREEZE-v16.json"
FREEZE_VERIFICATION = HERE.parent / "FREEZE-VERIFICATION-v16.json"
LIMITATIONS = HERE.parent / "LIMITATIONS-v16.json"
STATIC_CLOSURE = HERE.parent / "STATIC-CLOSURE-v16.json"
DRY_RUN = HERE.parent / "DRY-RUN-v16.json"
DRY_RUN_INTENT = HERE.parent / "DRY-RUN-INTENT-v16.json"
DRY_RUN_CALIBRATION_STDOUT = HERE.parent / "DRY-RUN-CALIBRATION-v16.stdout"
DRY_RUN_CALIBRATION_STDERR = HERE.parent / "DRY-RUN-CALIBRATION-v16.stderr"
DRY_RUN_CALIBRATION_TERMINAL = HERE.parent / "DRY-RUN-CALIBRATION-TERMINAL-v16.json"
DRY_RUN_DISPOSITION = HERE.parent / "DRY-RUN-DISPOSITION-v16.json"
DRY_RUN_FAILED = HERE.parent / "DRY-RUN-FAILED-v16.json"
PREMEASUREMENT_REVISE = HERE.parent / "PREMEASUREMENT-REVISE-v16.json"
WRAPPER_CALIBRATION_INTENT = HERE.parent / "WRAPPER-CALIBRATION-INTENT-v16.json"
WRAPPER_CALIBRATION_RAW = HERE.parent / "WRAPPER-CALIBRATION-RAW-v16.jsonl"
WRAPPER_CALIBRATION_RESULT = HERE.parent / "WRAPPER-CALIBRATION-RESULT-v16.json"
PRIMARY = HERE / "analyzers/primary.py"
INDEPENDENT = HERE / "analyzers/independent.py"

CHECKPOINT = "d58c5a1307253dfc221fe50de996c183deb9458a"
BRANCH = "codex/empty-worktree"
DATE = "20260823"
LOCK = REPO / "target/BENCHMARK_LOCK"
INPUT_ROOT = REPO / f"target/phase4-g5-trusted-reopen-edit-inputs-{DATE}-v10"
SCREEN_RESULT = REPO / f"target/phase4-g5-trusted-reopen-edit-{DATE}-v16-screen-attempt-2"
GATE_RESULT = REPO / f"target/phase4-g5-trusted-reopen-edit-{DATE}-v16-attempt-2"
WRAPPER_CALIBRATION_ROOT = (
    REPO / f"target/phase4-g5-trusted-reopen-edit-wrapper-calibration-{DATE}-v16"
)

# V16 changes the product timer equation and uses a fresh hash-bound release.
G5_CHILD_BINARY = HERE.parent / "g5-benchmark/target/release/layerfs-g5-trusted-child-v16"
FIXTURE_FLAG = "--g5-fixture"
PREPARE_FLAG = "--g5-prepare"
CHILD_FLAG = "--g5-child"
SEMANTIC_FLAG = "--g5-semantic"
CHILD_READY_SCHEMA = "phase4-g5-trusted-child-ready-v10"
CHILD_ENVELOPE_SCHEMA = "phase4-g5-trusted-child-row-v10"
CHILD_TERMINAL_SCHEMA = "phase4-g5-trusted-child-terminal-v10"
FIXTURE_SCHEMA = "phase4-g5-trusted-fixture-v10"
PREPARE_SCHEMA = "phase4-g5-trusted-prepare-v10"
SEMANTIC_SCHEMA = "phase4-g5-trusted-semantic-v10"
SEMANTIC_TERMINAL_SCHEMA = "phase4-g5-trusted-semantic-terminal-v10"
SENTINEL_SCHEMA = "phase4-g5-1-protected-sentinel-v16"
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
G4_ARM_RAW = G4_EXECUTABLE.parents[1] / "ARM-RAW-v1.jsonl"
G4_ARM_RAW_SHA256 = "84bd25d0cb7b63eb218a02fa3fdc59aa8adf07d30452afeb328cef33876676a2"
V7_PREMEASUREMENT = HISTORY / "v7/PREMEASUREMENT-REVISE-v7.json"
V7_PREMEASUREMENT_SHA256 = "fce57857882471bc06f327b8c2b0e5ec07443662fc2986e5c77c5f0ce1a6f01d"
V7_PREPARATION_AUDIT = HISTORY / "v7/PREPARATION-FAILURE-AUDIT-v7.json"
V7_PREPARATION_AUDIT_SHA256 = "3ae6e471bf5d4c3e7a522b3cb19cacb5f0a94429f86ab3c21373d7889c4b24fa"
V7_PARTIAL_INPUT_ROOT = REPO / "target/phase4-g5-trusted-reopen-edit-inputs-20260823-v7"
V7_PARTIAL_TREE_SHA256 = "69645cc89c3b815f07df67d55ccaaf82c9035d92bc5e9f7f916c1112f19825b2"
V8_PREMEASUREMENT = HISTORY / "v8/PREMEASUREMENT-REVISE-v8.json"
V8_PREMEASUREMENT_SHA256 = "9f155086b9f31246d9430076e521b29a001db01f49fde692f6bf8f862c0d4a09"
V9_PREMEASUREMENT = HISTORY / "v9/PREMEASUREMENT-REVISE-v9.json"
V9_PREMEASUREMENT_SHA256 = "6b3340894747b30a3ee26b3283aa285064cd2bbc3271d3ccb3a6f00ec01d3ec7"
V9_SUPERSESSION = HISTORY / "v10/V9-SUPERSESSION-v10.json"
V9_SUPERSESSION_SHA256 = "30c4972ea88f3548d7be3ec5a126227955231d7ba643f67464c3d46ceaf91421"
V10_PREMEASUREMENT = HISTORY / "v10/PREMEASUREMENT-REVISE-v10.json"
V10_PREMEASUREMENT_SHA256 = "4e98e28269289218bcc1ff06f03ab68d23a31870610246d0933b8c7c2f039339"
V10_DISPOSITION = HISTORY / "v10/DRY-RUN-DISPOSITION-v10.json"
V10_DISPOSITION_SHA256 = "57fb3652093aa6ab27054d3a97d5ef722d85a07d35be977575ccd18f0783d104"
V10_INPUT_MANIFEST = HISTORY / "v10/method/INPUT-MANIFEST-v10.tsv"
V10_INPUT_MANIFEST_SHA256 = "69482a350edd0c39128487f963cc3b60b19974437dbc7b538fe66932a637d8df"
V10_METHOD_MANIFEST = HISTORY / "v10/method/METHOD-MANIFEST-v10.tsv"
V10_METHOD_MANIFEST_SHA256 = "73869b9a31304f55acf9d88f6035edcd0f3ae81bc0eca4373249e116acafd963"
V10_SOURCE_FREEZE = HISTORY / "v10/method/SOURCE-FREEZE-v10.json"
V10_SOURCE_FREEZE_SHA256 = "f47079fe90622ad8f7ccc49564b104d647b808d8e7e8ae2b65f2723414e3546e"
V10_FREEZE_VERIFICATION = HISTORY / "v10/FREEZE-VERIFICATION-v10.json"
V10_FREEZE_VERIFICATION_SHA256 = "3db726d94f1194e3b6439112797ea4bf445b135cf09da640120dfbb4283668c6"
V10_RELEASE_SHA256 = "89226715912afcb1c2b002b5b17bbfaba0406825658349283a31a3b202f0e07b"
V16_RELEASE_SHA256 = "479ff27ca30a562b0de0b27710739867d5d844c508f6a18138100dd2132435ad"
V10_SUPERSESSION = HISTORY / "v11/V10-SUPERSESSION-v11.json"
V10_SUPERSESSION_SHA256 = "fcfd9f7c4444447b4c03bd78e98bf102dc447d77c6d84f2b03c29ac84a3afdd3"
V11_PREMEASUREMENT = HISTORY / "v11/PREMEASUREMENT-REVISE-v11.json"
V11_PREMEASUREMENT_SHA256 = "3439381a99f3bb78fd3be12568a55f7293b086539062e3b46db380c963a06a63"
V11_DISPOSITION = HISTORY / "v11/DRY-RUN-DISPOSITION-v11.json"
V11_DISPOSITION_SHA256 = "197923f30b329cf0ed296c6e945674c367e45954ed5d95accb3061a00dad9a09"
V11_METHOD_MANIFEST = HISTORY / "v11/method/METHOD-MANIFEST-v11.tsv"
V11_METHOD_MANIFEST_SHA256 = "e2ac6799f4fdcfd65526e4b38f7ea833a2f1d169be430ca6ac8432d73fb161ec"
V11_FREEZE_VERIFICATION = HISTORY / "v11/FREEZE-VERIFICATION-v11.json"
V11_FREEZE_VERIFICATION_SHA256 = "3671642e333d53ec6b9385c39206833ac066090b6902e39c61c80d83662c5b99"
V11_SUPERSESSION = HISTORY / "v12/V11-SUPERSESSION-v12.json"
V11_SUPERSESSION_SHA256 = "992dae78996a4f03d0798b2e0140a79c8becbb07c33691d726c862c3a92f04e4"
V12_PREMEASUREMENT = HISTORY / "v12/PREMEASUREMENT-REVISE-v12.json"
V12_PREMEASUREMENT_SHA256 = "e15ec11bebfc8cac7f73deacee5677ece62f5c8463b8c5d961434f5dfe0d15bc"
V12_POST_DRY_FAILURE = HISTORY / "v12/POST-DRY-RUN-VERIFICATION-FAILURE-v12.json"
V12_POST_DRY_FAILURE_SHA256 = "9fbb99582b038719dec3cc20924bc1ffdd1dbf468dff625ac0daaaa189727787"
V12_DISPOSITION = HISTORY / "v12/DRY-RUN-DISPOSITION-v12.json"
V12_DISPOSITION_SHA256 = "8bf82679317c01896feaf9449c9233ac5f27501885d0f8330bd8fbe76a4cfc83"
V12_METHOD_MANIFEST = HISTORY / "v12/method/METHOD-MANIFEST-v12.tsv"
V12_METHOD_MANIFEST_SHA256 = "93e705d6a5d3cc169b6ee20c84cb6f39c0fa129a6cd64615ba53fcaf1a7e4cbb"
V12_FREEZE_VERIFICATION = HISTORY / "v12/FREEZE-VERIFICATION-v12.json"
V12_FREEZE_VERIFICATION_SHA256 = "eb99a28502ff9b1eafdbe08a28a994ea167ec6b7042292dbe32095f9358f9c41"
V12_SUPERSESSION = HISTORY / "v13/V12-SUPERSESSION-v13.json"
V12_SUPERSESSION_SHA256 = "11bdabb541103014d418ee525c785f327f438a483d86f80615cababa2a45be9f"
V13_PREMEASUREMENT = HISTORY / "v13/PREMEASUREMENT-REVISE-v13.json"
V13_PREMEASUREMENT_SHA256 = "7b0688dbc38dc4ed44ab1186fff0d0eb5af6e3e36e56d6e6bdf0cd153a7bf10c"
V13_SCREEN_FAILURE = HISTORY / "v13/SCREEN-PREFLIGHT-FAILURE-v13.json"
V13_SCREEN_FAILURE_SHA256 = "ee54eddabe0f8ba9a25f813126f8a0f1ad24cd17306e0d38be6bc5c48637918b"
V13_DISPOSITION = HISTORY / "v13/DRY-RUN-DISPOSITION-v13.json"
V13_DISPOSITION_SHA256 = "2a84a0202199d0df251901d2fffbf905379b6c2533a0ef8a3377a302c4c3ad69"
V13_METHOD_MANIFEST = HISTORY / "v13/method/METHOD-MANIFEST-v13.tsv"
V13_METHOD_MANIFEST_SHA256 = "c266fcced5edb85ac535ee91d34a3ab98d8f59c6711572e5e8817c0c83d7adbb"
V13_FREEZE_VERIFICATION = HISTORY / "v13/FREEZE-VERIFICATION-v13.json"
V13_FREEZE_VERIFICATION_SHA256 = "321347299f56212ff2f68861179237fcde14427edc527b1acc3d67daf3b733ad"
V13_LOCK_RELEASE = REPO / "target/LOCK-RELEASE-v13.json"
V13_LOCK_RELEASE_SHA256 = "c815c640b99fe6292cc861bdfca50944823d1528093ce643c12dd87cf92f4464"
V13_LOCK_ATTESTATION = REPO / "target/BENCHMARK-LOCK-RELEASE-ATTESTATION-v13.json"
V13_LOCK_ATTESTATION_SHA256 = "3fd520e53b03fe8a33cfcc863f33c8a952473f1b0dd97dc2e1a7b8bc82a52a9c"
V13_SUPERSESSION = HISTORY / "v14/V13-SUPERSESSION-v14.json"
V13_SUPERSESSION_SHA256 = "cf5b5b7df77985a7f6a46b1a863fd3778f1dc063b06b942abb12af7c39077a2b"
V14_SUPERSESSION = HISTORY / "v15/V14-SUPERSESSION-v15.json"
V14_SUPERSESSION_SHA256 = "1cda67abb650c9da62c7ff740810d93d46672fb5a40ca50e8c0974ab43cd0f94"

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
    "S02": ("semantic", "touched-error-matrix"),
    "S03": ("semantic", "unrelated-corruption"),
    "S04": ("semantic", "trusted-verified-reopen", "reconciliation"),
    "S07": ("frozen-g4-protected", "full-create", "range"),
}
GATE_ARM_OBSERVATIONS = 200
RETAINED_G4_ROUNDED_CHILD_ARM_NS = 250_000_000
RETAINED_G4_FRESH_RECONSTRUCTION_MAX_NS = 334_756_708
RETAINED_G5_FOUNDATION_COMPLETE_WALL_NS = 9_254_244_292
FIXED_CAMPAIGN_FINALIZATION_NS = 10_000_000_000
WRAPPER_CALIBRATION_SAMPLES = 3
WRAPPER_INITIALIZATION_SAMPLES = 1
WRAPPER_CALIBRATION_CONSERVATIVE_FACTOR = 4
BASE_FORECAST_COMPONENTS_NS = {
    "retained_child_and_checkpoint_work": (
        GATE_ARM_OBSERVATIONS * RETAINED_G4_ROUNDED_CHILD_ARM_NS
        + 56 * RETAINED_G4_FRESH_RECONSTRUCTION_MAX_NS
    ),
    "fixed_campaign_finalization": FIXED_CAMPAIGN_FINALIZATION_NS,
}
CALIBRATION_SIZE = 104_857_600
HASH_CALIBRATION_DIVISOR = 2
FORECAST_MODEL_VERSION = "phase4-g5-1-v16-fast-law-forecast-v5"
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
    "expected_cdc_sequence_fingerprint": "c2b4a92188569d206717210b596dde9b8aeade1c9c81b87f02b8d0d6ebda1112",
    "root_id": "18f33e3ca6030e966cf8ed41c0b43f4769de8b02247f453fae447627bee4b77c",
    "transition_id": "60d191810b303b26d12453add0b9e1718b1f1b654473615d9323f0ee477a9b7d",
    "ordered_closure_digest": "7e806f7023c3e33914c59d2b0d0d84bca8859fdbd7663b55f5f5c99313252d42",
    "q_current": 0,
}
S07_FULL = {
    **S07_COMMON,
    "operation": "full",
    "canonical_bytes_written": 1_051_409,
    "canonical_new_write_bytes": 1_051_409,
    "canonical_bytes_authenticated": 3_259_186,
    "objects_created": 57,
    "objects_authenticated": 186,
    "objects_reused": 0,
    "mapping_bytes_rewritten": 2_144,
    "source_bytes_read": 1_048_576,
    "raw_bytes_hashed": 0,
    "payload_io_bytes": 2_097_156,
    "d_bytes": 1_048_580,
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
    "canonical_bytes_authenticated": 1_051_290,
    "objects_authenticated": 55,
}
S07_FULL_RANGE_SHAPES = [
    {"label": "zero", "start": 0, "end": 0, "returned_bytes": 0, "canonical_bytes_authenticated": 89, "objects_authenticated": 1},
    {"label": "first-byte", "start": 0, "end": 1, "returned_bytes": 1, "canonical_bytes_authenticated": 34_806, "objects_authenticated": 3},
    {"label": "cross-chunk", "start": 32_767, "end": 32_769, "returned_bytes": 2, "canonical_bytes_authenticated": 52_174, "objects_authenticated": 4},
    {"label": "last-byte", "start": 1_048_575, "end": 1_048_576, "returned_bytes": 1, "canonical_bytes_authenticated": 17_580, "objects_authenticated": 3},
    {"label": "eof", "start": 1_048_576, "end": 1_048_576, "returned_bytes": 0, "canonical_bytes_authenticated": 89, "objects_authenticated": 1},
]
S07_RANGE = {
    **S07_COMMON,
    "operation": "read-range-1m",
    "canonical_bytes_authenticated": 1_051_433,
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

CLONE_RECEIPT_SCHEMA = "g5-v16-native-clone-receipt-v1"
CLONE_COPY_CONTENT = "NotRehashedPerFastLaw"
CLONE_CUSTODY_PROOF = "preverified-sealed-master-plus-native-clone-receipt"
ROOTED_STATE_SCHEMA = "g5-v16-rooted-logical-state-v1"
ROOTED_STATE_SEMANTICS = (
    "product-authenticated-root-transition-ordered-closure-not-all-object-table"
)
ALL_ROW_CATALOG_PARITY = "NotClaimedSeparateFutureAllRowCasAudit"
PHYSICAL_ALLOCATION_SCHEMA = "g5-v16-physical-allocation-observation-v1"
PHYSICAL_ALLOCATION_CLASSIFICATION = "NotLogicalParity"
COMMON_SECONDARY_TIMER_FIELDS = (
    "edit_base_scope_ns",
    "mapping_and_construction_ns",
    "proof_ns",
    "publication_commit_ns",
    "reconciliation_ns",
)
FULL_INTERVAL_CLASSIFICATION = "full-first-edit-equation"
COMMON_INTERVAL_CLASSIFICATION = "common-edit-through-reconciliation"
CHILD_LIFECYCLE_SCHEMA = "phase4-g5-1-product-child-lifecycle-v16"
RSS_CLASSIFICATION = "PerProductChildRetainedPeak"
SYNCHRONOUS_RSS_KIND = "synchronous-one-shot"
ARM_CLEANUP_SCHEMA = "phase4-g5-1-arm-cleanup-receipt-v16"
WORK_ROOT_LIFECYCLE_SCHEMA = "phase4-g5-1-work-root-lifecycle-v16"


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
    verify_file(G4_ARM_RAW, G4_ARM_RAW_SHA256)
    verify_file(V7_PREMEASUREMENT, V7_PREMEASUREMENT_SHA256)
    verify_file(V7_PREPARATION_AUDIT, V7_PREPARATION_AUDIT_SHA256)
    if path_kind_size_mode_sha256_tree(V7_PARTIAL_INPUT_ROOT) != V7_PARTIAL_TREE_SHA256:
        raise RuntimeError("preserved v7 partial input custody mismatch")
    verify_file(V8_PREMEASUREMENT, V8_PREMEASUREMENT_SHA256)
    verify_file(V9_PREMEASUREMENT, V9_PREMEASUREMENT_SHA256)
    verify_file(V9_SUPERSESSION, V9_SUPERSESSION_SHA256)
    verify_file(V10_PREMEASUREMENT, V10_PREMEASUREMENT_SHA256)
    verify_file(V10_DISPOSITION, V10_DISPOSITION_SHA256)
    verify_file(V10_INPUT_MANIFEST, V10_INPUT_MANIFEST_SHA256)
    verify_file(V10_METHOD_MANIFEST, V10_METHOD_MANIFEST_SHA256)
    verify_file(V10_SOURCE_FREEZE, V10_SOURCE_FREEZE_SHA256)
    verify_file(V10_FREEZE_VERIFICATION, V10_FREEZE_VERIFICATION_SHA256)
    verify_file(G5_CHILD_BINARY, V16_RELEASE_SHA256)
    verify_file(V10_SUPERSESSION, V10_SUPERSESSION_SHA256)
    supersession = json.loads(V10_SUPERSESSION.read_text(encoding="utf-8"))
    if (
        supersession.get("status") != "PASS"
        or supersession.get("superseded_disposition") != "PREMEASUREMENT_REVISE"
        or supersession.get("input_reuse") is not True
        or supersession.get("product_release_reuse") is not True
        or supersession.get("measured_rows_reused") is not False
        or supersession.get("v10_measured_rows") != 0
        or supersession.get("v10_screen_result_present") is not False
        or supersession.get("v10_gate_result_present") is not False
        or supersession.get("v10_global_lock_present") is not False
    ):
        raise RuntimeError("v10 supersession control mismatch")
    chain = supersession.get("terminal_chain")
    if not isinstance(chain, dict) or len(chain) != 15:
        raise RuntimeError("v10 supersession terminal chain mismatch")
    for label, binding in chain.items():
        if not isinstance(binding, dict):
            raise RuntimeError(f"v10 supersession binding shape: {label}")
        path = REPO / binding.get("path", "")
        verify_file(path, binding.get("sha256"))
        if "rows" in binding:
            line_count = len(path.read_text(encoding="utf-8").splitlines())
            if path.suffix == ".tsv":
                line_count -= 1
            if line_count != binding["rows"]:
                raise RuntimeError(f"v10 supersession row count mismatch: {label}")
    for path, expected in (
        (V11_PREMEASUREMENT, V11_PREMEASUREMENT_SHA256),
        (V11_DISPOSITION, V11_DISPOSITION_SHA256),
        (V11_METHOD_MANIFEST, V11_METHOD_MANIFEST_SHA256),
        (V11_FREEZE_VERIFICATION, V11_FREEZE_VERIFICATION_SHA256),
        (V11_SUPERSESSION, V11_SUPERSESSION_SHA256),
    ):
        verify_file(path, expected)
    v11_supersession = json.loads(V11_SUPERSESSION.read_text(encoding="utf-8"))
    contradiction = v11_supersession.get("v11_calibration_semantic_contradiction")
    if (
        v11_supersession.get("status") != "PASS"
        or v11_supersession.get("superseded_disposition") != "PREMEASUREMENT_REVISE"
        or v11_supersession.get("v11_measured_rows") != 0
        or v11_supersession.get("v11_screen_result_present") is not False
        or v11_supersession.get("v11_gate_result_present") is not False
        or v11_supersession.get("v11_global_lock_present") is not False
        or not isinstance(contradiction, dict)
        or contradiction.get("cardinality") != 3
        or contradiction.get("status") != "FROZEN_METHOD_DEFECT"
    ):
        raise RuntimeError("v11 supersession control mismatch")
    v11_chain = v11_supersession.get("terminal_chain")
    if not isinstance(v11_chain, dict) or len(v11_chain) != 18:
        raise RuntimeError("v11 supersession terminal chain mismatch")
    for label, binding in v11_chain.items():
        if not isinstance(binding, dict):
            raise RuntimeError(f"v11 supersession binding shape: {label}")
        path = REPO / binding.get("path", "")
        verify_file(path, binding.get("sha256"))
        if "rows" in binding:
            line_count = len(path.read_text(encoding="utf-8").splitlines())
            if path.suffix == ".tsv":
                line_count -= 1
            if line_count != binding["rows"]:
                raise RuntimeError(f"v11 supersession row count mismatch: {label}")
    for path, expected in (
        (V12_PREMEASUREMENT, V12_PREMEASUREMENT_SHA256),
        (V12_POST_DRY_FAILURE, V12_POST_DRY_FAILURE_SHA256),
        (V12_DISPOSITION, V12_DISPOSITION_SHA256),
        (V12_METHOD_MANIFEST, V12_METHOD_MANIFEST_SHA256),
        (V12_FREEZE_VERIFICATION, V12_FREEZE_VERIFICATION_SHA256),
        (V12_SUPERSESSION, V12_SUPERSESSION_SHA256),
    ):
        verify_file(path, expected)
    v12_supersession = json.loads(V12_SUPERSESSION.read_text(encoding="utf-8"))
    v12_failure = v12_supersession.get("v12_failure", {})
    if (
        v12_supersession.get("status") != "PASS"
        or v12_supersession.get("superseded_disposition") != "PREMEASUREMENT_REVISE"
        or v12_supersession.get("input_reuse") is not True
        or v12_supersession.get("product_release_reuse") is not True
        or v12_supersession.get("measured_rows_reused") is not False
        or v12_failure.get("status") != "FROZEN_METHOD_DEFECT"
        or v12_failure.get("producer_missing_fields")
        != {
            "wrapper_initialization_samples_completed": 0,
            "wrapper_recurring_samples_completed": 0,
        }
        or v12_failure.get("measured_rows") != 0
        or v12_failure.get("screen_result_present") is not False
        or v12_failure.get("gate_result_present") is not False
        or v12_failure.get("global_lock_present") is not False
    ):
        raise RuntimeError("v12 supersession control mismatch")
    v12_chain = v12_supersession.get("terminal_chain")
    if not isinstance(v12_chain, dict) or len(v12_chain) != 19:
        raise RuntimeError("v12 supersession terminal chain mismatch")
    for label, binding in v12_chain.items():
        if not isinstance(binding, dict):
            raise RuntimeError(f"v12 supersession binding shape: {label}")
        path = REPO / binding.get("path", "")
        verify_file(path, binding.get("sha256"))
        if "rows" in binding:
            line_count = len(path.read_text(encoding="utf-8").splitlines())
            if path.suffix == ".tsv":
                line_count -= 1
            if line_count != binding["rows"]:
                raise RuntimeError(f"v12 supersession row count mismatch: {label}")
    for path, expected in (
        (V13_PREMEASUREMENT, V13_PREMEASUREMENT_SHA256),
        (V13_SCREEN_FAILURE, V13_SCREEN_FAILURE_SHA256),
        (V13_DISPOSITION, V13_DISPOSITION_SHA256),
        (V13_METHOD_MANIFEST, V13_METHOD_MANIFEST_SHA256),
        (V13_FREEZE_VERIFICATION, V13_FREEZE_VERIFICATION_SHA256),
        (V13_LOCK_RELEASE, V13_LOCK_RELEASE_SHA256),
        (V13_LOCK_ATTESTATION, V13_LOCK_ATTESTATION_SHA256),
        (V13_SUPERSESSION, V13_SUPERSESSION_SHA256),
    ):
        verify_file(path, expected)
    v13_supersession = json.loads(V13_SUPERSESSION.read_text(encoding="utf-8"))
    v13_failure = v13_supersession.get("v13_failure", {})
    if (
        v13_supersession.get("status") != "PASS"
        or v13_supersession.get("superseded_disposition") != "PREMEASUREMENT_REVISE"
        or v13_supersession.get("input_reuse") is not True
        or v13_supersession.get("product_release_reuse") is not True
        or v13_supersession.get("measured_rows_reused") is not False
        or v13_failure.get("status") != "FROZEN_METHOD_DEFECT"
        or v13_failure.get("historical_calibration_global_lock_absent") is not True
        or v13_failure.get("invalid_live_predicate") != "LOCK.exists()"
        or v13_failure.get("screen_attempts") != 1
        or v13_failure.get("screen_result_roots") != 0
        or v13_failure.get("measured_rows") != 0
        or v13_failure.get("product_children_started") != 0
        or v13_failure.get("global_lock_present_terminal") is not False
    ):
        raise RuntimeError("v13 supersession control mismatch")
    lock_release = json.loads(V13_LOCK_RELEASE.read_text(encoding="utf-8"))
    lock_attestation = json.loads(V13_LOCK_ATTESTATION.read_text(encoding="utf-8"))
    if (
        lock_release.get("schema") != "phase4-g5-1-lock-release-v13"
        or lock_release.get("status") != "REVISE"
        or lock_release.get("state") != "failure"
        or lock_release.get("lock_absent") is not True
        or lock_release.get("attestation_sha256") != V13_LOCK_ATTESTATION_SHA256
        or lock_attestation.get("schema") != "phase4-g5-1-lock-v13"
        or lock_attestation.get("state") != "failure"
        or (lock_release.get("device"), lock_release.get("inode"))
        != (lock_attestation.get("device"), lock_attestation.get("inode"))
        or lock_release.get("token_sha256")
        != sha256_bytes(lock_attestation.get("token", "").encode())
    ):
        raise RuntimeError("v13 lock release custody mismatch")
    v13_chain = v13_supersession.get("terminal_chain")
    if not isinstance(v13_chain, dict) or len(v13_chain) != 21:
        raise RuntimeError("v13 supersession terminal chain mismatch")
    for label, binding in v13_chain.items():
        if not isinstance(binding, dict):
            raise RuntimeError(f"v13 supersession binding shape: {label}")
        path = REPO / binding.get("path", "")
        verify_file(path, binding.get("sha256"))
        if "rows" in binding:
            line_count = len(path.read_text(encoding="utf-8").splitlines())
            if path.suffix == ".tsv":
                line_count -= 1
            if line_count != binding["rows"]:
                raise RuntimeError(f"v13 supersession row count mismatch: {label}")
    verify_file(V14_SUPERSESSION, V14_SUPERSESSION_SHA256)
    v14_supersession = json.loads(V14_SUPERSESSION.read_text(encoding="utf-8"))
    v14_failure = v14_supersession.get("v14_failure", {})
    if (
        v14_supersession.get("status") != "PASS"
        or v14_supersession.get("superseded_disposition") != "SCREEN_REVISE"
        or v14_supersession.get("input_reuse") is not True
        or v14_supersession.get("product_release_reuse") is not False
        or v14_supersession.get("measured_rows_reused") is not False
        or v14_failure.get("status") != "PRODUCT_TIMER_EQUATION_DEFECT"
        or v14_failure.get("product_rows") != 1
        or v14_failure.get("accepted_rows") != 0
        or v14_failure.get("first_edit_timer_equation_matches") is not False
        or v14_failure.get("terminal_q") != 0
        or v14_failure.get("terminal_lock_present") is not False
    ):
        raise RuntimeError("v14 supersession control mismatch")
    v14_chain = v14_supersession.get("terminal_chain")
    if not isinstance(v14_chain, dict) or len(v14_chain) != 16:
        raise RuntimeError("v14 supersession terminal chain mismatch")
    for label, binding in v14_chain.items():
        path = REPO / binding.get("path", "")
        verify_file(path, binding.get("sha256"))


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
            HERE / "PREREGISTRATION-v16.md",
            HERE / "REVIEW-SYNTHESIS-v16.md",
            HERE / "SAMPLE-COUNT-INTERPRETATION-ADDENDUM-v16.md",
            HERE / "FOCUSED-TEST-ATTEMPTS-v16.json",
            HERE / "PREMEASUREMENT-READINESS-AUDIT-v16.json",
            LIMITATIONS,
            HERE / "runner.py",
            PRIMARY,
            INDEPENDENT,
            SCHEDULE,
            EXPECTED,
            INPUT_MANIFEST,
            G4_EXECUTABLE,
            G4_FINAL_MANIFEST,
            G4_ARM_RAW,
            V7_PREMEASUREMENT,
            V7_PREPARATION_AUDIT,
            V8_PREMEASUREMENT,
            V9_PREMEASUREMENT,
            V9_SUPERSESSION,
            V10_PREMEASUREMENT,
            V10_DISPOSITION,
            V10_INPUT_MANIFEST,
            V10_METHOD_MANIFEST,
            V10_SOURCE_FREEZE,
            V10_FREEZE_VERIFICATION,
            V10_SUPERSESSION,
            V11_PREMEASUREMENT,
            V11_DISPOSITION,
            V11_METHOD_MANIFEST,
            V11_FREEZE_VERIFICATION,
            V11_SUPERSESSION,
            V12_PREMEASUREMENT,
            V12_POST_DRY_FAILURE,
            V12_DISPOSITION,
            V12_METHOD_MANIFEST,
            V12_FREEZE_VERIFICATION,
            V12_SUPERSESSION,
            V13_PREMEASUREMENT,
            V13_SCREEN_FAILURE,
            V13_DISPOSITION,
            V13_METHOD_MANIFEST,
            V13_FREEZE_VERIFICATION,
            V13_LOCK_RELEASE,
            V13_LOCK_ATTESTATION,
            V13_SUPERSESSION,
            V14_SUPERSESSION,
            HERE / "g5-benchmark/Cargo.toml",
            HERE / "g5-benchmark/Cargo.lock",
            HERE / "g5-benchmark/build.rs",
            HERE / "g5-benchmark/src/main.rs",
            HERE / "g5-benchmark/src/session.rs",
        )
    }
    fixed.update(CONTROLLING_HASHES)
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


def freeze_interface_contract():
    return {
        "product_operand_version": "v16",
        "product_release_reuse": False,
        "product_release_sha256": V16_RELEASE_SHA256,
        "sealed_input_reuse": True,
        "sealed_input_manifest_sha256": V10_INPUT_MANIFEST_SHA256,
        "prepare_flag": PREPARE_FLAG,
        "fixture_flag": FIXTURE_FLAG,
        "child_flag": CHILD_FLAG,
        "semantic_flag": SEMANTIC_FLAG,
        "child_ready_schema": CHILD_READY_SCHEMA,
        "child_envelope_schema": CHILD_ENVELOPE_SCHEMA,
        "child_terminal_schema": CHILD_TERMINAL_SCHEMA,
        "request_fields": list(REQUEST_FIELDS),
        "wrapper_calibration": {
            "intent_path": str(WRAPPER_CALIBRATION_INTENT.relative_to(REPO)),
            "raw_path": str(WRAPPER_CALIBRATION_RAW.relative_to(REPO)),
            "result_path": str(WRAPPER_CALIBRATION_RESULT.relative_to(REPO)),
            "root_path": str(WRAPPER_CALIBRATION_ROOT.relative_to(REPO)),
            "intent_schema": "phase4-g5-1-wrapper-calibration-intent-v16",
            "initialization_schema": "phase4-g5-1-wrapper-initialization-sample-v16",
            "sample_schema": "phase4-g5-1-wrapper-calibration-sample-v16",
            "result_schema": "phase4-g5-1-wrapper-calibration-result-v16",
            "initialization_samples": WRAPPER_INITIALIZATION_SAMPLES,
            "recurring_samples": WRAPPER_CALIBRATION_SAMPLES,
            "conservative_factor": WRAPPER_CALIBRATION_CONSERVATIVE_FACTOR,
            "initialization_multiplier": 1,
            "recurring_multiplier": GATE_ARM_OBSERVATIONS,
            "product_scope": "zero-product",
        },
    }


def write_input_and_method_manifests():
    input_text = manifest_text(INPUT_ROOT, key="input_relative_path")
    write_text(INPUT_MANIFEST, input_text)
    sources = method_source_names()
    method_text = "repo_relative_path\tbytes\tsha256\n" + "".join(
        f"{name}\t{(REPO / name).stat().st_size}\t{sha256(REPO / name)}\n" for name in sources
    )
    write_text(METHOD_MANIFEST, method_text)
    freeze = {
        "schema": "phase4-g5-1-source-freeze-v16",
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
        "g4_retained_arm_raw_sha256": G4_ARM_RAW_SHA256,
        "g5_executable_sha256": sha256(G5_CHILD_BINARY),
        "schedule_sha256": sha256(SCHEDULE),
        "expectations_sha256": sha256(EXPECTED),
        "limitations_sha256": sha256(LIMITATIONS),
        "post_freeze_verification": {
            "path": str(FREEZE_VERIFICATION.relative_to(REPO)),
            "schema": "phase4-g5-1-freeze-verification-v16",
            "method_authority": False,
        },
        "interface": freeze_interface_contract(),
        "forecast_model": FORECAST_MODEL_VERSION,
        "base_forecast_components_ns": BASE_FORECAST_COMPONENTS_NS,
        "full_wrapper_limit_ns": LIMIT_NS["gate"],
        "frozen_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
    }
    write_json(SOURCE_FREEZE, freeze)


def independent_manifest_rehash(root, manifest, key):
    root = pathlib.Path(root).resolve()
    manifest = pathlib.Path(manifest)
    raw = manifest.read_bytes()
    text = raw.decode("utf-8", errors="strict")
    reader = csv.DictReader(text.splitlines(), delimiter="\t")
    if reader.fieldnames != [key, "bytes", "sha256"]:
        raise RuntimeError(f"independent manifest header mismatch: {manifest}")
    rows = list(reader)
    names = [row[key] for row in rows]
    if names != sorted(names) or len(names) != len(set(names)):
        raise RuntimeError(f"independent manifest ordering mismatch: {manifest}")
    actual_rows = []
    for row in rows:
        candidate = root / row[key]
        target = candidate.resolve()
        if (
            not target.is_relative_to(root)
            or candidate.is_symlink()
            or not candidate.is_file()
        ):
            raise RuntimeError(f"independent manifest target mismatch: {candidate}")
        actual = {
            key: row[key],
            "bytes": str(target.stat().st_size),
            "sha256": sha256(target),
        }
        if actual != row:
            raise RuntimeError(f"independent manifest row mismatch: {target}")
        actual_rows.append(actual)
    reconstructed = f"{key}\tbytes\tsha256\n" + "".join(
        f"{row[key]}\t{row['bytes']}\t{row['sha256']}\n" for row in actual_rows
    )
    if reconstructed.encode() != raw:
        raise RuntimeError(f"independent manifest reconstruction mismatch: {manifest}")
    return {
        "manifest_sha256": sha256(manifest),
        "row_count": len(actual_rows),
        "row_bindings_sha256": sha256_bytes(compact(actual_rows).encode()),
        "all_rows_reopened_and_rehashed": True,
        "byte_reconstruction_exact": True,
        "row_paths": names,
    }


def current_freeze_bindings(freeze):
    input_rehash = independent_manifest_rehash(
        INPUT_ROOT, INPUT_MANIFEST, "input_relative_path"
    )
    method_rehash = independent_manifest_rehash(
        REPO, METHOD_MANIFEST, "repo_relative_path"
    )
    explicit_sources = freeze.get("explicit_sources")
    if not isinstance(explicit_sources, list):
        raise RuntimeError("source freeze explicit source list missing")
    if method_rehash["row_paths"] != explicit_sources:
        raise RuntimeError("method manifest/source freeze explicit source mismatch")
    verification_relative = str(FREEZE_VERIFICATION.relative_to(REPO))
    supersession_relatives = {
        str(V9_SUPERSESSION.relative_to(REPO)),
        str(V10_SUPERSESSION.relative_to(REPO)),
        str(V11_SUPERSESSION.relative_to(REPO)),
        str(V12_SUPERSESSION.relative_to(REPO)),
        str(V13_SUPERSESSION.relative_to(REPO)),
        str(V14_SUPERSESSION.relative_to(REPO)),
    }
    if verification_relative in explicit_sources or not supersession_relatives.issubset(
        explicit_sources
    ):
        raise RuntimeError("freeze verification cycle or supersession omission")
    if freeze.get("post_freeze_verification") != {
        "path": verification_relative,
        "schema": "phase4-g5-1-freeze-verification-v16",
        "method_authority": False,
    }:
        raise RuntimeError("source freeze verification protocol mismatch")
    direct_hashes = {
        "method_manifest_sha256": method_rehash["manifest_sha256"],
        "input_manifest_sha256": input_rehash["manifest_sha256"],
        "g4_verified_executable_sha256": sha256(G4_EXECUTABLE),
        "g4_retained_arm_raw_sha256": sha256(G4_ARM_RAW),
        "g5_executable_sha256": sha256(G5_CHILD_BINARY),
        "schedule_sha256": sha256(SCHEDULE),
        "expectations_sha256": sha256(EXPECTED),
        "limitations_sha256": sha256(LIMITATIONS),
    }
    if any(freeze.get(key) != value for key, value in direct_hashes.items()):
        raise RuntimeError("source freeze direct hash mismatch")
    expected_interface = freeze_interface_contract()
    if (
        freeze.get("interface") != expected_interface
        or freeze.get("forecast_model") != FORECAST_MODEL_VERSION
        or freeze.get("base_forecast_components_ns") != BASE_FORECAST_COMPONENTS_NS
        or freeze.get("full_wrapper_limit_ns") != LIMIT_NS["gate"]
    ):
        raise RuntimeError("source freeze direct control field mismatch")
    explicit_sources_sha256 = hash_explicit_sources(explicit_sources)
    if freeze.get("explicit_sources_sha256") != explicit_sources_sha256:
        raise RuntimeError("source freeze explicit aggregate mismatch")
    head = subprocess.check_output(
        ["git", "rev-parse", "HEAD"], cwd=REPO, text=True
    ).strip()
    branch = subprocess.check_output(
        ["git", "branch", "--show-current"], cwd=REPO, text=True
    ).strip()
    git_status_sha256 = sha256_bytes(status_bytes())
    tracked_diff_sha256 = tracked_diff_hash()
    if (
        freeze.get("checkpoint") != head
        or freeze.get("branch") != branch
        or freeze.get("git_status_sha256") != git_status_sha256
        or freeze.get("tracked_diff_sha256") != tracked_diff_sha256
    ):
        raise RuntimeError("source freeze repository identity mismatch")
    input_rehash.pop("row_paths")
    method_rehash.pop("row_paths")
    return {
        "source_freeze_sha256": sha256(SOURCE_FREEZE),
        "branch": branch,
        "head": head,
        "git_status_sha256": git_status_sha256,
        "tracked_diff_sha256": tracked_diff_sha256,
        "input_manifest": input_rehash,
        "method_manifest": method_rehash,
        "direct_hashes": direct_hashes,
        "explicit_sources_count": len(explicit_sources),
        "explicit_sources_sha256": explicit_sources_sha256,
        "v9_supersession_sha256": sha256(V9_SUPERSESSION),
        "v10_supersession_sha256": sha256(V10_SUPERSESSION),
        "v11_supersession_sha256": sha256(V11_SUPERSESSION),
        "v12_supersession_sha256": sha256(V12_SUPERSESSION),
        "v13_supersession_sha256": sha256(V13_SUPERSESSION),
        "v14_supersession_sha256": sha256(V14_SUPERSESSION),
        "verification_evidence_excluded_from_method_authority": True,
    }


def write_freeze_verification():
    if FREEZE_VERIFICATION.exists():
        raise RuntimeError("freeze verification evidence already exists")
    freeze = json.loads(SOURCE_FREEZE.read_text(encoding="utf-8"))
    value = {
        "schema": "phase4-g5-1-freeze-verification-v16",
        "status": "PASS",
        "classification": "PostFreezeIndependentRehashNotMethodAuthority",
        "bindings": current_freeze_bindings(freeze),
        "verified_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
    }
    write_json(FREEZE_VERIFICATION, value)
    verify_freeze_verification(freeze)


def verify_freeze_verification(freeze):
    value = json.loads(FREEZE_VERIFICATION.read_text(encoding="utf-8"))
    required = {
        "schema": "phase4-g5-1-freeze-verification-v16",
        "status": "PASS",
        "classification": "PostFreezeIndependentRehashNotMethodAuthority",
        "bindings": current_freeze_bindings(freeze),
    }
    if any(value.get(key) != expected for key, expected in required.items()):
        raise RuntimeError("freeze verification evidence mismatch")
    if FREEZE_VERIFICATION.stat().st_mtime_ns <= max(
        SOURCE_FREEZE.stat().st_mtime_ns,
        METHOD_MANIFEST.stat().st_mtime_ns,
        INPUT_MANIFEST.stat().st_mtime_ns,
    ):
        raise RuntimeError("freeze verification does not postdate freeze generation")
    return value


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
    chronology = evidence_root / "CHRONOLOGY-v16.jsonl"
    intent = evidence_root / f"{prefix}.intent.json"
    stdout_path = evidence_root / f"{prefix}.stdout"
    stderr_path = evidence_root / f"{prefix}.stderr"
    terminal_path = evidence_root / f"{prefix}.terminal.json"
    started_utc = datetime.datetime.now(datetime.timezone.utc).isoformat()
    started_ns = time.monotonic_ns()
    write_json(
        intent,
        {
            "schema": "phase4-g5-1-preparation-command-intent-v16",
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
        "schema": "phase4-g5-1-preparation-command-terminal-v16",
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
    if LOCK.exists() or SCREEN_RESULT.exists() or GATE_RESULT.exists() or not INPUT_ROOT.is_dir():
        raise RuntimeError("adopt-inputs requires absent lock/results and the sealed v10 input root")
    if any(
        path.exists()
        for path in (
            INPUT_MANIFEST,
            METHOD_MANIFEST,
            SOURCE_FREEZE,
            FREEZE_VERIFICATION,
            DRY_RUN,
            DRY_RUN_INTENT,
            DRY_RUN_CALIBRATION_STDOUT,
            DRY_RUN_CALIBRATION_STDERR,
            DRY_RUN_CALIBRATION_TERMINAL,
            DRY_RUN_DISPOSITION,
            DRY_RUN_FAILED,
            PREMEASUREMENT_REVISE,
            STATIC_CLOSURE,
            WRAPPER_CALIBRATION_INTENT,
            WRAPPER_CALIBRATION_RAW,
            WRAPPER_CALIBRATION_RESULT,
            WRAPPER_CALIBRATION_ROOT,
        )
    ):
        raise RuntimeError("v16 method/freeze evidence already exists")
    if not os.access(G5_CHILD_BINARY, os.X_OK):
        raise RuntimeError(f"hash-bound v16 child is not executable: {G5_CHILD_BINARY}")
    v10_rehash = independent_manifest_rehash(
        INPUT_ROOT, V10_INPUT_MANIFEST, "input_relative_path"
    )
    if (
        v10_rehash["manifest_sha256"] != V10_INPUT_MANIFEST_SHA256
        or v10_rehash["row_count"] != 93
    ):
        raise RuntimeError("v10 sealed input custody mismatch")
    if manifest_text(INPUT_ROOT, key="input_relative_path").encode() != V10_INPUT_MANIFEST.read_bytes():
        raise RuntimeError("v10 sealed input manifest reconstruction mismatch")
    root_mode = stat.S_IMODE(INPUT_ROOT.stat(follow_symlinks=False).st_mode)
    entries = sorted(INPUT_ROOT.rglob("*"))
    if (
        INPUT_ROOT.is_symlink()
        or root_mode != 0o555
        or any(path.is_symlink() for path in entries)
        or any(not (path.is_dir() or path.is_file()) for path in entries)
        or any(
            stat.S_IMODE(path.stat(follow_symlinks=False).st_mode)
            != (0o555 if path.is_dir() else 0o444)
            for path in entries
        )
    ):
        raise RuntimeError("v10 sealed input kind/mode mismatch")
    future_input = str(INPUT_MANIFEST.relative_to(REPO))
    sources = method_source_names()
    missing = [
        name
        for name in sources
        if name != future_input and not (REPO / name).is_file()
    ]
    if future_input not in sources or missing:
        raise RuntimeError(f"v16 method source preflight mismatch: {missing}")
    write_input_and_method_manifests()
    if verify_sealed_input_manifest() != 93:
        raise RuntimeError("v16 reused input manifest cardinality mismatch")
    write_freeze_verification()
    print(
        compact(
            {
                "status": "PASS",
                "classification": "HashVerifiedV10ReleaseAndSealedInputReuse",
                "input_reuse": True,
                "release_reuse": True,
                "input_root": str(INPUT_ROOT),
                "source_freeze": str(SOURCE_FREEZE),
            }
        )
    )
    return 0


def dry_run_initial_progress():
    return {
        "measured_rows": 0,
        "benchmark_child_processes_started": 0,
        "calibration_processes_started": 0,
        "stores_opened": 0,
        "base_copies_created": 0,
        "benchmark_base_copies_created": 0,
        "wrapper_calibration_samples_completed": 0,
        "wrapper_initialization_samples_completed": 0,
        "wrapper_recurring_samples_completed": 0,
        "measurement_timers_started": 0,
    }


def verify_dry_run(freeze):
    value = json.loads(DRY_RUN.read_text(encoding="utf-8"))
    intent = json.loads(DRY_RUN_INTENT.read_text(encoding="utf-8"))
    disposition = json.loads(DRY_RUN_DISPOSITION.read_text(encoding="utf-8"))
    calibration_source = INPUT_ROOT / "fixtures" / str(CALIBRATION_SIZE) / "S1-100.source"
    calibration_manifest = manifest_entry(calibration_source)
    wrapper_calibration = verify_wrapper_calibration(freeze)
    wrapper_plan = wrapper_calibration_plan()
    fixed_retained_evidence = retained_forecast_evidence()
    required = {
        "schema": "phase4-g5-1-dry-run-v16",
        "status": "PASS",
        "measured_rows": 0,
        "benchmark_child_processes_started": 0,
        "stores_opened": 0,
        "base_copies_created": WRAPPER_CALIBRATION_SAMPLES,
        "benchmark_base_copies_created": 0,
        "wrapper_calibration_samples_completed": (
            WRAPPER_INITIALIZATION_SAMPLES + WRAPPER_CALIBRATION_SAMPLES
        ),
        "wrapper_initialization_samples_completed": WRAPPER_INITIALIZATION_SAMPLES,
        "wrapper_recurring_samples_completed": WRAPPER_CALIBRATION_SAMPLES,
        "measurement_timers_started": 0,
        "gate_arm_observations": GATE_ARM_OBSERVATIONS,
        "fixed_complete_roundtrip_arms": 56,
        "source_freeze_sha256": sha256(SOURCE_FREEZE),
        "freeze_verification_sha256": sha256(FREEZE_VERIFICATION),
        "method_manifest_sha256": freeze["method_manifest_sha256"],
        "input_manifest_sha256": freeze["input_manifest_sha256"],
        "full_wrapper_limit_ns": LIMIT_NS["gate"],
        "full_wrapper_forecast_status": "PASS",
        "full_wrapper_forecast_overrun_ns": 0,
        "wrapper_calibration": wrapper_calibration,
        "wrapper_calibration_plan": wrapper_plan,
        "fixed_retained_evidence": fixed_retained_evidence,
    }
    if any(value.get(key) != expected for key, expected in required.items()):
        raise RuntimeError("dry-run custody/zero-row mismatch")
    intent_required = {
        "schema": "phase4-g5-1-dry-run-intent-v16",
        "status": "STARTED",
        "branch": BRANCH,
        "head": CHECKPOINT,
        "git_status_sha256": freeze["git_status_sha256"],
        "tracked_diff_sha256": freeze["tracked_diff_sha256"],
        "source_freeze_sha256": sha256(SOURCE_FREEZE),
        "freeze_verification_sha256": sha256(FREEZE_VERIFICATION),
        "method_manifest_sha256": freeze["method_manifest_sha256"],
        "input_manifest_sha256": freeze["input_manifest_sha256"],
        "schedule_sha256": freeze["schedule_sha256"],
        "gate_arm_observations": GATE_ARM_OBSERVATIONS,
        "fixed_complete_roundtrip_arms": 56,
        **dry_run_initial_progress(),
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
        "wrapper_calibration_plan": wrapper_plan,
        "zero_product_counters": {
            "store_opens": 0,
            "product_children_started": 0,
            "product_rows": 0,
            "locks_acquired": 0,
        },
        "fixed_retained_evidence": fixed_retained_evidence,
    }
    if any(intent.get(key) != expected for key, expected in intent_required.items()):
        raise RuntimeError("dry-run intent custody mismatch")
    if (
        DRY_RUN_FAILED.exists()
        or PREMEASUREMENT_REVISE.exists()
        or disposition.get("schema") != "phase4-g5-1-dry-run-disposition-v16"
        or disposition.get("status") != "PASS"
        or disposition.get("dry_run_sha256") != sha256(DRY_RUN)
        or disposition.get("freeze_verification_sha256")
        != sha256(FREEZE_VERIFICATION)
        or disposition.get("intent_sha256") != sha256(DRY_RUN_INTENT)
        or disposition.get("calibration_stdout_sha256")
        != sha256(DRY_RUN_CALIBRATION_STDOUT)
        or disposition.get("calibration_stderr_sha256")
        != sha256(DRY_RUN_CALIBRATION_STDERR)
        or disposition.get("calibration_terminal_sha256")
        != sha256(DRY_RUN_CALIBRATION_TERMINAL)
        or disposition.get("premeasurement_revise_sha256") is not None
        or disposition.get("wrapper_calibration") != wrapper_calibration
        or disposition.get("wrapper_calibration_plan_sha256")
        != wrapper_calibration["plan_sha256"]
        or disposition.get("fixed_retained_evidence") != fixed_retained_evidence
    ):
        raise RuntimeError("dry-run disposition custody mismatch")
    components = value.get("full_wrapper_forecast_components_ns")
    calibration = value.get("hash_calibration", {})
    observed = [
        calibration.get("python", {}).get("bytes_per_second"),
        calibration.get("external_shasum", {}).get("bytes_per_second"),
    ]
    floor = calibration.get("conservative_floor_bytes_per_second")
    expected_hash_components, expected_hash_bytes = gate_hash_bytes()
    expected_hash_forecast_ns = (
        expected_hash_bytes * 1_000_000_000 + floor - 1
    ) // floor if type(floor) is int and floor > 0 else -1
    expected_forecast_components = {
        **BASE_FORECAST_COMPONENTS_NS,
        "calibrated_one_time_wrapper_initialization": wrapper_calibration[
            "initialization_bound_ns"
        ],
        "calibrated_recurring_per_arm_wrapper_work": wrapper_calibration[
            "recurring_forecast_component_ns"
        ],
        "external_bulk_hash_bytes_at_calibrated_floor": expected_hash_forecast_ns,
    }
    if (
        not isinstance(components, dict)
        or not components
        or len(components) != 5
        or value.get("expected_gate_hash_components_bytes") != expected_hash_components
        or value.get("expected_gate_hash_bytes") != expected_hash_bytes
        or components != expected_forecast_components
        or value.get("base_forecast_components_ns") != BASE_FORECAST_COMPONENTS_NS
        or value.get("prospective_workload_counts") != gate_workload_enumeration()
        or value.get("full_wrapper_forecast_reserve_classification")
        != "RemainingTimeNotWorkAndNotTimingEvidence"
        or any(components.get(name) != number for name, number in BASE_FORECAST_COMPONENTS_NS.items())
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
    verify_freeze_verification(freeze)
    VERIFIED_INPUT_CUSTODY = input_manifest_index()
    VERIFIED_INPUT_MANIFEST_SHA256 = freeze["input_manifest_sha256"]
    if require_dry:
        verify_dry_run(freeze)
    if require_static:
        static = json.loads(STATIC_CLOSURE.read_text(encoding="utf-8"))
        screen_terminal = SCREEN_RESULT / "TERMINAL-VERIFICATION-v16.json"
        screen_final_manifest = SCREEN_RESULT / "FINAL-ARTIFACT-HASHES-v16.tsv"
        screen_final_verification = SCREEN_RESULT / "FINAL-READONLY-VERIFICATION-v16.json"
        screen_complete_wall = SCREEN_RESULT / "COMPLETE-WALL-v16.json"
        required = {
            "schema": "phase4-g5-1-static-closure-v16",
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
    prior_method_bytes = sum(
        int(row["bytes"])
        for manifest in (
            V10_METHOD_MANIFEST, V11_METHOD_MANIFEST, V12_METHOD_MANIFEST,
            V13_METHOD_MANIFEST,
        )
        for row in read_tsv(manifest)
    )
    repository_identity_bytes = sum((REPO / name).stat().st_size for name in CONTROLLING_HASHES)
    repository_identity_bytes += (
        G4_EXECUTABLE.stat().st_size
        + G4_FINAL_MANIFEST.stat().st_size
        + G4_ARM_RAW.stat().st_size
    )
    preserved_history_bytes = (
        V7_PREMEASUREMENT.stat().st_size
        + V7_PREPARATION_AUDIT.stat().st_size
        + V8_PREMEASUREMENT.stat().st_size
        + V9_PREMEASUREMENT.stat().st_size
        + V9_SUPERSESSION.stat().st_size
        + V10_PREMEASUREMENT.stat().st_size
        + V10_DISPOSITION.stat().st_size
        + V10_SUPERSESSION.stat().st_size
        + V11_PREMEASUREMENT.stat().st_size
        + V11_DISPOSITION.stat().st_size
        + V11_SUPERSESSION.stat().st_size
        + V12_PREMEASUREMENT.stat().st_size
        + V12_POST_DRY_FAILURE.stat().st_size
        + V12_DISPOSITION.stat().st_size
        + V12_SUPERSESSION.stat().st_size
        + V13_PREMEASUREMENT.stat().st_size
        + V13_SCREEN_FAILURE.stat().st_size
        + V13_DISPOSITION.stat().st_size
        + V13_LOCK_RELEASE.stat().st_size
        + V13_LOCK_ATTESTATION.stat().st_size
        + V13_SUPERSESSION.stat().st_size
    )
    preserved_history_bytes += sum(
        path.stat().st_size for path in V7_PARTIAL_INPUT_ROOT.rglob("*") if path.is_file()
    )
    direct_freeze_bytes = (
        METHOD_MANIFEST.stat().st_size
        + INPUT_MANIFEST.stat().st_size
        + SOURCE_FREEZE.stat().st_size
        + FREEZE_VERIFICATION.stat().st_size
        + G5_CHILD_BINARY.stat().st_size
    )
    observations = expanded_observations("gate")
    if len(observations) != GATE_ARM_OBSERVATIONS:
        raise RuntimeError("gate hash forecast arm count mismatch")
    operand_recheck_bytes = 3 * (G4_EXECUTABLE.stat().st_size + G5_CHILD_BINARY.stat().st_size)
    components = {
        "repository_identity": repository_identity_bytes,
        "preserved_v7_v8_v9_v10_v11_v12_v13_failure_evidence": preserved_history_bytes,
        "explicit_method_sources_four_passes": explicit_bytes * 4,
        "prior_method_manifest_rows_four_passes": prior_method_bytes * 4,
        "direct_freeze_files": direct_freeze_bytes,
        "sealed_input_manifest_two_preflight_passes": input_bytes * 2,
        "operand_copy_custody_and_terminal_rechecks": operand_recheck_bytes,
    }
    return components, sum(components.values())


def retained_forecast_evidence():
    scoreboard_relative = "implementation-detail/phase-4/baseline/current-benchmark-scoreboard.md"
    verify_file(REPO / scoreboard_relative, CONTROLLING_HASHES[scoreboard_relative])
    verify_file(G4_ARM_RAW, G4_ARM_RAW_SHA256)
    scoreboard = (REPO / scoreboard_relative).read_text(encoding="utf-8")
    if "Fresh-process authenticated reconstruction | **237.381 ms" not in scoreboard:
        raise RuntimeError("retained G4 rounded child-arm source value drift")
    retained_rows = [
        json.loads(line)
        for line in G4_ARM_RAW.read_text(encoding="utf-8").splitlines()
        if line
    ]
    matches = [
        row
        for row in retained_rows
        if row.get("record") == "guard-same-count-edit-100m"
        and row.get("role") == "protected-candidate"
    ]
    if (
        len(matches) != 1
        or matches[0].get("payload", {}).get("reconstruction_wall_ns")
        != RETAINED_G4_FRESH_RECONSTRUCTION_MAX_NS
    ):
        raise RuntimeError("retained G4 checkpoint reconstruction evidence drift")
    foundation_relative = (
        "implementation-detail/phase-4/experiments/g5-foundation-h11/v9/"
        "G5-0-TERMINAL-AUDIT-v9.json"
    )
    verify_file(REPO / foundation_relative, CONTROLLING_HASHES[foundation_relative])
    foundation = json.loads((REPO / foundation_relative).read_text(encoding="utf-8"))
    if (
        foundation.get("status") != "PASS"
        or foundation.get("complete_wall_ns")
        != RETAINED_G5_FOUNDATION_COMPLETE_WALL_NS
        or FIXED_CAMPAIGN_FINALIZATION_NS < RETAINED_G5_FOUNDATION_COMPLETE_WALL_NS
    ):
        raise RuntimeError("retained G5-0 campaign-finalization evidence drift")
    return {
        "child_arm_ns": RETAINED_G4_ROUNDED_CHILD_ARM_NS,
        "classification": "RoundedUpRetainedG4FreshProcess237.381ms",
        "source": scoreboard_relative,
        "source_sha256": CONTROLLING_HASHES[scoreboard_relative],
        "checkpoint_reconstruction_ns": RETAINED_G4_FRESH_RECONSTRUCTION_MAX_NS,
        "checkpoint_source": str(G4_ARM_RAW.relative_to(REPO)),
        "checkpoint_source_sha256": G4_ARM_RAW_SHA256,
        "checkpoint_record": "guard-same-count-edit-100m",
        "checkpoint_role": "protected-candidate",
        "checkpoint_field": "payload.reconstruction_wall_ns",
        "campaign_finalization_ns": FIXED_CAMPAIGN_FINALIZATION_NS,
        "campaign_finalization_classification": "RoundedUpRetainedG5FoundationCompleteWall",
        "campaign_finalization_inference": "ProspectiveAllowanceNotProvenUpperBound",
        "campaign_finalization_source_ns": RETAINED_G5_FOUNDATION_COMPLETE_WALL_NS,
        "campaign_finalization_source": foundation_relative,
        "campaign_finalization_source_sha256": CONTROLLING_HASHES[foundation_relative],
        "campaign_finalization_scope": "lock-analyzers-manifests-terminal-not-per-arm",
    }


def gate_workload_enumeration():
    if VERIFIED_INPUT_CUSTODY is None:
        raise RuntimeError("input manifest has not been preverified")
    observations = expanded_observations("gate")
    sequences = consecutive_observation_groups(observations, ("sequence_id",))
    g4_arms = sum(row["role"] == "g4_verified" for row in observations)
    g5_requests = sum(row["role"].startswith("g5_") for row in observations)
    g5_sessions = sum(
        len({row["role"] for row in sequence if row["role"].startswith("g5_")})
        for sequence in sequences
    )
    clonefile_calls = 0
    clonefile_bytes = 0
    clone_directory_fsyncs = 0
    for observation in observations:
        master = master_path(observation)
        files = [path for path in master.rglob("*") if path.is_file()]
        directories = [path for path in master.rglob("*") if path.is_dir()]
        clonefile_calls += len(files)
        clonefile_bytes += sum(manifest_entry(path)["bytes"] for path in files)
        clone_directory_fsyncs += len(directories) + 2
    checkpoint_count = sum(row["fixed_checkpoint"] for row in observations)
    return {
        "schema": "phase4-g5-1-prospective-workload-v16",
        "gate_arm_observations": len(observations),
        "child_arm_observations": len(observations),
        "g4_one_shot_product_children": g4_arms,
        "g5_persistent_product_children": g5_sessions,
        "g5_persistent_row_requests": g5_requests,
        "fixed_complete_roundtrip_validations": checkpoint_count,
        "prearm_wrapper_initializations": 1,
        "prearm_wrapper_initialization_action_counts": (
            wrapper_initialization_planned_actions()
        ),
        "clonefile_calls": clonefile_calls,
        "clonefile_bytes": clonefile_bytes,
        "clonefile_content_classification": CLONE_COPY_CONTENT,
        "clonefile_content_hash_bytes": 0,
        "cloned_file_fsync_calls": clonefile_calls,
        "clone_directory_fsync_calls": clone_directory_fsyncs,
        "immediate_cleanup_roots": len(observations),
        "immediate_cleanup_parent_fsync_calls": len(observations),
        "inventory_enumerations": len(observations) * 4,
        "published_visible_state": {
            "invocations": len(observations),
            "database_discovery_enumerations": len(observations),
            "constant_head_rows": len(observations),
            "constant_head_receipt_bytes": len(observations) * 216,
            "physical_pragma_observations": len(observations),
            "query_only_pragma_queries": len(observations) * 2,
            "physical_pragma_queries": len(observations) * 3,
            "sqlite_schema_rootpage_queries": len(observations),
            "sqlite_schema_rootpage_rows": len(observations) * 3,
            "ordered_object_all_row_scans": 0,
            "all_object_table_catalog_parity": ALL_ROW_CATALOG_PARITY,
            "reachable_published_result_parity": "ClaimedHardGated",
        },
        "small_sidecar_sha256_calls": len(observations) * 2,
        "chronology_file_fsync_calls": len(observations),
        "chronology_directory_fsync_calls": len(observations),
        "persistent_transport_line_file_fsync_calls": g5_requests + 2 * g5_sessions,
        "persistent_transport_line_directory_fsync_calls": g5_requests + 2 * g5_sessions,
        "persistent_terminal_sidecar_file_fsync_calls": 2 * g5_sessions,
        "persistent_terminal_sidecar_directory_fsync_calls": 2 * g5_sessions,
        "g4_one_shot_evidence_file_fsync_calls": g4_arms * 3,
        "g4_one_shot_evidence_directory_fsync_calls": g4_arms * 3,
        "runner_fsync_call_enumeration": {
            "preflight_file_calls": 6,
            "preflight_directory_calls": 7,
            "prearm_initialization_file_calls": 2,
            "prearm_initialization_directory_calls": 2,
            "clone_file_calls": clonefile_calls,
            "clone_directory_calls": clone_directory_fsyncs,
            "immediate_cleanup_directory_calls": len(observations),
            "chronology_file_calls": len(observations),
            "chronology_directory_calls": len(observations),
            "persistent_transport_file_calls": g5_requests + 4 * g5_sessions,
            "persistent_transport_directory_calls": g5_requests + 4 * g5_sessions,
            "g4_evidence_file_calls": g4_arms * 3,
            "g4_evidence_directory_calls": g4_arms * 3,
            "preanalysis_fixed_file_calls": 6,
            "preanalysis_fixed_directory_calls": 6,
            "postanalysis_fixed_file_calls": 12,
            "postanalysis_fixed_directory_calls": 14,
            "classification": "ExactRunnerCallsProductAndAnalyzerInternalFsyncsExcluded",
        },
        "analyzer_invocations": 2,
        "analyzer_output_file_fsync_calls": 2,
        "payload_manifest_generations": 1,
        "payload_manifest_verifications": 2,
        "final_manifest_generations": 1,
        "final_manifest_verifications": 1,
        "measured_terminal_writes": 1,
        "terminal_verification_writes": 1,
        "complete_wall_writes": 1,
        "retained_timing_evidence": retained_forecast_evidence(),
        "wrapper_work_classification": "SeparatelyCalibratedZeroProductPerArmComponent",
        "remaining_reserve_classification": "NotWorkAndNotTimingEvidence",
    }


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
            "schema": "phase4-g5-1-dry-run-calibration-terminal-v16",
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
        "schema": "phase4-g5-1-hash-calibration-v16",
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


def wrapper_calibration_plan():
    masters = {
        master_path(observation) for observation in expanded_observations("gate")
    }
    candidates = []
    for master in masters:
        files = sorted(path for path in master.rglob("*") if path.is_file())
        directories = sorted(path for path in master.rglob("*") if path.is_dir())
        candidates.append(
            {
                "relative_path": str(master.relative_to(INPUT_ROOT)),
                "size_bytes": CALIBRATION_SIZE,
                "file_count": len(files),
                "total_manifest_bytes": sum(
                    manifest_entry(path)["bytes"] for path in files
                ),
                "directory_count": len(directories),
            }
        )
    if not candidates:
        raise RuntimeError("wrapper calibration has no 100-MiB prepared master")
    for candidate in candidates:
        candidate["dominance_tuple"] = [
            candidate["file_count"],
            candidate["total_manifest_bytes"],
            candidate["directory_count"],
            candidate["relative_path"],
        ]
    candidates.sort(key=lambda value: value["relative_path"])
    selected = max(candidates, key=lambda value: tuple(value["dominance_tuple"]))
    plan = {
        "schema": "phase4-g5-1-wrapper-calibration-plan-v16",
        "candidate_table": candidates,
        "candidate_count": len(candidates),
        "selection_rule": (
            "lexicographic-max(file_count,total_manifest_bytes,directory_count,relative_path)"
        ),
        "selected_master": selected,
        "initialization_sample_count": WRAPPER_INITIALIZATION_SAMPLES,
        "recurring_sample_count": WRAPPER_CALIBRATION_SAMPLES,
        "planned_initialization_actions": wrapper_initialization_planned_actions(),
    }
    plan["planned_actions_per_sample"] = wrapper_calibration_planned_actions(selected)
    return plan


def wrapper_initialization_planned_actions():
    return {
        "database_discovery_enumerations": 1,
        "published_visible_state_invocations": 1,
        "published_visible_head_rows": 1,
        "published_visible_head_receipt_bytes": 216,
        "query_only_pragma_queries": 2,
        "physical_pragma_queries": 3,
        "sqlite_schema_rootpage_queries": 1,
        "sqlite_schema_rootpage_rows": 3,
        "ordered_object_all_row_scans": 0,
        "initialization_evidence_write_calls": 1,
        "initialization_evidence_file_fsync_calls": 1,
        "initialization_evidence_directory_fsync_calls": 1,
        "store_opens": 0,
        "product_children_started": 0,
        "product_rows": 0,
        "locks_acquired": 0,
    }


def wrapper_calibration_planned_actions(master):
    return {
        "clonefile_calls": master["file_count"],
        "clonefile_bytes": master["total_manifest_bytes"],
        "clonefile_content_hash_bytes": 0,
        "clonefile_content_classification": CLONE_COPY_CONTENT,
        "cloned_file_fsync_calls": master["file_count"],
        "clone_directory_fsync_calls": master["directory_count"] + 2,
        "inventory_enumerations": 4,
        "database_discovery_enumerations": 1,
        "published_visible_state_invocations": 1,
        "published_visible_head_rows": 1,
        "published_visible_head_receipt_bytes": 216,
        "query_only_pragma_queries": 2,
        "physical_pragma_queries": 3,
        "sqlite_schema_rootpage_queries": 1,
        "sqlite_schema_rootpage_rows": 3,
        "ordered_object_all_row_scans": 0,
        "mutation_work_evidence_assemblies": 1,
        "wrapper_evidence_assemblies": 1,
        "sidecar_sha256_calls": 2,
        "representative_evidence_append_calls": 1,
        "representative_evidence_file_fsync_calls": 1,
        "representative_evidence_directory_fsync_calls": 1,
        "immediate_cleanup_roots": 1,
        "immediate_cleanup_parent_fsync_calls": 1,
        "store_opens": 0,
        "product_children_started": 0,
        "product_rows": 0,
        "locks_acquired": 0,
    }


def wrapper_calibration_forecast(initialization_total_ns, sample_totals_ns):
    if (
        type(initialization_total_ns) is not int
        or initialization_total_ns <= 0
        or len(sample_totals_ns) != WRAPPER_CALIBRATION_SAMPLES
        or any(type(value) is not int or value <= 0 for value in sample_totals_ns)
    ):
        raise RuntimeError("wrapper calibration sample totals mismatch")
    maximum = max(sample_totals_ns)
    per_arm_bound_ns = maximum * WRAPPER_CALIBRATION_CONSERVATIVE_FACTOR
    initialization_bound_ns = (
        initialization_total_ns * WRAPPER_CALIBRATION_CONSERVATIVE_FACTOR
    )
    recurring_component_ns = GATE_ARM_OBSERVATIONS * per_arm_bound_ns
    return {
        "initialization_sample_total_ns": initialization_total_ns,
        "initialization_bound_ns": initialization_bound_ns,
        "max_recurring_sample_total_ns": maximum,
        "conservative_factor": WRAPPER_CALIBRATION_CONSERVATIVE_FACTOR,
        "recurring_per_arm_bound_ns": per_arm_bound_ns,
        "gate_arm_observations": GATE_ARM_OBSERVATIONS,
        "recurring_forecast_component_ns": recurring_component_ns,
        "forecast_component_ns": initialization_bound_ns + recurring_component_ns,
    }


def elapsed_action(action):
    started = time.monotonic_ns()
    value = action()
    return value, max(1, time.monotonic_ns() - started)


def run_wrapper_calibration(freeze, plan):
    if any(
        path.exists()
        for path in (
            WRAPPER_CALIBRATION_INTENT,
            WRAPPER_CALIBRATION_RAW,
            WRAPPER_CALIBRATION_RESULT,
            WRAPPER_CALIBRATION_ROOT,
        )
    ):
        raise RuntimeError("wrapper calibration requires absent evidence and root")
    if LOCK.exists() or SCREEN_RESULT.exists() or GATE_RESULT.exists():
        raise RuntimeError("wrapper calibration requires absent lock and campaign roots")
    if VERIFIED_INPUT_CUSTODY is None or VERIFIED_INPUT_MANIFEST_SHA256 is None:
        raise RuntimeError("wrapper calibration requires verified frozen inputs")
    expected_plan = wrapper_calibration_plan()
    if plan != expected_plan:
        raise RuntimeError("wrapper calibration plan drift")
    master = plan["selected_master"]
    planned_actions = plan["planned_actions_per_sample"]
    initialization_actions = plan["planned_initialization_actions"]
    intent = {
        "schema": "phase4-g5-1-wrapper-calibration-intent-v16",
        "status": "STARTED",
        "classification": "ZeroProductWrapperCalibration",
        "source_freeze_sha256": sha256(SOURCE_FREEZE),
        "freeze_verification_sha256": sha256(FREEZE_VERIFICATION),
        "input_manifest_sha256": freeze["input_manifest_sha256"],
        "calibration_root": str(WRAPPER_CALIBRATION_ROOT.relative_to(REPO)),
        "calibration_root_absent_before": True,
        "global_lock_absent_before": True,
        "plan": plan,
        "initialization_sample_count": WRAPPER_INITIALIZATION_SAMPLES,
        "recurring_sample_count": WRAPPER_CALIBRATION_SAMPLES,
        "conservative_factor": WRAPPER_CALIBRATION_CONSERVATIVE_FACTOR,
        "gate_arm_observations": GATE_ARM_OBSERVATIONS,
        "planned_actions_per_sample": planned_actions,
        "planned_initialization_actions": initialization_actions,
        "zero_product_counters": {
            "store_opens": 0,
            "product_children_started": 0,
            "product_rows": 0,
            "locks_acquired": 0,
        },
    }
    write_json(WRAPPER_CALIBRATION_INTENT, intent)
    WRAPPER_CALIBRATION_ROOT.mkdir(mode=0o700)
    fsync_dir(WRAPPER_CALIBRATION_ROOT.parent)
    root_lifecycle = ArmWorkRootLifecycle(WRAPPER_CALIBRATION_ROOT)
    samples = []
    representative_records = []
    representative_path = WRAPPER_CALIBRATION_ROOT / "representative-arm-evidence.jsonl"
    initialization_evidence_path = (
        WRAPPER_CALIBRATION_ROOT / "initialization-evidence.jsonl"
    )
    try:
        initialization_started = time.monotonic_ns()
        initialization_databases = sorted(
            path
            for path in (INPUT_ROOT / master["relative_path"]).rglob("*.sqlite")
            if path.is_file()
        )
        if len(initialization_databases) != 1:
            raise RuntimeError("wrapper initialization requires one sealed SQLite database")
        initialization_database = initialization_databases[0]
        (initialization_state, initialization_physical), initialization_query_ns = (
            elapsed_action(
                lambda: published_visible_state(
                    initialization_database, None, "CaptureOnly"
                )
            )
        )
        initialization_evidence = {
            "schema": "phase4-g5-1-wrapper-initialization-evidence-v16",
            "classification": "CalibrationFirstUseInitializationNotProductAuthority",
            "master": master["relative_path"],
            "database": str(initialization_database.relative_to(INPUT_ROOT)),
            "database_manifest": manifest_entry(initialization_database),
            "action_counts": initialization_actions,
            "rooted_state": initialization_state,
            "physical_allocation_observation": initialization_physical,
        }
        _, initialization_write_ns = elapsed_action(
            lambda: append_text(
                initialization_evidence_path,
                compact(initialization_evidence) + "\n",
            )
        )
        initialization_measured_ns = max(
            1, time.monotonic_ns() - initialization_started
        )
        initialization_phases = {
            "published_visible_state_ns": initialization_query_ns,
            "initialization_evidence_append_fsync_ns": initialization_write_ns,
        }
        initialization_phase_sum = sum(initialization_phases.values())
        initialization_total_ns = max(
            initialization_measured_ns, initialization_phase_sum
        )
        initialization_phases["timer_accounting_residual_ns"] = (
            initialization_total_ns - initialization_phase_sum
        )
        initialization = {
            "schema": "phase4-g5-1-wrapper-initialization-sample-v16",
            "status": "PASS",
            "sample_class": "OneTimeProcessInitialization",
            "master": master["relative_path"],
            "phases_ns": initialization_phases,
            "measured_total_ns": initialization_measured_ns,
            "total_ns": initialization_total_ns,
            "phase_sum_matches": sum(initialization_phases.values())
            == initialization_total_ns,
            "action_counts": initialization_actions,
            "rooted_state": initialization_state,
            "physical_classification": initialization_physical["classification"],
            "physical_schema_rootpage_rows": len(
                initialization_physical["sqlite_schema_rootpages"]
            ),
            "evidence": initialization_evidence,
            "evidence_sha256": sha256_bytes(
                compact(initialization_evidence).encode()
            ),
        }
        append_text(WRAPPER_CALIBRATION_RAW, compact(initialization) + "\n")
        for sample_index in range(1, WRAPPER_CALIBRATION_SAMPLES + 1):
            sample_root = WRAPPER_CALIBRATION_ROOT / f"sample-{sample_index}"
            root_lifecycle.begin(sample_root)
            total_started = time.monotonic_ns()
            clone_receipt, clone_ns = elapsed_action(
                lambda: clone_master_attested(INPUT_ROOT / master["relative_path"], sample_root)
            )
            allowed_inventory, allowed_inventory_ns = elapsed_action(
                lambda: exact_inventory(sample_root)
            )
            databases = sorted(
                path for path in sample_root.rglob("*.sqlite") if path.is_file()
            )
            if len(databases) != 1:
                raise RuntimeError("wrapper calibration requires one cloned SQLite database")
            database = databases[0]
            (rooted_state, physical), published_visible_state_ns = elapsed_action(
                lambda: published_visible_state(database, None, "CaptureOnly")
            )

            def sidecar_hashes():
                authority = pathlib.Path(f"{database}.authority")
                expectations = pathlib.Path(f"{database}.expectations")
                return {
                    "authority": {
                        "sha256": sha256(authority),
                        "bytes": authority.stat().st_size,
                    },
                    "expectations": {
                        "sha256": sha256(expectations),
                        "bytes": expectations.stat().st_size,
                    },
                }

            sidecars, sidecar_hash_ns = elapsed_action(sidecar_hashes)
            post_inventory, post_inventory_ns = elapsed_action(
                lambda: exact_inventory(sample_root)
            )
            if allowed_inventory != post_inventory:
                raise RuntimeError("wrapper calibration mutated cloned inventory")
            calibration_product = {
                **{name: 0 for name in MUTATION_WORK_FIELDS},
                "publication_status": "CalibrationShapeOnly",
                "root_id": rooted_state["head_root_id"],
                "transition_id": rooted_state["head_transition_id"],
            }
            (work, work_sha256), mutation_work_ns = elapsed_action(
                lambda: mutation_work_evidence(calibration_product)
            )
            wrapper_evidence, wrapper_assembly_ns = elapsed_action(
                lambda: assemble_post_row_evidence(
                    rooted_state,
                    physical,
                    sidecars["authority"],
                    sidecars["expectations"],
                    work,
                    work_sha256,
                    allowed_inventory,
                    post_inventory,
                )
            )
            action_counts = dict(planned_actions)
            representative = {
                "schema": "phase4-g5-1-wrapper-calibration-representative-arm-v16",
                "sample": sample_index,
                "master": master["relative_path"],
                "action_counts": action_counts,
                "rooted_state_schema": rooted_state["schema"],
                "head_receipt_sha256": rooted_state["head_receipt_sha256"],
                "all_object_table_catalog_parity": rooted_state[
                    "all_object_table_catalog_parity"
                ],
                "physical_classification": physical["classification"],
                "physical_schema_rootpage_rows": len(
                    physical["sqlite_schema_rootpages"]
                ),
                "wrapper_evidence": wrapper_evidence,
            }
            _, evidence_append_ns = elapsed_action(
                lambda: append_text(
                    representative_path, compact(representative) + "\n"
                )
            )
            receipt, cleanup_ns = elapsed_action(
                lambda: root_lifecycle.cleanup(
                    sample_root,
                    {
                        "ordinal": sample_index,
                        "sequence_id": "WRAPPER-CALIBRATION",
                        "pair": sample_index,
                        "role": "zero-product-wrapper",
                    },
                    post_inventory,
                    classification="WrapperCalibrationImmediateCleanup",
                )
            )
            measured_total_ns = max(1, time.monotonic_ns() - total_started)
            phases = {
                "clone_master_attested_ns": clone_ns,
                "allowed_inventory_ns": allowed_inventory_ns,
                "published_visible_state_ns": published_visible_state_ns,
                "sidecar_sha256_ns": sidecar_hash_ns,
                "post_inventory_ns": post_inventory_ns,
                "mutation_work_evidence_ns": mutation_work_ns,
                "wrapper_evidence_assembly_ns": wrapper_assembly_ns,
                "representative_evidence_append_fsync_ns": evidence_append_ns,
                "immediate_cleanup_parent_fsync_ns": cleanup_ns,
            }
            phase_sum = sum(phases.values())
            accounted_total_ns = max(measured_total_ns, phase_sum)
            phases["timer_accounting_residual_ns"] = accounted_total_ns - phase_sum
            sample = {
                "schema": "phase4-g5-1-wrapper-calibration-sample-v16",
                "status": "PASS",
                "sample": sample_index,
                "master": master["relative_path"],
                "phases_ns": phases,
                "measured_total_ns": measured_total_ns,
                "total_ns": accounted_total_ns,
                "phase_sum_matches": sum(phases.values()) == accounted_total_ns,
                "action_counts": action_counts,
                "clone_receipt_entries": len(clone_receipt["entries"]),
                "clone_receipt_bytes": sum(
                    entry["bytes"] for entry in clone_receipt["entries"]
                ),
                "rooted_state": rooted_state,
                "physical_classification": physical["classification"],
                "physical_schema_rootpage_rows": len(
                    physical["sqlite_schema_rootpages"]
                ),
                "sidecar_evidence": sidecars,
                "wrapper_evidence_sha256": sha256_bytes(
                    compact(wrapper_evidence).encode()
                ),
                "cleanup_receipt": receipt,
                "active_row_roots_after_sample": int(
                    root_lifecycle.active_row_root is not None
                ),
            }
            append_text(WRAPPER_CALIBRATION_RAW, compact(sample) + "\n")
            samples.append(sample)
            representative_records.append(representative)
        representative_bytes = representative_path.read_bytes()
        root_snapshot = root_lifecycle.terminal_snapshot(WRAPPER_CALIBRATION_SAMPLES)
    finally:
        try:
            if root_lifecycle.active_row_root is not None:
                root_lifecycle.cleanup_active_failure()
        finally:
            if WRAPPER_CALIBRATION_ROOT.exists():
                if (
                    WRAPPER_CALIBRATION_ROOT.parent != REPO / "target"
                    or WRAPPER_CALIBRATION_ROOT.name
                    != f"phase4-g5-trusted-reopen-edit-wrapper-calibration-{DATE}-v16"
                ):
                    raise RuntimeError("refusing unsafe wrapper calibration cleanup")
                shutil.rmtree(WRAPPER_CALIBRATION_ROOT)
                fsync_dir(WRAPPER_CALIBRATION_ROOT.parent)
    if WRAPPER_CALIBRATION_ROOT.exists():
        raise RuntimeError("wrapper calibration root remained after calibration")
    forecast = wrapper_calibration_forecast(
        initialization["total_ns"], [sample["total_ns"] for sample in samples]
    )
    result = {
        "schema": "phase4-g5-1-wrapper-calibration-result-v16",
        "status": "PASS",
        "classification": "ZeroProductWrapperCalibration",
        "intent_sha256": sha256(WRAPPER_CALIBRATION_INTENT),
        "raw_sha256": sha256(WRAPPER_CALIBRATION_RAW),
        "initialization_sample_count": WRAPPER_INITIALIZATION_SAMPLES,
        "recurring_sample_count": len(samples),
        "samples_sha256": sha256_bytes(compact([initialization, *samples]).encode()),
        "initialization_sample": initialization,
        "initialization_action_counts": initialization_actions,
        "action_counts_per_sample": samples[0]["action_counts"],
        "actions_equal_across_samples": all(
            sample["action_counts"] == samples[0]["action_counts"]
            for sample in samples
        ),
        **forecast,
        "calibration_root_absent": True,
        "global_lock_absent": not LOCK.exists(),
        "representative_evidence_sha256": sha256_bytes(representative_bytes),
        "representative_records": representative_records,
        "work_root_lifecycle": root_snapshot,
        "zero_product_counters": intent["zero_product_counters"],
        "fixed_retained_evidence": retained_forecast_evidence(),
    }
    if result["actions_equal_across_samples"] is not True:
        raise RuntimeError("wrapper calibration action counts changed across samples")
    write_json(WRAPPER_CALIBRATION_RESULT, result)
    return verify_wrapper_calibration(freeze)


def wrapper_calibration_lock_evidence_valid(
    historical_global_lock_absent,
    historical_locks_acquired,
    calibration_root_exists,
    _current_lock_exists,
):
    return (
        historical_global_lock_absent is True
        and historical_locks_acquired == 0
        and not calibration_root_exists
    )


def verify_wrapper_calibration(freeze):
    intent = json.loads(WRAPPER_CALIBRATION_INTENT.read_text(encoding="utf-8"))
    records = [
        json.loads(line)
        for line in WRAPPER_CALIBRATION_RAW.read_text(encoding="utf-8").splitlines()
        if line
    ]
    if len(records) != WRAPPER_INITIALIZATION_SAMPLES + WRAPPER_CALIBRATION_SAMPLES:
        raise RuntimeError("wrapper calibration record cardinality mismatch")
    initialization = records[0]
    samples = records[1:]
    result = json.loads(WRAPPER_CALIBRATION_RESULT.read_text(encoding="utf-8"))
    plan = wrapper_calibration_plan()
    master = plan["selected_master"]
    planned_actions = plan["planned_actions_per_sample"]
    initialization_actions = plan["planned_initialization_actions"]
    zero_product = {
        "store_opens": 0,
        "product_children_started": 0,
        "product_rows": 0,
        "locks_acquired": 0,
    }
    intent_required = {
        "schema": "phase4-g5-1-wrapper-calibration-intent-v16",
        "status": "STARTED",
        "classification": "ZeroProductWrapperCalibration",
        "source_freeze_sha256": sha256(SOURCE_FREEZE),
        "freeze_verification_sha256": sha256(FREEZE_VERIFICATION),
        "input_manifest_sha256": freeze["input_manifest_sha256"],
        "calibration_root": str(WRAPPER_CALIBRATION_ROOT.relative_to(REPO)),
        "calibration_root_absent_before": True,
        "global_lock_absent_before": True,
        "plan": plan,
        "initialization_sample_count": WRAPPER_INITIALIZATION_SAMPLES,
        "recurring_sample_count": WRAPPER_CALIBRATION_SAMPLES,
        "conservative_factor": WRAPPER_CALIBRATION_CONSERVATIVE_FACTOR,
        "gate_arm_observations": GATE_ARM_OBSERVATIONS,
        "planned_actions_per_sample": planned_actions,
        "planned_initialization_actions": initialization_actions,
        "zero_product_counters": zero_product,
    }
    if intent != intent_required:
        raise RuntimeError("wrapper calibration intent mismatch")
    raw_reconstructed = "".join(compact(record) + "\n" for record in records).encode()
    if (
        WRAPPER_CALIBRATION_RAW.read_bytes() != raw_reconstructed
    ):
        raise RuntimeError("wrapper calibration sample cardinality mismatch")
    initialization_phases = initialization.get("phases_ns")
    if (
        initialization.get("schema")
        != "phase4-g5-1-wrapper-initialization-sample-v16"
        or initialization.get("status") != "PASS"
        or initialization.get("sample_class") != "OneTimeProcessInitialization"
        or initialization.get("master") != master["relative_path"]
        or not isinstance(initialization_phases, dict)
        or set(initialization_phases)
        != {
            "published_visible_state_ns",
            "initialization_evidence_append_fsync_ns",
            "timer_accounting_residual_ns",
        }
        or any(
            type(value) is not int or value < 0
            for value in initialization_phases.values()
        )
        or type(initialization.get("total_ns")) is not int
        or initialization["total_ns"] <= 0
        or sum(initialization_phases.values()) != initialization["total_ns"]
        or initialization.get("phase_sum_matches") is not True
        or initialization.get("action_counts") != initialization_actions
        or initialization.get("rooted_state", {}).get("semantics")
        != "CalibrationConstantRowShapeNotProductAuthority"
        or initialization.get("rooted_state", {}).get("closure_provenance")
        != "CalibrationShapeOnlyNoProductParity"
        or initialization.get("rooted_state", {}).get(
            "reachable_published_result_parity"
        )
        != "NotClaimedCalibrationShapeOnly"
        or initialization.get("physical_classification")
        != PHYSICAL_ALLOCATION_CLASSIFICATION
        or initialization.get("physical_schema_rootpage_rows") != 3
        or sha256_bytes(compact(initialization.get("evidence")).encode())
        != initialization.get("evidence_sha256")
        or initialization.get("evidence", {}).get("action_counts")
        != initialization_actions
    ):
        raise RuntimeError("wrapper initialization sample mismatch")
    dynamic_actions = None
    for sample_index, sample in enumerate(samples, start=1):
        phases = sample.get("phases_ns")
        actions = sample.get("action_counts")
        if dynamic_actions is None:
            dynamic_actions = actions
        if (
            sample.get("schema") != "phase4-g5-1-wrapper-calibration-sample-v16"
            or sample.get("status") != "PASS"
            or sample.get("sample") != sample_index
            or sample.get("master") != master["relative_path"]
            or not isinstance(phases, dict)
            or set(phases) != {
                "clone_master_attested_ns",
                "allowed_inventory_ns",
                "published_visible_state_ns",
                "sidecar_sha256_ns",
                "post_inventory_ns",
                "mutation_work_evidence_ns",
                "wrapper_evidence_assembly_ns",
                "representative_evidence_append_fsync_ns",
                "immediate_cleanup_parent_fsync_ns",
                "timer_accounting_residual_ns",
            }
            or any(type(value) is not int or value < 0 for value in phases.values())
            or type(sample.get("measured_total_ns")) is not int
            or sample.get("measured_total_ns") <= 0
            or type(sample.get("total_ns")) is not int
            or sample.get("total_ns") <= 0
            or sum(phases.values()) != sample.get("total_ns")
            or sample.get("phase_sum_matches") is not True
            or not isinstance(actions, dict)
            or set(actions) != set(planned_actions)
            or any(actions.get(key) != value for key, value in planned_actions.items())
            or actions != dynamic_actions
            or actions.get("published_visible_state_invocations") != 1
            or actions.get("published_visible_head_rows") != 1
            or actions.get("published_visible_head_receipt_bytes") != 216
            or actions.get("query_only_pragma_queries") != 2
            or actions.get("physical_pragma_queries") != 3
            or actions.get("sqlite_schema_rootpage_queries") != 1
            or actions.get("sqlite_schema_rootpage_rows") != 3
            or actions.get("ordered_object_all_row_scans") != 0
            or actions.get("mutation_work_evidence_assemblies") != 1
            or actions.get("wrapper_evidence_assemblies") != 1
            or sample.get("clone_receipt_entries") != master["file_count"]
            or sample.get("clone_receipt_bytes") != master["total_manifest_bytes"]
            or sample.get("physical_classification") != PHYSICAL_ALLOCATION_CLASSIFICATION
            or sample.get("physical_schema_rootpage_rows") != 3
            or sample.get("rooted_state", {}).get("schema") != ROOTED_STATE_SCHEMA
            or sample.get("rooted_state", {}).get("all_object_table_catalog_parity")
            != ALL_ROW_CATALOG_PARITY
            or sample.get("rooted_state", {}).get("closure_provenance")
            != "CalibrationShapeOnlyNoProductParity"
            or sample.get("rooted_state", {}).get("reachable_published_result_parity")
            != "NotClaimedCalibrationShapeOnly"
            or sample.get("rooted_state", {}).get("head_receipt_semantics")
            != "CalibrationOpaqueHeadReceiptHashNotClosureOrFreshness"
            or sample.get("rooted_state", {}).get("semantics")
            != "CalibrationConstantRowShapeNotProductAuthority"
            or len(sample.get("physical_classification", "")) == 0
            or sample.get("active_row_roots_after_sample") != 0
            or sample.get("cleanup_receipt", {}).get("status") != "PASS"
            or sample.get("cleanup_receipt", {}).get("classification")
            != "WrapperCalibrationImmediateCleanup"
        ):
            raise RuntimeError(f"wrapper calibration sample mismatch: {sample_index}")
    forecast = wrapper_calibration_forecast(
        initialization["total_ns"], [sample["total_ns"] for sample in samples]
    )
    representative_records = result.get("representative_records")
    representative_bytes = "".join(
        compact(value) + "\n" for value in representative_records or []
    ).encode()
    result_required = {
        "schema": "phase4-g5-1-wrapper-calibration-result-v16",
        "status": "PASS",
        "classification": "ZeroProductWrapperCalibration",
        "intent_sha256": sha256(WRAPPER_CALIBRATION_INTENT),
        "raw_sha256": sha256(WRAPPER_CALIBRATION_RAW),
        "initialization_sample_count": WRAPPER_INITIALIZATION_SAMPLES,
        "recurring_sample_count": WRAPPER_CALIBRATION_SAMPLES,
        "samples_sha256": sha256_bytes(compact(records).encode()),
        "initialization_sample": initialization,
        "initialization_action_counts": initialization_actions,
        "action_counts_per_sample": dynamic_actions,
        "actions_equal_across_samples": True,
        **forecast,
        "calibration_root_absent": True,
        "global_lock_absent": True,
        "representative_evidence_sha256": sha256_bytes(representative_bytes),
        "representative_records": representative_records,
        "work_root_lifecycle": result.get("work_root_lifecycle"),
        "zero_product_counters": zero_product,
        "fixed_retained_evidence": retained_forecast_evidence(),
    }
    work = result.get("work_root_lifecycle")
    if (
        result != result_required
        or not wrapper_calibration_lock_evidence_valid(
            result.get("global_lock_absent"),
            result.get("zero_product_counters", {}).get("locks_acquired"),
            WRAPPER_CALIBRATION_ROOT.exists(),
            LOCK.exists(),
        )
        or not isinstance(representative_records, list)
        or len(representative_records) != WRAPPER_CALIBRATION_SAMPLES
        or any(
            record.get("sample") != sample["sample"]
            or not wrapper_evidence_semantics_match(record.get("wrapper_evidence"))
            or sha256_bytes(compact(record.get("wrapper_evidence")).encode())
            != sample.get("wrapper_evidence_sha256")
            for record, sample in zip(representative_records, samples)
        )
        or not isinstance(work, dict)
        or work.get("status") != "PASS"
        or work.get("started_row_roots") != WRAPPER_CALIBRATION_SAMPLES
        or work.get("cleaned_row_roots") != WRAPPER_CALIBRATION_SAMPLES
        or work.get("max_active_row_roots") != 1
        or work.get("active_row_roots_terminal") != 0
        or work.get("receipts")
        != [sample.get("cleanup_receipt") for sample in samples]
    ):
        raise RuntimeError("wrapper calibration result mismatch")
    return {
        "intent_sha256": sha256(WRAPPER_CALIBRATION_INTENT),
        "raw_sha256": sha256(WRAPPER_CALIBRATION_RAW),
        "result_sha256": sha256(WRAPPER_CALIBRATION_RESULT),
        "plan_sha256": sha256_bytes(compact(plan).encode()),
        "initialization_sample_count": WRAPPER_INITIALIZATION_SAMPLES,
        "recurring_sample_count": WRAPPER_CALIBRATION_SAMPLES,
        "initialization_action_counts": initialization_actions,
        "action_counts_per_sample": dynamic_actions,
        **forecast,
        "calibration_root_absent": True,
        "global_lock_absent": True,
        "zero_product_counters": zero_product,
        "fixed_retained_evidence": retained_forecast_evidence(),
    }


def dry_artifact_state(path):
    path = pathlib.Path(path)
    if not path.exists():
        return {"present": False, "kind": None, "bytes": None, "sha256": None}
    if path.is_file():
        return {
            "present": True,
            "kind": "file",
            "bytes": path.stat().st_size,
            "sha256": sha256(path),
        }
    if path.is_dir():
        try:
            inventory = exact_inventory(path)
            tree_sha256 = path_kind_size_mode_sha256_tree(path)
            error = None
        except Exception as state_error:
            inventory = None
            tree_sha256 = None
            error = repr(state_error)
        return {
            "present": True,
            "kind": "directory",
            "inventory": inventory,
            "tree_sha256": tree_sha256,
            "inspection_error": error,
        }
    return {
        "present": True,
        "kind": "unsupported",
        "bytes": None,
        "sha256": None,
    }


def dry_failure_documents(error_type, error_message, intent_sha256, collected, artifacts):
    failed = {
        "schema": "phase4-g5-1-dry-run-failed-v16",
        "status": "REVISE",
        "classification": "UNEXPECTED_DRY_RUN_FAILURE",
        "error_type": error_type,
        "error": error_message,
        "intent_sha256": intent_sha256,
        "collected": collected,
        "artifacts": artifacts,
        "measured_rows": 0,
        "global_lock_acquired": False,
    }
    failed_sha256 = sha256_bytes((compact(failed) + "\n").encode())
    disposition = {
        "schema": "phase4-g5-1-dry-run-disposition-v16",
        "status": "REVISE",
        "classification": "UNEXPECTED_DRY_RUN_FAILURE",
        "intent_sha256": intent_sha256,
        "dry_run_failed_sha256": failed_sha256,
        "wrapper_calibration_artifacts": {
            key: artifacts[key]
            for key in (
                "wrapper_calibration_intent",
                "wrapper_calibration_raw",
                "wrapper_calibration_result",
                "wrapper_calibration_root",
            )
        },
        "wrapper_calibration_plan_sha256": (
            sha256_bytes(compact(collected["wrapper_calibration_plan"]).encode())
            if isinstance(collected.get("wrapper_calibration_plan"), dict)
            else None
        ),
        "fixed_retained_evidence": collected.get("fixed_retained_evidence"),
        "dry_run_sha256": artifacts["dry_run"].get("sha256"),
        "premeasurement_revise_sha256": artifacts["premeasurement_revise"].get(
            "sha256"
        ),
        "measured_rows": 0,
        "global_lock_acquired": False,
    }
    return failed, disposition


def persist_dry_failure(error, collected):
    artifact_paths = {
        "wrapper_calibration_intent": WRAPPER_CALIBRATION_INTENT,
        "wrapper_calibration_raw": WRAPPER_CALIBRATION_RAW,
        "wrapper_calibration_result": WRAPPER_CALIBRATION_RESULT,
        "wrapper_calibration_root": WRAPPER_CALIBRATION_ROOT,
        "hash_calibration_stdout": DRY_RUN_CALIBRATION_STDOUT,
        "hash_calibration_stderr": DRY_RUN_CALIBRATION_STDERR,
        "hash_calibration_terminal": DRY_RUN_CALIBRATION_TERMINAL,
        "dry_run": DRY_RUN,
        "premeasurement_revise": PREMEASUREMENT_REVISE,
    }
    artifacts = {
        name: dry_artifact_state(path) for name, path in artifact_paths.items()
    }
    failed, disposition = dry_failure_documents(
        type(error).__name__,
        str(error),
        sha256(DRY_RUN_INTENT),
        collected,
        artifacts,
    )
    if DRY_RUN_FAILED.exists() or DRY_RUN_DISPOSITION.exists():
        raise RuntimeError("dry-run failure evidence already exists") from error
    write_json(DRY_RUN_FAILED, failed)
    if sha256(DRY_RUN_FAILED) != disposition["dry_run_failed_sha256"]:
        raise RuntimeError("dry-run failed-record hash mismatch") from error
    write_json(DRY_RUN_DISPOSITION, disposition)


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
        WRAPPER_CALIBRATION_INTENT,
        WRAPPER_CALIBRATION_RAW,
        WRAPPER_CALIBRATION_RESULT,
        WRAPPER_CALIBRATION_ROOT,
    )
    if any(path.exists() for path in evidence_paths):
        raise RuntimeError("v16 dry-run evidence already exists")
    if not FREEZE_VERIFICATION.is_file():
        raise RuntimeError("v16 dry-run requires durable freeze verification")
    freeze = verify_freeze()
    wrapper_plan = wrapper_calibration_plan()
    fixed_retained_evidence = retained_forecast_evidence()
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
        "schema": "phase4-g5-1-dry-run-intent-v16",
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
        "freeze_verification_sha256": sha256(FREEZE_VERIFICATION),
        "method_manifest_sha256": sha256(METHOD_MANIFEST),
        "input_manifest_sha256": sha256(INPUT_MANIFEST),
        "schedule_sha256": sha256(SCHEDULE),
        "gate_arm_observations": len(observations),
        "fixed_complete_roundtrip_arms": fixed_checkpoints,
        **dry_run_initial_progress(),
        "global_lock_absent": not LOCK.exists(),
        "global_lock_acquired": False,
        "result_roots_absent": not SCREEN_RESULT.exists() and not GATE_RESULT.exists(),
        "calibration_source": str(calibration_source),
        "calibration_source_bytes": CALIBRATION_SIZE,
        "calibration_source_manifest_sha256": calibration_manifest["sha256"],
        "calibration_external_argv": calibration_external_argv,
        "forecast_model_version": FORECAST_MODEL_VERSION,
        "full_wrapper_limit_ns": LIMIT_NS["gate"],
        "wrapper_calibration_plan": wrapper_plan,
        "zero_product_counters": {
            "store_opens": 0,
            "product_children_started": 0,
            "product_rows": 0,
            "locks_acquired": 0,
        },
        "fixed_retained_evidence": fixed_retained_evidence,
    }
    write_json(DRY_RUN_INTENT, intent)
    collected = {
        "intent": intent,
        "freeze": freeze,
        "wrapper_calibration_plan": wrapper_plan,
        "fixed_retained_evidence": fixed_retained_evidence,
    }
    try:
        wrapper_calibration = run_wrapper_calibration(freeze, wrapper_plan)
        collected["wrapper_calibration"] = wrapper_calibration
        if LOCK.exists() or SCREEN_RESULT.exists() or GATE_RESULT.exists():
            raise RuntimeError("dry-run requires absent lock and result roots")
        rows = schedule_rows()
        calibration = hash_calibration()
        collected["hash_calibration"] = calibration
        hash_components, expected_hash_bytes = gate_hash_bytes()
        collected["expected_gate_hash_components_bytes"] = hash_components
        collected["expected_gate_hash_bytes"] = expected_hash_bytes
        workload = gate_workload_enumeration()
        collected["prospective_workload_counts"] = workload
        floor = calibration["conservative_floor_bytes_per_second"]
        hash_forecast_ns = (expected_hash_bytes * 1_000_000_000 + floor - 1) // floor
        forecast_components = {
            **BASE_FORECAST_COMPONENTS_NS,
            "calibrated_one_time_wrapper_initialization": wrapper_calibration[
                "initialization_bound_ns"
            ],
            "calibrated_recurring_per_arm_wrapper_work": wrapper_calibration[
                "recurring_forecast_component_ns"
            ],
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
            "schema": "phase4-g5-1-dry-run-v16",
            "status": status,
            "measured_rows": 0,
            "benchmark_child_processes_started": 0,
            "calibration_processes_started": 1,
            "stores_opened": 0,
            "base_copies_created": WRAPPER_CALIBRATION_SAMPLES,
            "benchmark_base_copies_created": 0,
            "wrapper_calibration_samples_completed": (
                WRAPPER_INITIALIZATION_SAMPLES + WRAPPER_CALIBRATION_SAMPLES
            ),
            "wrapper_initialization_samples_completed": WRAPPER_INITIALIZATION_SAMPLES,
            "wrapper_recurring_samples_completed": WRAPPER_CALIBRATION_SAMPLES,
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
            "sample_count_interpretation": "deliberately-stricter-v16-choice-not-unambiguous-user-minimum",
            "hash_calibration": calibration,
            "expected_gate_hash_components_bytes": hash_components,
            "expected_gate_hash_bytes": expected_hash_bytes,
            "prospective_workload_counts": workload,
            "gate_hash_scope": (
                "external predictable bulk hashes only; clonefile content is NotRehashedPerFastLaw"
            ),
            "base_forecast_components_ns": BASE_FORECAST_COMPONENTS_NS,
            "wrapper_calibration": wrapper_calibration,
            "wrapper_calibration_plan": wrapper_plan,
            "fixed_retained_evidence": fixed_retained_evidence,
            "full_wrapper_forecast_components_ns": forecast_components,
            "full_wrapper_forecast_ns": full_wrapper_forecast_ns,
            "full_wrapper_limit_ns": LIMIT_NS["gate"],
            "full_wrapper_forecast_status": status,
            "full_wrapper_forecast_overrun_ns": forecast_overrun_ns,
            "full_wrapper_forecast_reserve_ns": forecast_reserve_ns,
            "full_wrapper_forecast_reserve_classification": (
                "RemainingTimeNotWorkAndNotTimingEvidence"
            ),
            "forecast_model_version": FORECAST_MODEL_VERSION,
            "generated_non_authoritative_residue": generated_residue,
            "generated_residue_policy": "__pycache__ is generated non-authoritative residue; preserve rather than delete history",
            "source_freeze_sha256": sha256(SOURCE_FREEZE),
            "freeze_verification_sha256": sha256(FREEZE_VERIFICATION),
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
                    "schema": "phase4-g5-1-premeasurement-revise-v16",
                    "status": "REVISE",
                    "classification": "CALIBRATED_COMPLETE_WALL_FORECAST_EXCEEDS_LIMIT",
                    "intent_sha256": sha256(DRY_RUN_INTENT),
                    "dry_run_sha256": sha256(DRY_RUN),
                    "freeze_verification_sha256": sha256(FREEZE_VERIFICATION),
                    "full_wrapper_forecast_ns": full_wrapper_forecast_ns,
                    "full_wrapper_limit_ns": LIMIT_NS["gate"],
                    "full_wrapper_forecast_overrun_ns": forecast_overrun_ns,
                    "full_wrapper_forecast_reserve_ns": forecast_reserve_ns,
                    "wrapper_calibration": wrapper_calibration,
                    "wrapper_calibration_plan_sha256": wrapper_calibration[
                        "plan_sha256"
                    ],
                    "fixed_retained_evidence": fixed_retained_evidence,
                    "measured_rows": 0,
                    "global_lock_acquired": False,
                },
            )
            revise_sha256 = sha256(PREMEASUREMENT_REVISE)
        write_json(
            DRY_RUN_DISPOSITION,
            {
                "schema": "phase4-g5-1-dry-run-disposition-v16",
                "status": status,
                "intent_sha256": sha256(DRY_RUN_INTENT),
                "calibration_stdout_sha256": sha256(DRY_RUN_CALIBRATION_STDOUT),
                "calibration_stderr_sha256": sha256(DRY_RUN_CALIBRATION_STDERR),
                "calibration_terminal_sha256": sha256(DRY_RUN_CALIBRATION_TERMINAL),
                "dry_run_sha256": sha256(DRY_RUN),
                "freeze_verification_sha256": sha256(FREEZE_VERIFICATION),
                "premeasurement_revise_sha256": revise_sha256,
                "wrapper_calibration": wrapper_calibration,
                "wrapper_calibration_plan_sha256": wrapper_calibration[
                    "plan_sha256"
                ],
                "fixed_retained_evidence": fixed_retained_evidence,
                "measured_rows": 0,
                "global_lock_acquired": False,
            },
        )
        print(compact(value))
        return 0 if status == "PASS" else 1
    except Exception as error:
        persist_dry_failure(error, collected)
        raise


def acquire_lock():
    started = time.monotonic_ns()
    descriptor = os.open(LOCK, os.O_RDWR | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
    token = os.urandom(32).hex()
    content = (
        compact(
            {
                "schema": "phase4-g5-1-lock-v16",
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
        attestation = result / "BENCHMARK-LOCK-RELEASE-ATTESTATION-v16.json"
        if attestation.exists():
            raise RuntimeError("lock release attestation exists")
        payload = (
            compact(
                {
                    "schema": "phase4-g5-1-lock-v16",
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
            "schema": "phase4-g5-1-lock-release-v16",
            "status": "PASS" if state == "release" else "REVISE",
            "state": state,
            "device": lock["device"],
            "inode": lock["inode"],
            "token_sha256": sha256_bytes(lock["token"].encode()),
            "attestation_sha256": sha256(attestation),
            "terminal_verification_sha256": sha256(terminal_verification) if terminal_verification else None,
            "lock_absent": True,
        }
        write_json(result / "LOCK-RELEASE-v16.json", value)
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


def run_semantic(executable, case, root, result, label, lifecycle=None):
    stdout = result / f"children-v16/{label}.stdout"
    stderr = result / f"children-v16/{label}.stderr"
    sidecar = result / f"time-v16/{label}.txt"
    command = [str(executable), SEMANTIC_FLAG, case, str(root)]
    if lifecycle is not None:
        lifecycle.synchronous_start(label)
    try:
        completed = subprocess.run(
            ["/usr/bin/time", "-l", "-o", str(sidecar), *command],
            cwd=REPO,
            text=True,
            capture_output=True,
        )
    finally:
        if lifecycle is not None:
            lifecycle.synchronous_finish(label)
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
        "residue", "q_high_water", "q_current", "fault_case", "error_class",
        "failure_boundary",
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
    if case != "touched-error-matrix" and any(
        value.get(field) is not None
        for value in values
        for field in ("fault_case", "error_class", "failure_boundary")
    ):
        raise RuntimeError(f"semantic matrix-only field mismatch: {case}")
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
    if case == "touched-error-matrix":
        expected_errors = {
            "missing-object": "MissingObject",
            "identity-mismatch": "IdentityMismatch",
            "wrong-logical-role": "WrongLogicalRole",
            "malformed-logical-record": "UnexpectedEof",
        }
        observed = {
            (value.get("integrity_mode"), value.get("fault_case")): value
            for value in values
        }
        expected_keys = {
            (mode, fault_case)
            for mode in ("verified", "trusted-local-dev")
            for fault_case in expected_errors
        }
        if (
            len(values) != 8
            or set(observed) != expected_keys
            or any(
                value.get("error_class") != expected_errors[fault_case]
                or value.get("failure_boundary") != "PreCommit"
                or value.get("error") is None
                or value.get("transactions") != 1
                or value.get("commits") != 0
                or value.get("publication_status") is not None
                or value.get("reconciliation") != "NotAttempted"
                or value.get("verified_carry_forward") is not False
                or value.get("head_unchanged") is not True
                or value.get("cleanup_ok") is not True
                or value.get("q_current") != 0
                for (_, fault_case), value in observed.items()
            )
        ):
            raise RuntimeError("semantic touched-error matrix mismatch")
    terminal["external_time"] = parse_time(sidecar)
    if lifecycle is not None:
        lifecycle.record_rss(label, "semantic-one-shot", terminal["external_time"])
    terminal["role"] = f"semantic_{case.replace('-', '_')}"
    return values, terminal, command


def timed_external_command(command, result, label, env=None, lifecycle=None):
    stdout = result / f"children-v16/{label}.stdout"
    stderr = result / f"children-v16/{label}.stderr"
    sidecar = result / f"time-v16/{label}.txt"
    if lifecycle is not None:
        lifecycle.synchronous_start(label)
    try:
        completed = subprocess.run(
            ["/usr/bin/time", "-l", "-o", str(sidecar), *map(str, command)],
            cwd=REPO,
            text=True,
            capture_output=True,
            env=env,
        )
    finally:
        if lifecycle is not None:
            lifecycle.synchronous_finish(label)
    write_text(stdout, completed.stdout)
    write_text(stderr, completed.stderr)
    fsync_file(sidecar)
    fsync_dir(sidecar.parent)
    if completed.returncode != 0:
        raise RuntimeError(f"S07 command failed: {label}: {completed.stderr.strip()}")
    external = parse_time(sidecar)
    if lifecycle is not None:
        lifecycle.record_rss(label, "s07-one-shot", external)
    return completed, external


def run_s07(g4_executable, work, result, lifecycle=None):
    size = 1_048_576
    frozen_fixture = INPUT_ROOT / "fixtures" / str(size)
    if not frozen_fixture.is_dir():
        raise RuntimeError("S07 frozen 1-MiB fixture missing")
    commands, records = [], []
    probe = work / "s07-fixture-probe"
    probe.mkdir()
    command = [g4_executable, "--fast-fixture", probe, str(size)]
    _, external = timed_external_command(
        command, result, "s07-01-fixture", lifecycle=lifecycle
    )
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
        _, prepare_external = timed_external_command(
            prepare,
            result,
            f"s07-{prepare_index:02d}-{route}-prepare",
            lifecycle=lifecycle,
        )
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
            row_command,
            result,
            f"s07-{row_index:02d}-{route}-row",
            row_env,
            lifecycle,
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
            or product.get("base_copy_method") != "regenerated-isolated-database"
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
        else:
            ranges = product.get("range_measurements")
            if not isinstance(ranges, list) or len(ranges) != len(S07_FULL_RANGE_SHAPES) or any(
                any(observed.get(key) != expected for key, expected in shape.items())
                for observed, shape in zip(ranges, S07_FULL_RANGE_SHAPES)
            ):
                raise RuntimeError(f"S07 full-create range counters mismatch: {ranges}")
        state = post_row_state(root, product, allowed_inventory, "CompleteRoundTrip")
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
        "schema": "phase4-g5-1-operation-v16",
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
        "schema": "phase4-g5-1-operation-v16",
        "status": row.get("status", "PASS"),
        "integrity_mode": "Verified",
        "mode_provenance": "frozen-g4-one-shot",
        "timer_availability": "G4 preflight/open split unavailable; common retained intervals only",
        "timers_ns": timers,
        "total_ns": sum(timers.values()),
        "decision_ns": sum(timers.values()),
        "product": row,
    }


def select_comparison_interval(operation_row, comparison, operation, role=None):
    timers = operation_row["timers_ns"]
    if (
        comparison == "g4-verified-vs-g5-verified"
        and operation != "first-edit-after-reopen"
    ):
        fields = COMMON_SECONDARY_TIMER_FIELDS
        classification = COMMON_INTERVAL_CLASSIFICATION
    else:
        fields = TIMER_FIELDS
        classification = FULL_INTERVAL_CLASSIFICATION
    interval = sum(timers[name] for name in fields)
    if classification == FULL_INTERVAL_CLASSIFICATION and interval != operation_row["decision_ns"]:
        raise RuntimeError("full comparison interval does not match decision_ns")
    operation_row.update(
        comparison_interval_ns=interval,
        comparison_interval_classification=classification,
        comparison_interval_components=list(fields),
    )
    named_intervals = {}
    named_classifications = {}
    if comparison == "g4-g5-triple":
        if role in ("g4_verified", "g5_verified"):
            g4_fields = (
                COMMON_SECONDARY_TIMER_FIELDS
                if operation != "first-edit-after-reopen"
                else TIMER_FIELDS
            )
            named_intervals["g4_verified_vs_g5_verified"] = sum(
                timers[name] for name in g4_fields
            )
            named_classifications["g4_verified_vs_g5_verified"] = (
                COMMON_INTERVAL_CLASSIFICATION
                if g4_fields == COMMON_SECONDARY_TIMER_FIELDS
                else FULL_INTERVAL_CLASSIFICATION
            )
        if role in ("g5_verified", "g5_trusted"):
            named_intervals["g5_verified_vs_g5_trusted"] = operation_row["decision_ns"]
            named_classifications[
                "g5_verified_vs_g5_trusted"
            ] = FULL_INTERVAL_CLASSIFICATION
    else:
        key = comparison.replace("-", "_")
        named_intervals[key] = interval
        named_classifications[key] = classification
    operation_row.update(
        comparison_intervals_ns=named_intervals,
        comparison_interval_classifications=named_classifications,
    )
    return operation_row


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


def fixed_blob(value, length, label):
    if not isinstance(value, bytes) or len(value) != length:
        raise RuntimeError(f"published state {label} is not a {length}-byte BLOB")
    return value


def published_visible_state(database, product, validation_scope):
    database = pathlib.Path(database)
    connection = sqlite3.connect(
        database.resolve().as_uri() + "?mode=ro&immutable=1",
        uri=True,
        isolation_level=None,
    )
    try:
        connection.execute("PRAGMA query_only=ON")
        query_only = connection.execute("PRAGMA query_only").fetchone() == (1,)
        autocommit = not connection.in_transaction
        if not query_only or not autocommit:
            raise RuntimeError("published state connection is not query-only autocommit")
        head_rows = connection.execute(
            "SELECT generation, child, transition, validation_receipt "
            "FROM wp4m_visible_head WHERE id = 1"
        ).fetchall()
        if len(head_rows) != 1:
            raise RuntimeError("published state requires exactly one visible head")
        generation, root_id, transition_id, receipt = head_rows[0]
        generation = fixed_blob(generation, 8, "head_generation")
        root_id = fixed_blob(root_id, 32, "head_root")
        transition_id = fixed_blob(transition_id, 32, "head_transition")
        receipt = fixed_blob(receipt, 216, "head_receipt")
        root_hex = root_id.hex()
        transition_hex = transition_id.hex()
        calibration_shape_only = product is None
        if calibration_shape_only:
            product = {
                "root_id": root_hex,
                "transition_id": transition_hex,
                "ordered_closure_digest": "0" * 64,
            }
        closure = product.get("ordered_closure_digest")
        if (
            root_hex != product.get("root_id")
            or transition_hex != product.get("transition_id")
            or not isinstance(closure, str)
            or len(closure) != 64
        ):
            raise RuntimeError("published state does not match product result")
        if validation_scope not in ("CaptureOnly", "CompleteRoundTrip"):
            raise RuntimeError("unknown published state validation scope")

        page_size = connection.execute("PRAGMA page_size").fetchone()[0]
        page_count = connection.execute("PRAGMA page_count").fetchone()[0]
        freelist_count = connection.execute("PRAGMA freelist_count").fetchone()[0]
        rootpages = connection.execute(
            "SELECT type, name, tbl_name, rootpage FROM sqlite_schema "
            "WHERE name NOT LIKE 'sqlite_%' ORDER BY type, name"
        ).fetchall()
        for value, label in (
            (page_size, "page_size"),
            (page_count, "page_count"),
            (freelist_count, "freelist_count"),
        ):
            if type(value) is not int or value < 0:
                raise RuntimeError(f"invalid SQLite {label}")
        return (
            {
                "schema": ROOTED_STATE_SCHEMA,
                "semantics": (
                    "CalibrationConstantRowShapeNotProductAuthority"
                    if calibration_shape_only
                    else ROOTED_STATE_SEMANTICS
                ),
                "query_only": query_only,
                "autocommit": autocommit,
                "head_generation": int.from_bytes(generation, "big"),
                "head_root_id": root_hex,
                "head_transition_id": transition_hex,
                "head_receipt_bytes": len(receipt),
                "head_receipt_sha256": sha256_bytes(receipt),
                "head_receipt_semantics": (
                    "CalibrationOpaqueHeadReceiptHashNotClosureOrFreshness"
                    if calibration_shape_only
                    else "ProductAuthenticatedHeadTupleOpaqueHashNotClosureOrFreshness"
                ),
                "ordered_closure_digest": closure,
                "closure_provenance": (
                    "CalibrationShapeOnlyNoProductParity"
                    if calibration_shape_only
                    else (
                        "ObservedVerifiedCompleteRoundTrip"
                        if validation_scope == "CompleteRoundTrip"
                        else "PreparedGoldenBoundByExactRootTransitionAndProductQualification"
                    )
                ),
                "reachable_published_result_parity": (
                    "NotClaimedCalibrationShapeOnly"
                    if calibration_shape_only
                    else "ClaimedHardGated"
                ),
                "all_object_table_catalog_parity": ALL_ROW_CATALOG_PARITY,
                "rollback_freshness": "NotProtected",
            },
            {
                "schema": PHYSICAL_ALLOCATION_SCHEMA,
                "classification": PHYSICAL_ALLOCATION_CLASSIFICATION,
                "database_file_bytes": database.stat().st_size,
                "sqlite_page_size": page_size,
                "sqlite_page_count": page_count,
                "sqlite_freelist_count": freelist_count,
                "sqlite_allocated_bytes": page_size * page_count,
                "sqlite_freelist_bytes": page_size * freelist_count,
                "sqlite_schema_rootpages": [
                    {
                        "type": row_type,
                        "name": name,
                        "table_name": table_name,
                        "rootpage": rootpage,
                    }
                    for row_type, name, table_name, rootpage in rootpages
                ],
            },
        )
    finally:
        connection.close()


def mutation_work_evidence(product):
    missing = [name for name in MUTATION_WORK_FIELDS if name not in product]
    if missing:
        raise RuntimeError(f"post-row exact mutation work unavailable: {missing}")
    work = {name: product[name] for name in MUTATION_WORK_FIELDS}
    work.update(root_id=product.get("root_id"), transition_id=product.get("transition_id"))
    if work["root_id"] is None or work["transition_id"] is None:
        raise RuntimeError("post-row root/transition identity unavailable")
    return work, sha256_bytes(compact(work).encode())


def assemble_post_row_evidence(
    rooted_state,
    physical_allocation,
    authority_evidence,
    expectations_evidence,
    work,
    work_sha256,
    allowed_inventory,
    inventory,
):
    return {
        "post_database_hash_semantics": rooted_state["semantics"],
        "rooted_logical_state": rooted_state,
        "physical_allocation_observation": physical_allocation,
        "post_authority_sha256": authority_evidence["sha256"],
        "post_authority_bytes": authority_evidence["bytes"],
        "post_expectations_sha256": expectations_evidence["sha256"],
        "post_expectations_bytes": expectations_evidence["bytes"],
        "mutation_work": work,
        "mutation_work_sha256": work_sha256,
        "allowed_inventory": allowed_inventory,
        "post_inventory": inventory,
        "inventory_residue": [],
    }


def wrapper_evidence_semantics_match(evidence):
    rooted = evidence.get("rooted_logical_state") if isinstance(evidence, dict) else None
    return (
        isinstance(rooted, dict)
        and evidence.get("post_database_hash_semantics") == rooted.get("semantics")
    )


def post_row_state(root, product, allowed_inventory, validation_scope):
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
    work, work_sha256 = mutation_work_evidence(product)
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
    rooted_state, physical_allocation = published_visible_state(
        database, product, validation_scope
    )
    return assemble_post_row_evidence(
        rooted_state,
        physical_allocation,
        {"sha256": sha256(authority), "bytes": authority.stat().st_size},
        {"sha256": sha256(expectations), "bytes": expectations.stat().st_size},
        work,
        work_sha256,
        allowed_inventory,
        inventory,
    )


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


class ArmWorkRootLifecycle:
    def __init__(self, work_root):
        self.work_root = pathlib.Path(work_root)
        self.active_row_root = None
        self.started_row_roots = 0
        self.cleaned_row_roots = 0
        self.max_active_row_roots = 0
        self.cleanup_failures = 0
        self.receipts = []

    def begin(self, row_root):
        row_root = pathlib.Path(row_root)
        if row_root.parent != self.work_root or self.active_row_root is not None:
            raise RuntimeError(f"row-root lifecycle overlap or scope mismatch: {row_root}")
        if row_root.exists():
            raise RuntimeError(f"row root exists before lifecycle begin: {row_root}")
        self.active_row_root = row_root
        self.started_row_roots += 1
        self.max_active_row_roots = max(self.max_active_row_roots, 1)

    def cleanup(self, row_root, observation, inventory, classification="ImmediatePostEvidence"):
        row_root = pathlib.Path(row_root)
        if self.active_row_root != row_root:
            raise RuntimeError(f"row-root cleanup ownership mismatch: {row_root}")
        if row_root.is_symlink() or not row_root.is_dir():
            self.cleanup_failures += 1
            raise RuntimeError(f"row-root cleanup target mismatch: {row_root}")
        inventory = list(inventory)
        inventory_sha256 = sha256_bytes(compact(inventory).encode())
        try:
            shutil.rmtree(row_root)
            fsync_dir(self.work_root)
        except BaseException:
            self.cleanup_failures += 1
            raise
        if row_root.exists():
            self.cleanup_failures += 1
            raise RuntimeError(f"row root remained after cleanup: {row_root}")
        self.active_row_root = None
        self.cleaned_row_roots += 1
        receipt = {
            "schema": ARM_CLEANUP_SCHEMA,
            "status": "PASS",
            "classification": classification,
            "ordinal": observation.get("ordinal"),
            "sequence_id": observation.get("sequence_id"),
            "pair": observation.get("pair"),
            "role": observation.get("role"),
            "row_root_name": row_root.name,
            "inventory_entries_removed": len(inventory),
            "inventory_sha256": inventory_sha256,
            "row_root_absent": True,
            "parent_directory_fsynced": True,
            "active_row_roots_after_cleanup": 0,
        }
        self.receipts.append(receipt)
        return receipt

    def cleanup_active_failure(self, observation=None):
        row_root = self.active_row_root
        if row_root is None:
            return None
        observation = observation or {}
        if row_root.exists() and row_root.is_dir() and not row_root.is_symlink():
            inventory = exact_inventory(row_root)
            return self.cleanup(
                row_root,
                observation,
                inventory,
                classification="FailurePathImmediateCleanup",
            )
        if row_root.exists():
            self.cleanup_failures += 1
            raise RuntimeError(f"unsupported failure-path row root: {row_root}")
        self.active_row_root = None
        self.cleaned_row_roots += 1
        fsync_dir(self.work_root)
        receipt = {
            "schema": ARM_CLEANUP_SCHEMA,
            "status": "PASS",
            "classification": "FailurePathAbsentRootReconciliation",
            "ordinal": observation.get("ordinal"),
            "sequence_id": observation.get("sequence_id"),
            "pair": observation.get("pair"),
            "role": observation.get("role"),
            "row_root_name": row_root.name,
            "inventory_entries_removed": 0,
            "inventory_sha256": sha256_bytes(b"[]"),
            "row_root_absent": True,
            "parent_directory_fsynced": True,
            "active_row_roots_after_cleanup": 0,
        }
        self.receipts.append(receipt)
        return receipt

    def snapshot(self):
        active = int(self.active_row_root is not None)
        status = (
            "PASS"
            if active == 0
            and self.max_active_row_roots <= 1
            and self.started_row_roots == self.cleaned_row_roots == len(self.receipts)
            and self.cleanup_failures == 0
            else "REVISE"
        )
        return {
            "schema": WORK_ROOT_LIFECYCLE_SCHEMA,
            "status": status,
            "lifecycle_scope": "ImmediatePerArmPostEvidenceCleanup",
            "started_row_roots": self.started_row_roots,
            "cleaned_row_roots": self.cleaned_row_roots,
            "max_active_row_roots": self.max_active_row_roots,
            "active_row_roots_terminal": active,
            "cleanup_failures": self.cleanup_failures,
            "receipts": self.receipts,
        }

    def terminal_snapshot(self, expected_rows):
        value = self.snapshot()
        if value["status"] != "PASS" or value["started_row_roots"] != expected_rows:
            raise RuntimeError(f"work-root lifecycle mismatch: {value}")
        return value


class ProductChildLifecycle:
    def __init__(self):
        self._persistent_children = []
        self._current_sequence_children = []
        self._synchronous_children = set()
        self.terminals = []
        self.pair_scopes = []
        self._current_pair = None
        self.rss_observations = []
        self.started_product_children = 0
        self.max_simultaneous_product_children = 0
        self.construction_failures = 0
        self.request_failures = 0
        self.close_failures = 0

    def active_product_children(self):
        return len(self._synchronous_children) + sum(
            child.is_running() for child in self._persistent_children
        )

    def _observe_high_water(self):
        active = self.active_product_children()
        self.max_simultaneous_product_children = max(
            self.max_simultaneous_product_children, active
        )
        if self._current_pair is not None:
            self._current_pair["max_simultaneous_product_children"] = max(
                self._current_pair["max_simultaneous_product_children"], active
            )

    def begin_pair(self, sequence_id, pair, expected_roles, required_g5_children):
        if self._current_pair is not None:
            raise RuntimeError("product child pair scopes overlap")
        self._current_pair = {
            "sequence_id": sequence_id,
            "pair": pair,
            "expected_roles": list(expected_roles),
            "required_g5_children": list(required_g5_children),
            "observed_roles": [],
            "max_simultaneous_product_children": self.active_product_children(),
            "row_q_zero": False,
            "pair_status": "IN_PROGRESS",
        }

    def finish_pair(self, observed_roles, row_q_zero):
        if self._current_pair is None:
            raise RuntimeError("product child pair scope is absent")
        self._current_pair.update(
            observed_roles=list(observed_roles),
            row_q_zero=row_q_zero,
            active_product_children_after_pair=self.active_product_children(),
            pair_status="PASS",
        )
        self.pair_scopes.append(self._current_pair)
        self._current_pair = None

    def fail_pair(self, observed_roles, row_q_zero):
        if self._current_pair is None:
            return
        self._current_pair.update(
            observed_roles=list(observed_roles),
            row_q_zero=row_q_zero,
            active_product_children_before_sequence_cleanup=self.active_product_children(),
            pair_status="REVISE",
        )
        self.pair_scopes.append(self._current_pair)
        self._current_pair = None

    def finish_sequence(self, sequence_id, required_g5_children):
        terminals = {
            value.get("product_child_label"): value
            for value in self.terminals
            if value.get("product_child_label") in required_g5_children
        }
        owner_fields = (
            "argument_owners",
            "request_owners",
            "schedule_owners",
            "timing_owners",
            "report_owners",
        )
        terminal_presence = set(terminals) == set(required_g5_children)
        terminal_status_pass = terminal_presence and all(
            value.get("status") == "PASS" for value in terminals.values()
        )
        terminal_q_zero = terminal_presence and all(
            value.get("q_current") == 0 for value in terminals.values()
        )
        terminal_owners_zero = terminal_presence and all(
            all(value.get(field) == 0 for field in owner_fields)
            for value in terminals.values()
        )
        active = self.active_product_children()
        for scope in self.pair_scopes:
            if scope["sequence_id"] == sequence_id:
                scope.update(
                    sequence_terminal_records_present=terminal_presence,
                    sequence_terminal_status_pass=terminal_status_pass,
                    sequence_terminal_q_zero=terminal_q_zero,
                    sequence_terminal_owners_zero=terminal_owners_zero,
                    failure_cleanup_complete=active == 0,
                    active_product_children_after_sequence=active,
                )

    def register_persistent(self, child):
        if child in self._persistent_children:
            raise RuntimeError("product child registered twice")
        self._persistent_children.append(child)
        self._current_sequence_children.append(child)
        self.started_product_children += 1
        self._observe_high_water()

    def start(self, factory):
        try:
            child = factory(self.register_persistent)
        except BaseException as construction_error:
            self.construction_failures += 1
            try:
                self.close_sequence()
            except BaseException as cleanup_error:
                raise ExceptionGroup(
                    "product child construction and cleanup failed",
                    [construction_error, cleanup_error],
                )
            raise
        if child not in self._persistent_children:
            self.register_persistent(child)
        return child

    def request(self, child, request):
        try:
            return child.request(request)
        except BaseException as request_error:
            self.request_failures += 1
            try:
                self.close_sequence()
            except BaseException as cleanup_error:
                raise ExceptionGroup(
                    "product child request and cleanup failed",
                    [request_error, cleanup_error],
                )
            raise

    def synchronous_start(self, label):
        if label in self._synchronous_children:
            raise RuntimeError(f"synchronous product child already active: {label}")
        self._synchronous_children.add(label)
        self.started_product_children += 1
        self._observe_high_water()

    def synchronous_finish(self, label, external_time=None):
        if label not in self._synchronous_children:
            raise RuntimeError(f"synchronous product child was not active: {label}")
        self._synchronous_children.remove(label)
        if external_time is not None:
            self.record_rss(label, SYNCHRONOUS_RSS_KIND, external_time)

    def record_rss(self, label, kind, external_time):
        rss = external_time.get("maximum_resident_set_size")
        if type(rss) is not int or rss <= 0:
            raise RuntimeError(f"invalid retained RSS observation: {label}")
        self.rss_observations.append(
            {
                "label": label,
                "kind": kind,
                "classification": RSS_CLASSIFICATION,
                "maximum_resident_set_size": rss,
                "limit_bytes_per_product_child": RSS_LIMIT,
                "within_per_product_child_limit": rss <= RSS_LIMIT,
                "aggregate_rss_claim": "NotClaimed",
            }
        )

    def close_sequence(self):
        children, self._current_sequence_children = self._current_sequence_children, []
        errors = []
        for child in reversed(children):
            try:
                terminal = child.close()
                if terminal is not None:
                    self.record_rss(
                        child.label,
                        "persistent-g5-row-transport",
                        terminal["external_time"],
                    )
                    self.terminals.append(terminal)
            except BaseException as error:
                self.close_failures += 1
                child.abort()
                errors.append(error)
            if child.is_running():
                child.abort()
            if child.is_running():
                errors.append(RuntimeError(f"product child remained active: {child.label}"))
        self._observe_high_water()
        if errors:
            raise ExceptionGroup("product child close failure", errors)

    def snapshot(self):
        active = self.active_product_children()
        rss_pass = all(
            value["within_per_product_child_limit"] for value in self.rss_observations
        )
        rss_complete = len(self.rss_observations) == self.started_product_children
        pair_pass = self._current_pair is None and all(
            scope.get("pair_status") == "PASS"
            and scope.get("observed_roles") == scope.get("expected_roles")
            and scope.get("row_q_zero") is True
            and scope.get("sequence_terminal_records_present") is True
            and scope.get("sequence_terminal_status_pass") is True
            and scope.get("sequence_terminal_q_zero") is True
            and scope.get("sequence_terminal_owners_zero") is True
            and scope.get("failure_cleanup_complete") is True
            and scope.get("active_product_children_after_sequence") == 0
            for scope in self.pair_scopes
        )
        status = (
            "PASS"
            if active == 0
            and self.max_simultaneous_product_children <= 2
            and self.construction_failures == 0
            and self.request_failures == 0
            and self.close_failures == 0
            and rss_pass
            and rss_complete
            and pair_pass
            else "REVISE"
        )
        return {
            "schema": CHILD_LIFECYCLE_SCHEMA,
            "status": status,
            "lifecycle_scope": "SequenceScopedMatchedPairs",
            "started_product_children": self.started_product_children,
            "reaped_product_children": self.started_product_children - active,
            "max_simultaneous_product_children": self.max_simultaneous_product_children,
            "active_product_children_terminal": active,
            "construction_failures": self.construction_failures,
            "request_failures": self.request_failures,
            "close_failures": self.close_failures,
            "rss_classification": RSS_CLASSIFICATION,
            "rss_limit_bytes_per_product_child": RSS_LIMIT,
            "aggregate_product_children_rss_claim": "NotClaimed",
            "rss_observations_complete": rss_complete,
            "per_product_child_rss": self.rss_observations,
            "pair_scopes": self.pair_scopes,
        }

    def terminal_snapshot(self):
        value = self.snapshot()
        if value["status"] != "PASS":
            raise RuntimeError(f"product child lifecycle mismatch: {value}")
        return value


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
        on_spawn=None,
    ):
        self.mode = mode
        self.size_bytes = size_bytes
        self.operation = operation
        self.label = label or f"g5-{mode}-{size_bytes}-{operation}"
        self.stdout_path = result / f"children-v16/{self.label}.stdout"
        self.stderr_path = result / f"children-v16/{self.label}.stderr"
        self.time_path = result / f"time-v16/{self.label}.txt"
        self.stderr_handle = None
        self.process = None
        self._closed = False
        self.stdout_path.parent.mkdir(parents=True, exist_ok=True)
        self.time_path.parent.mkdir(parents=True, exist_ok=True)
        command = [
            "/usr/bin/time", "-l", "-o", str(self.time_path), str(executable), CHILD_FLAG,
            "trusted" if mode == "trusted-local-dev" else "verified",
            str(size_bytes), operation, str(expected_rows), str(forecast_ns),
            str(LIMIT_NS["gate"]), executable_sha256, custody["database_sha256"],
            custody["authority_sha256"], custody["expectations_sha256"],
        ]
        self.command = command
        try:
            self.stderr_handle = self.stderr_path.open("x", encoding="utf-8")
            self.process = subprocess.Popen(
                command,
                cwd=REPO,
                text=True,
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                stderr=self.stderr_handle,
                bufsize=1,
            )
            if on_spawn is not None:
                on_spawn(self)
            if self.process.stdout is None:
                raise RuntimeError("persistent child stdout unavailable")
            ready_line = self.process.stdout.readline()
            if not ready_line:
                raise RuntimeError(f"persistent child omitted READY: {self.label}")
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
                raise RuntimeError(f"persistent child READY mismatch: {self.label}")
        except BaseException:
            self.abort()
            raise

    def is_running(self):
        return self.process is not None and self.process.poll() is None

    def abort(self):
        process = self.process
        if process is not None:
            try:
                if process.stdin is not None and not process.stdin.closed:
                    process.stdin.close()
            except Exception:
                pass
            if process.poll() is None:
                try:
                    process.terminate()
                    process.wait(timeout=1)
                except Exception:
                    try:
                        process.kill()
                        process.wait(timeout=1)
                    except Exception:
                        pass
            try:
                if process.stdout is not None:
                    process.stdout.close()
            except Exception:
                pass
        if self.stderr_handle is not None and not self.stderr_handle.closed:
            try:
                self.stderr_handle.flush()
                os.fsync(self.stderr_handle.fileno())
            except Exception:
                pass
            self.stderr_handle.close()
        self._closed = True

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
        if self._closed:
            return None
        process = self.process
        if process is None:
            self.abort()
            raise RuntimeError(f"persistent child was never started: {self.label}")
        terminal_line = ""
        remainder = ""
        try:
            if process.stdin is not None:
                process.stdin.close()
            terminal_line = process.stdout.readline() if process.stdout is not None else ""
            if terminal_line:
                append_text(self.stdout_path, terminal_line)
            remainder = process.stdout.read() if process.stdout is not None else ""
            if remainder:
                append_text(self.stdout_path, remainder)
            returncode = process.wait()
        except BaseException:
            self.abort()
            raise
        finally:
            if process.stdout is not None:
                process.stdout.close()
            if self.stderr_handle is not None and not self.stderr_handle.closed:
                self.stderr_handle.flush()
                os.fsync(self.stderr_handle.fileno())
                self.stderr_handle.close()
            self._closed = True
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
        terminal["product_child_label"] = self.label
        terminal["rss_classification"] = RSS_CLASSIFICATION
        terminal["rss_limit_bytes_per_product_child"] = RSS_LIMIT
        terminal["aggregate_product_children_rss_claim"] = "NotClaimed"
        return terminal


def run_oneshot(
    executable,
    request,
    result,
    label,
    g4=False,
    custody=None,
    lifecycle=None,
):
    stdout = result / f"children-v16/{label}.stdout"
    stderr = result / f"children-v16/{label}.stderr"
    sidecar = result / f"time-v16/{label}.txt"
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
    if lifecycle is not None:
        lifecycle.synchronous_start(label)
    try:
        completed = subprocess.run(
            ["/usr/bin/time", "-l", "-o", str(sidecar), *command],
            cwd=REPO,
            text=True,
            capture_output=True,
            env=environment,
        )
    finally:
        if lifecycle is not None:
            lifecycle.synchronous_finish(label)
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
    if lifecycle is not None:
        lifecycle.record_rss(label, SYNCHRONOUS_RSS_KIND, row["external_time"])
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


def consecutive_observation_groups(observations, fields):
    groups = []
    for observation in observations:
        key = tuple(observation[field] for field in fields)
        if not groups or groups[-1][0] != key:
            groups.append((key, []))
        groups[-1][1].append(observation)
    return [values for _, values in groups]


def analyze(result):
    outputs = []
    for analyzer, name in (
        (PRIMARY, "PRIMARY-ANALYSIS-v16.json"),
        (INDEPENDENT, "INDEPENDENT-RECOMPUTATION-v16.json"),
    ):
        output = result / name
        completed = subprocess.run(
            [
                sys.executable, str(analyzer), str(result / "RAW-v16.jsonl"),
                str(result / "TIMINGS-v16.tsv"), str(SCHEDULE), str(EXPECTED), str(output),
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
        result / "ANALYZER-AGREEMENT-v16.json",
        {
            "schema": "phase4-g5-1-analyzer-agreement-v16",
            "status": "PASS" if agreement else "REVISE",
            "exact_normalized_agreement": agreement,
        },
    )
    if not agreement or any(output.get("status") != "PASS" for output in outputs):
        raise RuntimeError("analysis disposition REVISE")


def run_prearm_wrapper_initialization(result, freeze, dry, lock):
    started = time.monotonic_ns()
    plan = dry["wrapper_calibration_plan"]
    master = plan["selected_master"]
    databases = sorted(
        path
        for path in (INPUT_ROOT / master["relative_path"]).rglob("*.sqlite")
        if path.is_file()
    )
    if len(databases) != 1 or not verify_owned_lock(lock):
        raise RuntimeError("prearm wrapper initialization custody mismatch")
    database = databases[0]
    (rooted_state, physical), query_ns = elapsed_action(
        lambda: published_visible_state(database, None, "CaptureOnly")
    )
    bound_ns = dry["wrapper_calibration"]["initialization_bound_ns"]
    action_counts = wrapper_initialization_planned_actions()
    if action_counts != dry["wrapper_calibration"]["initialization_action_counts"]:
        raise RuntimeError("prearm initialization action-count drift")
    evidence = {
        "schema": "phase4-g5-1-prearm-wrapper-initialization-evidence-v16",
        "classification": "OneTimeRunnerSQLiteInitializationNotOperationObservation",
        "master": master["relative_path"],
        "database": str(database.relative_to(INPUT_ROOT)),
        "database_manifest": manifest_entry(database),
        "input_manifest_sha256": freeze["input_manifest_sha256"],
        "plan_sha256": dry["wrapper_calibration"]["plan_sha256"],
        "action_counts": action_counts,
        "rooted_state": rooted_state,
        "physical_allocation_observation": physical,
    }
    evidence_path = result / "PREARM-WRAPPER-INITIALIZATION-EVIDENCE-v16.json"
    write_json(evidence_path, evidence)
    total_ns = max(1, time.monotonic_ns() - started)
    value = {
        **evidence,
        "schema": "phase4-g5-1-prearm-wrapper-initialization-v16",
        "status": "PASS" if total_ns <= bound_ns else "REVISE",
        "chronology": "AfterLockAndFrozenCustodyBeforeOrdinal1",
        "query_ns": query_ns,
        "total_ns": total_ns,
        "elapsed_ns": total_ns,
        "evidence_sha256": sha256(evidence_path),
        "dry_initialization_bound_ns": bound_ns,
        "within_dry_initialization_bound": total_ns <= bound_ns,
        "product_children_started": 0,
        "product_rows": 0,
        "stores_opened": 0,
        "lock_owned": True,
        "evidence_write_inside_measured_initialization": True,
        "terminal_artifact_write_inside_complete_wall": True,
        "terminal_artifact_write_classification": "OutsideInitializationBoundInsideCompleteWallFinalization",
        "terminal_artifact_file_fsync_calls": 1,
        "terminal_artifact_directory_fsync_calls": 1,
    }
    write_json(result / "PREARM-WRAPPER-INITIALIZATION-v16.json", value)
    if value["status"] != "PASS":
        raise RuntimeError("prearm wrapper initialization exceeded dry bound")
    return value


def ladder_prelock(campaign):
    paths = [
        FREEZE_VERIFICATION,
        WRAPPER_CALIBRATION_INTENT,
        WRAPPER_CALIBRATION_RAW,
        WRAPPER_CALIBRATION_RESULT,
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
                SCREEN_RESULT / "TERMINAL-VERIFICATION-v16.json",
                SCREEN_RESULT / "FINAL-ARTIFACT-HASHES-v16.tsv",
                SCREEN_RESULT / "FINAL-READONLY-VERIFICATION-v16.json",
                SCREEN_RESULT / "COMPLETE-WALL-v16.json",
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
    lifecycle = ProductChildLifecycle()
    work_roots = None
    started, lock = acquire_lock()
    try:
        freeze = verify_freeze(require_static=campaign == "gate", require_dry=True)
        if ladder_prelock(campaign) != prelock_ladder:
            raise RuntimeError("ladder evidence changed across lock acquisition")
        dry = verify_dry_run(freeze)
        forecast_ns = dry["full_wrapper_forecast_ns"]
        result.mkdir(mode=0o700)
        work = result.parent / f"{result.name}-work-v16"
        work.mkdir(mode=0o700)
        work_roots = ArmWorkRootLifecycle(work)
        for name in ("operands-v16", "children-v16", "time-v16"):
            (result / name).mkdir()
        fsync_dir(result)
        fsync_dir(result.parent)

        g4_copy = result / "operands-v16/frozen-g4-verified"
        g5_copy = result / "operands-v16/g5-verified-trusted"
        exclusive_operand_copy(G4_EXECUTABLE, g4_copy, G4_EXECUTABLE_SHA256)
        exclusive_operand_copy(G5_CHILD_BINARY, g5_copy, freeze["g5_executable_sha256"])
        write_json(
            result / "OPERAND-CUSTODY-v16.json",
            {
                "schema": "phase4-g5-1-operand-custody-v16",
                "g4_verified": {"path": str(g4_copy), "sha256": sha256(g4_copy), "mode": "0500"},
                "g5_verified_trusted": {"path": str(g5_copy), "sha256": sha256(g5_copy), "mode": "0500"},
                "same_g5_bytes": True,
            },
        )
        write_json(
            result / "PREFLIGHT-v16.json",
            {
                "schema": "phase4-g5-1-preflight-v16",
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
            result / "ENVIRONMENT-v16.json",
            {
                "schema": "phase4-g5-1-environment-v16",
                "platform": platform.platform(),
                "machine": platform.machine(),
                "python": platform.python_version(),
                "rustc": subprocess.check_output(["rustc", "--version"], text=True).strip(),
                "controlled_cold": "Unavailable",
                "physical_io_bytes": "Unavailable",
            },
        )
        shutil.copyfile(INPUT_MANIFEST, result / "INPUT-CUSTODY-v16.tsv")
        with (result / "INPUT-CUSTODY-v16.tsv").open("rb") as handle:
            os.fsync(handle.fileno())

        masters = {master_path(observation) for observation in observations}
        master_custody = {path: manifest_master_custody(path) for path in masters}
        prearm_initialization = run_prearm_wrapper_initialization(
            result, freeze, dry, lock
        )
        raw, timings, commands, semantics = [prearm_initialization], [], [], []
        semantic_terminals = []
        try:
            for sequence in consecutive_observation_groups(observations, ("sequence_id",)):
                sequence_id = sequence[0]["sequence_id"]
                sequence_children = {}
                required_g5_children = list(
                    dict.fromkeys(
                        f"{sequence_id}-{observation['role']}"
                        for observation in sequence
                        if observation["role"].startswith("g5_")
                    )
                )
                try:
                    for pair_rows in consecutive_observation_groups(sequence, ("pair",)):
                        expected_roles = [observation["role"] for observation in pair_rows]
                        pair_children = [
                            f"{sequence_id}-{role}"
                            for role in expected_roles
                            if role.startswith("g5_")
                        ]
                        lifecycle.begin_pair(
                            sequence_id,
                            pair_rows[0]["pair"],
                            expected_roles,
                            pair_children,
                        )
                        observed_roles = []
                        row_q_zero = True
                        current_observation = None
                        try:
                            for observation in pair_rows:
                                current_observation = observation
                                row_root = work / (
                                    f"{observation['ordinal']:03d}-"
                                    f"{observation['sequence_id']}-{observation['role']}"
                                )
                                work_roots.begin(row_root)
                                clone_receipt = clone_master_attested(
                                    master_path(observation), row_root
                                )
                                expected_custody = master_custody[master_path(observation)]
                                pre_dispatch_custody = dict(expected_custody)
                                allowed_inventory = exact_inventory(row_root)
                                request = {
                                    **observation,
                                    "root": str(row_root),
                                    "warmup": "false",
                                    "validation": observation["validation"],
                                }
                                label = (
                                    f"{observation['ordinal']:03d}-"
                                    f"{observation['sequence_id']}-{observation['role']}"
                                )
                                request["id"] = label
                                if observation["role"] == "g4_verified":
                                    value = run_oneshot(
                                        g4_copy,
                                        request,
                                        result,
                                        label,
                                        g4=True,
                                        custody=pre_dispatch_custody,
                                        lifecycle=lifecycle,
                                    )
                                else:
                                    key = (
                                        observation["mode"],
                                        int(observation["size_bytes"]),
                                        observation["operation"],
                                    )
                                    child_label = f"{sequence_id}-{observation['role']}"
                                    if child_label not in sequence_children:
                                        expected_rows = sum(
                                            candidate["role"] == observation["role"]
                                            for candidate in sequence
                                        )
                                        child_custody = master_custody[
                                            master_path(observation)
                                        ]

                                        def spawn(register, key=key, expected_rows=expected_rows,
                                                  child_custody=child_custody,
                                                  child_label=child_label):
                                            return PersistentChild(
                                                g5_copy,
                                                *key,
                                                expected_rows,
                                                result,
                                                child_custody,
                                                forecast_ns,
                                                freeze["g5_executable_sha256"],
                                                label=child_label,
                                                on_spawn=register,
                                            )

                                        sequence_children[child_label] = lifecycle.start(spawn)
                                    value = lifecycle.request(
                                        sequence_children[child_label], request
                                    )
                                select_comparison_interval(
                                    value,
                                    observation["comparison"],
                                    observation["operation"],
                                    observation["role"],
                                )
                                state = post_row_state(
                                    row_root,
                                    value["product"],
                                    allowed_inventory,
                                    observation["validation_scope"],
                                )
                                arm_cleanup_receipt = work_roots.cleanup(
                                    row_root,
                                    observation,
                                    state["post_inventory"],
                                )
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
                                        "comparison_intervals_ns": value[
                                            "comparison_intervals_ns"
                                        ],
                                        "comparison_interval_classifications": value[
                                            "comparison_interval_classifications"
                                        ],
                                        "arm_cleanup_receipt": arm_cleanup_receipt,
                                        **state,
                                    }
                                )
                                raw.append(value)
                                observed_roles.append(observation["role"])
                                row_q_zero = (
                                    row_q_zero and value["product"].get("q_current") == 0
                                )
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
                                        "comparison_interval_ns": value[
                                            "comparison_interval_ns"
                                        ],
                                        "comparison_interval_classification": value[
                                            "comparison_interval_classification"
                                        ],
                                    }
                                )
                                commands.append(
                                    {
                                        "ordinal": observation["ordinal"],
                                        "label": label,
                                        "role": observation["role"],
                                    }
                                )
                                if observation["category"] in (
                                    "semantic",
                                    "fault",
                                    "sentinel",
                                ):
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
                                    result / "CHRONOLOGY-v16.jsonl",
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
                        except BaseException:
                            try:
                                work_roots.cleanup_active_failure(current_observation)
                            finally:
                                lifecycle.fail_pair(observed_roles, row_q_zero)
                            raise
                        else:
                            lifecycle.finish_pair(observed_roles, row_q_zero)
                finally:
                    try:
                        lifecycle.close_sequence()
                    finally:
                        lifecycle.finish_sequence(sequence_id, required_g5_children)
            if campaign == "screen":
                for case in (
                    "touched-error-matrix",
                    "unrelated-corruption",
                    "trusted-verified-reopen",
                    "reconciliation",
                ):
                    label = f"semantic-{case}"
                    values, terminal, command = run_semantic(
                        g5_copy,
                        case,
                        work / label,
                        result,
                        label,
                        lifecycle,
                    )
                    raw.extend(values)
                    semantic_terminals.append(terminal)
                    commands.append({"label": label, "role": "semantic", "command": command})
                    semantics.extend(
                        {
                            "ordinal": "native",
                            "sequence_id": case,
                            "role": value["integrity_mode"],
                            "expectation_id": "native-semantic-v16",
                            "status": value["status"],
                            "error": value["error"],
                        }
                        for value in values
                    )
                sentinel_records, sentinel_commands = run_s07(
                    g4_copy, work, result, lifecycle
                )
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
            lifecycle.close_sequence()
        child_lifecycle = lifecycle.terminal_snapshot()
        work_root_lifecycle = work_roots.terminal_snapshot(len(observations))
        raw.extend(lifecycle.terminals)
        raw.extend(semantic_terminals)
        raw.append(child_lifecycle)
        raw.append(work_root_lifecycle)
        write_json(result / "PRODUCT-CHILD-LIFECYCLE-v16.json", child_lifecycle)
        write_json(result / "WORK-ROOT-LIFECYCLE-v16.json", work_root_lifecycle)

        write_text(result / "RAW-v16.jsonl", "".join(compact(row) + "\n" for row in raw))
        timing_fields = (
            "ordinal", "sequence_id", "comparison", "pair", "role", "operation",
            *TIMER_FIELDS, "total_ns", "decision_ns", "comparison_interval_ns",
            "comparison_interval_classification",
        )
        write_text(
            result / "TIMINGS-v16.tsv",
            "\t".join(timing_fields) + "\n"
            + "".join("\t".join(str(row[name]) for name in timing_fields) + "\n" for row in timings),
        )
        write_text(
            result / "SEMANTIC-FAULT-RESULTS-v16.tsv",
            "ordinal\tsequence_id\trole\texpectation_id\tstatus\terror\n"
            + "".join(
                f"{row['ordinal']}\t{row['sequence_id']}\t{row['role']}\t{row['expectation_id']}\t{row['status']}\t{row['error']}\n"
                for row in semantics
            ),
        )
        write_json(result / "COMMANDS-v16.json", {"schema": "phase4-g5-1-commands-v16", "commands": commands})
        analyze(result)

        if work.parent != result.parent or not work.name.endswith("-work-v16"):
            raise RuntimeError("refusing unsafe work cleanup")
        shutil.rmtree(work)
        fsync_dir(work.parent)
        residue = [str(path) for path in result.parent.glob(f"{result.name}-work-v16")]
        write_json(
            result / "CLEANUP-v16.json",
            {
                "schema": "phase4-g5-1-cleanup-v16",
                "status": "PASS" if not residue else "REVISE",
                "work_residue": residue,
                "lock_owned": verify_owned_lock(lock),
            },
        )
        if residue:
            raise RuntimeError("work residue")

        payload_excluded = {
            "PAYLOAD-MANIFEST-v16.tsv", "MEASURED-TERMINAL-v16.json",
            "TERMINAL-VERIFICATION-v16.json", "COMPLETE-WALL-v16.json",
            "BENCHMARK-LOCK-RELEASE-ATTESTATION-v16.json", "LOCK-RELEASE-v16.json",
            "FINAL-ARTIFACT-HASHES-v16.tsv", "FINAL-READONLY-VERIFICATION-v16.json",
        }
        payload = result / "PAYLOAD-MANIFEST-v16.tsv"
        write_text(payload, manifest_text(result, excluded=payload_excluded))
        payload_count = verify_manifest(result, payload, "result_relative_path")
        terminal = result / "MEASURED-TERMINAL-v16.json"
        write_json(
            terminal,
            {
                "schema": "phase4-g5-1-measured-terminal-v16",
                "status": "PASS",
                "campaign": campaign,
                "rows": len(timings),
                "payload_files": payload_count,
                "payload_manifest_sha256": sha256(payload),
                "product_child_lifecycle_sha256": sha256(
                    result / "PRODUCT-CHILD-LIFECYCLE-v16.json"
                ),
                "work_root_lifecycle_sha256": sha256(
                    result / "WORK-ROOT-LIFECYCLE-v16.json"
                ),
                "prearm_wrapper_initialization_sha256": sha256(
                    result / "PREARM-WRAPPER-INITIALIZATION-v16.json"
                ),
                "prearm_wrapper_initialization_elapsed_ns": prearm_initialization[
                    "elapsed_ns"
                ],
                "max_simultaneous_product_children": child_lifecycle[
                    "max_simultaneous_product_children"
                ],
                "active_product_children_terminal": child_lifecycle[
                    "active_product_children_terminal"
                ],
                "rss_classification": child_lifecycle["rss_classification"],
                "rss_limit_bytes_per_product_child": child_lifecycle[
                    "rss_limit_bytes_per_product_child"
                ],
                "aggregate_product_children_rss_claim": "NotClaimed",
                "max_active_row_roots": work_root_lifecycle[
                    "max_active_row_roots"
                ],
                "active_row_roots_terminal": work_root_lifecycle[
                    "active_row_roots_terminal"
                ],
                "elapsed_before_terminal_verification_ns": time.monotonic_ns() - started,
            },
        )
        verification = result / "TERMINAL-VERIFICATION-v16.json"
        write_json(
            verification,
            {
                "schema": "phase4-g5-1-terminal-verification-v16",
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
        final = result / "FINAL-ARTIFACT-HASHES-v16.tsv"
        write_text(
            final,
            manifest_text(
                result,
                excluded={final.name, "FINAL-READONLY-VERIFICATION-v16.json", "COMPLETE-WALL-v16.json"},
            ),
        )
        final_count = verify_manifest(result, final, "result_relative_path")
        write_json(
            result / "FINAL-READONLY-VERIFICATION-v16.json",
            {
                "schema": "phase4-g5-1-final-readonly-verification-v16",
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
            result / "COMPLETE-WALL-v16.json",
            {
                "schema": "phase4-g5-1-complete-wall-v16",
                "status": "PASS",
                "campaign": campaign,
                "complete_wall_ns": complete_ns,
                "limit_ns": LIMIT_NS[campaign],
                "from": "fail-fast global lock acquisition",
                "through": "final manifest and read-only verification fsync",
                "terminal_self_exclusion": "COMPLETE-WALL-v16.json follows the verified final manifest",
            },
        )
        fsync_dir(result)
        print(compact({"status": "PASS", "campaign": campaign, "result": str(result), "complete_wall_ns": complete_ns}))
        return 0
    except Exception as error:
        child_cleanup_error = None
        work_root_cleanup_error = None
        try:
            lifecycle.close_sequence()
        except BaseException as cleanup_error:
            child_cleanup_error = repr(cleanup_error)
        if work_roots is not None:
            try:
                work_roots.cleanup_active_failure()
            except BaseException as cleanup_error:
                work_root_cleanup_error = repr(cleanup_error)
        if result.exists() and not (result / "FAILED-v16.json").exists():
            write_json(
                result / "FAILED-v16.json",
                {
                    "schema": "phase4-g5-1-failure-v16",
                    "status": "REVISE",
                    "error": str(error),
                    "product_child_cleanup_error": child_cleanup_error,
                    "product_child_lifecycle": lifecycle.snapshot(),
                    "work_root_cleanup_error": work_root_cleanup_error,
                    "work_root_lifecycle": (
                        work_roots.snapshot() if work_roots is not None else None
                    ),
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


class _SelfCheckChild:
    def __init__(self, label, *, fail_request=False, fail_close=False):
        self.label = label
        self.running = True
        self.fail_request = fail_request
        self.fail_close = fail_close

    def is_running(self):
        return self.running

    def request(self, request):
        if self.fail_request:
            raise RuntimeError("self-check request failure")
        return request

    def close(self):
        if not self.running:
            return None
        self.running = False
        if self.fail_close:
            raise RuntimeError("self-check close failure")
        terminal = {
            "status": "PASS",
            "q_current": 0,
            "external_time": {"maximum_resident_set_size": 1},
            "product_child_label": self.label,
        }
        terminal.update(
            (field, 0)
            for field in (
                "argument_owners",
                "request_owners",
                "schedule_owners",
                "timing_owners",
                "report_owners",
            )
        )
        return terminal

    def abort(self):
        self.running = False


def run_self_checks():
    global INPUT_ROOT, VERIFIED_INPUT_CUSTODY, VERIFIED_INPUT_MANIFEST_SHA256
    timers = {name: index for index, name in enumerate(TIMER_FIELDS, start=1)}
    full = sum(timers.values())
    secondary = select_comparison_interval(
        {"timers_ns": timers, "decision_ns": full},
        "g4-verified-vs-g5-verified",
        "same-middle",
    )
    assert secondary["comparison_interval_classification"] == COMMON_INTERVAL_CLASSIFICATION
    assert secondary["comparison_interval_ns"] == sum(
        timers[name] for name in COMMON_SECONDARY_TIMER_FIELDS
    )
    for comparison, operation in (
        ("g4-verified-vs-g5-verified", "first-edit-after-reopen"),
        ("g5-verified-vs-g5-trusted", "same-middle"),
        ("g4-g5-triple", "plus1-middle"),
    ):
        selected = select_comparison_interval(
            {"timers_ns": timers, "decision_ns": full}, comparison, operation
        )
        assert selected["comparison_interval_classification"] == FULL_INTERVAL_CLASSIFICATION
        assert selected["comparison_interval_ns"] == full
    s06_verified = select_comparison_interval(
        {"timers_ns": timers, "decision_ns": full},
        "g4-g5-triple",
        "plus1-middle",
        "g5_verified",
    )
    assert s06_verified["comparison_intervals_ns"] == {
        "g4_verified_vs_g5_verified": sum(
            timers[name] for name in COMMON_SECONDARY_TIMER_FIELDS
        ),
        "g5_verified_vs_g5_trusted": full,
    }
    assert s06_verified["comparison_interval_classifications"] == {
        "g4_verified_vs_g5_verified": COMMON_INTERVAL_CLASSIFICATION,
        "g5_verified_vs_g5_trusted": FULL_INTERVAL_CLASSIFICATION,
    }

    lifecycle = ProductChildLifecycle()
    labels = ["G01-g5_verified", "G01-g5_trusted"]
    lifecycle.begin_pair("G01", 1, ["g5_verified", "g5_trusted"], labels)
    for label in labels:
        child = _SelfCheckChild(label)
        lifecycle.start(lambda register, child=child: (register(child), child)[1])
    lifecycle.finish_pair(["g5_verified", "g5_trusted"], True)
    lifecycle.close_sequence()
    lifecycle.finish_sequence("G01", labels)
    snapshot = lifecycle.terminal_snapshot()
    assert snapshot["max_simultaneous_product_children"] == 2
    assert snapshot["active_product_children_terminal"] == 0
    assert snapshot["rss_observations_complete"] is True

    for failure_kind in ("construction", "request", "close"):
        failed = ProductChildLifecycle()
        label = f"failure-{failure_kind}"
        failed.begin_pair("FAIL", 1, ["g5_verified"], [label])
        child = _SelfCheckChild(
            label,
            fail_request=failure_kind == "request",
            fail_close=failure_kind == "close",
        )
        failed.start(lambda register, child=child: (register(child), child)[1])
        try:
            if failure_kind == "construction":
                def broken(register):
                    spawned = _SelfCheckChild("failed-construction")
                    register(spawned)
                    spawned.abort()
                    raise RuntimeError("self-check construction failure")

                failed.start(broken)
            elif failure_kind == "request":
                failed.request(child, {})
            else:
                failed.close_sequence()
        except BaseException:
            pass
        else:
            raise AssertionError(f"self-check {failure_kind} failure did not fail")
        failed.fail_pair([], False)
        try:
            failed.close_sequence()
        except BaseException:
            pass
        assert failed.active_product_children() == 0
        assert failed.max_simultaneous_product_children <= 2

    with tempfile.TemporaryDirectory(prefix="layerfs-g5-v16-self-check-") as temporary:
        work = pathlib.Path(temporary) / "work-v16"
        work.mkdir()
        row_root = work / "001-G01-g5_verified"
        root_lifecycle = ArmWorkRootLifecycle(work)
        root_lifecycle.begin(row_root)
        row_root.mkdir()
        operand = row_root / "operand"
        operand.write_bytes(b"x")
        inventory = exact_inventory(row_root)
        receipt = root_lifecycle.cleanup(
            row_root,
            {"ordinal": 1, "sequence_id": "G01", "pair": 1, "role": "g5_verified"},
            inventory,
        )
        root_snapshot = root_lifecycle.terminal_snapshot(1)
        assert receipt["row_root_absent"] is True
        assert root_snapshot["max_active_row_roots"] == 1
        assert root_snapshot["active_row_roots_terminal"] == 0

    original_input = INPUT_ROOT
    original_custody = VERIFIED_INPUT_CUSTODY
    original_manifest_sha256 = VERIFIED_INPUT_MANIFEST_SHA256
    try:
        with tempfile.TemporaryDirectory(prefix="layerfs-g5-v16-forecast-check-") as temporary:
            INPUT_ROOT = pathlib.Path(temporary)
            VERIFIED_INPUT_CUSTODY = {}
            VERIFIED_INPUT_MANIFEST_SHA256 = "0" * 64
            for operation in SUPPORTED_CHILD_OPERATIONS:
                master = INPUT_ROOT / "bases" / f"{operation}-104857600"
                master.mkdir(parents=True)
                for index, size in enumerate((1, 2, 3)):
                    path = master / f"operand-{index}"
                    path.write_bytes(bytes([index]) * size)
                    VERIFIED_INPUT_CUSTODY[str(path.relative_to(INPUT_ROOT))] = {
                        "bytes": size,
                        "sha256": sha256(path),
                    }
            workload = gate_workload_enumeration()
            calibration_plan = wrapper_calibration_plan()
            assert workload["gate_arm_observations"] == 200
            assert workload["g4_one_shot_product_children"] == 50
            assert workload["g5_persistent_product_children"] == 21
            assert workload["g5_persistent_row_requests"] == 150
            assert workload["fixed_complete_roundtrip_validations"] == 56
            assert workload["clonefile_calls"] == 600
            assert workload["clonefile_bytes"] == 1_200
            assert workload["clonefile_content_hash_bytes"] == 0
            assert workload["immediate_cleanup_roots"] == 200
            assert workload["inventory_enumerations"] == 800
            candidates = calibration_plan["candidate_table"]
            assert calibration_plan["candidate_count"] == 7
            assert [value["relative_path"] for value in candidates] == sorted(
                value["relative_path"] for value in candidates
            )
            assert calibration_plan["selected_master"] == max(
                candidates, key=lambda value: tuple(value["dominance_tuple"])
            )
            assert calibration_plan["selection_rule"] == (
                "lexicographic-max(file_count,total_manifest_bytes,directory_count,relative_path)"
            )
            assert calibration_plan["planned_actions_per_sample"]["clonefile_calls"] == 3
            assert calibration_plan["planned_actions_per_sample"]["clonefile_bytes"] == 6
            assert calibration_plan["planned_actions_per_sample"]["physical_pragma_queries"] == 3
            assert calibration_plan["planned_actions_per_sample"]["query_only_pragma_queries"] == 2
            assert calibration_plan["planned_actions_per_sample"]["sqlite_schema_rootpage_queries"] == 1
            assert calibration_plan["planned_actions_per_sample"]["sqlite_schema_rootpage_rows"] == 3
            assert calibration_plan["planned_actions_per_sample"]["ordered_object_all_row_scans"] == 0
            assert calibration_plan["initialization_sample_count"] == 1
            assert calibration_plan["recurring_sample_count"] == 3
            assert calibration_plan["planned_initialization_actions"]["published_visible_state_invocations"] == 1
    finally:
        INPUT_ROOT = original_input
        VERIFIED_INPUT_CUSTODY = original_custody
        VERIFIED_INPUT_MANIFEST_SHA256 = original_manifest_sha256

    rooted_state_keys = {
        "head_generation",
        "head_root_id",
        "head_transition_id",
        "head_receipt_sha256",
        "ordered_closure_digest",
    }
    physical_keys = {
        "database_file_bytes",
        "sqlite_page_size",
        "sqlite_page_count",
        "sqlite_freelist_count",
        "sqlite_allocated_bytes",
        "sqlite_freelist_bytes",
        "sqlite_schema_rootpages",
    }
    assert rooted_state_keys.isdisjoint(physical_keys)
    assert ALL_ROW_CATALOG_PARITY == "NotClaimedSeparateFutureAllRowCasAudit"
    assert "ordered-closure" in ROOTED_STATE_SEMANTICS
    assert PHYSICAL_ALLOCATION_CLASSIFICATION == "NotLogicalParity"
    synthetic_rooted = {"semantics": "CalibrationConstantRowShapeNotProductAuthority"}
    synthetic_evidence = assemble_post_row_evidence(
        synthetic_rooted,
        {},
        {"sha256": "a" * 64, "bytes": 1},
        {"sha256": "b" * 64, "bytes": 1},
        {},
        "c" * 64,
        [],
        [],
    )
    assert wrapper_evidence_semantics_match(synthetic_evidence)
    assert not wrapper_evidence_semantics_match(
        {**synthetic_evidence, "post_database_hash_semantics": ROOTED_STATE_SEMANTICS}
    )

    gate = expanded_observations("gate")
    assert len(gate) == 200
    assert sum(row["fixed_checkpoint"] for row in gate) == 56
    assert len(consecutive_observation_groups(gate, ("sequence_id", "pair"))) == 100
    retained = retained_forecast_evidence()
    assert retained["checkpoint_reconstruction_ns"] == 334_756_708
    assert retained["campaign_finalization_source_ns"] == 9_254_244_292
    assert retained["campaign_finalization_ns"] == 10_000_000_000
    assert retained["campaign_finalization_inference"] == (
        "ProspectiveAllowanceNotProvenUpperBound"
    )
    assert BASE_FORECAST_COMPONENTS_NS == {
        "retained_child_and_checkpoint_work": 68_746_375_648,
        "fixed_campaign_finalization": 10_000_000_000,
    }
    synthetic_wrapper = wrapper_calibration_forecast(5, [10, 20, 30])
    assert synthetic_wrapper == {
        "initialization_sample_total_ns": 5,
        "initialization_bound_ns": 20,
        "max_recurring_sample_total_ns": 30,
        "conservative_factor": 4,
        "recurring_per_arm_bound_ns": 120,
        "gate_arm_observations": 200,
        "recurring_forecast_component_ns": 24_000,
        "forecast_component_ns": 24_020,
    }
    synthetic_components = {
        **BASE_FORECAST_COMPONENTS_NS,
        "calibrated_one_time_wrapper_initialization": synthetic_wrapper[
            "initialization_bound_ns"
        ],
        "calibrated_recurring_per_arm_wrapper_work": synthetic_wrapper[
            "recurring_forecast_component_ns"
        ],
        "external_bulk_hash_bytes_at_calibrated_floor": 1,
    }
    assert len(synthetic_components) == 5
    assert SYNCHRONOUS_RSS_KIND == "synchronous-one-shot"
    absent = {"present": False, "kind": None, "bytes": None, "sha256": None}
    for stage, present in (
        ("before-wrapper", ()),
        ("within-wrapper", ("wrapper_calibration_intent", "wrapper_calibration_raw")),
        (
            "after-wrapper-result",
            (
                "wrapper_calibration_intent",
                "wrapper_calibration_raw",
                "wrapper_calibration_result",
            ),
        ),
    ):
        artifact_names = (
            "wrapper_calibration_intent",
            "wrapper_calibration_raw",
            "wrapper_calibration_result",
            "wrapper_calibration_root",
            "hash_calibration_stdout",
            "hash_calibration_stderr",
            "hash_calibration_terminal",
            "dry_run",
            "premeasurement_revise",
        )
        artifacts = {
            name: (
                {"present": True, "kind": "file", "bytes": 1, "sha256": "a" * 64}
                if name in present
                else dict(absent)
            )
            for name in artifact_names
        }
        injected_collected = {
            "failure_injection_stage": stage,
            "wrapper_calibration_plan": {"stage": stage},
            "fixed_retained_evidence": {"status": "synthetic"},
        }
        failed, disposition = dry_failure_documents(
            "InjectedFailure",
            stage,
            "b" * 64,
            injected_collected,
            artifacts,
        )
        assert failed["artifacts"] == artifacts
        assert disposition["status"] == "REVISE"
        assert disposition["dry_run_failed_sha256"] == sha256_bytes(
            (compact(failed) + "\n").encode()
        )
        assert disposition["wrapper_calibration_plan_sha256"] == sha256_bytes(
            compact(injected_collected["wrapper_calibration_plan"]).encode()
        )
        assert disposition["wrapper_calibration_artifacts"][
            "wrapper_calibration_result"
        ]["present"] == (stage == "after-wrapper-result")
    assert freeze_interface_contract() == {
        "product_operand_version": "v16",
        "product_release_reuse": False,
        "product_release_sha256": V16_RELEASE_SHA256,
        "sealed_input_reuse": True,
        "sealed_input_manifest_sha256": V10_INPUT_MANIFEST_SHA256,
        "prepare_flag": "--g5-prepare",
        "fixture_flag": "--g5-fixture",
        "child_flag": "--g5-child",
        "semantic_flag": "--g5-semantic",
        "child_ready_schema": "phase4-g5-trusted-child-ready-v10",
        "child_envelope_schema": "phase4-g5-trusted-child-row-v10",
        "child_terminal_schema": "phase4-g5-trusted-child-terminal-v10",
        "request_fields": ["id", "root", "iteration", "warmup", "validation"],
        "wrapper_calibration": {
            "intent_path": str(WRAPPER_CALIBRATION_INTENT.relative_to(REPO)),
            "raw_path": str(WRAPPER_CALIBRATION_RAW.relative_to(REPO)),
            "result_path": str(WRAPPER_CALIBRATION_RESULT.relative_to(REPO)),
            "root_path": str(WRAPPER_CALIBRATION_ROOT.relative_to(REPO)),
            "intent_schema": "phase4-g5-1-wrapper-calibration-intent-v16",
            "initialization_schema": "phase4-g5-1-wrapper-initialization-sample-v16",
            "sample_schema": "phase4-g5-1-wrapper-calibration-sample-v16",
            "result_schema": "phase4-g5-1-wrapper-calibration-result-v16",
            "initialization_samples": 1,
            "recurring_samples": 3,
            "conservative_factor": 4,
            "initialization_multiplier": 1,
            "recurring_multiplier": 200,
            "product_scope": "zero-product",
        },
    }
    assert FORECAST_MODEL_VERSION == "phase4-g5-1-v16-fast-law-forecast-v5"
    assert LIMIT_NS["gate"] == 120_000_000_000
    assert dry_run_initial_progress() == {
        "measured_rows": 0,
        "benchmark_child_processes_started": 0,
        "calibration_processes_started": 0,
        "stores_opened": 0,
        "base_copies_created": 0,
        "benchmark_base_copies_created": 0,
        "wrapper_calibration_samples_completed": 0,
        "wrapper_initialization_samples_completed": 0,
        "wrapper_recurring_samples_completed": 0,
        "measurement_timers_started": 0,
    }
    assert wrapper_calibration_lock_evidence_valid(True, 0, False, False)
    assert wrapper_calibration_lock_evidence_valid(True, 0, False, True)
    assert not wrapper_calibration_lock_evidence_valid(False, 0, False, False)
    assert not wrapper_calibration_lock_evidence_valid(True, 1, False, True)
    assert not wrapper_calibration_lock_evidence_valid(True, 0, True, False)
    assert not wrapper_calibration_lock_evidence_valid(True, 0, True, True)
    sources = method_source_names()
    assert str(V9_SUPERSESSION.relative_to(REPO)) in sources
    assert str(V10_SUPERSESSION.relative_to(REPO)) in sources
    assert str(V11_SUPERSESSION.relative_to(REPO)) in sources
    assert str(V12_SUPERSESSION.relative_to(REPO)) in sources
    assert str(V13_SUPERSESSION.relative_to(REPO)) in sources
    assert str(V14_SUPERSESSION.relative_to(REPO)) in sources
    assert str(FREEZE_VERIFICATION.relative_to(REPO)) not in sources
    assert all(
        str(path.relative_to(REPO)) not in sources
        for path in (
            WRAPPER_CALIBRATION_INTENT,
            WRAPPER_CALIBRATION_RAW,
            WRAPPER_CALIBRATION_RESULT,
        )
    )
    print(compact({"status": "PASS", "checks": 15, "gate_arms": 200, "checkpoints": 56}))
    return 0


def main():
    if len(sys.argv) != 2 or sys.argv[1] not in (
        "--prepare-inputs", "--dry-run", "--screen", "--gate", "--self-check",
    ):
        raise SystemExit(
            "usage: runner.py --prepare-inputs|--dry-run|--screen|--gate|--self-check"
        )
    if sys.argv[1] == "--self-check":
        return run_self_checks()
    if sys.argv[1] == "--prepare-inputs":
        return prepare_inputs()
    if sys.argv[1] == "--dry-run":
        return dry_run()
    return run_campaign(sys.argv[1][2:])


if __name__ == "__main__":
    raise SystemExit(main())
