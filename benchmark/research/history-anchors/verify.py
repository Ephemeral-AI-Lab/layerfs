#!/usr/bin/env python3
"""Verify the two frozen history-anchor evidence files without dependencies."""

from __future__ import annotations

import argparse
import csv
import hashlib
import json
from pathlib import Path


DEPTHS = {1, 10, 100, 1000}


def load(path: Path) -> list[dict[str, str]]:
    with path.open(newline="", encoding="utf-8") as handle:
        rows = list(csv.DictReader(handle, delimiter="\t"))
    if len(rows) != 56:
        raise AssertionError(f"{path}: expected 56 rows, found {len(rows)}")
    return rows


def one(rows: list[dict[str, str]], depth: int, strategy: str, operation: str) -> dict[str, str]:
    matches = [
        row
        for row in rows
        if int(row["depth"]) == depth
        and row["strategy"] == strategy
        and row["operation"] == operation
    ]
    if len(matches) != 1:
        raise AssertionError(f"expected one row for {depth}/{strategy}/{operation}")
    return matches[0]


def verify(path: Path) -> dict[str, object]:
    rows = load(path)
    assert {int(row["depth"]) for row in rows} == DEPTHS
    assert all(row["correct"] == "true" for row in rows)
    assert all(row["store_unchanged"] == "true" for row in rows)
    for depth in DEPTHS:
        assert int(one(rows, depth, "baseline", "early-lookup")["nodes_visited"]) == 0
        baseline = one(rows, depth, "baseline", "distant-diff")
        multiscale = one(rows, depth, "multiscale", "distant-diff")
        assert int(baseline["nodes_visited"]) == depth + 1
        if depth == 100:
            assert int(multiscale["nodes_visited"]) * 4 <= int(baseline["nodes_visited"])
        if depth == 1000:
            assert int(multiscale["metadata_bytes"]) * 100 <= int(multiscale["database_bytes"])
    key = {}
    for depth in (10, 100, 1000):
        key[str(depth)] = {
            strategy: {
                "distant_diff_ns": int(one(rows, depth, strategy, "distant-diff")["median_ns"]),
                "distant_diff_nodes": int(one(rows, depth, strategy, "distant-diff")["nodes_visited"]),
                "metadata_bytes": int(one(rows, depth, strategy, "distant-diff")["metadata_bytes"]),
            }
            for strategy in ("baseline", "fixed-10", "multiscale")
        }
    return {
        "file": path.name,
        "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
        "rows": len(rows),
        "all_correct": True,
        "all_store_unchanged": True,
        "gates_pass": True,
        "key": key,
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path)
    parser.add_argument(
        "files",
        nargs="*",
        type=Path,
        default=[Path("results/comparison-run1.tsv"), Path("results/comparison-run2.tsv")],
    )
    args = parser.parse_args()
    result = {
        "schema": "layerfs-history-anchor-evidence-v1",
        "baseline_commit": "1e81e9b8cf871324341c221a51b0a0239c580da9",
        "verdict": "YES — focused PR justified",
        "runs": [verify(path) for path in args.files],
    }
    encoded = json.dumps(result, indent=2, sort_keys=True, ensure_ascii=False) + "\n"
    if args.output:
        args.output.write_text(encoded, encoding="utf-8")
    print(encoded, end="")


if __name__ == "__main__":
    main()
