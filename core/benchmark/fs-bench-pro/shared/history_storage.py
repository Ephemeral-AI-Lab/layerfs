"""Closed-file C2+C5 accounting; every missing axis fails closed."""
import os
from pathlib import Path
import sqlite3
import sys

BACKEND = Path(__file__).resolve().parents[2] / "fs-bench-pro-storage-content"
sys.path.insert(0, str(BACKEND / "shared"))
import space  # noqa: E402

GATE = "g1.o6-total-retained-below-v016-v1"
CREATION = "in-process-create-new-exclusive-v1"
C2_TABLES = {"store_policy", "saves", "object_packs", "metadata_value_groups", "objects", "content_signatures"}
C5_TABLES = {"history_meta", "layer_stacks", "layers", "commits", "branches", "workspace_stages", "scope_allocator"}


def gate(version):
    if version == "v1":
        return GATE
    if version == "v4":
        return "g1.o6-total-retained-within-110pct-v016-v4"
    return f"g1.o6-total-retained-below-v016-{version}"


def evaluate(total, ceiling, attribution):
    if type(total) is not int or total < 0:
        return "INCOMPLETE"
    if attribution != CREATION:
        return "INELIGIBLE"
    return "PASS" if total < ceiling else "FAIL"


def owner(path, expected_tables, version, application):
    info = path.lstat()
    if path.is_symlink() or not path.is_file() or info.st_nlink != 1:
        raise ValueError("nonexclusive owner")
    result = space.stat_space(path).as_fields()
    result.update(space.sqlite_space(path).as_fields())
    result["sidecars"] = space.sidecars(path)
    result.update(space.schema_shape(path))
    with sqlite3.connect(f"file:{path}?mode=ro", uri=True) as db:
        result["user_version"] = db.execute("PRAGMA user_version").fetchone()[0]
        result["quick_check"] = db.execute("PRAGMA quick_check").fetchone()[0]
        result["foreign_key_errors"] = db.execute("PRAGMA foreign_key_check").fetchall()
        tables = set(result["tables"]) - {"sqlite_sequence"}
        if tables != expected_tables or result["user_version"] != version or result["application_id"] != application:
            raise ValueError("product schema identity mismatch")
        result["rows"] = {table: db.execute(f'SELECT COUNT(*) FROM "{table}"').fetchone()[0]
                          for table in sorted(tables)}
        if expected_tables == C2_TABLES:
            values = db.execute("SELECT MIN(canonical_length), MAX(canonical_length), MIN(object_role), MAX(object_role) FROM objects GROUP BY object_id").fetchall()
            if not values or any(lo != hi or role != last for lo, hi, role, last in values):
                raise ValueError("missing or inconsistent canonical identity counters")
            result["canonical_bytes"] = sum(v[0] for v in values)
            result["canonical_objects"] = len(values)
            result["pack_bodies_bytes"] = space.pack_bodies(path)
            result["canonical_by_role"] = space.canonical_by_role(path)
            result["pack_directory"] = space.pack_directory(path).as_fields()
            if result["pack_bodies_bytes"] > result["database_bytes"]:
                raise ValueError("pack bodies exceed logical database size")
    if result["sidecars"] or result["quick_check"] != "ok" or result["foreign_key_errors"]:
        raise ValueError("integrity or cleanup mismatch")
    return result


def collect(directory, ceiling, states, attribution=CREATION, distinct_indexes=(), version="v1"):
    directory = Path(directory)
    result = {"gate": gate(version),
              "ceiling_bytes": ceiling, "strict": True,
              "allocation_attribution": attribution, "owners": {}, "issues": [],
              "total_retained_allocated_bytes": None, "status": "INCOMPLETE"}
    try:
        result["owners"]["C2"] = owner(directory / "sample.sqlite", C2_TABLES, 10, 1279677261)
        result["owners"]["C5"] = owner(directory / "history.sqlite", C5_TABLES, 1, 1279677256)
        rows = result["owners"]["C5"]["rows"]
        expected = {"history_meta": 1, "layer_stacks": 1, "branches": states, "layers": states,
                    "commits": states - 1, "workspace_stages": 0, "scope_allocator": 0}
        result["retained_counts_status"] = "PASS" if rows == expected else "FAIL"
        result["expected_C5_rows"] = expected
        identities = {(os.stat(directory / name).st_dev, os.stat(directory / name).st_ino)
                      for name in ("sample.sqlite", "history.sqlite")}
        if len(identities) != 2:
            raise ValueError("C2 and C5 must have separate owners")
        for index, path in enumerate(distinct_indexes):
            path = Path(path)
            info = path.lstat()
            identity = (info.st_dev, info.st_ino)
            if info.st_nlink != 1 or path.is_symlink():
                raise ValueError("nonexclusive persistent index")
            if identity not in identities:
                identities.add(identity)
                result["owners"][f"index-{index}"] = space.stat_space(path).as_fields()
        for name, field in (("total_retained_allocated_bytes", "allocated_bytes"),
                            ("total_retained_apparent_bytes", "apparent_bytes")):
            result[name] = sum(item[field] for item in result["owners"].values())
        result["total_retained_logical_database_bytes"] = sum(
            item["database_bytes"] for item in result["owners"].values() if "database_bytes" in item)
        result["status"] = evaluate(result["total_retained_allocated_bytes"], ceiling, attribution)
        result["allocation_threshold_status"] = result["status"]
        if result["retained_counts_status"] == "FAIL":
            result["issues"].append("persisted C5 row counts disagree")
            result["status"] = "INCOMPLETE"
    except (OSError, sqlite3.Error, ValueError, space.Incomplete) as error:
        result["issues"].append(str(error))
        if "nonexclusive" in str(error):
            result["status"] = "INELIGIBLE"
    return result
