#!/usr/bin/env python3
"""Count the frozen v2 POSIX shift bytes against Core's replay contract."""
import json
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[6]
REGISTRY = ROOT / "core/benchmark/fs-bench-pro/registry/workspace-exec-edit-v2.json"
BASELINE = ROOT / "core/docs/issues/232/exec-fuse-edit-v2-baseline.md"
BRIDGE = ROOT / "core/crates/layerfs-bridge/src/contract/request.rs"
PIECES = ROOT / "core/crates/layerfs-workspace/src/overlay/pieces.rs"
WORKLOAD = ROOT / "core/benchmark/fs-bench-pro/workload/src/main.rs"
FUSE = ROOT / "core/crates/layerfs-fuse/src/adapter.rs"
WORKSPACE = ROOT / "core/crates/layerfs-workspace/src/types.rs"


def main():
    assert "pub const MAX_REPLAY: u64 = 8 * 1024 * 1024;" in BRIDGE.read_text()
    assert "if edits > 256 || bytes > if complete { MAX_FILE } else { MAX_REPLAY }" in PIECES.read_text()
    assert "const SHIFT_BLOCK_BYTES: usize = 128 * 1024;" in WORKLOAD.read_text()
    assert ".set_max_write(MAX_READ_BYTES as u32)" in FUSE.read_text()
    assert "pub const MAX_READ_BYTES: usize = 128 * 1024;" in WORKSPACE.read_text()
    assert "let mut new = crate::backing::metadata_index::vector(1024)?;" in PIECES.read_text()
    registry = json.loads(REGISTRY.read_text())
    assert registry["scenario_version"] == 2 and len(registry["cases"]) == 56
    limit = 8 * 1024 * 1024
    shifts = []
    for case in registry["cases"]:
        if not case["editor_algorithm"].startswith("in-place-window-shift"):
            continue
        moved = case["fixture_bytes"] - case["edit_start"] - case["delete_len"]
        assert moved >= 0
        shifts.append({
            "scenario_id": case["scenario_id"],
            "shifted_suffix_bytes": moved,
            "minimum_shift_blocks": (moved + 128 * 1024 - 1) // (128 * 1024),
            "over_replay_limit": moved > limit,
        })
    assert len(shifts) == 20
    over = {row["scenario_id"].removesuffix("-exec-v2") for row in shifts
            if row["over_replay_limit"]}
    failures = set(re.findall(
        r"^\| `([^`]+)` \| .*? \| FAIL \| NOT_RUN \|",
        BASELINE.read_text(), re.M,
    ))
    assert len(over) == 11 and over == failures
    largest = max(case["fixture_bytes"] for case in registry["cases"])
    minimum_fresh_writes = (largest + 128 * 1024 - 1) // (128 * 1024)
    assert largest == 500 * 1024 * 1024 and minimum_fresh_writes == 4000
    result = {"schema": "issue232-v2-posix-replay-limit-analysis-v1",
              "registry": str(REGISTRY.relative_to(ROOT)),
              "replay_limit_bytes": limit,
              "registered_cases": len(registry["cases"]),
              "in_place_shift_cases": len(shifts),
              "shift_cases_over_replay_limit": len(over),
              "baseline_failures": sorted(failures),
              "over_limit_matches_all_baseline_failures": over == failures,
              "fresh_file_500mib_source_bound": {
                  "file_bytes": largest,
                  "max_fuse_write_bytes": 128 * 1024,
                  "minimum_write_callbacks": minimum_fresh_writes,
                  "current_max_pieces": 1024,
                  "qualification": "each current WRITE owns a distinct payload record; adjacent Local pieces require one payload identity to coalesce",
              },
              "cases": shifts,
              "classification": "source-and-registry deduction; no new performance sample"}
    print(json.dumps(result, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
