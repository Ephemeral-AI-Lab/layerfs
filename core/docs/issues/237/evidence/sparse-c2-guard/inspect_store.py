#!/usr/bin/env python3
"""Write one exclusive, read-only SQLite/pack geometry receipt for a closed Store."""

import hashlib
import json
import sqlite3
import sys
from collections import Counter
from pathlib import Path


def inspect(store: Path) -> dict:
    digest = hashlib.sha256()
    with store.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(block)
    stat = store.stat()
    connection = sqlite3.connect(f"file:{store}?mode=ro", uri=True)
    connection.execute("PRAGMA query_only=ON")
    page_size = connection.execute("PRAGMA page_size").fetchone()[0]
    if page_size != 4096:
        raise ValueError(f"SQLite page size {page_size}, expected 4096")
    versions = Counter()
    capacity = used = packs = 0
    for length, header in connection.execute(
        "SELECT length(data), substr(data, 1, 24) FROM object_packs ORDER BY pack_id"
    ):
        if len(header) != 24 or header[:8] != b"LFPACK\0\0":
            raise ValueError(f"invalid pack header at pack {packs + 1}")
        version = int.from_bytes(header[8:12], "little")
        assembled = int.from_bytes(header[16:20], "little")
        if version < 9 or not 24 <= assembled <= length:
            raise ValueError(f"invalid pack version/used at pack {packs + 1}")
        versions[str(version)] += 1
        packs += 1
        capacity += length
        used += assembled
    result = {
        "store": str(store),
        "sha256": digest.hexdigest(),
        "page_size": page_size,
        "page_count": connection.execute("PRAGMA page_count").fetchone()[0],
        "freelist_count": connection.execute("PRAGMA freelist_count").fetchone()[0],
        "apparent_bytes": stat.st_size,
        "allocated_bytes": stat.st_blocks * 512,
        "object_rows": connection.execute("SELECT count(*) FROM objects").fetchone()[0],
        "distinct_object_ids": connection.execute(
            "SELECT count(DISTINCT object_id) FROM objects"
        ).fetchone()[0],
        "pack_rows": packs,
        "pack_capacity_bytes": capacity,
        "pack_used_bytes": used,
        "pack_slack_bytes": capacity - used,
        "pack_versions": dict(sorted(versions.items())),
    }
    connection.close()
    return result


if __name__ == "__main__":
    if len(sys.argv) != 3:
        raise SystemExit("usage: inspect_store.py STORE.sqlite FRESH_OUTPUT.json")
    receipt = inspect(Path(sys.argv[1]).resolve())
    with Path(sys.argv[2]).open("x") as output:
        json.dump(receipt, output, indent=2, sort_keys=True)
        output.write("\n")
