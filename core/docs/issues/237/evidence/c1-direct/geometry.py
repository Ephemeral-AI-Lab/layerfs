#!/usr/bin/env python3
"""Read closed #237 SQLite Store geometry without changing its pages."""
import hashlib
import json
from pathlib import Path
import sqlite3
import sys

path = Path(sys.argv[1]).resolve()
database = sqlite3.connect(f"file:{path}?mode=ro", uri=True)
page_size, page_count, free = (database.execute(f"pragma {name}").fetchone()[0]
                               for name in ("page_size", "page_count", "freelist_count"))
packs = database.execute("select save_id, substr(data,1,20), length(data) from object_packs")
used = capacity = count = 0
versions = {}
saves = {}
for save_id, header, length in packs:
    if header[:8] != b"LFPACK\0\0" or len(header) != 20:
        raise ValueError("pack header")
    amount = int.from_bytes(header[16:20], "little")
    if not 24 <= amount <= length:
        raise ValueError("pack used bytes")
    version = int.from_bytes(header[8:12], "little")
    row = versions.setdefault(version, {"count": 0, "capacity": 0, "used": 0})
    row["count"] += 1
    row["capacity"] += length
    row["used"] += amount
    saved = saves.setdefault(save_id, {"count": 0, "capacity": 0, "used": 0})
    saved["count"] += 1
    saved["capacity"] += length
    saved["used"] += amount
    used += amount
    capacity += length
    count += 1
ids = hashlib.sha256()
objects = 0
for (object_id,) in database.execute("select object_id from objects order by object_id, save_id"):
    ids.update(object_id)
    objects += 1
print(json.dumps({"store": str(path), "file_bytes": path.stat().st_size,
                  "allocated_bytes": path.stat().st_blocks * 512,
                  "sqlite_page_size": page_size, "sqlite_page_count": page_count,
                  "sqlite_freelist_count": free, "pack_count": count,
                  "pack_capacity_bytes": capacity, "pack_used_bytes": used,
                  "pack_spare_bytes": capacity - used, "pack_versions": versions,
                  "packs_by_save": saves,
                  "objects_by_save": dict(database.execute(
                      "select save_id, count(*) from objects group by save_id")),
                  "objects": objects, "ordered_object_ids_sha256": ids.hexdigest()},
                 sort_keys=True, indent=2))
