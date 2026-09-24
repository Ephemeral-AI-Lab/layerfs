"""Frozen #232 Workspace Exec/FUSE edit contract (scenario version 1).

This module is the single source of truth for the 56 registered
`sdk-exec-fuse-edit-commit-v1` cases. It freezes, before any benchmark driver or
timed sample exists:

* the operation contract (entrypoint, surface, acknowledgement, executors),
* the six reusable pristine fixture sizes and their byte recipe, including the
  capped 500 MiB inputs and repetition 1,
* each case's command template, editor algorithm and allowed syscalls,
* each case's pre/post identity (size, payload digest, bounded oracle window),
* the per-case historical G2 engineering target taken from the #152 final
  report's C1/G2 column (0.01 ms precision),
* the declared cache method and the eligibility rule.

Run `python3 shared/edit_contract.py` from this directory to regenerate
`registry/workspace-exec-edit-v1.json` and print its SHA-256.
"""
import hashlib
import json
from pathlib import Path

HERE = Path(__file__).resolve().parent
BENCH = HERE.parent
ROUTE = "sdk-exec-fuse-edit-commit-v1"
# SHA-256 of the committed registry document; regenerate with this module.
REGISTRY_SHA256 = "e4b4d2fc67cb1630dc8f15283db3085663466071bb373c829a6026ffddb66aab"
SCENARIO_VERSION = 1
REPETITION = 1
OPERATION_CONTRACT_ID = "workspace-exec-fuse-edit-commit-v1"
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
STRUCTURAL_NOT_RUN_REASON = (
    "structural-shift-algorithm-unfrozen: no authentic POSIX/FUSE algorithm is frozen yet for "
    "insert/delete/prepend/replace-grow/replace-shrink; an in-place window shift moves up to "
    "hundreds of MiB through the projection and a temporary-file-and-rename save needs up to "
    "500 MiB against a 16 MiB /tmp and a 1 GiB Workspace disk budget; the case stays registered "
    "and visible as NOT_RUN"
)

