"""Frozen #232 Workspace Exec/FUSE edit contract (scenario version 2).

This module is the single source of truth for the 56 registered
`sdk-exec-fuse-edit-commit-v1` cases. It freezes, before any benchmark driver or
timed sample exists:

* the operation contract (entrypoint, surface, acknowledgement, executors),
* the six reusable pristine fixture sizes and their byte recipe, including the
  capped 500 MiB inputs and repetition 1,
* each case's command template, editor algorithm and allowed syscalls,
* each case's pre/post identity (size, payload digest, bounded oracle windows),
* the per-case historical G2 engineering target taken from the #152 final
  report's C1/G2 column (0.01 ms precision),
* the declared cache method and the eligibility rule.

Scenario version 2 registers all 56 cases with their frozen structural shift
algorithm; version 1 kept the twenty structural rows `NOT_RUN` under
`structural-shift-algorithm-unfrozen` and pinned registry
`registry/workspace-exec-edit-v1.json`
(`e4b4d2fc67cb1630dc8f15283db3085663466071bb373c829a6026ffddb66aab`). That
registry, its receipts and the Phase 4 sample taken under it remain historical
evidence: version 2 never reinterprets, relabels or overwrites them. The route
name `sdk-exec-fuse-edit-commit-v1` is unchanged because the measured route is
unchanged; the case identity carries the new scenario version.

Run `python3 shared/edit_contract.py` from this directory to regenerate
`registry/workspace-exec-edit-v2.json` and print its SHA-256.
"""
import hashlib
import json
from pathlib import Path

HERE = Path(__file__).resolve().parent
BENCH = HERE.parent
ROUTE = "sdk-exec-fuse-edit-commit-v1"
# SHA-256 of the committed registry document; regenerate with this module.
REGISTRY_SHA256 = "05a133b543db33729d60cc321cc88682046c16bfb1761daec6a57c5df3b11823"
REGISTRY_PATH = "registry/workspace-exec-edit-v2.json"
SCENARIO_VERSION = 2
SCENARIO_SUFFIX = "-exec-v2"
HISTORICAL_REGISTRY_PATH = "registry/workspace-exec-edit-v1.json"
HISTORICAL_REGISTRY_SHA256 = (
    "e4b4d2fc67cb1630dc8f15283db3085663466071bb373c829a6026ffddb66aab")
REPETITION = 1
OPERATION_CONTRACT_ID = "workspace-exec-fuse-edit-commit-v2"
OPERATION_SURFACE = "workspace-posix-fuse"
OPERATION_ENTRYPOINT = "WorkspaceApi::exec"
ACKNOWLEDGEMENT_BOUNDARY = "WorkspaceApi::commit"
ORCHESTRATION_EXECUTOR = "layerfs-sdk release benchmark driver"
MUTATION_EXECUTOR = "workspace-exec-posix-edit-tool"
IMPLEMENTATION_ROUTE = "layerfs-daemon-fuse-workspace-commit"
CLOCK_ID = "layerfs-telemetry caller operation timer (CLOCK_MONOTONIC)"
TELEMETRY_KEY = "sdk.edit_commit.fuse"

# Frozen fixture identity. The recipe is the v0.1.6 SDK-edit fixture recipe:
# a splitmix64 byte stream continued across 1 MiB blocks from one generator seed.
FIXTURE_GENERATOR_SEED = 0x4C41594552465331
FIXTURE_PATH = "payload.bin"
FIXTURE_DIRECTORY_MODE = 0o750
FIXTURE_FILE_MODE = 0o640
FIXTURE_MTIME_SECONDS = 1_700_000_000
PAYLOAD_DIRECTORY = "/layerfs-bench/payloads"
TOOL_PATH = "/layerfs-bench/bin/layerfs-edit-tool"
ALLOWED_PAYLOAD_BYTES = [4096, 65536]

