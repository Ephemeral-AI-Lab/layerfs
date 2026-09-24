"""Frozen four-case #241 mounted range-splice selection, derived from sealed v2 inputs."""
import copy
import hashlib
import json
from pathlib import Path
import sys

BENCH = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(BENCH))
from shared import edit_contract as base

REGISTRY_PATH = "registry/workspace-exec-insert-v3.json"
REGISTRY_SHA256 = "a11be713e3f7151b5586e4c7899361d33fadf77f06b9caf1ec8ad2ca5141e167"
ROUTE = "sdk-exec-fuse-range-splice-commit-v1"
OPERATION_CONTRACT_ID = "workspace-exec-fuse-range-splice-commit-v3"
SCENARIO_VERSION = 3
ABI = {
    "contract": "linux-projected-range-request-v2",
    "version": 2,
    "state": {"magic": "LFS2", "command": "0xc058f540", "input_bytes": 88,
              "output_bytes": 88},
    "edit": {"magic": "LFE2", "command": "0x5060f541", "input_bytes": 4192,
             "output_bytes": 0, "max_replacement_bytes": 4096},
    "caller_order": ["open", "STATE", "EDIT", "STATE", "fstat", "bounded-readback"],
}


def registry():
    source = BENCH / base.REGISTRY_PATH
    raw = source.read_bytes()
    if hashlib.sha256(raw).hexdigest() != base.REGISTRY_SHA256:
        raise ValueError("v2 source registry identity changed")
    old = json.loads(raw)
    if old.get("schema") != "core-fs-bench-pro-exec-fuse-edit-registry-v2":
        raise ValueError("v2 source registry schema changed")
    selected = [row for row in old["cases"] if row["operation_key"] == "insert-middle-4k"]
    if len(selected) != 4 or {row["fixture_bytes"] for row in selected} != {
        1_048_576, 10_485_760, 104_857_600, 524_283_904
    }:
        raise ValueError("v2 middle-insert selection changed")
    rows = []
    for source_row in selected:
        row = copy.deepcopy(source_row)
        if (not row["scenario_id"].endswith("-exec-v2") or row["delete_len"] != 0
                or row["replacement_len"] != 4096 or row["registration_status"] != "REGISTERED"):
            raise ValueError("v2 source row is not the frozen 4 KiB middle insert")
        row.update({
            "scenario_id": row["scenario_id"][:-len("-exec-v2")] + "-exec-v3",
            "scenario_version": SCENARIO_VERSION,
            "route": ROUTE,
            "operation_contract_id": OPERATION_CONTRACT_ID,
            "mutation_executor": "workspace-exec-linux-fuse-range-tool",
            "editor_algorithm": "linux-fuse-ioctl-range-splice-v2",
            "editor_syscalls": ["open", "fstat", "ioctl", "pread", "close"],
            "editor_shape": "STATE, one bounded EDIT, then STATE/fstat/bounded readback; "
                            "no suffix copy or fallback",
            "editor_direction": None,
            "editor_block_bytes": None,
            "carrier_protocol": ABI["contract"],
            "expected_range_state_callbacks": 2,
            "expected_range_callbacks": 1,
            "expected_write_callbacks": 0,
            "expected_shifted_suffix_bytes": 0,
            "command": (
                f"{base.TOOL_PATH} splice --file {base.FIXTURE_PATH} "
                f"--offset {row['edit_start']} --delete-length 0 --length 4096 "
                f"--payload {row['payload_source']} --expect-size {row['fixture_bytes']}"
            ),
        })
        rows.append(row)
    if len({row["scenario_id"] for row in rows}) != 4:
        raise ValueError("v3 scenario identity collision")
    return rows


def document():
    return {
        "schema": "core-fs-bench-pro-exec-fuse-insert-registry-v3",
        "route": ROUTE,
        "scenario_version": SCENARIO_VERSION,
        "source_registry": {"path": base.REGISTRY_PATH, "sha256": base.REGISTRY_SHA256},
        "operation_contract_id": OPERATION_CONTRACT_ID,
        "operation_surface": base.OPERATION_SURFACE,
        "operation_entrypoint": base.OPERATION_ENTRYPOINT,
        "acknowledgement_boundary": base.ACKNOWLEDGEMENT_BOUNDARY,
        "telemetry_key": base.TELEMETRY_KEY,
        "cache_contract": base.CACHE_CONTRACT,
        "clone_method": base.CLONE_METHOD,
        "cache_domains": {"macos-store": base.MACOS_STORE_DOMAIN,
                          "linux-fuse-backing": base.LINUX_FUSE_DOMAIN},
        "carrier_abi": ABI,
        "cases": registry(),
    }


def registry_body():
    return json.dumps(document(), sort_keys=True, indent=2).encode() + b"\n"


def validate_registry():
    raw = (BENCH / REGISTRY_PATH).read_bytes()
    if hashlib.sha256(raw).hexdigest() != REGISTRY_SHA256 or raw != registry_body():
        raise ValueError("v3 registry differs from its frozen contract")


if __name__ == "__main__":
    import sys
    if "--check" in sys.argv:
        validate_registry()
    else:
        (BENCH / REGISTRY_PATH).write_bytes(registry_body())
    print(hashlib.sha256((BENCH / REGISTRY_PATH).read_bytes()).hexdigest())
