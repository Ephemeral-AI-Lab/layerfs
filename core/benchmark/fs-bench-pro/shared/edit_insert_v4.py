"""Prospective four-case #241 splice selection with exact portable metadata."""
import copy
import hashlib
import json
from pathlib import Path
import sys

BENCH = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(BENCH))
from shared import edit_insert_v3 as prior

REGISTRY_PATH = "registry/workspace-exec-insert-v4.json"
REGISTRY_SHA256 = "9decc295b688083db7b1f6899ebbc315eeeddfa28d0fafe68537507d12394fbb"
ROUTE = prior.ROUTE
OPERATION_CONTRACT_ID = "workspace-exec-fuse-range-splice-commit-v4"
SCENARIO_VERSION = 4
ABI = prior.ABI
EXPECTED_CANONICAL = {
    1_048_576: ("9f557d08f00e034cac5eab1d3a3a7e86d69c560f1fa7c502d4234f52b3ab5984", 56),
    10_485_760: ("4911a494f8c0e267e9bb7d1f9afe53d8dc753dc71565864242f7e40a6c080275", 546),
    104_857_600: ("f4b508184e8ee3d894266b6e043c2534f6cae2337726d1447102f58c62927086", 5396),
    524_283_904: ("8a5534a728677961df1b83b226137ab82fa236b620e8f87c06919c2338770901", 26996),
}


def registry():
    prior.validate_registry()
    rows = []
    for source in prior.registry():
        if (source["scenario_version"] != 3 or prior.base.FIXTURE_FILE_MODE != 416 or
                not source["scenario_id"].endswith("-exec-v3") or
                source["fixture_bytes"] not in EXPECTED_CANONICAL):
            raise ValueError("v3 splice selection changed")
        row = copy.deepcopy(source)
        root, count = EXPECTED_CANONICAL[row["fixture_bytes"]]
        row.update({
            "scenario_id": source["scenario_id"][:-len("-exec-v3")] + "-exec-v4",
            "scenario_version": SCENARIO_VERSION,
            "operation_contract_id": OPERATION_CONTRACT_ID,
            "command": source["command"] + " --output-version 4",
            "canonical_root_expected": root,
            "canonical_count_expected": count,
            "tool_output_version": 4,
            "file_mode": prior.base.FIXTURE_FILE_MODE,
        })
        row["oracle"]["full_file_bytes_verified"] = True
        row["oracle"]["full_file_digest"] = True
        rows.append(row)
    if len(rows) != 4 or len({row["scenario_id"] for row in rows}) != 4:
        raise ValueError("v4 splice selection is incomplete")
    return rows


def document():
    return {
        "schema": "core-fs-bench-pro-exec-fuse-insert-registry-v4",
        "route": ROUTE,
        "scenario_version": SCENARIO_VERSION,
        "source_registry": {"path": prior.REGISTRY_PATH, "sha256": prior.REGISTRY_SHA256},
        "operation_contract_id": OPERATION_CONTRACT_ID,
        "operation_surface": prior.base.OPERATION_SURFACE,
        "operation_entrypoint": prior.base.OPERATION_ENTRYPOINT,
        "acknowledgement_boundary": prior.base.ACKNOWLEDGEMENT_BOUNDARY,
        "telemetry_key": prior.base.TELEMETRY_KEY,
        "cache_contract": prior.base.CACHE_CONTRACT,
        "clone_method": prior.base.CLONE_METHOD,
        "cache_domains": {"macos-store": prior.base.MACOS_STORE_DOMAIN,
                          "linux-fuse-backing": prior.base.LINUX_FUSE_DOMAIN},
        "carrier_abi": ABI,
        "tool_output_contract": {
            "flag": "--output-version 4",
            "format": "one untruncated JSON line",
            "fields": ["status", "operation", "direction", "final_bytes", "shifted_bytes",
                       "mtime_seconds", "mtime_nanoseconds"],
            "mtime_source": "post-EDIT STATE, checked against same-descriptor fstat",
        },
        "cases": registry(),
    }


def registry_body():
    return json.dumps(document(), sort_keys=True, indent=2).encode() + b"\n"


def validate_registry():
    raw = (BENCH / REGISTRY_PATH).read_bytes()
    if hashlib.sha256(raw).hexdigest() != REGISTRY_SHA256 or raw != registry_body():
        raise ValueError("v4 registry differs from its frozen contract")


if __name__ == "__main__":
    if "--check" in sys.argv:
        validate_registry()
    else:
        (BENCH / REGISTRY_PATH).write_bytes(registry_body())
    print(hashlib.sha256((BENCH / REGISTRY_PATH).read_bytes()).hexdigest())