SIZE_LABELS = [("1", 1_048_576), ("10", 10_485_760), ("100", 104_857_600), ("500", 524_288_000)]
# Six reusable pristine masters; 500 MiB cases keep the v0.1.6 capped input sizes.
PREPARED_SIZES = [1_048_576, 10_485_760, 104_857_600, 524_283_904, 524_285_952, 524_288_000]
PREPARED_LABELS = {
    1_048_576: "1mib",
    10_485_760: "10mib",
    104_857_600: "100mib",
    524_283_904: "500mib-capped-26k994",
    524_285_952: "500mib-capped-26k995",
    524_288_000: "500mib",
}
FIXTURE_SHA256 = {
    1_048_576: "d7dfe3d2828aceb85177e6efbeb600f23672a326c902e525e401c1545bb05bdc",
    10_485_760: "29c89128c748e4404f31b0147d447bd524d7b75afc98d56ac4debac762ee4b79",
    104_857_600: "1bb2d79d54f72ae15eb0bb76ad715b9aafeba8ff8f9aa4f47bad3e3f101885bd",
    524_283_904: "f1b6c61d9c126beba89dd2a310f727fd63cbbf131b793a78fe21247238c98c1f",
    524_285_952: "0e2cc5b14abf95553ba633a11b395c80de3f0a1642cdef0adf753cb5984fbe55",
    524_288_000: "bd782f202ec4c40a2070a1d08b78f5135a0ac604b871e4907846740bde906157",
}
# Pinned canonical identity of the six pristine fixtures (v0.1.6 family
# constants for the same bytes) and their extent counts.
FIXTURE_CANONICAL_ROOT = {
    1_048_576: "8fafdf06fac9dbdffb7ccb6b1bde3b2460c387ef1abc55717dee8be401ff6078",
    10_485_760: "dd79a6666e83927d787c8a7679b06f4c98ca5f80b6abd48d94b5e8f84aad1c85",
    104_857_600: "bbee7155df021324495d88954be4db125eca49442b50aadc16439f61f6c32efe",
    524_283_904: "6c74b4ba6ad67f352a0bd85879a2f16a77511286bf9a73883d5c8858d2eded8f",
    524_285_952: "138fcae123c3a4fccbf38aa38b2d01f60e13d087cb108c2ff1f8f456ecf78552",
    524_288_000: "e4ab3cdbf81fe421e6bd2df0b34e57639845dcf244d127507cf15d6ebe01e9a3",
}
FIXTURE_INITIAL_COUNT = {
    1_048_576: 54,
    10_485_760: 544,
    104_857_600: 5_394,
    524_283_904: 26_994,
    524_285_952: 26_995,
    524_288_000: 26_995,
}

# Cache contract, frozen before benchmark implementation.
CACHE_CONTRACT = "exec-fuse-edit-cache-v1"
CLONE_METHOD = "independent-writable-byte-copy"
MACOS_STORE_DOMAIN = {
    "domain": "macos-store",
    "method": "darwin-mmap-msync-invalidate-mincore-v1",
    "acquisition": "map the case's independent Store copy and history catalog read-only, "
                   "instantiate resident pages, msync(MS_INVALIDATE|MS_SYNC) them away, "
                   "then read residency with mincore without faulting data pages",
    "check": "non-faulting mincore over every page of the Store copy and history catalog",
    "eligibility": "PASS only when the reported resident page count is 0 for the whole domain",
}
LINUX_FUSE_DOMAIN = {
    "domain": "linux-fuse-backing",
    "method": "declared-warm-not-invalidatable-v1",
    "acquisition": "the per-case Sandbox and Mount create a fresh backing directory and spool "
                   "inside the sandbox container; no host-side or product-side invalidation exists",
    "check": "not checkable without a benchmark-only eviction between Edit and Commit, which the "
             "contract forbids; a post-Commit probe cannot qualify the measured Commit",
    "eligibility": "INELIGIBLE for a cold claim: Commit may read the bytes Edit just wrote from "
                   "resident backing pages, so the raw number is a declared-warm diagnostic",
}
ELIGIBILITY_RULES = [
    "one performance sample per registered case and arm at a frozen source identity; a rerun is "
    "never a better number",
    "the performance command is the release SDK driver through target/release/examples/",
    "the Exec result must be exit_status == 0 with untruncated output; otherwise the attempt is FAIL",
    "the row needs one caller LFT1 root with complete edit and commit children and no dropped or "
    "overflowed output",
    "the row needs the SDK-only route check: expected public call count/order and zero forbidden or "
    "fallback route counts, proved by post-timer public Workspace status projection counts",
    "the row needs its declared macOS Store-domain residency check to report zero resident pages; "
    "an unacquirable or resident domain is INELIGIBLE",
    "the Linux FUSE/backing domain is declared warm-by-construction and cannot be invalidated; a "
    "row carrying it is INELIGIBLE for a cold claim and keeps its raw diagnostic number",
    "the independent verifier must PASS with a fresh reopen and the declared bounded oracle inside "
    "its 15 s hard cap",
    "cleanup must be confirmed through public SandboxApi::delete with no orphan container or volume",
    "the complete performance command, including fresh Sandbox create and delete, must finish "
    "inside 15 s",
]
ELIGIBILITY_INELIGIBLE_REASON = "fuse-backing-domain-served-by-resident-pages-not-invalidatable"
PREPARATION_GOAL_NS = {"1": 5_000_000_000, "10": 5_000_000_000, "100": 5_000_000_000,
                       "500": 10_000_000_000}
