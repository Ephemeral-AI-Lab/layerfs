#!/usr/bin/env python3
"""Inode value and binding hint blocks of a schema version 2 Store (#91).

Diagnostic decoder; never run against a live Store. (Its offline schema
version 1 migration was removed with schema version 1 support, #804.)

  store_blocks.py attribute <closed copy.sqlite>
      Decodes every block of a closed SQLite copy (opened immutable, read only)
      and prints per-table entry, block and byte counts with a per-field split,
      as JSON. DIAGNOSTIC: rows are no longer readable with plain SQL.
  store_blocks.py attribute --rows <file.tsv>
      The same for rows exported from a PostgreSQL schema, one per line:
      `value<TAB>first<TAB>last<TAB>hex body` or `hint<TAB>grp<TAB>seq<TAB>hex body`
      (for example `psql -At -F $'\\t' -c "SELECT 'value',first_ordinal,last_ordinal,
      encode(body,'hex') FROM <schema>.inode_value_block"`).

The block formats are documented in engine/crates/layerfs-persistence/src/blocks/.
"""

import json
import sqlite3
import sys
from collections import Counter, defaultdict

BODY_LIMIT = 4000
VALUE_BYTES = 73
RAW = 0xFF
GROUP_BITS = 6


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
    else:
        raise SystemExit(__doc__)


if __name__ == "__main__":
    main(sys.argv)
