#!/usr/bin/env python3
"""Independent finite canonical reference. No LayerFS imports or candidate inputs.

This is a development oracle, not a bounded production implementation. Source
tables and descriptions are frozen in contract.json; only raw BLAKE3 is supplied
by the already-pinned external hash helper.
"""
from __future__ import annotations

import hashlib
import json
import struct
import subprocess
from dataclasses import dataclass
from pathlib import Path

HERE = Path(__file__).resolve().parent
SPEC = json.loads((HERE / "contract.json").read_text())
U64 = (1 << 64) - 1


def u16(x):
    return struct.pack(">H", x)


def u32(x):
    return struct.pack(">I", x)


def u64(x):
    return struct.pack(">Q", x)


def name_bytes(name):
    raw = name.encode("utf-8") if isinstance(name, str) else name
    raw.decode("utf-8")
    if not 1 <= len(raw) <= 255 or raw in (b".", b".."):
        raise ValueError("name length/component")
    if any(x in raw for x in (b"\0", b"/", b"\\")):
        raise ValueError("name separator")
    return raw


def envelope(value):
    if len(value) > 8 * 1024 * 1024:
        raise ValueError("canonical field limit")
    return b"LFSO\x01" + u32(len(value) + 4) + u32(len(value)) + value


def unwrap(raw):
    if len(raw) < 13 or raw[:5] != b"LFSO\x01":
        raise ValueError("envelope")
    payload, size = struct.unpack(">II", raw[5:13])
    if size > 8 * 1024 * 1024 or payload != size + 4 or len(raw) != size + 13:
        raise ValueError("envelope length/EOF")
    return raw[13:]


class Hashes:
    def __init__(self):
        binary = HERE / "hash/target/release/layerfs-issue287-oracle-hash"
        if not binary.is_file():
            raise RuntimeError("build the locked hash helper before running the oracle")
        self.child = subprocess.Popen([str(binary)], stdin=subprocess.PIPE,
                                      stdout=subprocess.PIPE)

    def raw(self, value):
        self.child.stdin.write(u64(len(value)) + value)
        self.child.stdin.flush()
        result = self.child.stdout.read(32)
        if len(result) != 32:
            raise RuntimeError("hash provider terminated")
        return result

    def object(self, value):
        return self.raw(bytes.fromhex(SPEC["object_domain_hex"]) + value)

    def close(self):
        self.child.stdin.close()
        if self.child.wait() != 0:
            raise RuntimeError("hash provider exit")


@dataclass(frozen=True)
class MapNode:
    level: int
    entries: tuple

    @property
    def length(self):
        return sum(e[2] if self.level == 0 else e.length for e in self.entries)

    @property
    def count(self):
        return len(self.entries) if self.level == 0 else sum(e.count for e in self.entries)


def coalesce(extents):
    result = []
    for item in extents:
        if result and result[-1][0] == item[0] and result[-1][1] + result[-1][2] == item[1]:
            old = result.pop()
            result.append((old[0], old[1], old[2] + item[2]))
        else:
            result.append(item)
    return result


def map_children(children):
    if not children:
        return None
    if len(children) == 1:
        return children[0]
    if len({x.level for x in children}) != 1:
        raise ValueError("mixed mapping levels")
    level = children[0].level + 1
    if len(children) <= 128:
        return MapNode(level, tuple(children))
    half = len(children) // 2
    return MapNode(level + 1, (MapNode(level, tuple(children[:half])),
                               MapNode(level, tuple(children[half:]))))


def map_extents(extents):
    extents = coalesce(extents)
    if len(extents) <= 128:
        return MapNode(0, tuple(extents))
    half = len(extents) // 2
    return map_children([MapNode(0, tuple(extents[:half])),
                         MapNode(0, tuple(extents[half:]))])


