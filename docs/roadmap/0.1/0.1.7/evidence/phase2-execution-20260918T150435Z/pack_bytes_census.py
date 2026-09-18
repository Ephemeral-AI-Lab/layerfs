#!/usr/bin/env python3
"""Pack bytes and pack rows, summed from the stores an arm actually wrote.

A placement change must not move a pack boundary: the counters say what was
placed, and this says how many bytes each store holds and in how many pack rows.
Reads every `store.sqlite` under an arm's `output/` tree.

Usage: python3 pack_bytes_census.py <arm-dir> [<arm-dir> ...]
"""
import hashlib
import pathlib
import sqlite3
import sys


def census(arm: pathlib.Path):
    rows = []
    for path in sorted(arm.glob("output/**/store.sqlite")):
        connection = sqlite3.connect(path)
        packs, total = connection.execute(
            "SELECT COUNT(*), COALESCE(SUM(LENGTH(data)), 0) FROM object_packs"
        ).fetchone()
        objects = connection.execute("SELECT COUNT(*) FROM objects").fetchone()[0]
        digest = hashlib.sha256()
        for (data,) in connection.execute("SELECT data FROM object_packs ORDER BY pack_id"):
            digest.update(data)
        rows.append((str(path.relative_to(arm)), packs, total, objects, digest.hexdigest()[:16]))
        connection.close()
    return rows


def main() -> int:
    arms = [pathlib.Path(arg) for arg in sys.argv[1:]]
    censuses = {arm: census(arm) for arm in arms}
    first = arms[0]
    status = 0
    for row in censuses[first]:
        print(f"{row[0]}: packs {row[1]} pack_bytes {row[2]} objects {row[3]} sha {row[4]}")
    for arm in arms[1:]:
        for left, right in zip(censuses[first], censuses[arm]):
            if left != right:
                status = 1
                print(f"DIFF {arm}: {left} != {right}")
    print(f"stores compared across {len(arms)} arms: {len(censuses[first])}, identical: {status == 0}")
    return status


if __name__ == "__main__":
    raise SystemExit(main())