COMPLETE_COMMAND_BUDGET_NS = 15_000_000_000
VERIFIER_HARD_BUDGET_NS = 15_000_000_000
VERIFIER_DESIGN_GOAL_NS = 10_000_000_000
BOUNDED_ORACLE_BYTES = 196_608
BOUNDED_ORACLE_WINDOW = 65_536
FULL_DIGEST_MAX_BYTES = 104_857_600

# Declared editor algorithms. Each entry names the algorithm, the syscalls it
# may use and the operation shape it is allowed to have. `shift-grow` and
# `shift-shrink` are the two directions of the one frozen bounded-memory
# in-place window shift; `SHIFT_BLOCK_BYTES` is part of the algorithm identity
# and matches the projection's declared maximum transfer, so one block is one
# projection request.
SHIFT_BLOCK_BYTES = 128 * 1024
ALGORITHMS = {
    "positional-write": {
        "name": "single-positional-write",
        "syscalls": ["open", "fstat", "pwrite", "close"],
        "shape": "writes replacement_len bytes at start, extending the file when start is EOF",
    },
    "size-truncate": {
        "name": "bounded-truncate",
        "syscalls": ["open", "fstat", "ftruncate", "close"],
        "shape": "truncates the file to the declared final size",
    },
    "size-extend": {
        "name": "bounded-extend",
        "syscalls": ["open", "fstat", "ftruncate", "close"],
        "shape": "extends the file to the declared final size with zero bytes",
    },
    "shift-grow": {
        "name": "in-place-window-shift-grow",
        "syscalls": ["open", "fstat", "ftruncate", "pread", "pwrite", "close"],
        "block_bytes": SHIFT_BLOCK_BYTES,
        "shape": "extends the file by replacement_len - delete_len, then moves the affected "
                 "suffix [start + delete_len, initial) backward from the old end in "
                 "block_bytes blocks and writes the replacement over the stale window",
    },
    "shift-shrink": {
        "name": "in-place-window-shift-shrink",
        "syscalls": ["open", "fstat", "pread", "pwrite", "ftruncate", "close"],
        "block_bytes": SHIFT_BLOCK_BYTES,
        "shape": "moves the affected suffix [start + delete_len, initial) forward in "
                 "block_bytes blocks, truncates to the declared final size and writes the "
                 "replacement over the stale window",
    },
}


def shift_command(start, delete_len, replacement_len, initial, direction, payload_name):
    """The frozen command of one structural shift case."""
    parts = [TOOL_PATH, "shift", "--file", FIXTURE_PATH, "--offset", str(start),
             "--delete-length", str(delete_len), "--length", str(replacement_len),
             "--direction", direction, "--expect-size", str(initial)]
    if replacement_len:
        parts += ["--payload", PAYLOAD_DIRECTORY + "/" + payload_name]
    return " ".join(parts)


