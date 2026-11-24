#!/usr/bin/env python3
"""The 2.0 conversion of a phase-3 schema version 2 Store (layerfs-project#41).

Phase 3 kept the Store-side machine authority, Sandbox tombstone and operator
audit in three tables (`gc_machine_authority`, `gc_sandbox_tombstone`,
`gc_audit`) without bumping the Store's schema version. The lean 2.0 schema is
version 3: the home `MachineRecord.incarnation`, the
`machine/<machine>/sandbox-retired/<name>.json` tombstones and a JSONL audit
replace those tables, and the tables are gone. This tool converts a closed
schema version 2 Store once:

  store_gc_convert.py inspect <closed v2.sqlite>
      Reads the Store immutable, read only, and prints its version, its
      machines and the rows per authority table, as JSON.
  store_gc_convert.py copy <closed v2.sqlite> <home>
      Read only on the Store. For every machine the home owns (a
      `machine/<machine>/record.json` exists) it copies
      `gc_machine_authority.machine` into `MachineRecord.incarnation` and
      `gc_sandbox_tombstone` into `machine/<machine>/sandbox-retired/`.
      It exports every `gc_audit` row to `<home>/log/gc-audit.jsonl` and
      records the converted machines in `<home>/log/gc-conversion.json`.
      A machine with rows and no home record is reported as missing; it is
      never dropped implicitly.
  store_gc_convert.py convert <closed v2.sqlite> <new.sqlite> <home>
          [--converted <machine>]... [--gone <machine>]...
      `copy` first, then converts into a NEW closed Store file: the three
      tables are dropped, the two recorded `schema_version` checks and values
      move to version 3, and the recorded schema source/definition are the
      version 3 ones. The source is never modified, so it is the backup; the
      target must not exist. `convert` refuses while any machine with rows is
      neither owned by this home (and copied), named with `--converted` (its
      owner home copied it), nor named with `--gone`. `--gone` is the
      missing-machine policy: the operator declares the machine's home is
      gone, its authority and tombstone rows are not copied, the audit is
      exported, and only then are the tables dropped. A machine this home
      owns cannot be declared gone; convert it.

The conversion is per machine: a shared PostgreSQL Store holds rows for every
machine, and one home can copy only its own machines' rows. This tool converts
SQLite Stores; a PostgreSQL Store is converted in its own `psql` run after
every owner home has copied its rows (the offline copy/export half is the same
`copy` logic; no PostgreSQL connection is made here). The three tables are
dropped only in the converted target, so the old Store stays whole until every
known machine is accounted for.

This is an offline tool: never point it at a live Store (a non-empty `-wal` is
refused), and keep the source file until the new Store is accepted.
"""

import json
import os
import re
import shutil
import sqlite3
import sys
from pathlib import Path

APPLICATION_ID = 1279677264
TABLES = ("gc_audit", "gc_machine_authority", "gc_sandbox_tombstone")
# A phase-3 schema version 2 user_version -> the lean schema version 3 one.
VERSIONS = {36: 39, 37: 40, 38: 41, 42: 45, 43: 46, 44: 47}
# The three tables as the phase-3 branch's `gc.sql` appended them; the current
# `gc.sql` is that text without them. A version 2 Store recorded them in its
# schema source, so its source is the current one plus this text.
AUTHORITY = """CREATE TABLE gc_machine_authority (
    machine TEXT PRIMARY KEY,
    incarnation TEXT NOT NULL,
    updated_ms INTEGER NOT NULL CHECK (updated_ms >= 0)
) STRICT;

CREATE TABLE gc_sandbox_tombstone (
    sandbox TEXT NOT NULL,
    machine TEXT NOT NULL,
    generation TEXT NOT NULL,
    retired_ms INTEGER NOT NULL CHECK (retired_ms >= 0),
    PRIMARY KEY (sandbox, machine)
) STRICT, WITHOUT ROWID;

CREATE TABLE gc_audit (
    seq INTEGER PRIMARY KEY AUTOINCREMENT,
    at_ms INTEGER NOT NULL CHECK (at_ms >= 0),
    action TEXT NOT NULL,
    op_id BLOB,
    detail TEXT
) STRICT;
"""

