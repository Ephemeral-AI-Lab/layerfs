#!/usr/bin/env python3
"""Space accounting: `st_blocks`, SQLite pragmas, the pack SQL, and sidecars.

Four separate questions, four separate fields, **never pooled**:

* `apparent_bytes` — `st_size`;
* `allocated_bytes` — `st_blocks * 512`;
* SQLite's own `page_count * page_size` and `freelist_count`;
* pack bodies, summed out of `object_packs.data`.

Two rules make this module honest rather than convenient.

**No `COALESCE`.** The lifted form
`SELECT COALESCE(SUM(length(data)), 0) FROM object_packs` returns `0` after a
table rename, and `0 <= database` then passes the O6 gate. A missing table is
`INCOMPLETE`, never a zero, so the table is asserted to exist first and the sum is
read without a default.

**Allocation attribution.** A COW clone's `st_blocks` double-counts blocks shared
with its master, so an allocated-bytes figure is only meaningful for a row whose
`allocation_attribution` is `exclusive`. This module reports the figure; the gate
that consumes it refuses a shared attribution.
"""

from __future__ import annotations

import json
import os
import sqlite3
import sys
import tempfile
from dataclasses import dataclass, field
from pathlib import Path

SQLITE_SUFFIXES = (".sqlite", ".db", ".sqlite3")
SIDECARS = ("-wal", "-shm", "-journal")

# The one SQL statement this module gates on, written out so a self-check can
# inspect exactly what is executed rather than what the prose says.
PACK_SUM_SQL = "SELECT SUM(length(data)) FROM object_packs"
TABLE_EXISTS_SQL = "SELECT 1 FROM sqlite_master WHERE type='table' AND name=?"
OBJECT_ROWS_SQL = "SELECT COUNT(*) FROM objects"
# The pooled metadata catalogue. Two readings, never one: a *row* is a value
# group and a *value* is a pooled inode value the group carries. The table is
# asserted to exist first, so an absent table is `INCOMPLETE` and a legitimately
# empty one is a zero.
CATALOGUE_SQL = "SELECT COUNT(*), SUM(count) FROM metadata_value_groups"


class Incomplete(Exception):
    """A required measurement is unavailable. Never reported as a zero."""


@dataclass(frozen=True)
class StatSpace:
    """Allocated and apparent bytes of one path."""

    apparent_bytes: int
    allocated_bytes: int

    def as_fields(self) -> dict[str, int]:
        return {
            "apparent_bytes": self.apparent_bytes,
            "allocated_bytes": self.allocated_bytes,
        }


def stat_space(path: str | Path) -> StatSpace:
    """Reads `st_size` and `st_blocks * 512`.

    Both are reported. They answer different questions and the larger one is not
    automatically the interesting one.
    """
    info = os.stat(path)
    return StatSpace(apparent_bytes=info.st_size, allocated_bytes=info.st_blocks * 512)


@dataclass(frozen=True)
class SqliteSpace:
    """SQLite's own view of a Store file."""

    page_size: int
    page_count: int
    freelist_count: int
    database_bytes: int
    application_id: int
    schema_version: int

    def as_fields(self) -> dict[str, int]:
        return {
            "page_size": self.page_size,
            "page_count": self.page_count,
            "freelist_count": self.freelist_count,
            "database_bytes": self.database_bytes,
            "application_id": self.application_id,
            "schema_version": self.schema_version,
        }


def sqlite_space(path: str | Path) -> SqliteSpace:
    """Reads the pragmas. A missing field raises rather than defaulting."""
    connection = sqlite3.connect(f"file:{Path(path)}?mode=ro", uri=True)
    try:
        page_size = int(connection.execute("PRAGMA page_size").fetchone()[0])
        page_count = int(connection.execute("PRAGMA page_count").fetchone()[0])
        freelist = int(connection.execute("PRAGMA freelist_count").fetchone()[0])
        application_id = int(connection.execute("PRAGMA application_id").fetchone()[0])
        schema_version = int(connection.execute("PRAGMA schema_version").fetchone()[0])
        return SqliteSpace(
            page_size,
            page_count,
            freelist,
            page_size * page_count,
            application_id,
            schema_version,
        )
    finally:
        connection.close()


