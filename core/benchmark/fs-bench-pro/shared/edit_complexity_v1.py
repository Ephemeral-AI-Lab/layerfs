"""Frozen #232 Phase 1C diagnostic rows, independent of the v3/v4 registries."""

import hashlib
import json
from pathlib import Path
import sys

BENCH = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(BENCH))
from shared import edit_contract as base, edit_insert_v4  # noqa: E402

REGISTRY_PATH = "registry/workspace-exec-complexity-v1.json"
REGISTRY_SHA256 = "670973c95d747828fe2db02c82323a4df4f585bc3d9ed011e2c378159a731cad"
PAYLOAD_NAME = "insert-middle-4k.bin"
SOURCE_ROW = edit_insert_v4.registry()[0]
PAYLOAD = base.payload_bytes(
    SOURCE_ROW["payload_seed"], 4096, SOURCE_ROW["replacement_kind"]
)
PAYLOAD_SHA256 = hashlib.sha256(PAYLOAD).hexdigest()


def _update_fixture(digest, start, stop):
    while start < stop:
        length = min(8 << 20, stop - start)
        digest.update(base.stream_bytes(base.FIXTURE_GENERATOR_SEED, start, length))
        start += length


def _fixture_digest(size):
    if size in base.FIXTURE_SHA256:
        return base.FIXTURE_SHA256[size]
    digest = hashlib.sha256()
    _update_fixture(digest, 0, size)
    return digest.hexdigest()


def _result_digest(size, count):
    digest = hashlib.sha256()
    if count:
        width = 4096 // count
        cursor = 0
        for index in range(count):
            offset = 524288 + index * 256
            _update_fixture(digest, cursor, offset)
            digest.update(PAYLOAD[index * width:(index + 1) * width])
            cursor = offset
        _update_fixture(digest, cursor, size)
    else:
        start = (size - 4096) // 2
        _update_fixture(digest, 0, start)
        digest.update(PAYLOAD)
        _update_fixture(digest, start + 4096, size)
    return digest.hexdigest()


def rows():
    values = [
        ("repeated-1", 1_048_576, 1),
        ("repeated-32", 1_048_576, 32),
        ("repeated-128", 1_048_576, 128),
        ("locality-1m", 1_048_576, 0),
        ("locality-100m", 104_857_600, 0),
        ("locality-capped500m", 524_283_904, 0),
        ("cutoff-below", 131_071, 0),
        ("cutoff-at", 131_072, 0),
        ("cutoff-above", 131_073, 0),
    ]
    result = []
    for label, size, count in values:
        batch = bool(count)
        offset = 524288 if batch else (size - 4096) // 2
        command = [base.TOOL_PATH, "splice-batch" if batch else "splice",
                   "--file", base.FIXTURE_PATH, "--offset", str(offset),
                   "--delete-length", "0" if batch else "4096",
                   "--length", "4096", "--payload",
                   f"{base.PAYLOAD_DIRECTORY}/{PAYLOAD_NAME}",
                   "--expect-size", str(size)]
        if batch:
            command += ["--count", str(count)]
        command += ["--output-version", "5" if batch else "4"]
        result.append({
            "label": label, "scenario_id": label + "-complexity-v1",
            "group": label.split("-", 1)[0], "fixture_bytes": size,
            "fixture_sha256": _fixture_digest(size),
            "edit_start": offset, "delete_len": 0 if batch else 4096,
            "replacement_len": 4096, "edit_count": count or 1,
            "final_bytes": size + 4096 if batch else size,
            "final_sha256": _result_digest(size, count),
            "replacement_sha256": PAYLOAD_SHA256,
            "operation_contract_id": (
                "workspace-exec-fuse-range-splice-batch-commit-v1" if batch else
                "workspace-exec-fuse-range-splice-complexity-commit-v1"),
            "command": " ".join(command),
            "expected_state_callbacks": 2 * (count or 1),
            "expected_edit_callbacks": count or 1,
            "expected_accepted_bytes": 4096,
            "expected_shifted_suffix_bytes": 0,
            "expected_chunked": int((size + 4096 if batch else size) >= 131072),
        })
    return result


def body():
    return (json.dumps({"schema": "issue232-phase1c-complexity-registry-v1",
                        "payload_sha256": PAYLOAD_SHA256, "cases": rows()},
                       indent=2, sort_keys=True) + "\n").encode()


def check():
    actual = (BENCH / REGISTRY_PATH).read_bytes()
    if actual != body() or hashlib.sha256(actual).hexdigest() != REGISTRY_SHA256:
        raise ValueError("Phase 1C registry differs from frozen generator/hash")


if __name__ == "__main__":
    import sys
    if sys.argv[1:] == ["--write"]:
        (BENCH / REGISTRY_PATH).write_bytes(body())
    else:
        check()
    print(hashlib.sha256((BENCH / REGISTRY_PATH).read_bytes()).hexdigest())
