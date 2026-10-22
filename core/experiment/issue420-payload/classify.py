#!/usr/bin/env python3
"""#420 payload study (question A): content classes of full payload records; zlib/gzip anatomy.

usage: classify.py DUMPDIR OUT_JSONL LO HI
Records with index in [LO, HI) of index.pkl. For every non-PREFIX record writes one row:
  id, role, tag, rec, raw, cls (class of the file head: WholeFile -> its own bytes; Chunk -> its
  file's first chunk, via extents.pkl), head (chunk is at file offset 0), sub ('stored', 'poor'
  = FULL with rec/raw > 0.9, 'good').
For WholeFile records of class zlib: inflate; git header; inflated size; zstd-3 size of the inflated
bytes (whole-file frame parameters); sha1 of inflated bytes (= Git object id for loose objects);
reproduction: smallest zlib level 1..9 whose one-shot deflate equals the stored bytes, else the
Git two-call form (header, then body with Z_FINISH); inflated bytes are appended to
inflated-LO.bin (ioff/ilen) for the similarity study.
For WholeFile records of class gzip: same, on the raw deflate member (header and trailer parsed).
"""
import hashlib
import json
import os
import pickle
import struct
import sys
import zlib

sys.path.insert(0, __file__.rsplit("/", 1)[0])
from common import Raw, classify, full_frame, record_width, GIT_HDR, WF_WLOG  # noqa: E402


def reproduce_zlib(data, stored, hdr_len):
    for lvl in range(1, 10):
        if zlib.compress(data, lvl) == stored:
            return lvl, "oneshot"
    for lvl in range(1, 10):
        c = zlib.compressobj(lvl)
        out = c.compress(data[:hdr_len]) + c.compress(data[hdr_len:]) + c.flush()
        if out == stored:
            return lvl, "twocall"
    return 0, None


def gzip_member(raw):
    if len(raw) < 18 or raw[2] != 8:
        return None
    flg = raw[3]
    p = 10
    if flg & 4:
        xlen = struct.unpack_from("<H", raw, p)[0]
        p += 2 + xlen
    if flg & 8:
        p = raw.index(b"\x00", p) + 1
    if flg & 16:
        p = raw.index(b"\x00", p) + 1
    if flg & 2:
        p += 2
    return p


def main():
    d, outp, lo, hi = sys.argv[1], sys.argv[2], int(sys.argv[3]), int(sys.argv[4])
    raw = Raw(d)
    ext = pickle.load(open(os.path.join(d, "extents.pkl"), "rb"))
    by = {r["id"]: r for r in raw.rows}
    inf = open(os.path.join(d, f"inflated-{lo}.bin"), "wb")
    ioff = 0
    out = open(outp, "w")
    for i in range(lo, min(hi, len(raw.rows))):
        r = raw.rows[i]
        if r["tag"] == 1:
            continue
        data = raw.get(r)
        row = {"i": i, "id": r["id"], "role": r["role"], "tag": r["tag"], "rec": r["rec"], "raw": r["raw"]}
        if r["tag"] == 2:
            row["sub"] = "stored"
        else:
            row["sub"] = "poor" if r["rec"] > 0.9 * r["raw"] else "good"
        if r["role"] == 1:
            row["cls"] = classify(data[:600])
        else:
            first, off = ext[bytes.fromhex(r["id"])]
            row["head"] = off == 0
            fr = by[first.hex()]
            row["cls"] = classify(raw.get(fr)[:600]) if fr["id"] != r["id"] else classify(data[:600])
        if r["role"] == 1 and row["cls"] in ("zlib", "gzip"):
            try:
                if row["cls"] == "zlib":
                    start, wbits = 0, 15
                else:
                    start, wbits = gzip_member(data), -15
                dobj = zlib.decompressobj(wbits)
                infl = dobj.decompress(data[start:])
                ok = dobj.eof
                tail = len(dobj.unused_data)
            except Exception as e:  # noqa: BLE001
                ok, infl, tail = False, b"", -1
                row["err"] = str(e)[:60]
            row["complete"] = bool(ok)
            row["tail"] = tail
            if ok:
                row["ilen"] = len(infl)
                row["ioff"] = ioff
                row["ifile"] = f"inflated-{lo}.bin"
                inf.write(infl)
                ioff += len(infl)
                row["isha1"] = hashlib.sha1(infl).hexdigest()
                row["iz3"] = record_width(1, len(full_frame(infl, WF_WLOG if len(infl) <= 262144 else 20)), False) \
                    if infl else 1
                if row["cls"] == "zlib":
                    m = GIT_HDR.match(infl[:40])
                    hl = m.end() if m else 0
                    row["git"] = m.group(1).decode() if m else None
                    if m:
                        row["bsha256"] = hashlib.sha256(infl[hl:]).hexdigest()
                        row["hl"] = hl
                    stored = data[:len(data) - tail] if tail else data
                    lvl, how = reproduce_zlib(infl, stored, hl)
                else:
                    end = len(data) - tail - 8
                    stored = data[start:end]
                    lvl, how = 0, None
                    for L in range(1, 10):
                        c = zlib.compressobj(L, zlib.DEFLATED, -15)
                        if c.compress(infl) + c.flush() == stored:
                            lvl, how = L, "raw-deflate"
                            break
                    row["bsha256"] = hashlib.sha256(infl).hexdigest()
                row["lvl"], row["how"] = lvl, how
        out.write(json.dumps(row, separators=(",", ":")) + "\n")
    out.close()
    inf.close()
    print(json.dumps({"lo": lo, "hi": hi, "inflated_bytes": ioff}))


if __name__ == "__main__":
    main()