# Declared editor algorithms. Each entry names the algorithm and the syscalls it may use.
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
    "structural-shift": {
        "name": "unfrozen-in-place-window-shift",
        "syscalls": [],
        "shape": "not frozen: the structural length-changing cases remain NOT_RUN",
    },
}
COMMAND_TEMPLATE = {
    "positional-write": (TOOL_PATH + " pwrite --file " + FIXTURE_PATH +
                         " --offset {start} --length {replacement_len} "
                         "--payload " + PAYLOAD_DIRECTORY + "/{payload_name} --expect-size {initial}"),
    "size-truncate": (TOOL_PATH + " truncate --file " + FIXTURE_PATH + " --size {final} "
                      "--expect-size {initial}"),
    "size-extend": (TOOL_PATH + " extend --file " + FIXTURE_PATH + " --size {final} "
                    "--expect-size {initial}"),
    "structural-shift": None,
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


def boundary_window(start, delete_len, replacement_len, final):
    """The declared bounded oracle window: the edit plus one window either side."""
    begin = max(0, start - BOUNDED_ORACLE_WINDOW)
    end = min(final, start + delete_len + replacement_len + BOUNDED_ORACLE_WINDOW)
    if end - begin > BOUNDED_ORACLE_BYTES:
        end = begin + BOUNDED_ORACLE_BYTES
    return begin, end - begin


def payload_bytes(seed, length, kind):
    if kind == "zero":
        return bytes(length)
    return splitmix_bytes(seed, length)


def historical_id(operation, label):
    """The v0.1.6 selection ID this case inherits its semantics and target from."""
    if label in operation.get("capped_labels", ()):
        return f"{operation['key']}-on-{label}mib-result-capped-v2-ops-1"
    return f"{operation['key']}-on-{label}mib-ops-1"


def case_row(family_id, operation, fixture_bytes, label, target_ms, structural=False):
    start, delete_len = operation["locate"](fixture_bytes)
    replacement_len = operation["replacement_len"]
    final = fixture_bytes - delete_len + replacement_len
    replacement = payload_bytes(operation["payload_seed"], replacement_len,
                                operation["replacement_kind"])
    payload_sha256 = hashlib.sha256(replacement).hexdigest()
    if payload_sha256 != operation["payload_sha256"]:
        raise ValueError(f"{family_id}/{operation['key']} payload recipe mismatch")
    historical = historical_id(operation, label)
    algorithm = ALGORITHMS[operation["algorithm"]]
    begin, length = boundary_window(start, delete_len, replacement_len, final)
    return {
        "family_id": family_id,
        "operation_key": operation["key"],
        "scenario_id": historical + "-exec-v1",
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
        "final_sha256": None if structural else final_sha256(fixture_bytes, start, delete_len,
                                                             replacement),
        "editor_algorithm": algorithm["name"],
        "editor_syscalls": algorithm["syscalls"],
        "editor_shape": algorithm["shape"],
        "payload_source": (PAYLOAD_DIRECTORY + "/" + operation["key"] + ".bin"
                           if replacement_len else None),
        "command": (None if structural else
                    COMMAND_TEMPLATE[operation["algorithm"]].format(
                        start=start, replacement_len=replacement_len,
                        initial=fixture_bytes, final=final,
                        payload_name=operation["key"] + ".bin")),
        "oracle": {
            "final_bytes": final,
            "bounded_window_offset": begin,
            "bounded_window_bytes": length,
            "full_file_digest": (not structural) and final <= FULL_DIGEST_MAX_BYTES,
            "full_file_bytes_verified": (not structural) and final <= FULL_DIGEST_MAX_BYTES,
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
        "registration_status": "NOT_RUN" if structural else "REGISTERED",
        "not_run_reason": STRUCTURAL_NOT_RUN_REASON if structural else None,
    }


def family_registry(family_id, operations, targets, structural_keys=()):
    rows = []
    for operation in operations:
        for label, size in SIZE_LABELS:
            fixture_bytes = operation.get("fixture_override", {}).get(label, size)
            historical = historical_id(operation, label)
            if historical not in targets:
                raise ValueError(f"{family_id}: no frozen G2 target for {historical}")
            rows.append(case_row(family_id, operation, fixture_bytes, label,
                                 targets[historical], operation["key"] in structural_keys))
    return rows


def registry():
    import sys as _sys
    if str(BENCH) not in _sys.path:
        _sys.path.insert(0, str(BENCH))
    from families import (workspace_exec_edit_canonical_chunk_count as canonical,
                          workspace_exec_edit_length_changing as changing,
                          workspace_exec_edit_length_preserving as preserving)
    rows = preserving.registry() + changing.registry() + canonical.registry()
    if len(rows) != 56:
        raise ValueError("registered Exec/FUSE edit cardinality")
    ids = {row["scenario_id"] for row in rows}
    if len(ids) != 56:
        raise ValueError("registered Exec/FUSE edit scenario identity")
    for row in rows:
        if row["fixture_bytes"] not in PREPARED_SIZES:
            raise ValueError(f"{row['scenario_id']}: fixture size")
        if row["fixture_sha256"] != FIXTURE_SHA256[row["fixture_bytes"]]:
            raise ValueError(f"{row['scenario_id']}: fixture digest")
        if row["registration_status"] == "REGISTERED":
            if row["command"] is None or not row["editor_syscalls"]:
                raise ValueError(f"{row['scenario_id']}: registered row needs a command")
            if row["edit_start"] + row["delete_len"] > row["fixture_bytes"]:
                raise ValueError(f"{row['scenario_id']}: edit bounds")
            if row["final_bytes"] != (row["fixture_bytes"] - row["delete_len"]
                                      + row["replacement_len"]):
                raise ValueError(f"{row['scenario_id']}: final size")
        elif row["not_run_reason"] != STRUCTURAL_NOT_RUN_REASON:
            raise ValueError(f"{row['scenario_id']}: NOT_RUN needs the frozen reason")
    return rows


def document():
    rows = registry()
    return {
        "schema": "core-fs-bench-pro-exec-fuse-edit-registry-v1",
        "route": ROUTE,
        "scenario_version": SCENARIO_VERSION,
        "repetition": REPETITION,
        "operation_contract_id": OPERATION_CONTRACT_ID,
        "operation_surface": OPERATION_SURFACE,
        "operation_entrypoint": OPERATION_ENTRYPOINT,
        "acknowledgement_boundary": ACKNOWLEDGEMENT_BOUNDARY,
        "telemetry_key": TELEMETRY_KEY,
        "clock_id": CLOCK_ID,
        "capped_v1_duplicates_added": 0,
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
    out = BENCH / "registry/workspace-exec-edit-v1.json"
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
