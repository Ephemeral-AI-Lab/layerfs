#!/usr/bin/env python3
"""E3: locate the crossing results of the stride10 corpus inside the retained
v0.1.7 Store and read their stored bytes.

A whole-file object is the canonical envelope plus the raw payload; its
objects.canonical_length is (logical length + 23) (B3-framing-codec.md).  The
large->small crossing results are therefore located by exact canonical_length,
and their stored bytes come from shared/space.py:whole_file_records, the one
per-object stored-size reading that is exact.

    python3 e3_crossing_objects.py /tmp/base187/sample.sqlite
"""
from __future__ import annotations

import json
import sqlite3
import sys
from pathlib import Path

HARNESS = Path("/Users/yifanxu/Ephemeral-AI-Lab/layerfs/core/benchmark/fs-bench-pro-storage-content")
sys.path.insert(0, str(HARNESS))
from shared import space  # noqa: E402

WHOLE_FILE_HEADER = 23

#: (from_state, to_state, path, result_bytes) for the 7 large->small crossings of
#: history-stride10, from e3_population.py.
LARGE_TO_SMALL = [
    (31, 41, "examples/acp-agent/tests/snapshots/hook-cc-posttool-block/session.jsonl", 31857),
    (51, 61, "packages/ui/tui/src/index.ts", 62693),
    (111, 121, "scripts/snapshots/translation-prompt-v4/request-response.expected.json", 129985),
    (131, 141, "scripts/snapshots/translation-prompt-v4/request-response.expected.json", 37715),
    (131, 141, "packages/host/apiproxy/src/api-proxy.ts", 19878),
    (141, 151, "docs/module-graph.md", 98045),
    (141, 151, "docs/module-graph.zh.md", 98021),
]


def main() -> int:
    path = Path(sys.argv[1])
    connection = sqlite3.connect(f"file:{path}?mode=ro", uri=True)
    try:
        rows = list(connection.execute(
            "SELECT object_id, canonical_length FROM objects WHERE object_role = 1"
        ))
    finally:
        connection.close()
    stored = space.whole_file_records(path)
    by_length: dict[int, list[bytes]] = {}
    for oid, canonical in rows:
        by_length.setdefault(int(canonical), []).append(bytes(oid))
    total_result = 0
    total_stored = 0
    print(f"{chr(39)}from{chr(39):>4} {chr(39)}to{chr(39)}  {'result':>8} {'canonical':>10} {'objects':>7} {'stored':>8}  path")
    for from_state, to_state, name, result in LARGE_TO_SMALL:
        canonical = result + WHOLE_FILE_HEADER
        found = by_length.get(canonical, [])
        sizes = [stored[oid] for oid in found if oid in stored]
        total_result += result
        total_stored += sum(sizes)
        print(f"{from_state:>4} {to_state:>4}  {result:>8,d} {canonical:>10,d} {len(found):>7} {sum(sizes):>8,d}  {name}")
    print(f"TOTAL result bytes {total_result:,d}   stored bytes {total_stored:,d}   "
          f"ratio {total_result / total_stored:.4f}x" if total_stored else "no stored bytes")
    return 0


if __name__ == "__main__":
    sys.exit(main())