def split(node, at):
    if not 0 <= at <= node.length:
        raise ValueError("split range")
    if at == 0:
        return None, node
    if at == node.length:
        return node, None
    if node.level == 0:
        left, right, cursor = [], [], 0
        for oid, offset, size in node.entries:
            end = cursor + size
            if end <= at:
                left.append((oid, offset, size))
            elif cursor >= at:
                right.append((oid, offset, size))
            else:
                part = at - cursor
                left.append((oid, offset, part))
                right.append((oid, offset + part, size - part))
            cursor = end
        return MapNode(0, tuple(left)), MapNode(0, tuple(right))
    cursor = 0
    for index, child in enumerate(node.entries):
        if cursor + child.length >= at:
            left, right = split(child, at - cursor)
            return (concat(map_children(node.entries[:index]), left),
                    concat(right, map_children(node.entries[index + 1:])))
        cursor += child.length
    raise ValueError("split summary")


def concat(left, right):
    if left is None:
        return right
    if right is None:
        return left
    if left.level == right.level:
        return (map_extents(left.entries + right.entries) if left.level == 0
                else map_children(left.entries + right.entries))
    if left.level > right.level:
        prefix = map_children(left.entries[:-1])
        boundary = concat(left.entries[-1], right)
        if prefix is None:
            return boundary
        if prefix.level == boundary.level:
            return concat(prefix, boundary)
        if prefix.level + 1 == boundary.level:
            return map_children((prefix,) + boundary.entries)
        if boundary.level + 1 == prefix.level:
            return map_children(prefix.entries + (boundary,))
    else:
        boundary = concat(left, right.entries[0])
        suffix = map_children(right.entries[1:])
        if suffix is None:
            return boundary
        if suffix.level == boundary.level:
            return concat(boundary, suffix)
        if suffix.level + 1 == boundary.level:
            return map_children(boundary.entries + (suffix,))
        if boundary.level + 1 == suffix.level:
            return map_children((boundary,) + suffix.entries)
    raise ValueError("mapping join levels")


@dataclass(frozen=True)
class SortedNode:
    form: str
    level: int
    entries: tuple

    @property
    def upper(self):
        return self.entries[-1][0] if self.level == 0 else self.entries[-1].upper

    @property
    def count(self):
        return len(self.entries) if self.level == 0 else sum(x.count for x in self.entries)

    def widths(self):
        if self.level > 0:
            return [34 + len(x.upper) if self.form == "directory" else 40
                    for x in self.entries]
        if self.form == "parent":
            return [19 + len(x[3]) for x in self.entries]
        if self.form == "inode":
            return [81 for _ in self.entries]
        return [10 + len(x[0]) for x in self.entries]

    @property
    def row_bytes(self):
        return sum(self.widths()) if self.level == 0 else sum(x.row_bytes for x in self.entries)

    @property
    def size(self):
        return 44 + sum(self.widths())

    def fits(self):
        limit = (100 if self.level == 0 else 127) if self.form == "inode" else 65535
        return self.size <= 8192 and len(self.entries) <= limit

    def filled(self):
        if self.form == "inode":
            return len(self.entries) >= (50 if self.level == 0 else 64)
        return self.size >= 3277


def ordered(rows):
    if any(a[0] >= b[0] for a, b in zip(rows, rows[1:])):
        raise ValueError("sorted key order")


def partition(form, level, entries):
    """Append until overflow; nearest byte half, ties choose smaller prefix."""
    output, pending = [], []
    for entry in entries:
        pending.append(entry)
        node = SortedNode(form, level, tuple(pending))
        if not node.fits():
            widths = node.widths()
            total = sum(widths)
            half = min(range(1, len(widths)), key=lambda i: (abs(total - 2 * sum(widths[:i])), i))
            output.append(SortedNode(form, level, tuple(pending[:half])))
            pending = pending[half:]
    if pending:
        output.append(SortedNode(form, level, tuple(pending)))
    return output


def sibling_pages(nodes):
    """Exact last-sibling redistribution; a merge includes child fill work."""
    output, pending = [], None
    for next_node in nodes:
        if pending is None:
            pending = next_node
        elif pending.filled() and next_node.filled():
            output.append(pending)
            pending = next_node
        else:
            entries = pending.entries + next_node.entries
            if pending.level > 0:
                entries = sibling_pages(entries)
            merged = partition(pending.form, pending.level, entries)
            output.extend(merged[:-1])
            pending = merged[-1] if merged else None
    return output + ([pending] if pending is not None else [])


