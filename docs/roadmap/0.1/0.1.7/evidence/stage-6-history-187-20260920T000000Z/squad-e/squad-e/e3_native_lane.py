#!/usr/bin/env python3
"""E3: decode the Native (chunk) lane of a retained Store, record by record.

Container decoding (magic, framing version, directory, group ranges) is
shared/space.py (pack_directory / _pack_group_ranges) and is NOT re-implemented
here.  What this file adds is the grammar *inside* a Native group body, which
space.py does not parse:

    group body = [count u32 LE][end u32 LE x count][record x count]
                 end[i] is the END of record i relative to the record region,
                 which begins at 4 + 4*count.
    record     = [tag u8][raw_len u32 LE][base 32 bytes if tag == 1][frame]

tag semantics:
    v0.1.6  crates/layerfs-layerstack-store/src/objects/delta.rs:96
            0 = FULL, 1|2 = DELTA
    v0.1.7  core/crates/layerfs-storage/src/encoding/delta/record.rs:15-17
            0 = FULL, 1 = PREFIX

Fail-closed: a group whose record directory disagrees with its body length, or
whose record count disagrees with the objects table, is refused rather than
rounded.

    python3 e3_native_lane.py <store.sqlite> [<store.sqlite> ...]
"""
from __future__ import annotations

import json
import sqlite3
import struct
import sys
from pathlib import Path

HARNESS = Path("/Users/yifanxu/Ephemeral-AI-Lab/layerfs/core/benchmark/fs-bench-pro-storage-content")
sys.path.insert(0, str(HARNESS))
from shared import space  # noqa: E402


class Refused(Exception):
    """The reading is not available; never a rounded number."""


def native_group_records(body: bytes, where: str) -> list[dict[str, int]]:
    """Every record of one Native group body, or a refusal."""
    if len(body) < 4:
        raise Refused(f"{where}: group body is shorter than its record count")
    count = struct.unpack_from("<I", body, 0)[0]
    base = 4 + 4 * count
    if count == 0 or base > len(body):
        raise Refused(f"{where}: record directory claims {count} records past the body")
    ends = struct.unpack_from(f"<{count}I", body, 4)
    records = []
    previous = 0
    for index in range(count):
        end = ends[index]
        if end < previous or base + end > len(body):
            raise Refused(f"{where}: record {index} runs outside the body")
        record = body[base + previous : base + end]
        tag = record[0]
        if tag > 2:
            raise Refused(f"{where}: record {index} tag {tag} is not 0/1/2")
        raw_len = struct.unpack_from("<I", record, 1)[0]
        head = 5 + (32 if tag == 1 else 0)
        if head > len(record):
            raise Refused(f"{where}: record {index} is shorter than its header")
        frame = record[head:]
        if frame[:4] != b"\x28\xb5\x2f\xfd":
            raise Refused(f"{where}: record {index} frame has no zstd magic")
        entry = {"tag": tag, "raw_len": raw_len, "stored": len(record), "base": None}
        if tag == 1:
            entry["base"] = record[5:37]
        records.append(entry)
        previous = end
    if base + previous != len(body):
        raise Refused(f"{where}: records do not sum to the group body")
    return records


def native_lane(path: Path) -> dict[str, object]:
    """Per-record tag/raw/stored facts for the Native lane of one Store."""
    connection = sqlite3.connect(f"file:{path}?mode=ro", uri=True)
    try:
        columns = [d[1] for d in connection.execute("PRAGMA table_info(objects)")]
        rows = list(connection.execute(
            "SELECT pack_id, group_number, COUNT(*) FROM objects GROUP BY pack_id, group_number"
        ))
        per_group = {(int(p), int(g)): int(n) for p, g, n in rows}
        # object_id -> (pack_id, group_number): used to name the ROLE of a native
        # prefix record's base without a role column (v0.1.6 has none).
        location = {
            bytes(oid): (int(pack), int(group))
            for oid, pack, group in connection.execute(
                "SELECT object_id, pack_id, group_number FROM objects"
            )
        }
    finally:
        connection.close()
    lane_of_group: dict[tuple[int, int], str] = {}
    for pack_id, blob in space._pack_blobs(path):
        lane, ranges = space._pack_group_ranges(blob, f"pack {pack_id}")
        for index in range(len(ranges)):
            lane_of_group[(pack_id, index)] = lane
    records: list[dict[str, int]] = []
    bases: dict[str, int] = {}
    groups = 0
    group_framing = 0
    checked = 0
    for pack_id, blob in space._pack_blobs(path):
        lane, ranges = space._pack_group_ranges(blob, f"pack {pack_id}")
        if lane != "native":
            continue
        for index, (start, length) in enumerate(ranges):
            groups += 1
            body = blob[start : start + length]
            parsed = native_group_records(body, f"pack {pack_id} group {index}")
            expected = per_group.get((pack_id, index))
            if expected is not None:
                checked += 1
                if expected != len(parsed):
                    raise Refused(
                        f"pack {pack_id} group {index}: {len(parsed)} records in the body, "
                        f"{expected} rows in the objects table"
                    )
            group_framing += 4 + 4 * len(parsed)
            records.extend(parsed)
    by_tag: dict[str, dict[str, int]] = {}
    base_roles: dict[str, int] = {}
    for record in records:
        entry = by_tag.setdefault(str(record["tag"]), {"records": 0, "raw": 0, "stored": 0})
        entry["records"] += 1
        entry["raw"] += record["raw_len"]
        entry["stored"] += record["stored"]
        if record["base"] is not None:
            key = location.get(record["base"])
            role = lane_of_group.get(key, "absent") if key else "absent"
            base_roles[role] = base_roles.get(role, 0) + 1
    return {
        "objects_columns": columns,
        "native_groups": groups,
        "groups_cross_checked_against_objects": checked,
        "native_records": len(records),
        "by_tag": by_tag,
        "record_bytes": sum(r["stored"] for r in records),
        "raw_bytes": sum(r["raw_len"] for r in records),
        "group_framing_bytes": group_framing,
        "prefix_base_lane": base_roles,
    }


def main() -> int:
    for argument in sys.argv[1:]:
        path = Path(argument)
        directory = space.pack_directory(path)
        print(f"=== {path} ===")
        print("apparent      :", space.stat_space(path).apparent_bytes)
        print("allocated     :", space.stat_space(path).allocated_bytes)
        print("sqlite        :", json.dumps(space.sqlite_space(path).as_fields(), sort_keys=True))
        print("pack bodies   :", space.pack_bodies(path))
        print("pack directory:", json.dumps(directory.as_fields(), sort_keys=True))
        try:
            print("canonical role:", json.dumps(space.canonical_by_role(path), sort_keys=True))
        except space.Incomplete as error:
            print("canonical role: INCOMPLETE", error)
        except sqlite3.OperationalError as error:
            print("canonical role: INCOMPLETE", error)
        try:
            records = space.whole_file_records(path)
            print("whole-file records:", len(records), "stored", sum(records.values()))
        except (space.Incomplete, sqlite3.OperationalError) as error:
            print("whole-file records: INCOMPLETE", error)
        print("native lane   :", json.dumps(native_lane(path), sort_keys=True))
    return 0


if __name__ == "__main__":
    sys.exit(main())