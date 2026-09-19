#!/usr/bin/env python3
"""W1: dump the native (chunk) lane records in the records.bin shape.

The native grammar is [tag u8][raw_len u32][base 32 if tag=1][frame], one record
per group. The Rust instrument derives raw_length as canonical - 23, so the
native raw length is written back into the canonical field.
"""
from __future__ import annotations

import sqlite3
import struct
import sys
from pathlib import Path

HERE = Path(__file__).resolve()
ROOT = HERE
while not (ROOT / "core" / "benchmark").is_dir():
    ROOT = ROOT.parent
sys.path.insert(0, str(ROOT / "core" / "benchmark" / "fs-bench-pro-storage-content" / "shared"))
import space  # noqa: E402

ZERO = bytes(32)


def main() -> int:
    store = Path(sys.argv[1])
    out = Path(sys.argv[2])
    out.mkdir(parents=True, exist_ok=True)
    packs = space._pack_blobs(store)
    native = {}
    for pack_id, blob in packs:
        lane, ranges = space._pack_group_ranges(blob, f"pack {pack_id}")
        if lane != "native":
            continue
        for i, (start, length) in enumerate(ranges):
            body = blob[start : start + length]
            # The native lane shares the ordinary in-group grammar: a u32 record
            # count, one u32 end offset per record, then the records.
            count = struct.unpack_from("<I", body, 0)[0]
            ends = struct.unpack_from(f"<{count}I", body, 4)
            # The end offsets are cumulative record lengths, relative to the
            # payload start (assemble.rs frame_group), not absolute offsets.
            first = 4 + 4 * count
            previous = 0
            for ordinal, end in enumerate(ends):
                native[(pack_id, i, ordinal)] = body[first + previous : first + end]
                previous = end
            if first + previous != len(body):
                raise SystemExit(f"pack {pack_id} group {i}: trailing bytes")
    con = sqlite3.connect(f"file:{store}?mode=ro", uri=True)
    rows = list(
        con.execute(
            "SELECT object_id, pack_id, group_number, record_number, canonical_length "
            "FROM objects WHERE object_role = 2"
        )
    )
    con.close()
    recs = []
    for oid, pack_id, group_number, record_number, canonical in rows:
        rec = native[(int(pack_id), int(group_number), int(record_number))]
        raw_len = struct.unpack_from("<I", rec, 1)[0]
        if rec[0] == 0:
            base, frame = ZERO, rec[5:]
        elif rec[0] == 1:
            base, frame = rec[5:37], rec[37:]
        else:
            raise SystemExit(f"native tag {rec[0]}")
        recs.append((bytes(oid), raw_len + 23, base, frame))
    with (out / "native.bin").open("wb") as fh:
        fh.write(struct.pack("<I", len(recs)))
        for oid, canonical, base, frame in recs:
            fh.write(oid)
            fh.write(struct.pack("<I", canonical))
            fh.write(base)
            fh.write(struct.pack("<I", len(frame)))
            fh.write(frame)
    print(
        f"native records {len(recs)} tag0 {sum(1 for r in recs if r[2] == ZERO)} "
        f"tag1 {sum(1 for r in recs if r[2] != ZERO)} "
        f"frames {sum(len(r[3]) for r in recs)} "
        f"bodies {sum(len(r[3]) + 1 + 32 * (r[2] != ZERO) for r in recs)}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