def table_exists(path: str | Path, table: str) -> bool:
    """Whether `table` is in `sqlite_master` under exactly that name."""
    connection = sqlite3.connect(f"file:{Path(path)}?mode=ro", uri=True)
    try:
        row = connection.execute(TABLE_EXISTS_SQL, (table,)).fetchone()
        return row is not None
    finally:
        connection.close()


def pack_bodies(path: str | Path) -> int:
    """Sums `length(data)` out of `object_packs`.

    Without `COALESCE`, and only after proving the table exists: a renamed or
    absent table is `INCOMPLETE`, and a fabricated zero would pass the O6 gate.
    """
    if not table_exists(path, "object_packs"):
        raise Incomplete("object_packs table is absent; the pack sum is not zero, it is unknown")
    connection = sqlite3.connect(f"file:{Path(path)}?mode=ro", uri=True)
    try:
        row = connection.execute(PACK_SUM_SQL).fetchone()
    finally:
        connection.close()
    if row is None or row[0] is None:
        raise Incomplete("object_packs holds no rows to sum; the pack sum is unknown")
    return int(row[0])


def object_rows(path: str | Path) -> int:
    """Number of rows in `objects`, or `Incomplete` when the table is absent."""
    if not table_exists(path, "objects"):
        raise Incomplete("objects table is absent")
    connection = sqlite3.connect(f"file:{Path(path)}?mode=ro", uri=True)
    try:
        return int(connection.execute(OBJECT_ROWS_SQL).fetchone()[0])
    finally:
        connection.close()


def catalogue(path: str | Path) -> tuple[int, int]:
    """`(groups, values)` of the `metadata_value_groups` catalogue.

    Raises `Incomplete` when the table is absent rather than returning zeros: the
    pooled lane's whole claim is that these rows exist.
    """
    if not table_exists(path, "metadata_value_groups"):
        raise Incomplete("metadata_value_groups table is absent")
    connection = sqlite3.connect(f"file:{Path(path)}?mode=ro", uri=True)
    try:
        groups, values = connection.execute(CATALOGUE_SQL).fetchone()
        groups = int(groups)
        return (groups, 0 if groups == 0 else int(values))
    finally:
        connection.close()


def schema_shape(path: str | Path) -> dict[str, list[str]]:
    """Tables and indexes the Store actually has, by name."""
    connection = sqlite3.connect(f"file:{Path(path)}?mode=ro", uri=True)
    try:
        tables = [
            row[0]
            for row in connection.execute(
                "SELECT name FROM sqlite_master WHERE type='table' ORDER BY name"
            )
        ]
        indexes = [
            row[0]
            for row in connection.execute(
                "SELECT name FROM sqlite_master WHERE type='index' ORDER BY name"
            )
        ]
        return {"tables": tables, "indexes": indexes}
    finally:
        connection.close()


def quick_check(path: str | Path) -> str:
    """`PRAGMA quick_check` outcome, verbatim."""
    connection = sqlite3.connect(f"file:{Path(path)}?mode=ro", uri=True)
    try:
        return str(connection.execute("PRAGMA quick_check").fetchone()[0])
    finally:
        connection.close()


def sidecars(path: str | Path) -> list[str]:
    """Which `-wal`/`-shm`/`-journal` files exist beside a Store, if any."""
    path = Path(path)
    return [
        f"{path}{suffix}" for suffix in SIDECARS if Path(f"{path}{suffix}").exists()
    ]


def store_files(directory: str | Path) -> list[str]:
    """Every SQLite-looking file in a directory."""
    directory = Path(directory)
    if not directory.is_dir():
        return []
    return sorted(
        str(entry)
        for entry in directory.iterdir()
        if entry.is_file() and entry.suffix in SQLITE_SUFFIXES
    )


