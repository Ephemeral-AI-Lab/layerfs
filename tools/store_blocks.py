#!/usr/bin/env python3
"""Inode value and binding hint blocks of a schema version 2 Store (#91).

Diagnostic decoder and offline SQLite migration; never run against a live Store.

  store_blocks.py attribute <closed copy.sqlite>
      Decodes every block of a closed SQLite copy (opened immutable, read only)
      and prints per-table entry, block and byte counts with a per-field split,
      as JSON. DIAGNOSTIC: rows are no longer readable with plain SQL.
  store_blocks.py attribute --rows <file.tsv>
      The same for rows exported from a PostgreSQL schema, one per line:
      `value<TAB>first<TAB>last<TAB>hex body` or `hint<TAB>grp<TAB>seq<TAB>hex body`
      (for example `psql -At -F $'\\t' -c "SELECT 'value',first_ordinal,last_ordinal,
      encode(body,'hex') FROM <schema>.inode_value_block"`).
  store_blocks.py migrate <closed v1 copy.sqlite> <new.sqlite>
      Writes a schema version 2 copy of a closed schema version 1 SQLite Store
      (user_version 16-18 or 22-24): inode values and binding hints become blocks,
      the version moves to 36-38 or 42-44 and the recorded schema is rewritten.
      The source is copied first and never changed; a Store whose recorded schema
      is not exactly one this release wrote (allowing the tables the open-time
      additive migrations add) is refused. PostgreSQL schemas are not migrated.

The block formats are documented in engine/crates/layerfs-persistence/src/blocks/.
"""

import json
import os
import shutil
import sqlite3
import sys
from collections import Counter, defaultdict

BODY_LIMIT = 4000
VALUE_BYTES = 73
RAW = 0xFF
GROUP_BITS = 6
APPLICATION_ID = 1279677264
SQL = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "engine", "crates", "layerfs-persistence", "sql")

# Schema version 1 table texts, as release 2.0 wrote them before #91.
V1_VALUES = """CREATE TABLE inode_value (
 ordinal INTEGER PRIMARY KEY CHECK(ordinal BETWEEN 1 AND 4294967295),
 value BLOB NOT NULL CHECK(length(value)=73)
) STRICT;"""
V1_HINTS = """CREATE TABLE binding_hint (
    child INTEGER NOT NULL CHECK (child != 0),
    parent INTEGER NOT NULL CHECK (parent != 0),
    name BLOB NOT NULL CHECK (length(name) BETWEEN 1 AND 255),
    PRIMARY KEY (child, parent, name)
) STRICT, WITHOUT ROWID;"""


def _crc_table():
    table = []
    for byte in range(256):
        crc = byte
        for _ in range(8):
            crc = (crc >> 1) ^ 0x82F63B78 if crc & 1 else crc >> 1
        table.append(crc)
    return table


CRC_TABLE = _crc_table()


def crc32c(data):
    crc = 0xFFFFFFFF
    for byte in data:
        crc = CRC_TABLE[(crc ^ byte) & 0xFF] ^ (crc >> 8)
    return crc ^ 0xFFFFFFFF


def varint(value):
    out = bytearray()
    while value >= 0x80:
        out.append((value & 0x7F) | 0x80)
        value >>= 7
    out.append(value)
    return bytes(out)


def zigzag(value):
    value = ((value + 2**63) % 2**64) - 2**63  # wrap to i64
    return ((value << 1) ^ (value >> 63)) & (2**64 - 1)


def unzigzag(value):
    return (value >> 1) ^ -(value & 1)


def wrap64(value):
    return ((value + 2**63) % 2**64) - 2**63


class Reader:
    def __init__(self, data):
        self.data, self.at = data, 0

    def take(self, n):
        if self.at + n > len(self.data):
            raise ValueError("block truncated")
        part = self.data[self.at : self.at + n]
        self.at += n
        return part

    def byte(self):
        return self.take(1)[0]

    def varint(self):
        value = 0
        for index in range(10):
            byte = self.byte()
            if (index == 9 and byte > 1) or (index > 0 and byte == 0):
                raise ValueError("non-minimal varint")
            value |= (byte & 0x7F) << (7 * index)
            if not byte & 0x80:
                return value
        raise ValueError("varint too long")


def opened(body):
    if len(body) < 4 or len(body) > BODY_LIMIT:
        raise ValueError("block length")
    content, trailer = body[:-4], body[-4:]
    if crc32c(content) != int.from_bytes(trailer, "little"):
        raise ValueError("block checksum")
    return content