SQL = Path(__file__).resolve().parent.parent / "engine/crates/layerfs-persistence/sql"
# The home name grammar (`layerfs_home::names`): a machine or Sandbox name is
# one folder under the home, so a row that is not one is never a path.
NAME = re.compile(r"[a-z0-9](?:[a-z0-9._-]{0,61}[a-z0-9])?\Z")
DEVICES = {"con", "nul", "aux", "prn"} | {f"com{n}" for n in range(1, 10)} | {f"lpt{n}" for n in range(1, 10)}


def refuse(message):
    raise SystemExit(f"refused: {message}")


def _read_sql(*parts):
    return (SQL.joinpath(*parts)).read_text()


def _layout(version):
    """The pack layout script names of a stored schema version (2 or 3)."""
    # 36/39 are Monolithic, 37/40 GroupRows, 38/41 GroupRowsIndexed; versions
    # 42.. and 45.. are the same three layouts with the acquisition tables.
    objects = {
        0: (("sqlite", "objects.sql"),),
        1: (("sqlite", "objects_units.sql"),),
        2: (("sqlite", "objects_units.sql"), ("sqlite", "objects_units_index.sql")),
    }
    if version not in VERSIONS and version not in VERSIONS.values():
        refuse(f"not a schema version 2 or 3 Store (user_version {version})")
    return objects[version % 3]


def _scripts(version):
    """The scripts `Session::initialize` runs, in order, for `version`."""
    names = _layout(version)
    parts = [_read_sql(*name) for name in names]
    for name in ("metadata.sql", "history.sql"):
        parts.append(_read_sql("sqlite", name).replace("schema_version=1", f"schema_version={version}"))
    parts.extend(
        _read_sql("sqlite", name) for name in ("lineage.sql", "merge_scratch.sql", "file_digest.sql", "gc.sql")
    )
    if version >= 42:
        parts.append(_read_sql("sqlite", "acquisition", "schema.sql"))
    return "".join(parts)


def _source(version):
    """The schema source a Store of `version` records (the lean source for 3)."""
    return _scripts(version)


def _old_source(version):
    """The phase-3 source a Store of version 2 records: the current one plus the three tables."""
    source = _scripts(version)
    gc = _read_sql("sqlite", "gc.sql")
    if source.count(gc) != 1:
        refuse("the engine's gc.sql is not the one this tool was written for")
    return source.replace(gc, gc + AUTHORITY, 1)


def _definition(con):
    rows = con.execute("SELECT type,name,sql FROM sqlite_schema WHERE substr(name,1,7)!='sqlite_' ORDER BY type,name")
    return "".join(f"{t}\n{n}\n{s}\n" for t, n, s in rows)


def _name_set(con):
    return {r[0] for r in con.execute("SELECT name FROM sqlite_schema WHERE type='table'")}


def _hex(value):
    return value.hex() if isinstance(value, (bytes, bytearray)) else value


def _open_closed(path):
    """Open `path` immutable and prove it is the closed Store this tool converts."""
    path = Path(path)
    if not path.is_file():
        refuse(f"{path} is not a file")
    wal = Path(str(path) + "-wal")
    if wal.exists() and wal.stat().st_size:
        refuse(f"{path} is not closed (its -wal is not empty); use a closed copy")
    con = sqlite3.connect(f"file:{path}?immutable=1", uri=True)
    app = con.execute("PRAGMA application_id").fetchone()[0]
    if app != APPLICATION_ID:
        refuse(f"{path} is not a LayerFS Store (application_id {app})")
    version = con.execute("PRAGMA user_version").fetchone()[0]
    if version not in VERSIONS:
        refuse(f"{path} is not a phase-3 schema version 2 Store (user_version {version}); this build writes 3")
    names = _name_set(con)
    missing = [table for table in TABLES if table not in names]
    if missing:
        refuse(f"{path} has no {', '.join(missing)}; only a phase-3 Store (the three authority tables) is converted")
    rows = con.execute("SELECT schema_source,schema_definition FROM history_meta WHERE id=1").fetchall()
    if len(rows) != 1:
        refuse(f"{path} has no one history_meta row")
    recorded, definition = rows[0]
    if definition != _definition(con):
        refuse(f"{path}: the live schema differs from the recorded definition")
    expected = _old_source(version)
    if recorded != expected:
        refuse(f"{path}: the recorded schema is not the phase-3 source this tool converts")
    catalog = con.execute("SELECT catalog_id FROM history_meta WHERE id=1").fetchone()[0]
    return con, version, bytes(catalog).hex()