@dataclass
class Footprint:
    """One footprint reading, with every axis separate."""

    store_path: str
    stat: StatSpace | None = None
    sqlite: SqliteSpace | None = None
    pack_bodies: int | None = None
    objects: int | None = None
    catalogue_groups: int | None = None
    catalogue_values: int | None = None
    quick_check: str | None = None
    schema_shape: dict[str, list[str]] = field(default_factory=dict)
    sidecars: list[str] = field(default_factory=list)
    incomplete: list[str] = field(default_factory=list)
    attribution: str = "exclusive"

    def as_fields(self) -> dict[str, object]:
        fields: dict[str, object] = {
            "store_path": self.store_path,
            "attribution": self.attribution,
            "sidecars": self.sidecars,
            "incomplete": self.incomplete,
        }
        if self.stat is not None:
            fields.update(self.stat.as_fields())
        if self.sqlite is not None:
            fields.update(self.sqlite.as_fields())
        if self.pack_bodies is not None:
            fields["pack_bodies_bytes"] = self.pack_bodies
        if self.objects is not None:
            fields["object_rows"] = self.objects
        if self.catalogue_groups is not None:
            fields["catalogue_groups"] = self.catalogue_groups
            fields["catalogue_values"] = self.catalogue_values
        if self.quick_check is not None:
            fields["quick_check"] = self.quick_check
        if self.schema_shape:
            fields["tables"] = self.schema_shape.get("tables", [])
            fields["indexes"] = self.schema_shape.get("indexes", [])
        return fields

    def pack_accounting(self) -> dict[str, object]:
        """The O6 cell, as a status rather than a boolean.

        `pack_bodies <= database` is the gate. When either side is unavailable the
        cell is `INCOMPLETE`; when the store is non-empty and the pack sum is zero
        the cell is `INCOMPLETE` too, because a fabricated zero must not pass.
        """
        if self.pack_bodies is None or self.sqlite is None:
            return {
                "status": "INCOMPLETE",
                "detail": "; ".join(self.incomplete) or "pack or database bytes unavailable",
            }
        if self.pack_bodies == 0 and (self.objects or 0) > 0 and self.sqlite.database_bytes > 0:
            return {
                "status": "INCOMPLETE",
                "detail": f"0 pack bytes for {self.objects} objects: a non-empty store cannot hold none",
            }
        held = self.pack_bodies <= self.sqlite.database_bytes
        return {
            "status": "PASS" if held else "FAIL",
            "detail": f"{self.pack_bodies} <= {self.sqlite.database_bytes}",
        }


def footprint(store_path: str | Path, attribution: str = "exclusive") -> Footprint:
    """Reads every space axis of one Store file.

    Each axis is read independently: one unavailable axis is recorded in
    `incomplete` and never substituted by a zero.
    """
    store_path = Path(store_path)
    reading = Footprint(store_path=str(store_path), attribution=attribution)
    reading.sidecars = sidecars(store_path)
    if not store_path.exists():
        reading.incomplete.append("store file does not exist")
        return reading
    try:
        reading.stat = stat_space(store_path)
    except OSError as error:
        reading.incomplete.append(f"stat: {error}")
    try:
        reading.sqlite = sqlite_space(store_path)
    except sqlite3.Error as error:
        reading.incomplete.append(f"pragmas: {error}")
    try:
        reading.pack_bodies = pack_bodies(store_path)
    except (Incomplete, sqlite3.Error) as error:
        reading.incomplete.append(f"pack bodies: {error}")
    try:
        reading.objects = object_rows(store_path)
    except (Incomplete, sqlite3.Error) as error:
        reading.incomplete.append(f"object rows: {error}")
    try:
        reading.catalogue_groups, reading.catalogue_values = catalogue(store_path)
    except (Incomplete, sqlite3.Error) as error:
        reading.incomplete.append(f"pooled catalogue: {error}")
    try:
        reading.quick_check = quick_check(store_path)
    except sqlite3.Error as error:
        reading.incomplete.append(f"quick_check: {error}")
    try:
        reading.schema_shape = schema_shape(store_path)
    except sqlite3.Error as error:
        reading.incomplete.append(f"schema: {error}")
    return reading


