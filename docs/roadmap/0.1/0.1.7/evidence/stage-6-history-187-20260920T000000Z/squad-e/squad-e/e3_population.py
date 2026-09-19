#!/usr/bin/env python3
"""E3: population of the two #185 halves on the history-stride10 corpus.

Reads only the corpus (manifest + oracles).  A path-state is a regular file
(mode 100644/100755); directories (40755) and symlinks (120000) are excluded
from every count below, and the excluded totals are printed so the exclusion
is visible.

T = small_file_threshold_bytes = 131072 (content-storage-design.md section 4:
0 < n < T -> WHOLE_FILE, n >= T -> CHUNKED).

    python3 e3_population.py [--json out.json]
"""
from __future__ import annotations

import argparse
import collections
import json
import sys
from pathlib import Path

CORPUS = Path("/Users/yifanxu/Ephemeral-AI-Lab/deepseek-history-data")
T = 131_072
SELECTION = tuple(sorted(set(range(1, 158, 10)) | {157}))


def load_state(index: int, checkpoints: list[dict]) -> dict[str, tuple[int, int, str]]:
    """hex(path) -> (size, mode, content sha256) for regular files only."""
    cp = checkpoints[index - 1]
    oracle = json.loads((CORPUS / "oracles" / f"{cp['sha']}.json").read_bytes())
    out: dict[str, tuple[int, int, str]] = {}
    for raw_path, (mode, size, digest) in oracle.items():
        if not mode.startswith("100"):
            continue
        out[raw_path] = (int(size), int(mode), str(digest))
    return out


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--json", type=Path)
    args = parser.parse_args()

    manifest = json.loads((CORPUS / "checkpoint-manifest.json").read_bytes())
    checkpoints = manifest["checkpoints"]
    states = {index: load_state(index, checkpoints) for index in SELECTION}

    report: dict[str, object] = {
        "corpus": str(CORPUS),
        "cutoff_T": T,
        "selection": list(SELECTION),
        "states": [],
        "distinct_ever_chunked": 0,
        "crossings": {},
        "in_place_chunk_edits": {},
    }

    ever_chunked: set[str] = set()
    ever_whole: set[str] = set()
    # crossings[(direction)] = list of records
    crossings: dict[str, list[dict]] = {"small_to_large": [], "large_to_small": []}
    # in-place chunked edits between consecutive selected states
    inplace: list[dict] = []

    for position, index in enumerate(SELECTION):
        state = states[index]
        chunked = {p: v for p, v in state.items() if v[0] >= T}
        whole = {p: v for p, v in state.items() if v[0] < T}
        ever_chunked |= set(chunked)
        ever_whole |= set(whole)
        report["states"].append(
            {
                "index": index,
                "sha": checkpoints[index - 1]["sha"],
                "regular_files": len(state),
                "chunked_files": len(chunked),
                "chunked_bytes": sum(v[0] for v in chunked.values()),
                "whole_files": len(whole),
                "whole_bytes": sum(v[0] for v in whole.values()),
                "empty_files": sum(1 for v in state.values() if v[0] == 0),
            }
        )
        if position == 0:
            continue
        previous = states[SELECTION[position - 1]]
        common = set(previous) & set(state)
        for path in common:
            before_size, _m, before_sha = previous[path]
            after_size, _m2, after_sha = state[path]
            before_chunked = before_size >= T
            after_chunked = after_size >= T
            if before_chunked == after_chunked:
                if before_chunked and before_sha != after_sha:
                    inplace.append(
                        {
                            "from_state": SELECTION[position - 1],
                            "to_state": index,
                            "path": path,
                            "base_bytes": before_size,
                            "result_bytes": after_size,
                        }
                    )
                continue
            direction = "small_to_large" if after_chunked else "large_to_small"
            crossings[direction].append(
                {
                    "from_state": SELECTION[position - 1],
                    "to_state": index,
                    "path": path,
                    "base_bytes": before_size,
                    "result_bytes": after_size,
                }
            )

    report["distinct_ever_chunked"] = len(ever_chunked)
    report["distinct_ever_whole"] = len(ever_whole)
    for direction, rows in crossings.items():
        report["crossings"][direction] = {
            "events": len(rows),
            "distinct_paths": len({r["path"] for r in rows}),
            "result_bytes": sum(r["result_bytes"] for r in rows),
            "base_bytes": sum(r["base_bytes"] for r in rows),
            "rows": rows,
        }
    report["in_place_chunk_edits"] = {
        "events": len(inplace),
        "distinct_paths": len({r["path"] for r in inplace}),
        "result_bytes": sum(r["result_bytes"] for r in inplace),
        "base_bytes": sum(r["base_bytes"] for r in inplace),
        "rows": inplace,
    }

    # per-path trajectory of every distinct ever-chunked file
    trajectories = {}
    for path in sorted(ever_chunked):
        trajectory = []
        for index in SELECTION:
            entry = states[index].get(path)
            trajectory.append(None if entry is None else entry[0])
        trajectories[path] = trajectory
    report["chunked_trajectories"] = trajectories

    text = json.dumps(report, indent=1, sort_keys=True)
    if args.json:
        args.json.write_text(text + "\n")
    print(json.dumps({k: v for k, v in report.items()
                      if k not in ("chunked_trajectories",)}, indent=1, sort_keys=True))
    return 0


if __name__ == "__main__":
    sys.exit(main())
