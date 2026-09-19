#!/usr/bin/env python3
"""Diagnostic pack-directory decoder for layerfs Stores (v0.1.6 and v0.1.7).

Read-only. Joins the pack directory to the objects table and reports, per lane,
the object count, canonical bytes and the bytes the lane's group bodies occupy
in the pack BLOBs. NOT admission evidence; diagnostic only.

Grammars (core/crates/layerfs-storage/src/pack/layout.rs for v0.1.7 and
crates/layerfs-layerstack-store/src/objects/pack.rs for the v0.1.6 reference):

  header  = MAGIC[8] "LFPACK\\0\\0" + version u32 LE + group_count u32 LE   (16 B)
  v1 / v2 / v5 / v6 / v7 directory entry = 16 B
      start u32 LE, encoded u32 LE, decoded u32 LE, codec u8 + 3 pad
  v4 directory entry = 4 B (start u32 LE); end = next start, last end = BLOB len

  lanes
    v0.1.7: 1 Ordinary, 2 Native, 4 WholeFile, 6 PooledMetadata, 7 Singleton
    v0.1.6: 1 Legacy/ordinary, 2 Native, 3 Small, 4 CompactSmall,
            5 Metadata, 6 PooledMetadata

  v4 body = record[0] tag + record[9..]  (two u32 length fields dropped at
  assembly).  tag 0 = FULL, tag 1/2 = PREFIX/DELTA carrying a 32-byte base
  identity at record[1..33].

Attribution: the v4 lanes store exactly one record per group, so a group body
belongs to exactly one object.  The v1/v2/v6 lanes pack many records into one
group, so a group body cannot be split per object without decoding the record
directory; those lanes are reported as a lane-level aggregate and their
canonical bytes are summed per object.  Summed over lanes, 'stored' equals
SUM(length(object_packs.data)) exactly.
"""
import sqlite3
import struct
import sys

MAGIC = b"LFPACK\x00\x00"
LANE_017 = {1: "Ordinary", 2: "Native", 4: "WholeFile", 6: "PooledMetadata", 7: "Singleton"}
LANE_016 = {1: "Ordinary", 2: "Native", 3: "Small", 4: "CompactSmall", 5: "Metadata", 6: "PooledMetadata"}


def decode(pack_id, blob):
    if blob[:8] != MAGIC or len(blob) < 16:
        raise ValueError("pack %d: bad magic" % pack_id)
    version, count = struct.unpack_from("<II", blob, 8)
    if not 1 <= count <= 256:
        raise ValueError("pack %d: group count %d" % (pack_id, count))
    out = []
    if version == 4:
        dir_end = 16 + 4 * count
        if len(blob) < dir_end:
            raise ValueError("pack %d: short v4 directory" % pack_id)
        for i in range(count):
            start = struct.unpack_from("<I", blob, 16 + 4 * i)[0]
            end = len(blob) if i + 1 == count else struct.unpack_from("<I", blob, 16 + 4 * (i + 1))[0]
            if not dir_end <= start < end <= len(blob):
                raise ValueError("pack %d: v4 group %d extent" % (pack_id, i))
            out.append((start, end, end - start, 0))
    else:
        dir_end = 16 + 16 * count
        if len(blob) < dir_end:
            raise ValueError("pack %d: short directory" % pack_id)
        for i in range(count):
            start, encoded, decoded = struct.unpack_from("<III", blob, 16 + 16 * i)
            codec = blob[16 + 16 * i + 12]
            if not dir_end <= start or start + encoded > len(blob):
                raise ValueError("pack %d: group %d extent" % (pack_id, i))
            out.append((start, start + encoded, decoded, codec))
    return version, count, out


def bucket(path, lane_names, has_role):
    conn = sqlite3.connect("file:%s?mode=ro" % path, uri=True)
    groups = {}
    pack_bytes = 0
    pack_count = 0
    for pack_id, blob in conn.execute("SELECT pack_id, data FROM object_packs"):
        version, count, parts = decode(pack_id, blob)
        pack_bytes += len(blob)
        pack_count += 1
        for g, (start, end, decoded, codec) in enumerate(parts):
            groups[(pack_id, g)] = (version, start, end, decoded, codec, blob)
    rows = conn.execute(
        "SELECT object_id, canonical_length, pack_id, group_number, record_number%s%s FROM objects"
        % (", object_role" if has_role else "", ", base_object_id" if has_role else "")
    )
    agg = {}
    group_seen = set()
    for row in rows:
        oid, canon, pack_id, group = row[0], row[1], row[2], row[3]
        base = row[6] if has_role else None
        key = (pack_id, group)
        if key not in groups:
            raise ValueError("object %s: no group %s" % (oid.hex(), key))
        version, start, end, decoded, codec, blob = groups[key]
        lane = lane_names.get(version, "v%d" % version)
        if version == 4:
            cls = "with-base" if blob[start] != 0 else "without-base"
        else:
            cls = "n/a"
        a = agg.setdefault((lane, cls), {"objects": 0, "canonical": 0, "stored": 0, "groups": set()})
        a["objects"] += 1
        a["canonical"] += canon
        a["groups"].add(key)
    # The pooled-metadata lane's groups are named by metadata_value_groups, not
    # by objects: a pooled leaf value group holds many metadata values and is
    # referenced once per ordinal range. Attribute those group bodies too, or the
    # lane total silently omits them.
    pool = agg.setdefault(("PooledMetadata", "n/a"), {"objects": 0, "canonical": 0, "stored": 0, "groups": set()})
    for pack_id, group in conn.execute("SELECT pack_id, group_number FROM metadata_value_groups"):
        pool["groups"].add((pack_id, group))
    for (lane, cls), a in agg.items():
        a["stored"] = sum(groups[k][2] - groups[k][1] for k in a["groups"])
    total = {
        "objects": sum(a["objects"] for a in agg.values()),
        "canonical": sum(a["canonical"] for a in agg.values()),
        "stored": sum(a["stored"] for a in agg.values()),
    }
    attributed = set()
    for a in agg.values():
        attributed |= a["groups"]
    if attributed != set(groups):
        raise ValueError("unattributed groups: %d of %d" % (len(set(groups) - attributed), len(groups)))
    return agg, total, pack_bytes, pack_count


def report(label, path, lane_names, has_role):
    agg, total, pack_bytes, pack_count = bucket(path, lane_names, has_role)
    print("### %s" % label)
    print("    store %s" % path)
    print("    packs %d   SUM(length(object_packs.data)) = %d   attributed = %d   residual = %d"
          % (pack_count, pack_bytes, total["stored"], pack_bytes - total["stored"]))
    print("    %-15s %-13s %8s %14s %14s %8s" % ("lane", "base", "objects", "canonical", "stored", "ratio"))
    for k in sorted(agg):
        a = agg[k]
        ratio = a["canonical"] / a["stored"] if a["stored"] else float("nan")
        print("    %-15s %-13s %8d %14d %14d %7.2fx" % (k[0], k[1], a["objects"], a["canonical"], a["stored"], ratio))
    ratio = total["canonical"] / total["stored"] if total["stored"] else float("nan")
    print("    %-15s %-13s %8d %14d %14d %7.2fx" % ("TOTAL", "", total["objects"], total["canonical"], total["stored"], ratio))
    return agg, total, pack_bytes, pack_count


if __name__ == "__main__":
    for arg in sys.argv[1:]:
        gen = "0.1.7" if "base187" in arg else "0.1.6"
        names = LANE_017 if gen == "0.1.7" else LANE_016
        report("%s  %s" % (gen, arg), arg, names, gen == "0.1.7")