def self_check() -> list[str]:
    """Proves the two rules this module exists for."""
    failures: list[str] = []
    with tempfile.TemporaryDirectory() as directory:
        present = Path(directory) / "present.sqlite"
        connection = sqlite3.connect(present)
        connection.execute("CREATE TABLE object_packs (data BLOB)")
        connection.execute("INSERT INTO object_packs (data) VALUES (?)", (b"x" * 4096,))
        connection.commit()
        connection.close()
        try:
            total = pack_bodies(present)
            if total != 4096:
                failures.append(f"pack_bodies read {total}, expected 4096")
        except Incomplete as error:
            failures.append(f"pack_bodies refused a table that exists: {error}")
        absent = Path(directory) / "absent.sqlite"
        connection = sqlite3.connect(absent)
        connection.execute("CREATE TABLE something_else (data BLOB)")
        connection.commit()
        connection.close()
        try:
            value = pack_bodies(absent)
            failures.append(
                f"a missing object_packs table produced {value}: a fabricated zero passes O6"
            )
        except Incomplete:
            pass
        reading = footprint(present)
        if reading.pack_accounting()["status"] == "INCOMPLETE":
            failures.append("a real store with real pack bytes reported INCOMPLETE")
        empty = Path(directory) / "empty.sqlite"
        connection = sqlite3.connect(empty)
        connection.execute("CREATE TABLE object_packs (data BLOB)")
        connection.execute("CREATE TABLE objects (id INTEGER)")
        connection.execute("INSERT INTO objects (id) VALUES (1)")
        connection.commit()
        connection.close()
        if footprint(empty).pack_accounting()["status"] != "INCOMPLETE":
            failures.append("a zero pack sum over a non-empty store was not INCOMPLETE")
        # The pooled catalogue is read the same way: an absent table is
        # INCOMPLETE, and a table that exists with no rows is a legitimate zero.
        try:
            groups, values = catalogue(absent)
            failures.append(f"a missing catalogue produced {groups} groups / {values} values")
        except Incomplete:
            pass
        connection = sqlite3.connect(empty)
        connection.execute("CREATE TABLE metadata_value_groups (first_ordinal INTEGER, count INTEGER)")
        connection.commit()
        connection.close()
        if catalogue(empty) != (0, 0):
            failures.append(f"an empty catalogue read {catalogue(empty)}, expected (0, 0)")
        connection = sqlite3.connect(empty)
        connection.execute("INSERT INTO metadata_value_groups VALUES (1, 165), (166, 100)")
        connection.commit()
        connection.close()
        if catalogue(empty) != (2, 265):
            failures.append(f"a real catalogue read {catalogue(empty)}, expected (2, 265)")
    executed = (
        PACK_SUM_SQL + " " + TABLE_EXISTS_SQL + " " + OBJECT_ROWS_SQL + " " + CATALOGUE_SQL
    ).upper()
    if "COALESCE" in executed:
        failures.append("the executed pack SQL uses COALESCE: the fabricated-zero hazard is back")
    if "IFNULL" in executed:
        failures.append("the executed pack SQL uses IFNULL: that is the same fabricated zero")
    if "SUM" not in PACK_SUM_SQL.upper():
        failures.append("the pack SQL does not sum anything, so it cannot measure pack bytes")
    return failures


def main() -> int:
    failures = self_check()
    for failure in failures:
        print(f"space: FAIL: {failure}", file=sys.stderr)
    if failures:
        return 1
    print("space: PASS (pack SQL refuses a fabricated zero; no COALESCE)")
    if "--json" in sys.argv:
        print(json.dumps({"status": "PASS"}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