def sorted_root(form, rows):
    ordered(rows)
    if not rows:
        if form == "inode":
            raise ValueError("empty inode table")
        return SortedNode(form, 0, ())
    nodes = partition(form, 0, rows)
    while len(nodes) > 1:
        nodes = partition(form, nodes[0].level + 1, nodes)
    return nodes[0]


def sorted_update(root, changes):
    ordered(changes)
    if not changes:
        return root
    pending = list(changes)

    def edit(node, bound):
        if not pending or (bound is not None and pending[0][0] > bound):
            return [node]
        if node.level == 0:
            values = {x[0]: x for x in node.entries}
            while pending and (bound is None or pending[0][0] <= bound):
                key, value = pending.pop(0)
                if value is None:
                    values.pop(key, None)
                else:
                    values[key] = value
            return partition(node.form, 0, sorted(values.values()))
        children = []
        for i, child in enumerate(node.entries):
            children.extend(edit(child, bound if i + 1 == len(node.entries) else child.upper))
        return partition(node.form, node.level, sibling_pages(children))

    nodes = edit(root, None)
    if not nodes:
        return SortedNode(root.form, 0, ())
    while len(nodes) > 1:
        nodes = partition(root.form, nodes[0].level + 1, nodes)
    result = nodes[0]
    while result.level > 0 and len(result.entries) == 1:
        result = result.entries[0]
    return result


