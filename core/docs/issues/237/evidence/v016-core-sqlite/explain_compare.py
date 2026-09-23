"""Read-only SQLite plan census for closed v0.1.6 and Core 10k Stores.

Usage: /usr/bin/python3 explain_compare.py OLD.sqlite CORE.sqlite HISTORY.sqlite > plans.json
EXPLAIN compiles writes without executing them. Both Stores must use 4 KiB pages.
"""

import collections
import hashlib
import json
import pathlib
import sqlite3
import sys


def explain(db, sql, args):
    plan = [list(row) for row in db.execute("EXPLAIN QUERY PLAN " + sql, args)]
    ops = [row[1] for row in db.execute("EXPLAIN " + sql, args)]
    return {
        "sql": sql,
        "bindings": len(args),
        "plan": plan,
        "opcode_count": len(ops),
        "opcodes": dict(sorted(collections.Counter(ops).items())),
    }


def digest(path):
    value = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            value.update(chunk)
    return value.hexdigest()


def inspect(path, core):
    path = pathlib.Path(path).resolve()
    before = path.stat()
    sha256 = digest(path)
    db = sqlite3.connect(f"file:{path}?mode=ro", uri=True)
    assert db.execute("PRAGMA page_size").fetchone()[0] == 4096
    assert db.execute("PRAGMA application_id").fetchone()[0] == (1279677261 if core else 1279677260)
    db.execute("PRAGMA foreign_keys=ON")
    if core:
        db.execute("CREATE TEMP TABLE layerfs_read_scope(save_id INTEGER NOT NULL,publication INTEGER NOT NULL)")
        db.execute("INSERT INTO temp.layerfs_read_scope VALUES(1,0)")
        sample = db.execute("SELECT object_id,object_role,canonical_length,pack_id,group_number,record_number FROM objects ORDER BY object_id LIMIT 128").fetchall()
        pack_id, save_id, capacity = db.execute("SELECT pack_id,save_id,length(data) FROM object_packs ORDER BY pack_id LIMIT 1").fetchone()
    else:
        sample = db.execute("SELECT object_id,canonical_length,pack_id,group_number,record_number FROM objects ORDER BY object_id LIMIT 128").fetchall()
        pack_id, capacity = db.execute("SELECT pack_id,length(data) FROM object_packs ORDER BY pack_id LIMIT 1").fetchone()
        save_id = None
    assert len(sample) == 128
    ids = [row[0] for row in sample]
    plans = {}
    for width in (1, 128):
        marks = ",".join("?" for _ in range(width))
        if core:
            lookup = ("SELECT o.object_id,o.object_role,o.canonical_length,o.pack_id,o.group_number,o.record_number,o.save_id,"
                      "(o.save_id=r.save_id OR s.publication<=r.publication) "
                      "FROM objects o JOIN saves s USING(save_id),temp.layerfs_read_scope r "
                      f"WHERE o.object_id IN ({marks}) AND o.pack_id<=?")
            plans[f"object_lookup_{width}"] = explain(db, lookup, ids[:width] + [pack_id])
            row = "(?,?,?,?,?,?,(SELECT save_id FROM temp.layerfs_read_scope))"
            insert = ("INSERT INTO objects(object_id,object_role,canonical_length,pack_id,group_number,record_number,save_id) VALUES "
                      + ",".join([row] * width))
            args = [value for values in sample[:width] for value in values]
        else:
            lookup = ("SELECT canonical_length,pack_id,group_number,record_number FROM objects WHERE object_id=?1"
                      if width == 1 else
                      "SELECT object_id,canonical_length,pack_id,group_number,record_number "
                      f"FROM objects WHERE object_id IN ({marks})")
            plans[f"object_lookup_{width}"] = explain(db, lookup, ids[:width])
            row = "(?,?,?,?,?)"
            insert = ("INSERT INTO objects(object_id,canonical_length,pack_id,group_number,record_number) VALUES "
                      + ",".join([row] * width))
            args = [value for values in sample[:width] for value in values]
        plans[f"object_insert_{width}"] = explain(db, insert, args)
    if core:
        plans["pack_insert"] = explain(db,
            "INSERT INTO object_packs(pack_id,data,save_id) VALUES(?1,zeroblob(?2),(SELECT save_id FROM temp.layerfs_read_scope))",
            [pack_id, capacity])
        plans["pack_append_ownership"] = explain(db,
            "SELECT pack_id FROM object_packs WHERE pack_id=?1 AND save_id=(SELECT save_id FROM temp.layerfs_read_scope) AND EXISTS(SELECT 1 FROM saves WHERE saves.save_id=object_packs.save_id AND publication IS NULL)",
            [pack_id])
        plans["pack_read"] = explain(db,
            "SELECT p.data FROM object_packs p JOIN saves s USING(save_id),temp.layerfs_read_scope r WHERE p.pack_id=?1 AND length(p.data) BETWEEN 32 AND ?2 AND (p.save_id=r.save_id OR s.publication<=r.publication)",
            [pack_id, 16781312])
    else:
        plans["pack_insert"] = explain(db,
            "INSERT INTO object_packs(pack_id,data) VALUES(?,?)", [pack_id, b"x"])
        plans["pack_append_update"] = explain(db,
            "UPDATE object_packs SET data=?2 WHERE pack_id=?1", [pack_id, b"x"])
        plans["pack_read"] = explain(db,
            "SELECT data FROM object_packs WHERE pack_id=?1", [pack_id])
        plans["stack_lookup"] = explain(db,
            "SELECT layer_stack_id,name,head_layer_id FROM layer_stacks WHERE layer_stack_id=?1", [b"0" * 17])
        plans["stack_name_lookup"] = explain(db,
            "SELECT layer_stack_id,name,head_layer_id FROM layer_stacks WHERE name=?1", ["example"])
        plans["stack_insert"] = explain(db,
            "INSERT INTO layer_stacks(layer_stack_id,name,head_layer_id) VALUES(?1,?2,?3)",
            [b"0" * 17, "example", b"0" * 33])
        plans["layer_insert"] = explain(db,
            "INSERT INTO layers(layer_id,layer_stack_id,parent_layer_id,root_id,source_branch_id,source_commit_id) VALUES(?1,?2,?3,?4,?5,?6)",
            [b"0" * 33, b"0" * 17, None, b"0" * 32, None, None])
    plans["commit"] = explain(db, "COMMIT", [])
    tables = [row[0] for row in db.execute("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name")]
    info = {
        "path": str(path), "bytes": before.st_size,
        "allocated_bytes_stat_blocks": before.st_blocks * 512, "sha256": sha256,
        "sqlite_version": sqlite3.sqlite_version,
        "reopened_read_only_pragmas": {name: db.execute("PRAGMA " + name).fetchone()[0]
                    for name in ("application_id", "user_version", "page_size", "page_count", "freelist_count", "cache_size", "cache_spill", "mmap_size")},
        "counts": {name: db.execute("SELECT count(*) FROM " + name).fetchone()[0] for name in tables},
        "indexes": [list(row) for row in db.execute("SELECT name,tbl_name,sql FROM sqlite_master WHERE type='index' ORDER BY name")],
        "btree_cells_and_bytes": [list(row) for row in db.execute(
            "SELECT name,sum(ncell),sum(pgsize) FROM dbstat GROUP BY name ORDER BY name")],
        "pack_capacity_bytes": db.execute("SELECT sum(length(data)) FROM object_packs").fetchone()[0],
        "sample_pack": {"pack_id": pack_id, "save_id": save_id, "bytes": capacity},
        "plans": plans,
    }
    db.close()
    after = path.stat()
    assert (before.st_size, before.st_mtime_ns) == (after.st_size, after.st_mtime_ns)
    return info


