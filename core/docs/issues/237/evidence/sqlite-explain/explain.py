"""Read-only EXPLAIN of the retained #237 Store with the system SQLite engine.

Run with /usr/bin/python3 explain.py STORE > plans.json. EXPLAIN compiles write
statements but never executes them. Only the per-connection TEMP scope is created.
"""

import collections
import hashlib
import json
import pathlib
import sqlite3
import sys


def plan(connection, sql, parameters):
    eqp = [tuple(row) for row in connection.execute("EXPLAIN QUERY PLAN " + sql, parameters)]
    opcodes = [row[1] for row in connection.execute("EXPLAIN " + sql, parameters)]
    return {
        "sql": sql,
        "parameter_count": len(parameters),
        "plan": eqp,
        "opcode_count": len(opcodes),
        "opcode_histogram": dict(sorted(collections.Counter(opcodes).items())),
    }


def candidate_sql(count):
    placeholders = ",".join(f"?{index}" for index in range(1, count + 1))
    return (
        "SELECT o.object_id,o.object_role,o.canonical_length,o.pack_id,o.group_number,"
        "o.record_number,o.save_id,(o.save_id = r.save_id OR s.publication <= r.publication) "
        "FROM objects o JOIN saves s USING(save_id),temp.layerfs_read_scope r "
        f"WHERE o.object_id IN ({placeholders}) AND o.pack_id <= ?{count + 1}"
    )


def insert_sql(count):
    row = "(" + ",".join("?" for _ in range(6)) + ",(SELECT save_id FROM temp.layerfs_read_scope))"
    return (
        "INSERT INTO objects "
        "(object_id, object_role, canonical_length, pack_id, group_number, record_number, save_id) VALUES "
        + ",".join(row for _ in range(count))
    )


