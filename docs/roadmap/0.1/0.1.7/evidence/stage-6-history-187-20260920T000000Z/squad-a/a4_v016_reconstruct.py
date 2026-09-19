#!/usr/bin/env python3
"""Squad A4: decode retained v0.1.6 (and v0.1.7/C1) Stores to per-bucket tables.

Read-only. Diagnostic only -- not admission evidence.

Grammar (crates/layerfs-layerstack-store/src/objects/pack.rs, v0.1.6):
  header  = MAGIC[8] "LFPACK\0\0" + version u32 LE + group_count u32 LE   (16 B)
  v4 (CompactSmall) directory entry = 4 B (start u32 LE); end = next start,
     last end = BLOB length.  One record per group.
     record = tag u8 [+ base ObjectId 32 B if tag in {1,2}] + zstd frame
       tag 0 = full (no base); tag 1 = delta via small_anchor; tag 2 = delta via
       small_predecessor (chain format).  Canonical length = raw length + 23.
  v1/v2/v3/v5/v6 directory entry = 16 B: start, encoded, decoded, codec u8 + pad
  lanes: v0.1.6 {1 Ordinary, 2 Native, 3 Small, 4 CompactSmall, 5 Metadata,
                 6 PooledMetadata};  v0.1.7/C1 {1 Ordinary, 2 Native,
                 4 WholeFile, 6 PooledMetadata, 7 Singleton}

Attribution rules used here:
  * v4 lanes hold exactly one record per group, so a group body belongs to
    exactly one object and can be charged to that object's bucket.
  * v1/v2/v6 groups hold many records; a group body is charged ONCE to its lane
    aggregate, never once per object (no naive join).
  * 'other lanes' = other-lane group bodies + every pack header + every pack
    directory entry.  This is the residual of SUM(length(object_packs.data)).
"""
import collections
import os
import sqlite3
import struct
import sys

MAGIC = b"LFPACK\x00\x00"


def decode_pack(pid, blob):
    if blob[:8] != MAGIC or len(blob) < 16:
        raise ValueError("pack %d: bad magic" % pid)
    version, count = struct.unpack_from("<II", blob, 8)
    if not 1 <= count <= 256:
        raise ValueError("pack %d: group count %d" % (pid, count))
    parts = []
    if version == 4:
        dir_end = 16 + 4 * count
        for i in range(count):
            start = struct.unpack_from("<I", blob, 16 + 4 * i)[0]
            end = len(blob) if i + 1 == count else struct.unpack_from("<I", blob, 16 + 4 * (i + 1))[0]
            if not dir_end <= start < end <= len(blob):
                raise ValueError("pack %d: v4 group %d extent" % (pid, i))
            parts.append((start, end))
    else:
        dir_end = 16 + 16 * count
        for i in range(count):
            start, encoded, _decoded = struct.unpack_from("<III", blob, 16 + 16 * i)
            if not dir_end <= start or start + encoded > len(blob):
                raise ValueError("pack %d: group %d extent" % (pid, i))
            parts.append((start, start + encoded))
    return version, count, parts, 16 + (4 if version == 4 else 16) * count


def analyse(path, label):
    con = sqlite3.connect("file:%s?mode=ro&immutable=1" % path, uri=True)
    cols = [r[1] for r in con.execute("pragma table_info(objects)")]
    has_base = "base_object_id" in cols
    groups = {}
    blobs = {}
    versions = collections.Counter()
    hdr_dir = 0
    blob_total = 0
    for pid, blob in con.execute("select pack_id, data from object_packs"):
        version, count, parts, hd = decode_pack(pid, blob)
        blobs[pid] = blob
        versions[version] += 1
        blob_total += len(blob)
        hdr_dir += hd
        for g, (start, end) in enumerate(parts):
            groups[(pid, g)] = (version, start, end)
    q = "select object_id, canonical_length, pack_id, group_number%s from objects" % (
        ", base_object_id" if has_base else "")
    lane_obj = collections.Counter()
    lane_canon = collections.Counter()
    bucket_obj = collections.Counter()
    bucket_canon = collections.Counter()
    bucket_stored = collections.Counter()
    lanes_seen = collections.Counter()
    kind_hist = collections.Counter()
    kind_canon = collections.Counter()
    kind_stored = collections.Counter()
    for row in con.execute(q):
        _oid, canon, pid, g = row[0], row[1], row[2], row[3]
        base = row[4] if has_base else None
        version, start, end = groups[(pid, g)]
        lane_obj[version] += 1
        lane_canon[version] += canon
        lanes_seen[(version, pid, g)] = True
        if version == 4:
            tag = blobs[pid][start]
            if has_base:
                key = "with" if base is not None else "without"
            else:
                key = "with" if tag in (1, 2) else "without"
            kind_hist[tag] += 1
            kind_canon[tag] += canon
            kind_stored[tag] += end - start
        else:
            key = "other"
        bucket_obj[key] += 1
        bucket_canon[key] += canon
        if version == 4:
            bucket_stored[key] += end - start
    other_bodies = 0
    for (pid, g) in groups:
        if groups[(pid, g)][0] != 4:
            other_bodies += groups[(pid, g)][2] - groups[(pid, g)][1]
    bucket_stored["other"] = other_bodies + hdr_dir
    size = os.path.getsize(path)
    print("### %s" % label)
    print("  %s  apparent %d B" % (path, size))
    print("  objects %d  canonical %d B" % (sum(bucket_obj.values()), sum(bucket_canon.values())))
    print("  packs %d %s  pack blob %d  headers+directories %d  bodies %d" % (
        sum(versions.values()), dict(sorted(versions.items())), blob_total, hdr_dir, blob_total - hdr_dir))
    print("  lanes: " + " | ".join("v%d %d obj %d B" % (v, lane_obj[v], lane_canon[v]) for v in sorted(lane_obj)))
    for key in ("with", "without", "other"):
        print("  bucket %-8s objs %6d  canonical %10d  stored %9d  ratio %6.3f" % (
            key, bucket_obj[key], bucket_canon[key], bucket_stored[key],
            bucket_canon[key] / bucket_stored[key]))
    print("  TOTAL    objs %6d  canonical %10d  stored %9d  ratio %6.4f  non-pack %d" % (
        sum(bucket_obj.values()), sum(bucket_canon.values()), blob_total,
        sum(bucket_canon.values()) / blob_total, size - blob_total))
    if not has_base:
        print("  v4 record tag histogram (v0.1.6):")
        for t in sorted(kind_hist):
            print("    tag %d: %6d objs  canonical %10d  stored %9d  ratio %6.3f" % (
                t, kind_hist[t], kind_canon[t], kind_stored[t], kind_canon[t] / kind_stored[t]))
    return con, blobs, groups


if __name__ == "__main__":
    for arg in sys.argv[1:]:
        analyse(arg, os.path.basename(os.path.dirname(arg)))