def _value_entry(value, modes, previous_mtime):
    """(entry bytes, new mode or None, mtime) of one value; modes is the block's table."""
    inline = value[0] & 0x80 and value[0] != RAW and not any(value[57:])
    if not inline and value[0] & 0x80:
        return bytes([RAW]) + value, None, previous_mtime
    entry = bytes([value[0]]) + varint(int.from_bytes(value[1:9], "big")) + value[9:41]
    if not inline:
        return entry + value[41:73], None, previous_mtime
    mode = value[41:45]
    new = None
    if mode in modes:
        index = modes.index(mode)
    elif len(modes) < 255:
        index, new = len(modes), mode
    else:
        return None, None, previous_mtime
    mtime = int.from_bytes(value[45:53], "big", signed=True)
    entry += bytes([index]) + varint(zigzag(mtime - previous_mtime)) + value[53:57]
    return entry, new, mtime


def encode_values(rows):
    """[(first, last, body)] of ascending distinct (ordinal, 73 bytes) rows, as the engine packs them."""
    blocks, block, previous = [], None, None

    def finish(b):
        body = bytes([1, b["count"], len(b["modes"])]) + b"".join(b["modes"]) + bytes(b["entries"])
        body += crc32c(body).to_bytes(4, "little")
        blocks.append((b["first"], b["first"] + b["count"] - 1, body))

    def push(b, value):
        if b["count"] == 255:
            return False
        entry, new, mtime = _value_entry(value, b["modes"], b["mtime"])
        if entry is None:
            return False
        size = 3 + 4 * len(b["modes"]) + len(b["entries"]) + 4 + len(entry) + (4 if new else 0)
        if b["count"] and size > BODY_LIMIT:
            return False
        if new:
            b["modes"].append(new)
        b["mtime"], b["count"] = mtime, b["count"] + 1
        b["entries"] += entry
        return True

    for ordinal, value in rows:
        value = bytes(value)
        if ordinal <= 0 or len(value) != VALUE_BYTES or (previous is not None and ordinal <= previous):
            raise ValueError("values must be ascending distinct 73-byte rows")
        follows = previous is not None and ordinal == previous + 1
        previous = ordinal
        if block and follows and push(block, value):
            continue
        if block:
            finish(block)
        block = {"first": ordinal, "count": 0, "modes": [], "mtime": 0, "entries": bytearray()}
        push(block, value)
    if block:
        finish(block)
    return blocks


def decode_values(first, last, body, fields=None):
    reader = Reader(opened(body))
    if reader.byte() != 1:
        raise ValueError("value block format")
    count = reader.byte()
    if count == 0 or last - first + 1 != count:
        raise ValueError("value block count")
    modes = [reader.take(4) for _ in range(reader.byte())]
    fields = fields if fields is not None else Counter()
    fields["header"] += 3 + 4 * len(modes) + 4
    mtime, values = 0, []
    for _ in range(count):
        start = reader.at
        tag = reader.byte()
        if tag == RAW:
            values.append(reader.take(VALUE_BYTES))
            fields["raw"] += reader.at - start
            continue
        at = reader.at
        refs = reader.varint()
        fields["refcount"] += reader.at - at
        value = bytes([tag]) + refs.to_bytes(8, "big") + reader.take(32)
        fields["content_root"] += 32
        if tag & 0x80 == 0:
            value += reader.take(32)
            fields["attribute_root"] += 32
        else:
            value += modes[reader.byte()]
            at = reader.at
            mtime = wrap64(mtime + unzigzag(reader.varint()))
            fields["mtime"] += reader.at - at
            value += mtime.to_bytes(8, "big", signed=True) + reader.take(4) + bytes(16)
            fields["mode_index+nanos"] += 5
        fields["tag"] += 1
        values.append(value)
    if reader.at != len(reader.data):
        raise ValueError("value block trailing bytes")
    return values


def encode_hints(rows):
    """[(grp, body)] of one write's (child, parent, name) hints, as the engine packs them."""
    hints = sorted({(c, p, bytes(n)) for c, p, n in rows})
    if any(c == 0 or p == 0 or not 1 <= len(n) <= 255 for c, p, n in hints):
        raise ValueError("binding hint out of range")
    blocks, block = [], None

    def finish(b):
        body = bytes([1]) + varint(b["count"]) + bytes(b["entries"])
        blocks.append((b["group"], body + crc32c(body).to_bytes(4, "little")))

    def push(b, hint):
        child, parent, name = b["previous"] or ((b["group"] << GROUP_BITS), 0, b"")
        common = 0
        while common < min(len(name), len(hint[2]), 255) and name[common] == hint[2][common]:
            common += 1
        entry = varint(hint[0] - child) + varint(zigzag(hint[1] - parent))
        entry += bytes([common]) + varint(len(hint[2]) - common) + hint[2][common:]
        size = 1 + len(varint(b["count"] + 1)) + len(b["entries"]) + len(entry) + 4
        if b["count"] and size > BODY_LIMIT:
            return False
        b["entries"] += entry
        b["count"] += 1
        b["previous"] = hint
        return True

    for hint in hints:
        group = hint[0] >> GROUP_BITS
        if block and block["group"] == group and push(block, hint):
            continue
        if block:
            finish(block)
        block = {"group": group, "count": 0, "entries": bytearray(), "previous": None}
        push(block, hint)
    if block:
        finish(block)
    return blocks