class Reference:
    def __init__(self, hashes):
        self.hashes = hashes
        self.objects = {}
        c = SPEC["cdc"]
        self.gear = [int(x, 16) for x in c["gear_hex"]]
        descriptor = (b"layerfs/fastcdc-profile/v1\0two-byte-rolling-gear-v1"
                      + u32(c["minimum"]) + u32(c["target"]) + u32(c["maximum"])
                      + bytes([c["normalization_shift"]]) + u64(c["seed"])
                      + b"".join(u64(c[k]) for k in ("small_mask", "large_mask", "shifted_small_mask", "shifted_large_mask"))
                      + b"".join(u64(x) for x in self.gear))
        self.cdc_profile = hashes.raw(descriptor)
        mapping = (b"layerfs/mapping-profile/bplus-extent/v3\0" + u16(3)
                   + bytes([8, 9, 10, 0]) + u16(64) + u16(128) + u16(0)
                   + u16(2) + bytes([31]) + u32(32768)
                   + bytes([4, 4, 8, 1, 1, 1, 1, 1]) + self.cdc_profile)
        self.mapping_profile = hashes.raw(mapping)
        self.edit_policy = hashes.object(bytes.fromhex(SPEC["edit_policy_description_hex"]))
        self.namespace_v1 = hashes.object(bytes.fromhex(SPEC["namespace_v1_description_hex"]))
        self.namespace_v2 = hashes.object(bytes.fromhex(SPEC["namespace_v2_description_prefix_hex"]) + self.edit_policy)

    def save(self, raw, role):
        unwrap(raw)
        oid = self.hashes.object(raw)
        prior = self.objects.setdefault(oid, (raw, role))
        if prior != (raw, role):
            raise ValueError("identity/role collision")
        return oid

    def cdc(self, raw):
        """Mathematical two-byte scanner; reset at each emitted replacement chunk."""
        c, result, pos = SPEC["cdc"], [], 0
        while pos < len(raw):
            start = pos
            pos = min(pos + c["minimum"], len(raw))
            h = c["seed"]
            cut = False
            while pos + 1 < len(raw) and pos - start < c["maximum"]:
                small = pos - start < c["target"]
                h = ((h << 2) + (self.gear[raw[pos]] << 1)) & U64
                if h & c["shifted_small_mask" if small else "shifted_large_mask"] == 0:
                    cut = True
                    break
                h = (h + self.gear[raw[pos + 1]]) & U64
                if h & c["small_mask" if small else "large_mask"] == 0:
                    pos += 1
                    cut = True
                    break
                pos += 2
            if pos == start:
                raise ValueError("CDC failed to advance")
            if not cut and pos + 1 == len(raw) and pos - start < c["maximum"]:
                pos += 1
            result.append(raw[start:pos])
        return result

    def chunk_tree(self, raw):
        """Fresh builder: stream flush128 at >192, then exact finish half split."""
        levels = [[]]

        def push(level, item):
            while len(levels) <= level:
                levels.append([])
            levels[level].append(item)
            if len(levels[level]) > 192:
                prefix, levels[level] = levels[level][:128], levels[level][128:]
                push(level + 1, MapNode(level, tuple(prefix)))

        for chunk in self.cdc(raw):
            oid = self.save(envelope(b"LFS4CHK\0" + chunk), 2)
            push(0, (oid, 0, len(chunk)))
        if not raw:
            return MapNode(0, ())
        level = 0
        while True:
            items = levels[level]
            higher = any(levels[level + 1:])
            if not higher and len(items) <= 128:
                return items[0] if level and len(items) == 1 else MapNode(level, tuple(items))
            if items:
                size = len(items) // 2 if len(items) > 128 else len(items)
                prefix, levels[level] = items[:size], items[size:]
                push(level + 1, MapNode(level, tuple(prefix)))
            else:
                level += 1

    def encode_map(self, node, root=True):
        if node.level > 31 or len(node.entries) > 128 or (not root and len(node.entries) < 64):
            raise ValueError("mapping fill/depth")
        if node.level and len(node.entries) < 2:
            raise ValueError("mapping root collapse")
        header = (b"LFS4MAP\0" + u16(3) + bytes([8 if node.level == 0 else 9, node.level, 0])
                  + u16(len(node.entries)) + u64(node.length) + u64(node.count))
        if node.level == 0:
            rows = b"".join(oid + u32(offset) + u32(size) for oid, offset, size in node.entries)
            if coalesce(node.entries) != list(node.entries):
                raise ValueError("uncoalesced mapping")
        else:
            rows, size, count = bytearray(), 0, 0
            for child in node.entries:
                size, count = size + child.length, count + child.count
                rows.extend(u64(size) + u64(count) + self.encode_map(child, False))
        return self.save(envelope(header + rows), 3 if node.level == 0 else 4)

    def state(self, node, version=2):
        mapping = self.encode_map(node)
        value = ((b"LFS4MAP\0" + u16(3)) if version == 1 else (b"LFS7FST\0" + u16(2)))
        value += bytes([10, 0]) + u64(node.length) + u64(node.count) + bytes([node.level])
        value += self.mapping_profile
        if version == 2:
            value += self.edit_policy
        value += mapping
        return self.save(envelope(value), 5)

    def complete(self, raw, version=2):
        if 0 < len(raw) < 131072:
            return self.save(envelope(b"LFS5SML\0" + u16(1) + raw), 1), None
        tree = self.chunk_tree(raw)
        return self.state(tree, version), tree

    def read_map(self, node):
        if node.level:
            return b"".join(self.read_map(x) for x in node.entries)
        result = []
        for oid, offset, size in node.entries:
            payload = unwrap(self.objects[oid][0])[8:]
            if offset + size > len(payload):
                raise ValueError("chunk slice coverage")
            result.append(payload[offset:offset + size])
        return b"".join(result)

    def edited(self, base_root, base_tree, base_bytes, edits, version=2):
        expected = base_bytes
        finalized = 0
        unchanged = True
        for start, end, replacement in edits:
            if not finalized <= start <= end <= len(expected):
                raise ValueError("edit coordinates/monotone replacement")
            unchanged = unchanged and expected[start:end] == replacement
            expected = expected[:start] + replacement + expected[end:]
            if replacement or version == 2:
                finalized = start + len(replacement)
        if unchanged:
            return base_root, base_tree, expected
        if not expected or len(expected) < 131072 or base_tree is None:
            oid, tree = self.complete(expected, version)
            return oid, tree, expected
        tree = base_tree
        for start, end, replacement in edits:
            left, tail = split(tree, start)
            _, right = split(tail, end - start) if tail else (None, None)
            middle = self.chunk_tree(replacement) if replacement else None
            tree = concat(concat(left, middle), right)
        if self.read_map(tree) != expected:
            raise ValueError("independent byte model mismatch")
        return self.state(tree, version), tree, expected

    def encode_sorted(self, node, root=True):
        if node.level > 31 or not node.fits() or (not root and not node.filled()):
            raise ValueError("sorted page fill/depth")
        if node.level and len(node.entries) < 2:
            raise ValueError("sorted root collapse")
        if node.form == "inode" and not node.entries:
            raise ValueError("empty inode page")
        magic, tags, roles = {
            "parent": (b"LFS7PAR\0", (14, 15), (14, 15)),
            "directory": (b"LFS6NSP\0", (1, 2), (7, 8)),
            "inode": (b"LFS6INT\0", (7, 8), (6, 9)),
        }[node.form]
        header = (magic + u16(1) + bytes([tags[bool(node.level)], node.level, 0])
                  + u16(len(node.entries)) + u64(node.count) + u64(node.row_bytes))
        if node.level:
            rows = b"".join((u16(len(child.upper)) + child.upper if node.form == "directory" else u64(child.upper))
                            + self.encode_sorted(child, False) for child in node.entries)
        elif node.form == "parent":
            ordered(node.entries)
            rows = b"".join(u64(serial) + bytes([kind]) + u64(parent) + u16(len(name)) + name
                            for serial, kind, parent, name in node.entries)
            for serial, kind, parent, name in node.entries:
                if not serial or not parent or serial == parent or kind not in (2, 3):
                    raise ValueError("parent row")
                name_bytes(name)
        elif node.form == "directory":
            ordered(node.entries)
            rows = b"".join(u16(len(name_bytes(name))) + name + u64(serial) for name, serial in node.entries)
        else:
            ordered(node.entries)
            rows = b"".join(u64(serial) + bytes([kind]) + u64(count) + content + metadata
                            for serial, kind, count, content, metadata in node.entries)
        return self.save(envelope(header + rows), roles[bool(node.level)])

    def filesystem(self, records, bindings, root=1, version=2):
        """Initial full certification from an independent final namespace ledger."""
        if records[root][0] != 2:
            raise ValueError("root kind")
        counts = {serial: 0 for serial in records}
        parents = {}
        directories = {serial: [] for serial, value in records.items() if value[0] == 2}
        for parent, name, target in sorted(bindings, key=lambda x: (x[0], name_bytes(x[1]))):
            name = name_bytes(name)
            if parent not in directories or target not in records or target == root:
                raise ValueError("namespace binding identity")
            directories[parent].append((name, target))
            counts[target] += 1
            if records[target][0] != 1:
                if target in parents:
                    raise ValueError("non-file alias")
                parents[target] = (target, records[target][0], parent, name)
        for serial, (kind, _, _) in records.items():
            if serial == root:
                if counts[serial] != 0:
                    raise ValueError("root count")
            elif counts[serial] == 0 or (kind != 1 and counts[serial] != 1):
                raise ValueError("namespace reference count")
            if kind != 1 and serial != root:
                seen, at = set(), serial
                while at != root:
                    if at in seen or at not in parents:
                        raise ValueError("cycle/unreachable namespace")
                    seen.add(at)
                    at = parents[at][2]
        directory_roots = {serial: self.encode_sorted(sorted_root("directory", rows))
                           for serial, rows in directories.items()}
        inode_rows = [(serial, kind, counts[serial], directory_roots[serial] if kind == 2 else content, metadata)
                      for serial, (kind, content, metadata) in sorted(records.items())]
        inode_root = self.encode_sorted(sorted_root("inode", inode_rows))
        parent_root = self.encode_sorted(sorted_root("parent", sorted(parents.values())))
        scope = self.hashes.object(b"layerfs/inode-scope/v1\0" + bytes(range(32)))
        profile = self.namespace_v1 if version == 1 else self.namespace_v2
        value = b"LFS6FSR\0" + u16(version) + bytes([6, 0]) + profile + scope + u64(root) + inode_root
        if version == 2:
            value += parent_root
        fs = self.save(envelope(value), 10)
        return {"root": fs.hex(), "inode_root": inode_root.hex(), "parent_root": parent_root.hex(),
                "scope": scope.hex(), "profile": profile.hex(), "inodes": len(records), "bindings": len(bindings)}


def object_manifest(ref):
    return [{"id": oid.hex(), "persisted_role": role, "canonical_bytes": len(raw),
             "sha256": hashlib.sha256(raw).hexdigest(),
             **({"canonical_hex": raw.hex()} if len(raw) <= 512 else {})}
            for oid, (raw, role) in sorted(ref.objects.items())]
