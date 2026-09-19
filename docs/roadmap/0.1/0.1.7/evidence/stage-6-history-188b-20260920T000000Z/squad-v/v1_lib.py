#!/usr/bin/env python3
"""V1 forensic: per-object comparison of the whole-file lane, v0.1.6 vs the T1 lane.

Read-only. Diagnostic only -- not admission evidence.

Grammar
  pack header = MAGIC[8] "LFPACK\0\0" + version u32 LE + group_count u32 LE
  v4 directory entry = 4 B (start u32 LE); end = next start, last end = BLOB len.
  v4 body (single record per group):
    v0.1.6 CompactSmall : tag u8 [+ base ObjectId 32 B if tag in {1,2}] + zstd frame
    C1      WholeFile   : tag u8 [+ base ObjectId 32 B if tag == 1] + zstd frame
  (C1 drops the two u32 length fields at assembly: WHOLE_FILE_COMPACT_DROP = 8.)
"""
import collections
import sqlite3
import struct
import sys

MAGIC = b"LFPACK\x00\x00"
LANE_017 = {1: "Ordinary", 2: "Native", 4: "WholeFile", 6: "PooledMetadata", 7: "Singleton"}
LANE_016 = {1: "Ordinary", 2: "Native", 3: "Small", 4: "CompactSmall", 5: "Metadata",
            6: "PooledMetadata"}


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


def zstd_header(frame):
    """Self-describing zstd frame header. None when the magic is absent."""
    if len(frame) < 6 or frame[:4] != b"\x28\xb5\x2f\xfd":
        return None
    fhd = frame[4]
    fcs_flag = fhd >> 6
    single = (fhd >> 5) & 1
    checksum = (fhd >> 2) & 1
    did_flag = fhd & 3
    pos = 5
    window_log = None
    window_size = None
    if not single:
        wd = frame[pos]
        pos += 1
        exponent = wd >> 3
        mantissa = wd & 7
        window_log = 10 + exponent
        base = 1 << window_log
        window_size = base + (base // 8) * mantissa
    did_size = (0, 1, 2, 4)[did_flag]
    did = int.from_bytes(frame[pos:pos + did_size], "little") if did_size else None
    pos += did_size
    fcs_size = (1 if single else 0, 2, 4, 8)[fcs_flag]
    fcs = None
    if fcs_size:
        raw = frame[pos:pos + fcs_size]
        if len(raw) == fcs_size:
            fcs = int.from_bytes(raw, "little")
            if fcs_size == 2:
                fcs += 256
    pos += fcs_size
    return {"fcs_flag": fcs_flag, "single_segment": single, "checksum": checksum,
            "did_flag": did_flag, "dict_id": did, "window_log": window_log,
            "window_size": window_size, "fcs": fcs, "header_len": pos}


class Store:
    """One decoded Store: per-object lane records plus the pack directory."""

    def __init__(self, path, generation):
        self.path = path
        self.generation = generation
        self.lanes = LANE_017 if generation == "c1" else LANE_016
        con = sqlite3.connect("file:%s?mode=ro&immutable=1" % path, uri=True)
        cols = [r[1] for r in con.execute("pragma table_info(objects)")]
        self.has_role = "object_role" in cols
        self.has_base = "base_object_id" in cols
        blobs = {}
        groups = {}
        pack_bytes = 0
        hdr_dir = 0
        versions = collections.Counter()
        q = "select pack_id, data from object_packs"
        for pid, blob in con.execute(q):
            version, count, parts, hd = decode_pack(pid, blob)
            blobs[pid] = blob
            pack_bytes += len(blob)
            hdr_dir += hd
            versions[version] += 1
            for g, (start, end) in enumerate(parts):
                groups[(pid, g)] = (version, start, end)
        self.blobs = blobs
        self.groups = groups
        self.pack_bytes = pack_bytes
        self.hdr_dir = hdr_dir
        self.versions = versions
        self.objects = {}
        self.rows = 0
        q = ("select object_id, canonical_length, pack_id, group_number"
             + (", object_role" if self.has_role else "")
             + (", base_object_id" if self.has_base else "") + " from objects")
        for row in con.execute(q):
            oid, canon, pid, g = row[0], row[1], row[2], row[3]
            role = row[4] if self.has_role else None
            base = row[5] if self.has_base else None
            version, start, end = groups[(pid, g)]
            rec = {"oid": oid, "canonical_length": canon, "pack_id": pid, "group": g,
                   "lane_version": version, "lane": self.lanes.get(version, "v%d" % version),
                   "role": role, "column_base": base, "body_start": start, "body_end": end,
                   "body_bytes": end - start}
            if version == 4:
                blob = blobs[pid]
                tag = blob[start]
                if tag == 0:
                    rec["tag"] = 0
                    rec["record_base"] = None
                    frame_off = start + 1
                elif tag in (1, 2):
                    rec["tag"] = tag
                    rec["record_base"] = blob[start + 1:start + 33]
                    frame_off = start + 33
                else:
                    raise ValueError("pack %d group %d: bad tag %d" % (pid, g, tag))
                rec["frame_off"] = frame_off
                rec["frame_len"] = end - frame_off
                rec["frame"] = blob[frame_off:end]
            self.objects[oid] = rec
            self.rows += 1
        self.con = con

    def lane4(self):
        return {o: r for o, r in self.objects.items() if r["lane_version"] == 4}

    def lane_stats(self):
        agg = collections.Counter()
        canon = collections.Counter()
        for r in self.objects.values():
            agg[r["lane"]] += 1
            canon[r["lane"]] += r["canonical_length"]
        return agg, canon


def totals(store):
    """Lane totals by body bytes, with the pooled lane charged through its catalogue."""
    by_lane = collections.Counter()
    seen_groups = set()
    for r in store.objects.values():
        key = (r["pack_id"], r["group"])
        if key in seen_groups:
            continue
        seen_groups.add(key)
        by_lane[r["lane"]] += r["body_bytes"]
    for pid, g in store.con.execute("select pack_id, group_number from metadata_value_groups"):
        key = (pid, g)
        if key in seen_groups:
            continue
        seen_groups.add(key)
        version, start, end = store.groups[key]
        by_lane[store.lanes.get(version, "v%d" % version)] += end - start
    return by_lane, seen_groups