def decode_hints(group, body, fields=None):
    reader = Reader(opened(body))
    if reader.byte() != 1:
        raise ValueError("hint block format")
    count = reader.varint()
    fields = fields if fields is not None else Counter()
    fields["header"] += reader.at + 4
    child, parent, name, hints = group << GROUP_BITS, 0, b"", []
    for _ in range(count):
        at = reader.at
        child += reader.varint()
        parent = wrap64(parent + unzigzag(reader.varint())) % 2**64
        fields["child+parent"] += reader.at - at
        common = reader.byte()
        rest = reader.varint()
        if common > len(name):
            raise ValueError("hint name prefix")
        name = name[:common] + reader.take(rest)
        fields["name"] += 1 + len(varint(rest)) + rest
        hint = (child, parent, name)
        if child >> GROUP_BITS != group or (hints and hints[-1] >= hint):
            raise ValueError("hint order")
        hints.append(hint)
    if reader.at != len(reader.data):
        raise ValueError("hint block trailing bytes")
    return hints


def attribute(values, hints):
    """Summary of [(first, last, body)] value blocks and [(grp, seq, body)] hint blocks."""
    vf, hf = Counter(), Counter()
    decoded = [v for first, last, body in values for v in decode_values(first, last, body, vf)]
    forms = Counter(
        "raw" if v[0] == RAW or (v[0] & 0x80 and any(v[57:])) else "inline" if v[0] & 0x80 else "root" for v in decoded
    )
    entries = sum(len(decode_hints(g, body, hf)) for g, _, body in hints)
    groups = defaultdict(int)
    for g, _, _ in hints:
        groups[g] += 1
    vb, hb = sum(len(b) for *_, b in values), sum(len(b) for *_, b in hints)
    return {
        "label": "DIAGNOSTIC",
        "inode_value_block": {
            "blocks": len(values),
            "entries": len(decoded),
            "body_bytes": vb,
            "body_bytes_per_entry": round(vb / len(decoded), 2) if decoded else None,
            "forms": dict(forms),
            "duplicate_values": len(decoded) - len(set(decoded)),
            "field_bytes": dict(vf),
        },
        "binding_hint_block": {
            "blocks": len(hints),
            "groups": len(groups),
            "max_blocks_per_group": max(groups.values(), default=0),
            "entries": entries,
            "body_bytes": hb,
            "body_bytes_per_entry": round(hb / entries, 2) if entries else None,
            "field_bytes": dict(hf),
        },
    }


def _read_sql(*parts):
    with open(os.path.join(SQL, *parts)) as stream:
        return stream.read()


def _source(version, layout, tables, without=()):
    """The schema source the engine records for a version (scripts in its order), less `without`."""
    objects = {16: "objects.sql", 17: "objects_units.sql", 18: "objects_units.sql"}[layout]
    scripts = [_read_sql("sqlite", objects) + (_read_sql("sqlite", "objects_units_index.sql") if layout == 18 else "")]
    scripts += [
        _read_sql("sqlite", n).replace("schema_version=1", f"schema_version={version}")
        for n in ("metadata.sql", "history.sql")
    ]
    scripts += [_read_sql("sqlite", n) for n in ("lineage.sql", "merge_scratch.sql", "file_digest.sql")]
    if tables:
        scripts.append(_read_sql("sqlite", "acquisition", "schema.sql"))
    source = "".join(scripts)
    for text in without:
        source = source.replace(text, "", 1)
    return source


def _statement(script, head):
    start = script.index(head)
    return script[start : start + script[start:].index(";\n") + 1]


def _definition(con):
    rows = con.execute("SELECT type,name,sql FROM sqlite_schema WHERE substr(name,1,7)!='sqlite_' ORDER BY type,name")
    return "".join(f"{t}\n{n}\n{s}\n" for t, n, s in rows)