COMMAND_TEMPLATE = {
    "positional-write": (TOOL_PATH + " pwrite --file " + FIXTURE_PATH +
                         " --offset {start} --length {replacement_len} "
                         "--payload " + PAYLOAD_DIRECTORY + "/{payload_name} --expect-size {initial}"),
    "size-truncate": (TOOL_PATH + " truncate --file " + FIXTURE_PATH + " --size {final} "
                      "--expect-size {initial}"),
    "size-extend": (TOOL_PATH + " extend --file " + FIXTURE_PATH + " --size {final} "
                    "--expect-size {initial}"),
}


GOLDEN = 0x9E3779B97F4A7C15
MASK = (1 << 64) - 1
# The fixture stream is a splitmix64 arithmetic progression: word k is the
# finalizer applied to seed + (k + 1) * GOLDEN, so any offset is addressable.
BLOCK_BYTES = 8 << 20


def _words(seed, first_word, count):
    """`count` splitmix64 output words starting at word index `first_word`."""
    import numpy
    index = numpy.arange(first_word + 1, first_word + 1 + count, dtype=numpy.uint64)
    state = (numpy.uint64(seed) + index * numpy.uint64(GOLDEN)).astype(numpy.uint64)
    values = numpy.bitwise_xor(state, numpy.right_shift(state, numpy.uint64(30)))
    values = (values * numpy.uint64(0xBF58476D1CE4E5B9)).astype(numpy.uint64)
    values = numpy.bitwise_xor(values, numpy.right_shift(values, numpy.uint64(27)))
    values = (values * numpy.uint64(0x94D049BB133111EB)).astype(numpy.uint64)
    return numpy.bitwise_xor(values, numpy.right_shift(values, numpy.uint64(31)))