def _checked(what, name):
    """A machine or Sandbox name as the home holds it, or a refusal."""
    if not isinstance(name, str) or not NAME.fullmatch(name) or name in DEVICES:
        refuse(f"{what} name {name!r} is not a checked home name; the three tables are not trusted paths")


def _authority(con):
    machines = {
        machine: [incarnation, updated_ms]
        for machine, incarnation, updated_ms in con.execute(
            "SELECT machine,incarnation,updated_ms FROM gc_machine_authority ORDER BY machine"
        )
    }
    tombstones = {}
    for sandbox, machine, generation, retired_ms in con.execute(
        "SELECT sandbox,machine,generation,retired_ms FROM gc_sandbox_tombstone ORDER BY machine,sandbox"
    ):
        _checked("machine", machine)
        _checked("Sandbox", sandbox)
        tombstones.setdefault(machine, []).append([sandbox, generation, retired_ms])
    for machine in machines:
        _checked("machine", machine)
    audit = [
        {"seq": seq, "at_ms": at_ms, "action": action, "op_id": op_id, "detail": detail}
        for seq, at_ms, action, op_id, detail in con.execute(
            "SELECT seq,at_ms,action,op_id,detail FROM gc_audit ORDER BY seq"
        )
    ]
    known = sorted(set(machines) | set(tombstones))
    return machines, tombstones, audit, known


def _lock_down(path):
    os.chmod(path, 0o600)


def _write_json(path, value):
    path = Path(path)
    path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    temporary = path.with_name(f".{path.name}.{os.getpid()}.tmp")
    temporary.write_text(json.dumps(value, separators=(",", ":")) + "\n")
    _lock_down(temporary)
    os.replace(temporary, path)


def _read_json(path):
    path = Path(path)
    if not path.is_file():
        return None
    try:
        return json.loads(path.read_text())
    except ValueError as error:
        refuse(f"{path}: {error}")


def _layout_is_current(home):
    home = Path(home)
    value = _read_json(home / "layerfs.json")
    if not isinstance(value, dict) or value.get("layout") != 3:
        refuse(f"{home} is not a current (layout 3) home; open it once with this build, then convert")


def _entity(value):
    """A record's value with the layout 3 `"v": 1` first, as `write_entity` writes it."""
    fields = [("v", 1)] + [(name, item) for name, item in value.items() if name != "v"]
    return dict(fields)


def _ordered(token):
    """A `p1:` ordered token, or None: an old random/time value has no order."""
    if not isinstance(token, str) or not token.startswith("p1:"):
        return None
    parts = token.split(":")
    if len(parts) != 4 or len(parts[2]) != 16 or not parts[2].isdigit():
        return None
    return token


def _newer(recorded, stored):
    """The value a home keeps: the newer of two ordered ones, else an ordered home value, else the Store's."""
    recorded_ordered, stored_ordered = _ordered(recorded), _ordered(stored)
    if recorded_ordered is not None and stored_ordered is not None:
        return max(recorded_ordered, stored_ordered)
    if recorded_ordered is not None:
        return recorded
    return stored


def _home_machine(home, machine):
    return (Path(home) / "machine" / machine / "record.json").is_file()


def _set_incarnation(home, machine, incarnation):
    path = Path(home) / "machine" / machine / "record.json"
    value = _read_json(path)
    if not isinstance(value, dict):
        refuse(f"{path}: not a machine record")
    recorded = value.get("incarnation")
    if recorded != incarnation:
        value["incarnation"] = incarnation if recorded is None else _newer(recorded, incarnation)
    _write_json(path, _entity(value))
    return value["incarnation"]


def _write_tombstone(home, machine, sandbox, generation, retired_ms):
    path = Path(home) / "machine" / machine / "sandbox-retired" / f"{sandbox}.json"
    value = _read_json(path) or {
        "name": sandbox,
        "machine": machine,
        "generation": generation,
        "retired_ms": retired_ms,
    }
    if not isinstance(value, dict):
        refuse(f"{path}: not a tombstone")
    recorded = value.get("generation")
    if recorded != generation:
        value["generation"] = generation if recorded is None else _newer(recorded, generation)
    value["retired_ms"] = max(int(value.get("retired_ms") or 0), retired_ms)
    _write_json(path, _entity(value))
    return value["generation"]


