#!/usr/bin/env python3
"""#420 payload study: dump every Chunk / WholeFile record of the payload packs.

Read-only on the fixture (immutable URI). For pack_id in [LO, HI) writes, into OUTDIR:
  meta-LO.jsonl.gz  one row per record: seq key (pack, group, record), oid hex, role (1 WF / 2 Chunk),
                   tag (0 FULL zstd, 1 PREFIX, 2 STORED), base hex, rec (stored record bytes),
                   raw (raw payload length), off (offset of raw payload in raw-LO.bin, -1 for PREFIX),
                   doff/dlen (offset of the PREFIX frame in delta-LO.bin)
  raw-LO.bin       raw payload of FULL and STORED records (FULL decompressed with its checksummed frame)
  delta-LO.bin     PREFIX frames (resolved later by resolve.py)
usage: dump.py DB OUTDIR LO HI
"""
import gzip
import json
import os
import sqlite3
import struct
import sys
import time
from compression import zstd

WF_V = {18, 23}
CHUNK_V = {20, 25}
SIZED = {23, 24, 25}
WF_OVERHEAD = 23


def main():
    db, outdir, lo, hi = sys.argv[1], sys.argv[2], int(sys.argv[3]), int(sys.argv[4])
    t0 = time.time()
    con = sqlite3.connect(f"file:{db}?mode=ro&immutable=1", uri=True)
    loc = {}
    for oid, role, clen, p, g, r in con.execute(
            "select object_id, role, canonical_length, pack_id, group_number, record_number "
            "from object_location where pack_id>=? and pack_id<? and role in (1,2)", (lo, hi)):
        loc[(p, g, r)] = (oid, role, clen)
    rawf = open(os.path.join(outdir, f"raw-{lo}.bin"), "wb")
    delf = open(os.path.join(outdir, f"delta-{lo}.bin"), "wb")
    meta = gzip.open(os.path.join(outdir, f"meta-{lo}.jsonl.gz"), "wt")
    roff = doff = 0
    n = 0
    for pid, length, body in con.execute(
            "select pack_id, length, body from pack where domain=1 and pack_id>=? and pack_id<? order by pack_id",
            (lo, hi)):
        ver = struct.unpack_from("<I", body, 8)[0]
        gc = struct.unpack_from("<I", body, 12)[0]
        if ver in WF_V:
            lane = 1
            starts = [struct.unpack_from("<I", body, 24 + 4 * i)[0] for i in range(gc)]
            views = [(starts[i], starts[i + 1] if i + 1 < gc else length, 0) for i in range(gc)]
        elif ver in CHUNK_V:
            lane = 2
            views = []
            for i in range(gc):
                s, enc, dec, fl = struct.unpack_from("<IIII", body, 24 + 16 * i)
                views.append((s, s + enc, fl & 0xff))
        else:
            continue  # singleton or ordinary lanes in domain 1 are not payload records
        for gi, (s, e, codec) in enumerate(views):
            assert codec == 0, (pid, gi, codec)
            g = body[s:e]
            cnt = struct.unpack_from("<I", g, 0)[0]
            ends = struct.unpack_from(f"<{cnt}I", g, 4)
            area, prev = 4 + 4 * cnt, 0
            for ri, end in enumerate(ends):
                rec = g[area + prev: area + end]
                prev = end
                oid, role, clen = loc[(pid, gi, ri)]
                assert role == lane, (pid, gi, ri, role, lane)
                tag = rec[0]
                if lane == 1:
                    rawlen = clen - WF_OVERHEAD
                    hdr = 1
                else:
                    rawlen = struct.unpack_from("<I", rec, 1)[0]
                    hdr = 5
                base = None
                if tag == 1:
                    base = rec[hdr:hdr + 32]
                    hdr += 32
                frame = rec[hdr:]
                row = {"k": [pid, gi, ri], "id": oid.hex(), "role": role, "tag": tag, "rec": len(rec),
                       "raw": rawlen, "clen": clen}
                if tag == 1:
                    row["base"] = base.hex()
                    row["doff"], row["dlen"] = doff, len(frame)
                    delf.write(frame)
                    doff += len(frame)
                    row["off"] = -1
                else:
                    raw = frame if tag == 2 else zstd.decompress(frame)
                    assert len(raw) == rawlen, (pid, gi, ri, len(raw), rawlen)
                    row["off"] = roff
                    rawf.write(raw)
                    roff += len(raw)
                meta.write(json.dumps(row, separators=(",", ":")) + "\n")
                n += 1
    meta.close()
    rawf.close()
    delf.close()
    print(json.dumps({"lo": lo, "hi": hi, "records": n, "raw_bytes": roff, "delta_bytes": doff,
                      "seconds": round(time.time() - t0, 1)}))


if __name__ == "__main__":
    main()