def chunk(seed, offset, length):
    """Bytes [offset, offset + length) of the stream, offset word aligned."""
    import numpy
    if length == 0:
        return b""
    if offset % 8:
        raise ValueError("stream offsets are word aligned")
    return _words(seed, offset // 8, length // 8).astype("<u8").tobytes()


def splitmix_bytes(seed, length):
    """One splitmix64 byte stream of `length` bytes from `seed`."""
    return chunk(seed, 0, length)


def fixture_sha256(size):
    """Digest of the pristine fixture, from the frozen recipe alone."""
    digest = hashlib.sha256()
    offset = 0
    while offset < size:
        take = min(BLOCK_BYTES, size - offset)
        digest.update(chunk(FIXTURE_GENERATOR_SEED, offset, take))
        offset += take
    return digest.hexdigest()


def final_sha256(size, start, delete_len, replacement):
    """Digest of the declared edit result, derived from the fixture recipe alone."""
    digest = hashlib.sha256()
    digest.update(chunk(FIXTURE_GENERATOR_SEED, 0, start))
    digest.update(replacement)
    tail = size - start - delete_len
    offset = start + delete_len
    written = 0
    while written < tail:
        take = min(BLOCK_BYTES, tail - written)
        digest.update(chunk(FIXTURE_GENERATOR_SEED, offset + written, take))
        written += take
    return digest.hexdigest()


def stream_bytes(seed, offset, length):
    """Bytes [offset, offset + length) of the stream, at any byte alignment."""
    if length <= 0:
        return b""
    base = offset - (offset % 8)
    extra = offset - base
    words = (extra + length + 7) // 8
    data = _words(seed, base // 8, words).astype("<u8").tobytes()
    return data[extra:extra + length]


def result_window(size, start, delete_len, replacement, offset, length):
    """Declared bytes of the edit result in [offset, offset + length).

    The result is the pristine fixture with [start, start + delete_len) replaced
    by `replacement`, so any window is a splice of those three regions.
    """
    final = size - delete_len + len(replacement)
    end = min(offset + length, final)
    out = bytearray()
    cursor = offset
    while cursor < end:
        if cursor < start:
            take = min(end - cursor, start - cursor)
            out.extend(stream_bytes(FIXTURE_GENERATOR_SEED, cursor, take))
        elif cursor < start + len(replacement):
            take = min(end - cursor, start + len(replacement) - cursor)
            out.extend(replacement[cursor - start:cursor - start + take])
        else:
            fixture_offset = start + delete_len + (cursor - start - len(replacement))
            take = min(end - cursor, 1 << 20)
            out.extend(stream_bytes(FIXTURE_GENERATOR_SEED, fixture_offset, take))
        cursor += len(out) - (cursor - offset) + (cursor - offset) - (cursor - offset)
        cursor = offset + len(out)
    return bytes(out)


def boundary_windows(start, delete_len, replacement_len, final):
    """The declared bounded oracle windows, at most BOUNDED_ORACLE_BYTES total.

    Window 0 covers the splice itself with one window of margin on each side and
    is capped so that window 1 still fits the frozen budget. Window 1 is the tail
    of the result, which is where a structural shift ends and where a truncation
    or extension is observed; it is declared only when it does not overlap
    window 0. Both windows are declared byte ranges with a pinned digest, never
    a substitute for the pristine expected bytes.
    """
    windows = []
    seam_begin = max(0, start - BOUNDED_ORACLE_WINDOW)
    seam_end = min(final, start + max(delete_len, replacement_len) + BOUNDED_ORACLE_WINDOW)
    seam_cap = BOUNDED_ORACLE_BYTES - BOUNDED_ORACLE_WINDOW
    if seam_end - seam_begin > seam_cap:
        seam_end = min(final, seam_begin + seam_cap)
    windows.append((seam_begin, seam_end - seam_begin))
    tail_begin = max(0, final - BOUNDED_ORACLE_WINDOW)
    if tail_begin >= seam_begin + windows[0][1]:
        windows.append((tail_begin, final - tail_begin))
    return windows


def payload_bytes(seed, length, kind):
    if kind == "zero":
        return bytes(length)
    return splitmix_bytes(seed, length)


def historical_id(operation, label):
    """The v0.1.6 selection ID this case inherits its semantics and target from."""
    if label in operation.get("capped_labels", ()):
        return f"{operation['key']}-on-{label}mib-result-capped-v2-ops-1"
    return f"{operation['key']}-on-{label}mib-ops-1"


def case_row(family_id, operation, fixture_bytes, label, target_ms):
    start, delete_len = operation["locate"](fixture_bytes)
    replacement_len = operation["replacement_len"]
    final = fixture_bytes - delete_len + replacement_len
    replacement = payload_bytes(operation["payload_seed"], replacement_len,
                                operation["replacement_kind"])
    payload_sha256 = hashlib.sha256(replacement).hexdigest()
    if payload_sha256 != operation["payload_sha256"]:
        raise ValueError(f"{family_id}/{operation['key']} payload recipe mismatch")
    historical = historical_id(operation, label)
    direction = None
    key = operation["algorithm"]
    if key == "shift":
        direction = "grow" if final > fixture_bytes else "shrink"
        if final == fixture_bytes:
            raise ValueError(f"{family_id}/{operation['key']} shift changes no size")
        key = f"shift-{direction}"
    algorithm = ALGORITHMS[key]
    windows = boundary_windows(start, delete_len, replacement_len, final)
    if sum(length for _, length in windows) > BOUNDED_ORACLE_BYTES:
        raise ValueError(f"{family_id}/{operation['key']} oracle budget")
    payload_name = operation["key"] + ".bin"
    if key == "positional-write":
        command = COMMAND_TEMPLATE[key].format(
            start=start, replacement_len=replacement_len, initial=fixture_bytes, final=final,
            payload_name=payload_name)
    elif key in ("size-truncate", "size-extend"):
        command = COMMAND_TEMPLATE[key].format(initial=fixture_bytes, final=final)
    else:
        command = shift_command(start, delete_len, replacement_len, fixture_bytes, direction,
                                payload_name)
    full_digest = final <= FULL_DIGEST_MAX_BYTES
    return {
        "family_id": family_id,
        "operation_key": operation["key"],
        "scenario_id": historical + SCENARIO_SUFFIX,
        "historical_selection_id": historical,
        "scenario_version": SCENARIO_VERSION,
        "route": ROUTE,
        "operation_contract_id": OPERATION_CONTRACT_ID,
        "operation_surface": OPERATION_SURFACE,
        "operation_entrypoint": OPERATION_ENTRYPOINT,
        "acknowledgement_boundary": ACKNOWLEDGEMENT_BOUNDARY,
        "orchestration_executor": ORCHESTRATION_EXECUTOR,
        "mutation_executor": MUTATION_EXECUTOR,
        "implementation_route": IMPLEMENTATION_ROUTE,
        "repetition": REPETITION,
        "fixture_bytes": fixture_bytes,
        "fixture_label": PREPARED_LABELS[fixture_bytes],
        "fixture_sha256": FIXTURE_SHA256[fixture_bytes],
        "fixture_generator_seed": FIXTURE_GENERATOR_SEED,
        "initial_count": FIXTURE_INITIAL_COUNT[fixture_bytes],
        "edit_start": start,
        "delete_len": delete_len,
        "replacement_len": replacement_len,
        "replacement_kind": operation["replacement_kind"],
        "replacement_sha256": payload_sha256,
        "payload_seed": operation["payload_seed"],
        "final_bytes": final,
        "final_sha256": final_sha256(fixture_bytes, start, delete_len, replacement),
        "editor_algorithm": algorithm["name"],
        "editor_syscalls": algorithm["syscalls"],
        "editor_shape": algorithm["shape"],
        "editor_direction": direction,
        "editor_block_bytes": algorithm.get("block_bytes"),
        "payload_source": (PAYLOAD_DIRECTORY + "/" + payload_name
                           if replacement_len else None),
        "command": command,
        "oracle": {
            "final_bytes": final,
            "windows": [
                {"offset": offset, "bytes": length,
                 "sha256": hashlib.sha256(
                     result_window(fixture_bytes, start, delete_len, replacement, offset,
                                   length)).hexdigest()}
                for offset, length in windows
            ],
            "bounded_window_bytes": sum(length for _, length in windows),
            "full_file_digest": full_digest,
            "full_file_bytes_verified": full_digest,
            "canonical_root_source": "public C1/C5 reader after the timed child exits",
            "branch_head_and_historical_root": True,
        },
        "g2_target_ms": target_ms,
        "g2_source": "#152 final report, C1/G2 candidate column, 0.01 ms precision",
        "performance_gate": "edit_commit_ns <= g2_target_ms at 0.01 ms precision",
        "cache_contract": CACHE_CONTRACT,
        "clone_method": CLONE_METHOD,
        "cache_domains": ["macos-store", "linux-fuse-backing"],
        "eligibility_rule": ELIGIBILITY_INELIGIBLE_REASON,
        "registration_status": "REGISTERED",
        "not_run_reason": None,
    }


def family_registry(family_id, operations, targets):
    rows = []
    for operation in operations:
        for label, size in SIZE_LABELS:
            fixture_bytes = operation.get("fixture_override", {}).get(label, size)
            historical = historical_id(operation, label)
            if historical not in targets:
                raise ValueError(f"{family_id}: no frozen G2 target for {historical}")
            rows.append(case_row(family_id, operation, fixture_bytes, label,
                                 targets[historical]))
    return rows


def registry():
    import sys as _sys
    if str(BENCH) not in _sys.path:
        _sys.path.insert(0, str(BENCH))
    from families import (edit_canonical_chunk_count as canonical,
                          edit_length_changing as changing,
                          edit_length_preserving as preserving)
    rows = preserving.registry() + changing.registry() + canonical.registry()
    if len(rows) != 56:
        raise ValueError("registered Exec/FUSE edit cardinality")
    ids = {row["scenario_id"] for row in rows}
    if len(ids) != 56:
        raise ValueError("registered Exec/FUSE edit scenario identity")
    for row in rows:
        if row["registration_status"] != "REGISTERED":
            raise ValueError(f"{row['scenario_id']}: every case is registered")
        if row["command"] is None or not row["editor_syscalls"]:
            raise ValueError(f"{row['scenario_id']}: registered row needs a command")
        if row["fixture_bytes"] not in PREPARED_SIZES:
            raise ValueError(f"{row['scenario_id']}: fixture size")
        if row["fixture_sha256"] != FIXTURE_SHA256[row["fixture_bytes"]]:
            raise ValueError(f"{row['scenario_id']}: fixture digest")
        if row["edit_start"] + row["delete_len"] > row["fixture_bytes"]:
            raise ValueError(f"{row['scenario_id']}: edit bounds")
        if row["final_bytes"] != (row["fixture_bytes"] - row["delete_len"]
                                  + row["replacement_len"]):
            raise ValueError(f"{row['scenario_id']}: final size")
        if row["editor_direction"] is None:
            if row["delete_len"] == 0 and row["replacement_len"] == 0:
                raise ValueError(f"{row['scenario_id']}: case changes nothing")
        elif (row["final_bytes"] > row["fixture_bytes"]) != (row["editor_direction"] == "grow"):
            raise ValueError(f"{row['scenario_id']}: shift direction")
        if len(row["final_sha256"]) != 64:
            raise ValueError(f"{row['scenario_id']}: final digest")
    return rows


def document():
    rows = registry()
    return {
        "schema": "core-fs-bench-pro-exec-fuse-edit-registry-v2",
        "route": ROUTE,
        "scenario_version": SCENARIO_VERSION,
        "scenario_suffix": SCENARIO_SUFFIX,
        "repetition": REPETITION,
        "operation_contract_id": OPERATION_CONTRACT_ID,
        "operation_surface": OPERATION_SURFACE,
        "operation_entrypoint": OPERATION_ENTRYPOINT,
        "acknowledgement_boundary": ACKNOWLEDGEMENT_BOUNDARY,
        "telemetry_key": TELEMETRY_KEY,
        "clock_id": CLOCK_ID,
        "capped_v1_duplicates_added": 0,
        "historical_registry": {"path": HISTORICAL_REGISTRY_PATH,
                                "sha256": HISTORICAL_REGISTRY_SHA256,
                                "scenario_version": 1},
        "prepared_sizes": PREPARED_SIZES,
        "fixture_recipe": {
            "generator_seed": FIXTURE_GENERATOR_SEED,
            "generator": "splitmix64-byte-stream-continued-v1",
            "path": FIXTURE_PATH,
            "directory_mode": FIXTURE_DIRECTORY_MODE,
            "file_mode": FIXTURE_FILE_MODE,
            "mtime_seconds": FIXTURE_MTIME_SECONDS,
            "sha256": FIXTURE_SHA256,
            "initial_count": FIXTURE_INITIAL_COUNT,
        },
        "cache_contract": CACHE_CONTRACT,
        "clone_method": CLONE_METHOD,
        "cache_domains": {"macos-store": MACOS_STORE_DOMAIN,
                          "linux-fuse-backing": LINUX_FUSE_DOMAIN},
        "eligibility_rules": ELIGIBILITY_RULES,
        "budgets": {
            "preparation_goal_ns": PREPARATION_GOAL_NS,
            "complete_command_budget_ns": COMPLETE_COMMAND_BUDGET_NS,
            "verifier_hard_budget_ns": VERIFIER_HARD_BUDGET_NS,
            "verifier_design_goal_ns": VERIFIER_DESIGN_GOAL_NS,
            "bounded_oracle_bytes": BOUNDED_ORACLE_BYTES,
            "bounded_oracle_window": BOUNDED_ORACLE_WINDOW,
            "full_file_digest_max_bytes": FULL_DIGEST_MAX_BYTES,
        },
        "algorithms": ALGORITHMS,
        "cases": rows,
    }


def registry_body():
    return json.dumps(document(), sort_keys=True, indent=2).encode() + b"\n"


def registry_sha256():
    return hashlib.sha256(registry_body()).hexdigest()


def main():
    out = BENCH / REGISTRY_PATH
    body = registry_body()
    digest = hashlib.sha256(body).hexdigest()
    if "--check" in __import__("sys").argv:
        if out.read_bytes() != body:
            raise SystemExit("registry file differs from the frozen contract")
        if digest != REGISTRY_SHA256:
            raise SystemExit("registry digest differs from the frozen value")
    else:
        out.write_bytes(body)
    print(json.dumps({"registry": str(out), "sha256": digest, "frozen": REGISTRY_SHA256,
                      "cases": len(registry())}, sort_keys=True))


if __name__ == "__main__":
    main()