def _write_audit(home, audit):
    path = Path(home) / "log" / "gc-audit.jsonl"
    path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    lines = []
    for row in audit:
        line = {"seq": row["seq"], "at_ms": row["at_ms"], "action": row["action"]}
        if row["op_id"] is not None:
            line["op_id"] = {"hex": bytes(row["op_id"]).hex()}
        if row["detail"] is not None:
            line["detail"] = row["detail"]
        lines.append(json.dumps(line, separators=(",", ":")))
    temporary = path.with_name(f".{path.name}.{os.getpid()}.tmp")
    temporary.write_text("".join(f"{line}\n" for line in lines))
    _lock_down(temporary)
    os.replace(temporary, path)


def _ledger_path(home):
    return Path(home) / "log" / "gc-conversion.json"


def _ledger(home, catalog):
    ledger = _read_json(_ledger_path(home)) or {"v": 1, "stores": {}}
    if not isinstance(ledger, dict) or not isinstance(ledger.get("stores"), dict):
        refuse("the conversion ledger is not a ledger")
    store = ledger["stores"].setdefault(catalog, {"machines": {}})
    if not isinstance(store.get("machines"), dict):
        refuse("the conversion ledger's machines are not a map")
    return ledger, store


def inspect(path):
    """The read-only summary of a phase-3 schema version 2 Store."""
    con, version, catalog = _open_closed(path)
    machines, tombstones, audit, known = _authority(con)
    con.close()
    return {
        "user_version": version,
        "catalog": catalog,
        "machines": known,
        "authority": machines,
        "tombstones": {machine: rows for machine, rows in sorted(tombstones.items())},
        "audit_rows": len(audit),
    }


def copy(path, home):
    """Copy this home's machines out of the Store; read only on the Store."""
    home = Path(home)
    _layout_is_current(home)
    con, version, catalog = _open_closed(path)
    machines, tombstones, audit, known = _authority(con)
    con.close()
    owned = [machine for machine in known if _home_machine(home, machine)]
    copied = {}
    for machine in owned:
        recorded = None
        if machine in machines:
            recorded = _set_incarnation(home, machine, machines[machine][0])
        retired = []
        for sandbox, generation, retired_ms in tombstones.get(machine, []):
            written = _write_tombstone(home, machine, sandbox, generation, retired_ms)
            retired.append({"sandbox": sandbox, "generation": written})
        copied[machine] = {
            "at_ms": _now_ms(),
            "incarnation": recorded,
            "tombstones": retired,
        }
    _write_audit(home, audit)
    ledger, store = _ledger(home, catalog)
    for machine, entry in copied.items():
        store["machines"][machine] = {
            "at_ms": entry["at_ms"],
            "incarnation": entry["incarnation"],
            "tombstones": len(entry["tombstones"]),
            "audit_rows": len(audit),
        }
    _write_json(_ledger_path(home), ledger)
    return {
        "source": str(Path(path)),
        "user_version": version,
        "catalog": catalog,
        "known": known,
        "owned": owned,
        "missing": [machine for machine in known if machine not in owned],
        "copied": copied,
        "audit_rows": len(audit),
        "audit_export": str(home / "log" / "gc-audit.jsonl"),
        "ledger": str(_ledger_path(home)),
    }


