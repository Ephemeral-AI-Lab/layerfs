"""Compile, without executing, current and candidate C2 admission SQL.

Usage: /usr/bin/python3 explain_alternatives.py CORE_STORE.sqlite > plans.json
"""

import collections
import hashlib
import json
from pathlib import Path
import sqlite3
import sys


def explain(db, sql, arguments):
    query_plan = [list(row) for row in db.execute("EXPLAIN QUERY PLAN " + sql, arguments)]
    opcodes = [row[1] for row in db.execute("EXPLAIN " + sql, arguments)]
    return {
        "bindings": len(arguments),
        "query_plan": query_plan,
        "opcode_count": len(opcodes),
        "opcodes": dict(sorted(collections.Counter(opcodes).items())),
    }


def main(path):
    path = Path(path).resolve()
    before = path.stat()
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    db = sqlite3.connect(f"file:{path}?mode=ro", uri=True)
    db.execute("PRAGMA foreign_keys=ON")
    db.execute("CREATE TEMP TABLE layerfs_read_scope(save_id INTEGER NOT NULL, publication INTEGER NOT NULL)")
    db.execute("INSERT INTO temp.layerfs_read_scope VALUES(1,0)")
    assert db.execute("PRAGMA page_size").fetchone()[0] == 4096
    rows = db.execute(
        "SELECT object_id,object_role,canonical_length,pack_id,group_number,record_number "
        "FROM objects ORDER BY object_id LIMIT 128"
    ).fetchall()
    assert len(rows) == 128
    pack_id, capacity = db.execute(
        "SELECT pack_id,length(data) FROM object_packs ORDER BY pack_id LIMIT 1"
    ).fetchone()
    plans = {}
    for width in (1, 16, 64, 128):
        values = [value for row in rows[:width] for value in row]
        current = (
            "INSERT INTO objects(object_id,object_role,canonical_length,pack_id,group_number,record_number,save_id) VALUES "
            + ",".join(["(?,?,?,?,?,?,(SELECT save_id FROM temp.layerfs_read_scope))"] * width)
        )
        common = "(object_id,object_role,canonical_length,pack_id,group_number,record_number)"
        candidate = (
            "WITH incoming" + common + " AS (VALUES "
            + ",".join(["(?,?,?,?,?,?)"] * width)
            + ") INSERT INTO objects(object_id,object_role,canonical_length,pack_id,group_number,record_number,save_id) "
            "SELECT i.object_id,i.object_role,i.canonical_length,i.pack_id,i.group_number,i.record_number,r.save_id "
            "FROM incoming i CROSS JOIN temp.layerfs_read_scope r"
        )
        plans[f"objects_current_{width}"] = explain(db, current, values)
        plans[f"objects_one_scope_{width}"] = explain(db, candidate, values)
    for width in (1, 4, 16):
        candidate = (
            "INSERT INTO object_packs(pack_id,data,save_id) VALUES "
            + ",".join(["(?,zeroblob(?),(SELECT save_id FROM temp.layerfs_read_scope))"] * width)
        )
        plans[f"pack_zero_blob_{width}"] = explain(db, candidate, [v for _ in range(width) for v in (pack_id, capacity)])
    db.close()
    after = path.stat()
    assert (before.st_size, before.st_mtime_ns) == (after.st_size, after.st_mtime_ns)
    return {
        "path": str(path),
        "sha256": digest.hexdigest(),
        "sqlite_version": sqlite3.sqlite_version,
        "page_size": 4096,
        "method": "read-only EXPLAIN; statements compiled but not executed",
        "plans": plans,
    }


if __name__ == "__main__":
    assert len(sys.argv) == 2
    print(json.dumps(main(sys.argv[1]), indent=2))
