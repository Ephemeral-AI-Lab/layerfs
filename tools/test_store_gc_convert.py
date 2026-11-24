"""Tests for store_gc_convert.py: a phase-3 schema version 2 Store becomes the
lean schema version 3 one, per machine, with the three tables dropped only
after every known machine is accounted for."""

import hashlib
import json
import os
import sqlite3
import tempfile
import unittest
from pathlib import Path

import store_gc_convert as sg


def store(path, version=36, machines=None, tombstones=None, audit=None):
    """A closed phase-3 schema version 2 Store as the released build wrote it."""
    source = sg._old_source(version)
    con = sqlite3.connect(path, isolation_level=None)
    con.executescript(source)
    con.execute(f"PRAGMA application_id={sg.APPLICATION_ID}")
    con.execute(f"PRAGMA user_version={version}")
    con.execute(
        "INSERT INTO store_policy(id,schema_version,format_profile,small_file_threshold_bytes,"
        "whole_file_delta_max_depth,chunk_delta_max_depth,metadata_delta_max_depth) "
        "VALUES(1,?,1,262144,3,3,3)",
        (version,),
    )
    con.execute(
        "INSERT INTO history_meta(id,catalog_id,catalog_incarnation,binding_key,"
        "identity_format,schema_version,schema_source,schema_definition) "
        "VALUES(1,?,1,?,3,?,?,?)",
        (bytes(32), b"binding", version, source, sg._definition(con)),
    )
    for machine, (incarnation, updated_ms) in (machines or {}).items():
        con.execute(
            "INSERT INTO gc_machine_authority(machine,incarnation,updated_ms) VALUES(?,?,?)",
            (machine, incarnation, updated_ms),
        )
    for sandbox, machine, generation, retired_ms in tombstones or ():
        con.execute(
            "INSERT INTO gc_sandbox_tombstone(sandbox,machine,generation,retired_ms) VALUES(?,?,?,?)",
            (sandbox, machine, generation, retired_ms),
        )
    for at_ms, action, op_id, detail in audit or ():
        con.execute(
            "INSERT INTO gc_audit(at_ms,action,op_id,detail) VALUES(?,?,?,?)",
            (at_ms, action, op_id, detail),
        )
    con.close()
    return path


def home(root, machines=("native",), layout=3):
    """A layout 3 home with one record per machine."""
    path = Path(root) / "home"
    path.mkdir(parents=True)
    (path / "layerfs.json").write_text(json.dumps({"layout": layout, "id": "a" * 32}) + "\n")
    for machine in machines:
        folder = path / "machine" / machine
        folder.mkdir(parents=True)
        (folder / "record.json").write_text(
            json.dumps({"v": 1, "name": machine, "platform": machine, "created": 1}, separators=(",", ":")) + "\n"
        )
    return path


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def fresh_tables(version):
    """The table set `Session::initialize` creates for a version 3 layout."""
    con = sqlite3.connect(":memory:")
    con.executescript(sg._source(version))
    names = {row[0] for row in con.execute("SELECT name FROM sqlite_schema WHERE type='table'")}
    con.close()
    return names


