"""Reproduce the count-only C2 placement census from retained stderr and Store.

Usage: python3 analyze_widths.py service.stderr store.sqlite > placement-width.json
"""

from collections import Counter
import json
from pathlib import Path
import re
import sqlite3
import sys


LINE = re.compile(r"LFS237_ADMISSION rows=(\d+) pack_writes=(\d+) creates=(\d+) appends=(\d+)")


def label(width):
    if width <= 8:
        return "1-8"
    if width <= 16:
        return "9-16"
    if width <= 32:
        return "17-32"
    if width <= 64:
        return "33-64"
    if width <= 128:
        return "65-128"
    return ">128"


def summarize(calls):
    return {
        "placement_calls": len(calls),
        "locator_rows": sum(call[0] for call in calls),
        "derived_object_insert_calls_at_128_rows_per_statement": sum((call[0] + 127) // 128 for call in calls),
        "locator_rows_min": min(call[0] for call in calls),
        "locator_rows_max": max(call[0] for call in calls),
        "locator_rows_per_placement_mean": sum(call[0] for call in calls) / len(calls),
        "locator_width_histogram": dict(sorted(Counter(label(call[0]) for call in calls).items())),
        "pack_writes": sum(call[1] for call in calls),
        "pack_creates": sum(call[2] for call in calls),
        "pack_appends": sum(call[3] for call in calls),
        "created_packs_per_call": dict(sorted(Counter(call[2] for call in calls).items())),
        "pack_writes_per_call": dict(sorted(Counter(call[1] for call in calls).items())),
    }


def main(stderr, store):
    calls = []
    for line in stderr.read_text().splitlines():
        match = LINE.fullmatch(line)
        if match:
            call = tuple(map(int, match.groups()))
            assert call[0] > 0 and call[1] == call[2] + call[3]
            calls.append(call)
    assert calls
    db = sqlite3.connect(f"file:{store.resolve()}?mode=ro", uri=True)
    assert db.execute("PRAGMA page_size").fetchone()[0] == 4096
    save_rows = db.execute("SELECT save_id,count(*) FROM objects GROUP BY save_id ORDER BY save_id").fetchall()
    save_packs = dict(db.execute("SELECT save_id,count(*) FROM object_packs GROUP BY save_id"))
    db.close()
    assert len(save_rows) == 3
    names = ("file", "prerequisite", "tree")
    result = {}
    position = 0
    for name, (save_id, expected_rows) in zip(names, save_rows):
        begin = position
        total = 0
        while total < expected_rows:
            assert position < len(calls)
            total += calls[position][0]
            position += 1
        assert total == expected_rows
        cohort = calls[begin:position]
        traced_creates = sum(call[2] for call in cohort)
        assert traced_creates <= save_packs[save_id]
        if name != "tree":
            assert traced_creates == save_packs[save_id]
        result[name] = {
            "save_id": save_id,
            "closed_store_pack_rows": save_packs[save_id],
            "pack_rows_outside_place_groups_trace": save_packs[save_id] - traced_creates,
            **summarize(cohort),
        }
    assert position == len(calls)
    return {
        "schema": "issue237-placement-width-census-v1",
        "method": "exact stderr call lines, bounded by closed Store object/pack rows per Save",
        "sqlite_version": sqlite3.sqlite_version,
        "page_size": 4096,
        "all": summarize(calls),
        "saves": result,
    }


if __name__ == "__main__":
    assert len(sys.argv) == 3
    print(json.dumps(main(Path(sys.argv[1]), Path(sys.argv[2])), indent=2, sort_keys=True))