def _convert_file(source, target, version):
    """Copy `source` to `target` and lay schema version 3 over it."""
    new = VERSIONS[version]
    shutil.copyfile(source, target)
    try:
        con = sqlite3.connect(target, isolation_level=None)
        con.execute("BEGIN IMMEDIATE")
        for table in TABLES:
            con.execute(f"DROP TABLE {table}")
        con.execute("PRAGMA writable_schema=ON")
        if version < 42:
            # gc_audit's AUTOINCREMENT created sqlite_sequence; the lean
            # non-acquisition table set has none, and SQLite cannot DROP it.
            con.execute("DELETE FROM sqlite_schema WHERE type='table' AND name='sqlite_sequence'")
        for table in ("store_policy", "history_meta"):
            con.execute(
                "UPDATE sqlite_schema SET sql=replace(sql,?,?) WHERE type='table' AND name=?",
                (f"schema_version={version})", f"schema_version={new})", table),
            )
        con.execute("PRAGMA writable_schema=OFF")
        con.execute("COMMIT")
        con.close()
        # Reopen so SQLite reads the patched schema text before the version columns change.
        con = sqlite3.connect(target, isolation_level=None)
        con.execute("BEGIN IMMEDIATE")
        con.execute(f"PRAGMA user_version={new}")
        con.execute("UPDATE store_policy SET schema_version=?", (new,))
        con.execute(
            "UPDATE history_meta SET schema_version=?,schema_source=?,schema_definition=?",
            (new, _source(new), _definition(con)),
        )
        con.execute("COMMIT")
        # VACUUM first: deleting the phase-3 `sqlite_sequence` schema row
        # leaves its page unused until the rebuild reclaims it.
        con.execute("VACUUM")
        if con.execute("PRAGMA integrity_check").fetchone()[0] != "ok":
            raise SystemExit("refused: the converted Store fails integrity_check; the target was removed")
        con.close()
        for suffix in ("-wal", "-shm"):
            leftover = Path(str(target) + suffix)
            if leftover.exists():
                if leftover.stat().st_size:
                    raise SystemExit(f"refused: the converted Store left a non-empty {suffix}; the target was removed")
                leftover.unlink()
    except BaseException:
        Path(target).unlink(missing_ok=True)
        raise
    return new


def convert(path, target, home, converted=(), gone=()):
    """Copy this home's machines, then convert; drop only with every machine accounted for."""
    target = Path(target)
    if target.exists():
        refuse(f"{target} exists; a conversion writes a new Store, never over one")
    home = Path(home)
    report = copy(path, home)
    known, owned = set(report["known"]), set(report["owned"])
    converted, gone = set(converted), set(gone)
    owned_gone = sorted(owned & gone)
    if owned_gone:
        refuse(f"this home owns {', '.join(owned_gone)}; convert it, do not declare it gone")
    uncovered = sorted(known - owned - converted - gone)
    if uncovered:
        refuse(
            "no home copied and no operator declaration covers "
            + ", ".join(uncovered)
            + "; run `copy` in each owner home, then name the rest with --converted or --gone"
        )
    temporary = target.with_name(f".{target.name}.{os.getpid()}.tmp")
    new = _convert_file(path, temporary, report["user_version"])
    os.replace(temporary, target)
    ledger, store = _ledger(home, report["catalog"])
    entry = {
        "at_ms": _now_ms(),
        "from": report["user_version"],
        "to": new,
        "target": str(target),
        "converted": sorted(converted),
        "gone": sorted(gone),
    }
    store["convert"] = entry
    _write_json(_ledger_path(home), ledger)
    report["converted"] = sorted(converted)
    report["gone"] = sorted(gone)
    report["target"] = str(target)
    report["from"] = report["user_version"]
    report["to"] = new
    report["dropped"] = list(TABLES)
    return report


def _now_ms():
    import time

    return int(time.time() * 1000)


def _usage():
    for paragraph in __doc__.strip().split("\n\n"):
        if "store_gc_convert.py inspect" in paragraph:
            return paragraph
    return __doc__


def main(argv):
    arguments = list(argv[1:])
    if not arguments or arguments[0] in ("-h", "--help"):
        print(_usage())
        return 0
    command, rest = arguments[0], arguments[1:]
    if command == "inspect":
        if len(rest) != 1:
            print(_usage(), file=sys.stderr)
            return 2
        print(json.dumps(inspect(rest[0]), separators=(",", ":")))
        return 0
    if command == "copy":
        if len(rest) != 2:
            print(_usage(), file=sys.stderr)
            return 2
        print(json.dumps(copy(rest[0], rest[1]), separators=(",", ":")))
        return 0
    if command == "convert":
        converted, gone, positional, option = [], [], [], None
        for word in rest:
            if option is not None:
                (converted if option == "--converted" else gone).append(word)
                option = None
            elif word in ("--converted", "--gone"):
                option = word
            else:
                positional.append(word)
        if option is not None or len(positional) != 3:
            print(_usage(), file=sys.stderr)
            return 2
        print(
            json.dumps(
                convert(positional[0], positional[1], positional[2], converted, gone),
                separators=(",", ":"),
            )
        )
        return 0
    print(f"refused: no command {command}", file=sys.stderr)
    return 2


if __name__ == "__main__":
    try:
        sys.exit(main(sys.argv))
    except SystemExit as error:
        if not isinstance(error.code, int):
            print(error.code, file=sys.stderr)
            sys.exit(1)
        raise