def main():
    path = pathlib.Path(sys.argv[1]).resolve()
    before = path.stat()
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    connection = sqlite3.connect(f"file:{path}?mode=ro", uri=True)
    connection.execute("PRAGMA foreign_keys = ON")
    connection.execute("PRAGMA temp_store = MEMORY")
    read_only_pragmas = {name: connection.execute(f"PRAGMA {name}").fetchone()[0]
                         for name in ("cache_size", "cache_spill", "page_size", "mmap_size")}
    fresh = sqlite3.connect(":memory:")
    fresh.execute("PRAGMA journal_mode = MEMORY")
    fresh.execute("PRAGMA synchronous = OFF")
    fresh.execute("PRAGMA temp_store = MEMORY")
    fresh.execute("PRAGMA foreign_keys = ON")
    fresh.execute("PRAGMA busy_timeout = 0")
    fresh_pragmas = {name: (row[0] if (row := fresh.execute(f"PRAGMA {name}").fetchone()) else None)
                     for name in ("journal_mode", "synchronous", "temp_store", "foreign_keys",
                                  "busy_timeout", "cache_size", "cache_spill", "page_size", "mmap_size")}
    fresh.close()
    connection.execute(
        "CREATE TEMP TABLE layerfs_read_scope (save_id INTEGER NOT NULL, publication INTEGER NOT NULL)"
    )
    connection.execute("INSERT INTO temp.layerfs_read_scope VALUES (1, 0)")
    rows = connection.execute(
        "SELECT object_id,object_role,canonical_length,pack_id,group_number,record_number "
        "FROM objects WHERE save_id = 1 ORDER BY object_id LIMIT 128"
    ).fetchall()
    assert len(rows) == 128
    ceiling = connection.execute("SELECT MAX(pack_id) FROM object_packs").fetchone()[0]
    pack_id, save_id, capacity = connection.execute(
        "SELECT pack_id,save_id,length(data) FROM object_packs WHERE save_id = 1 ORDER BY pack_id LIMIT 1"
    ).fetchone()
    parameters = lambda n: [item for row in rows[:n] for item in row]
    shapes = {}
    for width in (1, 128):
        shapes[f"candidate_{width}"] = plan(
            connection, candidate_sql(width), [row[0] for row in rows[:width]] + [ceiling]
        )
        shapes[f"insert_objects_{width}"] = plan(connection, insert_sql(width), parameters(width))
    shapes["append_ownership"] = plan(
        connection,
        "SELECT pack_id FROM object_packs WHERE pack_id = ?1 AND save_id = "
        "(SELECT save_id FROM temp.layerfs_read_scope) AND EXISTS "
        "(SELECT 1 FROM saves WHERE saves.save_id = object_packs.save_id AND publication IS NULL)",
        [pack_id],
    )
    shapes["insert_pack"] = plan(
        connection,
        "INSERT INTO object_packs (pack_id, data, save_id) VALUES "
        "(?1, zeroblob(?2), (SELECT save_id FROM temp.layerfs_read_scope))",
        [pack_id, capacity],
    )
    shapes["read_pack"] = plan(
        connection,
        "SELECT p.data FROM object_packs p JOIN saves s USING(save_id),temp.layerfs_read_scope r "
        "WHERE p.pack_id = ?1 AND length(p.data) BETWEEN 32 AND ?2 AND "
        "(p.save_id = r.save_id OR s.publication <= r.publication)",
        [pack_id, 16_781_312],
    )
    shapes["advance_pack"] = plan(
        connection,
        "UPDATE store_policy SET next_pack_id = ?1 WHERE id = 1 AND next_pack_id <= ?1",
        [ceiling + 1],
    )
    shapes["save_pack_ceiling"] = plan(
        connection,
        "UPDATE saves SET pack_ceiling = MAX(pack_ceiling, ?2) "
        "WHERE save_id = ?1 AND active_slot IS NOT NULL",
        [save_id, pack_id],
    )
    shapes["commit"] = plan(connection, "COMMIT", [])
    output = {
        "engine": {"python": sys.executable, "sqlite_version": sqlite3.sqlite_version},
        "pragmas": {"fresh_memory_connection_with_core_configuration": fresh_pragmas,
                    "read_only_retained_store_connection": read_only_pragmas},
        "store": {
            "path": str(path),
            "bytes": before.st_size,
            "sha256": digest.hexdigest(),
            "page_size": connection.execute("PRAGMA page_size").fetchone()[0],
            "application_id": connection.execute("PRAGMA application_id").fetchone()[0],
            "user_version": connection.execute("PRAGMA user_version").fetchone()[0],
            "page_count": connection.execute("PRAGMA page_count").fetchone()[0],
            "indexes": [tuple(row) for row in connection.execute(
                "SELECT name,tbl_name,sql FROM sqlite_master WHERE type='index' ORDER BY name"
            )],
            "counts": {table: connection.execute(f"SELECT COUNT(*) FROM {table}").fetchone()[0]
                       for table in ("objects", "object_packs", "saves", "content_signatures", "metadata_value_groups")},
            "file_save_roles": [tuple(row) for row in connection.execute(
                "SELECT object_role,COUNT(*) FROM objects WHERE save_id=1 GROUP BY object_role ORDER BY object_role"
            )],
            "file_save_groups": connection.execute(
                "SELECT COUNT(*) FROM (SELECT pack_id,group_number FROM objects WHERE save_id=1 "
                "GROUP BY pack_id,group_number)"
            ).fetchone()[0],
            "file_save_packs": connection.execute(
                "SELECT COUNT(*) FROM object_packs WHERE save_id=1"
            ).fetchone()[0],
            "sample_object_id_sha256": hashlib.sha256(rows[0][0]).hexdigest(),
            "sample_pack": {"pack_id": pack_id, "save_id": save_id, "capacity": capacity},
        },
        "temp_scope": {"schema": "CREATE TEMP TABLE layerfs_read_scope (save_id INTEGER NOT NULL, publication INTEGER NOT NULL)",
                       "row": [1, 0]},
        "plans": shapes,
    }
    connection.close()
    after = path.stat()
    assert (before.st_size, before.st_mtime_ns) == (after.st_size, after.st_mtime_ns)
    print(json.dumps(output, indent=2))


if __name__ == "__main__":
    main()