def inspect_history(path):
    path = pathlib.Path(path).resolve()
    before = path.stat()
    sha256 = digest(path)
    db = sqlite3.connect(f"file:{path}?mode=ro", uri=True)
    assert db.execute("PRAGMA page_size").fetchone()[0] == 4096
    assert db.execute("PRAGMA application_id").fetchone()[0] == 1279677256
    db.execute("PRAGMA foreign_keys=ON")
    plans = {
        "stack_lookup": explain(db,
            "SELECT layer_stack_id,name,scope_id,profile_id,head_layer_id FROM layer_stacks WHERE layer_stack_id=?1",
            [b"0" * 17]),
        "stack_name_lookup": explain(db,
            "SELECT 1 FROM layer_stacks WHERE name=?1", ["example"]),
        "stack_insert": explain(db,
            "INSERT INTO layer_stacks(layer_stack_id,name,scope_id,profile_id,head_layer_id) VALUES(?1,?2,?3,?4,?5)",
            [b"0" * 17, "example", b"0" * 32, b"0" * 32, b"0" * 33]),
        "layer_insert": explain(db,
            "INSERT INTO layers(layer_id,layer_stack_id,parent_layer_id,root_id,source_branch_id,source_commit_id) VALUES(?1,?2,?3,?4,?5,?6)",
            [b"0" * 33, b"0" * 17, None, b"0" * 32, None, None]),
    }
    tables = [row[0] for row in db.execute(
        "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name")]
    info = {
        "path": str(path), "bytes": before.st_size,
        "allocated_bytes_stat_blocks": before.st_blocks * 512, "sha256": sha256,
        "sqlite_version": sqlite3.sqlite_version,
        "reopened_read_only_pragmas": {name: db.execute("PRAGMA " + name).fetchone()[0]
                    for name in ("application_id", "user_version", "page_size", "page_count", "freelist_count")},
        "counts": {name: db.execute("SELECT count(*) FROM " + name).fetchone()[0] for name in tables},
        "indexes": [list(row) for row in db.execute(
            "SELECT name,tbl_name,sql FROM sqlite_master WHERE type='index' ORDER BY name")],
        "btree_cells_and_bytes": [list(row) for row in db.execute(
            "SELECT name,sum(ncell),sum(pgsize) FROM dbstat GROUP BY name ORDER BY name")],
        "plans": plans,
    }
    db.close()
    after = path.stat()
    assert (before.st_size, before.st_mtime_ns) == (after.st_size, after.st_mtime_ns)
    return info


if __name__ == "__main__":
    assert len(sys.argv) == 4, "expected v0.1.6 Store, Core Store and Core History catalog"
    print(json.dumps({"v0.1.6": inspect(sys.argv[1], False),
                      "Core": inspect(sys.argv[2], True),
                      "Core History": inspect_history(sys.argv[3])}, indent=2))
