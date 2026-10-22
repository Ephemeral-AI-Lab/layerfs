#!/usr/bin/env python3
"""#420 payload study (question A, chunked files): reassemble chunked zlib / gzip / Git pack files.

usage: chunkfiles.py DUMPDIR CLASSIFY_DIR OUT_JSON
For each chunked file (extent root) whose first chunk is zlib, gzip or git-pack:
  stored bytes attributable = stored record bytes of its chunks first seen under this file (each chunk
  counted once, under the earliest file that maps it, as extents.pkl does);
  zlib: inflate the whole file, git header, reproduction at levels 1..9, content match (sha256 of the
        blob body) against WholeFile / chunked file contents earlier in insertion order;
  gzip: inflate the member, reproduction of the raw deflate stream at levels 1..9;
  git-pack: parse the pack header and walk entries (type, inflated size, compressed size); for
        non-delta entries try reproduction at levels 1..9 (level of the first entry that reproduces
        is then tried first) and match blob bodies against earlier content; delta entries counted.
"""
import glob
import hashlib
import json
import os
import pickle
import sys
import time
import zlib
from collections import Counter

sys.path.insert(0, __file__.rsplit("/", 1)[0])
from common import Raw, GIT_HDR  # noqa: E402

PACK_TYPES = {1: "commit", 2: "tree", 3: "blob", 4: "tag", 6: "ofs_delta", 7: "ref_delta"}


def repro(data, stored, wbits=15, order=range(1, 10)):
    for lvl in order:
        c = zlib.compressobj(lvl, zlib.DEFLATED, wbits)
        if c.compress(data) + c.flush() == stored:
            return lvl
    return 0


def main():
    d, cdir, outp = sys.argv[1:4]
    t0 = time.time()
    raw = Raw(d)
    idx = {r["id"]: i for i, r in enumerate(raw.rows)}
    ext = pickle.load(open(os.path.join(d, "extents.pkl"), "rb"))
    files = pickle.load(open(os.path.join(d, "extents-files.pkl"), "rb"))
    cls = {}
    for p in glob.glob(cdir + "/c-*.jsonl"):
        for line in open(p):
            r = json.loads(line)
            if r["role"] == 2:
                cls[r["id"]] = r["cls"]
    # earliest content positions (WholeFile and chunked), as in blobmatch.py
    content = {}
    for i, r in enumerate(raw.rows):
        if r["role"] == 1:
            h = hashlib.sha256(raw.get(r)).digest()
            if h not in content or content[h] > i:
                content[h] = i
    fpos = {}
    for root, seq in files.items():
        hs = hashlib.sha256()
        pos = 0
        for cid, so, ln in seq:
            j = idx[cid.hex()]
            pos = max(pos, j)
            hs.update(raw.get(raw.rows[j])[so:so + ln])
        fpos[root] = pos
        h = hs.digest()
        if h not in content or content[h] > pos:
            content[h] = pos
    c = Counter()
    pref_level = 6
    for root, seq in files.items():
        if not seq:
            c["empty_roots"] += 1
            continue
        first = seq[0][0].hex()
        k = cls.get(first)
        if k not in ("zlib", "gzip", "git-pack"):
            continue
        own = [cid for cid, _, _ in seq if ext[cid][0] == seq[0][0]]
        own_rec = sum(raw.rows[idx[cid.hex()]]["rec"] for cid in set(own))
        data = b"".join(raw.get(raw.rows[idx[cid.hex()]])[so:so + ln] for cid, so, ln in seq)
        pos = fpos[root]
        c[f"{k}_files"] += 1
        c[f"{k}_bytes"] += len(data)
        c[f"{k}_own_rec"] += own_rec
        if k == "zlib":
            dobj = zlib.decompressobj()
            infl = dobj.decompress(data)
            if not dobj.eof:
                c["zlib_incomplete"] += 1
                continue
            lvl = repro(infl, data[:len(data) - len(dobj.unused_data)])
            c[f"zlib_lvl{lvl}"] += 1
            c[f"zlib_lvl{lvl}_own_rec"] += own_rec
            m = GIT_HDR.match(infl[:40])
            if m and m.group(1) == b"blob":
                h = hashlib.sha256(infl[m.end():]).digest()
                mp = content.get(h)
                w = "nomatch" if mp is None else ("earlier" if mp < pos else "later")
                c[f"zlib_blob_{w}"] += 1
                c[f"zlib_blob_{w}_own_rec"] += own_rec
        elif k == "gzip":
            flg = data[3]
            p = 10
            if flg & 4:
                p += 2 + int.from_bytes(data[p:p + 2], "little")
            if flg & 8:
                p = data.index(b"\x00", p) + 1
            if flg & 16:
                p = data.index(b"\x00", p) + 1
            if flg & 2:
                p += 2
            dobj = zlib.decompressobj(-15)
            infl = dobj.decompress(data[p:])
            stored = data[p:len(data) - len(dobj.unused_data)]
            lvl = repro(infl, stored, -15) if dobj.eof else -1
            c[f"gzip_lvl{lvl}"] += 1
            c[f"gzip_lvl{lvl}_own_rec"] += own_rec
            c["gzip_inflated"] += len(infl)
        else:
            if data[:4] != b"PACK":
                continue
            n = int.from_bytes(data[8:12], "big")
            p = 12
            c["pack_entries"] += n
            for _ in range(n):
                b = data[p]
                typ = (b >> 4) & 7
                size = b & 15
                shift = 4
                p += 1
                while b & 0x80:
                    b = data[p]
                    size |= (b & 0x7f) << shift
                    shift += 7
                    p += 1
                if typ == 6:
                    b = data[p]
                    p += 1
                    while b & 0x80:
                        b = data[p]
                        p += 1
                elif typ == 7:
                    p += 20
                mv = memoryview(data)
                lim = min(len(data), p + size + 4096)
                dobj = zlib.decompressobj()
                infl = dobj.decompress(mv[p:lim])
                if not dobj.eof:
                    dobj = zlib.decompressobj()
                    infl = dobj.decompress(mv[p:])
                    lim = len(data)
                if not dobj.eof:
                    c["pack_entry_incomplete"] += 1
                    break
                consumed = lim - p - len(dobj.unused_data)
                stored = data[p:p + consumed]
                tname = PACK_TYPES.get(typ, str(typ))
                c[f"pack_{tname}_n"] += 1
                c[f"pack_{tname}_zbytes"] += consumed
                c[f"pack_{tname}_ibytes"] += len(infl)
                order = [pref_level] + [x for x in range(1, 10) if x != pref_level]
                lvl = repro(infl, stored, 15, order)
                if lvl:
                    pref_level = lvl
                c[f"pack_{tname}_lvl{lvl}_n"] += 1
                c[f"pack_{tname}_lvl{lvl}_zbytes"] += consumed
                if typ == 3:
                    mp = content.get(hashlib.sha256(infl).digest())
                    w = "nomatch" if mp is None else ("earlier" if mp < pos else "later")
                    c[f"pack_blob_{w}_n"] += 1
                    c[f"pack_blob_{w}_zbytes"] += consumed
                p += consumed
            c["pack_trailer_ok"] += (len(data) - p == 20)
    res = {"seconds": round(time.time() - t0, 1), "c": dict(sorted(c.items()))}
    json.dump(res, open(outp, "w"), indent=1)
    print(json.dumps(res, indent=1))


if __name__ == "__main__":
    main()
