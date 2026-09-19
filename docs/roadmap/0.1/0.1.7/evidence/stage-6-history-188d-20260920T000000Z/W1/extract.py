#!/usr/bin/env python3
"""W1 extractor: whole-file lane records and ordinary/pooled group bodies.

Read-only. The pack *directory* is decoded by shared/space.py (the fail-closed
instrument); this script only joins it to the objects table and slices the
bodies it already located. The group directory's codec byte is re-read with
struct because space.py parses it and discards it.

  records.bin : u32 count; per record 32B id, u32 canonical, 32B base(0=none),
                u32 frame_len, frame
  groups.bin  : u32 count; per group u32 len, decoded body
"""
from __future__ import annotations

import sqlite3
import struct
import sys
from pathlib import Path

from compression import zstd

HERE = Path(__file__).resolve()
ROOT = HERE
while not (ROOT / "core" / "benchmark").is_dir():
    ROOT = ROOT.parent
    if ROOT == ROOT.parent:
        raise SystemExit("repo root not found")
sys.path.insert(0, str(ROOT / "core" / "benchmark" / "fs-bench-pro-storage-content" / "shared"))

import space  # noqa: E402

# The base is read from the record itself, never from a column: the current tree
# no longer has `objects.base_object_id` at all (it moved into the record), and the
# record is the authority in either schema.
WHOLE_FILE_LOCATORS = (
    "SELECT object_id, pack_id, group_number, canonical_length "
    "FROM objects WHERE object_role = 1"
)
ZERO = bytes(32)


def blobs(path):
    return space._pack_blobs(path)


def main() -> int:
    store = Path(sys.argv[1])
    out = Path(sys.argv[2])
    out.mkdir(parents=True, exist_ok=True)

    packs = blobs(store)
    wf = {}
    bodies = []
    codec_hist = {}
    for pack_id, blob in packs:
        lane, ranges = space._pack_group_ranges(blob, f"pack {pack_id}")
        if lane == "whole-file":
            for i, (start, length) in enumerate(ranges):
                wf[(pack_id, i)] = blob[start : start + length]
        elif lane in ("ordinary", "pooled-metadata"):
            version, count = struct.unpack_from("<II", blob, 8)
            for i, (start, length) in enumerate(ranges):
                entry = blob[space.PACK_HEADER_LEN + space.PACK_DIRECTORY_ENTRY_LEN * i :][:16]
                decoded = struct.unpack_from("<I", entry, 8)[0]
                codec = entry[12]
                codec_hist[(lane, codec)] = codec_hist.get((lane, codec), 0) + 1
                body = blob[start : start + length]
                if codec == 1:
                    body = zstd.ZstdDecompressor().decompress(body, decoded)
                bodies.append((lane, body))

    con = sqlite3.connect(f"file:{store}?mode=ro", uri=True)
    rows = list(con.execute(WHOLE_FILE_LOCATORS))
    con.close()

    recs = []
    for object_id, pack_id, group_number, canonical in rows:
        rec = wf[(int(pack_id), int(group_number))]
        tag = rec[0]
        if tag == 0:
            rbase = ZERO
            frame = rec[1:]
        elif tag == 1:
            rbase = rec[1:33]
            frame = rec[33:]
        else:
            raise SystemExit(f"record tag {tag}")
        recs.append((bytes(object_id), int(canonical), rbase, frame))

    with (out / "records.bin").open("wb") as fh:
        fh.write(struct.pack("<I", len(recs)))
        for oid, canonical, base, frame in recs:
            fh.write(oid)
            fh.write(struct.pack("<I", canonical))
            fh.write(base)
            fh.write(struct.pack("<I", len(frame)))
            fh.write(frame)

    for lane in ("ordinary", "pooled-metadata"):
        picked = [b for l, b in bodies if l == lane]
        with (out / f"groups_{lane}.bin").open("wb") as fh:
            fh.write(struct.pack("<I", len(picked)))
            for body in picked:
                fh.write(struct.pack("<I", len(body)))
                fh.write(body)
        print(f"{lane} bodies {len(picked)} decoded {sum(len(b) for b in picked)}")

    print(f"store {store}")
    print(f"whole-file objects {len(recs)}  records {len(wf)}")
    print(f"  tag0 {sum(1 for r in recs if r[2] == ZERO)}  tag1 {sum(1 for r in recs if r[2] != ZERO)}")
    print(f"  stored bodies {sum(len(r[3]) for r in recs)}")
    print(f"  canonical {sum(r[1] for r in recs)}")

    print(f"codec byte histogram {sorted(codec_hist.items())}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