def migrate(source_path, target_path):
    if os.path.exists(target_path) or os.path.exists(source_path + "-wal") and os.path.getsize(source_path + "-wal"):
        raise SystemExit("refused: target exists, or the source is not closed (non-empty WAL)")
    shutil.copyfile(source_path, target_path)
    con = sqlite3.connect(target_path, isolation_level=None)
    old = con.execute("PRAGMA user_version").fetchone()[0]
    if con.execute("PRAGMA application_id").fetchone()[0] != APPLICATION_ID or old not in (16, 17, 18, 22, 23, 24):
        raise SystemExit(f"refused: not a schema version 1 Store (user_version {old})")
    tables, layout, new = old >= 22, old - (6 if old >= 22 else 0), old + 20
    recorded, definition = con.execute("SELECT schema_source,schema_definition FROM history_meta").fetchone()
    if definition != _definition(con):
        raise SystemExit("refused: live schema differs from the recorded definition")
    metadata, history = _read_sql("sqlite", "metadata.sql"), _read_sql("sqlite", "history.sql")
    v2_values, v2_hints = (
        _statement(metadata, "CREATE TABLE inode_value_block ("),
        _statement(history, "CREATE TABLE binding_hint_block ("),
    )
    # The open-time additive migrations add these, newest first; a Store may lack a suffix of them.
    optional = [_read_sql("sqlite", n) for n in ("file_digest.sql", "merge_scratch.sql", "lineage.sql")]
    hint_table = f"{v2_hints}\n\n"
    for missing in range(len(optional) + 2):
        without = optional[:missing] + ([hint_table] if missing == len(optional) + 1 else [])
        v1 = _source(old, layout, tables, without).replace(v2_values, V1_VALUES).replace(v2_hints, V1_HINTS)
        if recorded == v1:
            break
    else:
        raise SystemExit("refused: the recorded schema is not one this release wrote")
    con.execute("BEGIN IMMEDIATE")
    values = encode_values(con.execute("SELECT ordinal,value FROM inode_value ORDER BY ordinal").fetchall())
    con.execute("DROP TABLE inode_value")
    con.execute(v2_values)
    con.executemany("INSERT INTO inode_value_block VALUES (?,?,?)", values)
    has_hints = "binding_hint" in {r[0] for r in con.execute("SELECT name FROM sqlite_schema WHERE type='table'")}
    if has_hints:
        hints = encode_hints(con.execute("SELECT child,parent,name FROM binding_hint").fetchall())
        con.execute("DROP TABLE binding_hint")
        con.execute(v2_hints)
        seq = Counter()
        for group, body in hints:
            seq[group] += 1
            con.execute("INSERT INTO binding_hint_block VALUES (?,?,?)", (group, seq[group], body))
    con.execute("PRAGMA writable_schema=ON")
    for table in ("store_policy", "history_meta"):
        con.execute(
            "UPDATE sqlite_schema SET sql=replace(sql,?,?) WHERE type='table' AND name=?",
            (f"schema_version={old})", f"schema_version={new})", table),
        )
    con.execute("PRAGMA writable_schema=OFF")
    con.execute("COMMIT")
    con.close()
    con = sqlite3.connect(target_path, isolation_level=None)
    con.execute("BEGIN IMMEDIATE")
    con.execute(f"PRAGMA user_version={new}")
    con.execute("UPDATE store_policy SET schema_version=?", (new,))
    source = _source(new, layout, tables, without)
    con.execute(
        "UPDATE history_meta SET schema_version=?,schema_source=?,schema_definition=?", (new, source, _definition(con))
    )
    con.execute("COMMIT")
    if con.execute("PRAGMA integrity_check").fetchone()[0] != "ok":
        raise SystemExit("integrity check failed; remove the target")
    con.execute("VACUUM")
    con.execute("PRAGMA wal_checkpoint(TRUNCATE)")
    con.close()
    for suffix in ("-wal", "-shm"):
        if os.path.exists(target_path + suffix) and not os.path.getsize(target_path + suffix):
            os.remove(target_path + suffix)
    return {"user_version": [old, new], "value_blocks": len(values), "hint_blocks": len(hints) if has_hints else 0}


def main(argv):
    if len(argv) == 3 and argv[1] == "attribute" and argv[2] != "--rows":
        con = sqlite3.connect(f"file:{argv[2]}?immutable=1", uri=True)
        values = con.execute("SELECT first_ordinal,last_ordinal,body FROM inode_value_block ORDER BY 1").fetchall()
        hints = con.execute("SELECT grp,seq,body FROM binding_hint_block ORDER BY 1,2").fetchall()
        print(json.dumps(attribute(values, hints), indent=1))
    elif len(argv) == 4 and argv[1:3] == ["attribute", "--rows"]:
        values, hints = [], []
        with open(argv[3]) as stream:
            for line in stream:
                kind, a, b, body = line.rstrip("\n").split("\t")
                (values if kind == "value" else hints).append((int(a), int(b), bytes.fromhex(body)))
        print(json.dumps(attribute(sorted(values), sorted(hints)), indent=1))
    elif len(argv) == 4 and argv[1] == "migrate":
        print(json.dumps(migrate(argv[2], argv[3])))
    else:
        raise SystemExit(__doc__)


if __name__ == "__main__":
    main(sys.argv)