class Conversion(unittest.TestCase):
    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self._tmp.cleanup)
        self.root = Path(self._tmp.name)
        self.v2 = self.root / "old.sqlite"
        self.target = self.root / "new.sqlite"

    def test_inspect_reads_the_phase_three_store(self):
        store(
            self.v2,
            machines={"native": ("old-incarnation", 5)},
            tombstones=[("box", "native", "old-generation", 6)],
            audit=[(7, "settle", bytes(16), "detail")],
        )
        found = sg.inspect(self.v2)
        self.assertEqual(found["user_version"], 36)
        self.assertEqual(found["catalog"], "00" * 32)
        self.assertEqual(found["machines"], ["native"])
        self.assertEqual(found["authority"], {"native": ["old-incarnation", 5]})
        self.assertEqual(found["tombstones"], {"native": [["box", "old-generation", 6]]})
        self.assertEqual(found["audit_rows"], 1)

    def test_copy_writes_home_records_and_leaves_the_store_alone(self):
        before = digest(
            store(
                self.v2,
                machines={"native": ("p1:native:0000000000000005:aa", 5), "docker": ("worker", 6)},
                tombstones=[("box", "native", "p1:sandbox:0000000000000007:bb", 7)],
                audit=[(8, "reconcile", bytes(range(16)), "start")],
            )
        )
        where = home(self.root, machines=("native",))
        report = sg.copy(self.v2, where)
        self.assertEqual(digest(self.v2), before, "the source is read only")
        self.assertEqual(report["known"], ["docker", "native"])
        self.assertEqual(report["owned"], ["native"])
        self.assertEqual(report["missing"], ["docker"])
        self.assertEqual(report["audit_rows"], 1)
        record = json.loads((where / "machine/native/record.json").read_text())
        self.assertEqual(
            record,
            {
                "v": 1,
                "name": "native",
                "platform": "native",
                "created": 1,
                "incarnation": "p1:native:0000000000000005:aa",
            },
        )
        tombstone = json.loads((where / "machine/native/sandbox-retired/box.json").read_text())
        self.assertEqual(
            tombstone,
            {
                "v": 1,
                "name": "box",
                "machine": "native",
                "generation": "p1:sandbox:0000000000000007:bb",
                "retired_ms": 7,
            },
        )
        lines = [json.loads(line) for line in (where / "log/gc-audit.jsonl").read_text().splitlines()]
        self.assertEqual(
            lines,
            [
                {
                    "seq": 1,
                    "at_ms": 8,
                    "action": "reconcile",
                    "op_id": {"hex": bytes(range(16)).hex()},
                    "detail": "start",
                }
            ],
        )
        ledger = json.loads((where / "log/gc-conversion.json").read_text())
        self.assertEqual(sorted(ledger["stores"]), ["00" * 32])
        self.assertEqual(sorted(ledger["stores"]["00" * 32]["machines"]), ["native"])

    def test_convert_writes_a_version_three_store(self):
        before = digest(
            store(
                self.v2,
                machines={"native": ("p1:native:0000000000000005:aa", 5)},
                tombstones=[("box", "native", "p1:sandbox:0000000000000007:bb", 7)],
                audit=[(9, "release", bytes(16), None)],
            )
        )
        where = home(self.root)
        report = sg.convert(self.v2, self.target, where)
        self.assertEqual(report["from"], 36)
        self.assertEqual(report["to"], 39)
        self.assertEqual(digest(self.v2), before, "the source is the backup and never changes")
        self.assertTrue(self.target.is_file())
        con = sqlite3.connect(self.target)
        self.assertEqual(con.execute("PRAGMA application_id").fetchone()[0], sg.APPLICATION_ID)
        self.assertEqual(con.execute("PRAGMA user_version").fetchone()[0], 39)
        names = {row[0] for row in con.execute("SELECT name FROM sqlite_schema WHERE type='table'")}
        for table in sg.TABLES:
            self.assertNotIn(table, names, table)
        self.assertNotIn("sqlite_sequence", names, "the phase-3 audit's sequence table is gone")
        self.assertEqual(names, fresh_tables(39), "the converted table set is a fresh version 3 one")
        for table in ("store_policy", "history_meta"):
            self.assertEqual(con.execute(f"SELECT schema_version FROM {table}").fetchone()[0], 39)
            text = con.execute("SELECT sql FROM sqlite_schema WHERE name=?", (table,)).fetchone()[0]
            self.assertIn("schema_version=39)", text)
            self.assertNotIn("schema_version=36)", text)
        source, definition = con.execute("SELECT schema_source,schema_definition FROM history_meta").fetchone()
        self.assertEqual(source, sg._source(39))
        self.assertEqual(definition, sg._definition(con))
        self.assertEqual(con.execute("PRAGMA integrity_check").fetchone()[0], "ok")
        con.close()
        ledger = json.loads((where / "log/gc-conversion.json").read_text())
        self.assertEqual(ledger["stores"]["00" * 32]["convert"]["to"], 39)

    def test_convert_refuses_while_a_machine_is_unaccounted_for(self):
        store(self.v2, machines={"native": ("a", 5), "elsewhere": ("b", 6)})
        where = home(self.root)
        with self.assertRaises(SystemExit) as caught:
            sg.convert(self.v2, self.target, where)
        self.assertIn("elsewhere", str(caught.exception.code))
        self.assertFalse(self.target.exists(), "a refused conversion writes no target")

    def test_gone_is_the_missing_machine_policy_after_the_audit_export(self):
        store(
            self.v2,
            machines={"native": ("a", 5), "elsewhere": ("b", 6)},
            tombstones=[("lost", "elsewhere", "gone-generation", 7)],
            audit=[(9, "settle", bytes(16), "gone")],
        )
        where = home(self.root)
        report = sg.convert(self.v2, self.target, where, gone=["elsewhere"])
        self.assertEqual(report["gone"], ["elsewhere"])
        self.assertEqual(report["to"], 39)
        self.assertTrue((where / "log/gc-audit.jsonl").is_file(), "the audit is exported before the drop")
        self.assertEqual(json.loads((where / "log/gc-audit.jsonl").read_text().splitlines()[0])["action"], "settle")
        con = sqlite3.connect(self.target)
        self.assertEqual(con.execute("PRAGMA user_version").fetchone()[0], 39)
        con.close()

    def test_a_three_table_fixture_converts_end_to_end(self):
        """The real old-format fixture: a version 2 Store with the three tables.

        One conversion test covers all four acceptance points together: the
        conversion, the `--gone` missing-machine policy, the source file
        unchanged, and the JSONL audit export. The fixture is the recorded
        phase-3 source (the only shape an old Store wrote); no old binary is
        needed, and `inspect` refuses anything that is not exactly that shape.
        """
        before_digest = digest(
            store(
                self.v2,
                machines={
                    "native": ("p1:native:0000000000000005:aa", 5),
                    "elsewhere": ("p1:worker:0000000000000006:bb", 6),
                },
                tombstones=[
                    ("box", "native", "p1:sandbox:0000000000000007:cc", 7),
                    ("lost", "elsewhere", "p1:sandbox:0000000000000008:dd", 8),
                ],
                audit=[(9, "release", bytes(range(16)), "operator=alice")],
            )
        )
        where = home(self.root, machines=("native",))
        before = sg.inspect(self.v2)
        self.assertEqual(before["user_version"], 36)
        self.assertEqual(before["machines"], ["elsewhere", "native"])
        self.assertEqual(before["audit_rows"], 1)

        report = sg.convert(self.v2, self.target, where, gone=["elsewhere"])
        self.assertEqual(report["gone"], ["elsewhere"])
        self.assertEqual(report["dropped"], list(sg.TABLES))
        self.assertEqual(report["from"], 36)
        self.assertEqual(report["to"], 39)

        self.assertEqual(digest(self.v2), before_digest, "the source is the backup")
        con = sqlite3.connect(self.target)
        self.assertEqual(con.execute("PRAGMA user_version").fetchone()[0], 39)
        names = {row[0] for row in con.execute("SELECT name FROM sqlite_schema WHERE type='table'")}
        for table in sg.TABLES:
            self.assertNotIn(table, names, table)
        self.assertEqual(con.execute("PRAGMA integrity_check").fetchone()[0], "ok")
        con.close()

        # The gone machine's authority and tombstone were not copied, and the
        # home the tool owns got its own machine and tombstone.
        self.assertFalse((where / "machine/elsewhere").exists(), "a gone machine is not copied")
        self.assertFalse((where / "machine/native/sandbox-retired/lost.json").exists())
        copied = json.loads((where / "machine/native/record.json").read_text())
        self.assertEqual(copied["incarnation"], "p1:native:0000000000000005:aa")
        self.assertTrue((where / "machine/native/sandbox-retired/box.json").is_file())

        # The old audit rows were exported before the tables were dropped.
        lines = [json.loads(line) for line in (where / "log/gc-audit.jsonl").read_text().splitlines()]
        self.assertEqual(len(lines), 1)
        self.assertEqual(lines[0]["action"], "release")
        self.assertEqual(lines[0]["op_id"], {"hex": bytes(range(16)).hex()})
        ledger = json.loads((where / "log/gc-conversion.json").read_text())
        self.assertEqual(ledger["stores"]["00" * 32]["convert"]["gone"], ["elsewhere"])

    def test_a_machine_the_home_owns_cannot_be_declared_gone(self):
        store(self.v2, machines={"native": ("a", 5)})
        where = home(self.root)
        with self.assertRaises(SystemExit) as caught:
            sg.convert(self.v2, self.target, where, gone=["native"])
        self.assertIn("convert it, do not declare it gone", str(caught.exception.code))
        self.assertFalse(self.target.exists())

    def test_the_command_line_converts(self):
        store(self.v2, machines={"native": ("a", 5), "elsewhere": ("b", 6)})
        where = home(self.root)
        self.assertEqual(
            sg.main(
                ["store_gc_convert.py", "convert", str(self.v2), str(self.target), str(where), "--gone", "elsewhere"]
            ),
            0,
        )
        self.assertTrue(self.target.is_file())
        self.assertEqual(sg.main(["store_gc_convert.py", "convert"]), 2, "a wrong command line is 2")

    def test_a_converted_elsewhere_machine_is_covered(self):
        store(self.v2, machines={"native": ("a", 5), "elsewhere": ("b", 6)})
        where = home(self.root)
        report = sg.convert(self.v2, self.target, where, converted=["elsewhere"])
        self.assertEqual(report["converted"], ["elsewhere"])
        self.assertTrue(self.target.is_file())

    def test_every_schema_version_2_layout_moves_by_three(self):
        for old, new in ((36, 39), (37, 40), (38, 41), (42, 45), (43, 46), (44, 47)):
            source = self.root / f"v{old}.sqlite"
            target = self.root / f"v{new}.sqlite"
            store(source, version=old, machines={"native": ("a", 5)})
            sg.convert(source, target, home(self.root / f"home-{old}"))
            con = sqlite3.connect(target)
            self.assertEqual(con.execute("PRAGMA user_version").fetchone()[0], new)
            self.assertEqual(con.execute("SELECT schema_source FROM history_meta").fetchone()[0], sg._source(new))
            names = {row[0] for row in con.execute("SELECT name FROM sqlite_schema WHERE type=?", ("table",))}
            self.assertEqual(names, fresh_tables(new), f"version {old} gives a fresh version {new} table set")
            self.assertEqual(con.execute("PRAGMA integrity_check").fetchone()[0], "ok")
            con.close()

    def test_the_acquisition_layout_moves_by_three(self):
        store(self.v2, version=42, machines={"native": ("a", 5)})
        sg.convert(self.v2, self.target, home(self.root))
        con = sqlite3.connect(self.target)
        self.assertEqual(con.execute("PRAGMA user_version").fetchone()[0], 45)
        names = {row[0] for row in con.execute("SELECT name FROM sqlite_schema WHERE type='table'")}
        self.assertIn("init_operation", names, "the acquisition tables stay")
        self.assertIn("sqlite_sequence", names, "the acquisition layout keeps its sequence table")
        self.assertEqual(names, fresh_tables(45), "the converted table set is a fresh version 3 one")
        source = con.execute("SELECT schema_source FROM history_meta").fetchone()[0]
        self.assertEqual(source, sg._source(45))
        con.close()

    def test_a_home_owned_ordered_value_wins_over_the_old_unordered_one(self):
        store(self.v2, machines={"native": ("old-random", 5)})
        where = home(self.root)
        record = where / "machine/native/record.json"
        record.write_text(
            json.dumps(
                {"v": 1, "name": "native", "incarnation": "p1:native:0000000000000009:cc"}, separators=(",", ":")
            )
            + "\n"
        )
        sg.copy(self.v2, where)
        self.assertEqual(json.loads(record.read_text())["incarnation"], "p1:native:0000000000000009:cc")

    def test_copy_is_idempotent(self):
        store(self.v2, machines={"native": ("a", 5)}, audit=[(9, "settle", None, None)])
        where = home(self.root)
        sg.copy(self.v2, where)
        first = json.loads((where / "log/gc-conversion.json").read_text())["stores"]["00" * 32]["machines"]
        sg.copy(self.v2, where)
        again = json.loads((where / "log/gc-conversion.json").read_text())["stores"]["00" * 32]["machines"]
        for entries in (first, again):
            for entry in entries.values():
                entry.pop("at_ms")
        self.assertEqual(again, first)
        self.assertEqual(len((where / "log/gc-audit.jsonl").read_text().splitlines()), 1)

    def test_refusals(self):
        current = store(self.root / "current.sqlite", version=39, machines={})
        with self.assertRaises(SystemExit) as caught:
            sg.inspect(current)
        self.assertIn("not a phase-3 schema version 2 Store", str(caught.exception.code))

        store(self.v2, machines={})
        with self.assertRaises(SystemExit) as caught:
            sg.copy(self.v2, home(self.root / "layout-2", layout=2))
        self.assertIn("not a current (layout 3) home", str(caught.exception.code))

        bad = self.root / "bad-name.sqlite"
        store(bad, machines={"../escape": ("a", 5)})
        with self.assertRaises(SystemExit) as caught:
            sg.inspect(bad)
        self.assertIn("not a checked home name", str(caught.exception.code))

        closed = store(self.root / "closed.sqlite", machines={})
        Path(str(closed) + "-wal").write_text("not empty")
        with self.assertRaises(SystemExit) as caught:
            sg.inspect(closed)
        self.assertIn("not closed", str(caught.exception.code))

        Path(str(closed) + "-wal").unlink()
        self.target.write_text("occupied")
        with self.assertRaises(SystemExit) as caught:
            sg.convert(self.v2, self.target, home(self.root / "layout-3"))
        self.assertIn("exists", str(caught.exception.code))


if __name__ == "__main__":
    unittest.main()
