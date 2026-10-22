#!/usr/bin/env python3
"""#420 payload study: map every Chunk object to the first chunk of a file that uses it.

usage: extents.py DB OUT_PKL
Decodes every ExtentLeaf (3) and ExtentBranch (4) record (ordinary lane, tag 0 + canonical bytes).
For each root-to-leaf walk, the leftmost chunk of the whole tree is the file's first chunk; every
chunk under that tree is mapped to it (first tree seen in insertion order wins). Also records each
chunk's byte offset in that file. Output: {chunk_id_bytes: (first_chunk_id_bytes, offset)}, and
OUT-files.pkl {root_id_bytes: [(chunk_id, source_offset, logical_length), ...]} in file order.
"""
import pickle
import sqlite3
import struct
import sys
import time
from compression import zstd

HDR = 13  # LFSO, kind, payload_len, value_len
NODE_HDR = 31


def main():
    db, outp = sys.argv[1], sys.argv[2]
    t0 = time.time()
    con = sqlite3.connect(f"file:{db}?mode=ro&immutable=1", uri=True)
    want = {}
    for oid, role, p, g, r in con.execute(
            "select object_id, role, pack_id, group_number, record_number from object_location where role in (3,4)"):
        want.setdefault(p, []).append((g, r, oid, role))
    nodes = {}
    order = []
    for p in sorted(want):
        body = con.execute("select body from pack where pack_id=?", (p,)).fetchone()[0]
        groups = {}
        for g, r, oid, role in want[p]:
            if g not in groups:
                s, enc, dec, fl = struct.unpack_from("<IIII", body, 24 + 16 * g)
                raw = body[s:s + enc]
                groups[g] = zstd.decompress(raw) if fl & 0xff == 1 else raw
            gb = groups[g]
            n = struct.unpack_from("<I", gb, 0)[0]
            ends = struct.unpack_from(f"<{n}I", gb, 4)
            area = 4 + 4 * n
            start = ends[r - 1] if r else 0
            rec = gb[area + start: area + ends[r]]
            assert rec[0] == 0
            canon = rec[1:]
            assert canon[:4] == b"LFSO", canon[:8]
            v = canon[HDR:]
            assert v[:8] == b"LFS4MAP\0"
            kind, count = v[10], struct.unpack_from(">H", v, 13)[0]
            ents = []
            if kind == 0x08:
                for i in range(count):
                    e = v[NODE_HDR + 40 * i: NODE_HDR + 40 * (i + 1)]
                    ents.append((e[:32], struct.unpack_from(">I", e, 36)[0], struct.unpack_from(">I", e, 32)[0]))
            else:
                assert kind == 0x09
                for i in range(count):
                    e = v[NODE_HDR + 48 * i: NODE_HDR + 48 * (i + 1)]
                    ents.append(e[16:48])
            nodes[oid] = (kind, ents)
            order.append(oid)
    children = set()
    for oid, (kind, ents) in nodes.items():
        if kind == 0x09:
            children.update(ents)
    roots = [o for o in order if o not in children]
    m = {}

    def leaves(o):
        kind, ents = nodes[o]
        if kind == 0x08:
            yield ents
        else:
            for c in ents:
                if c in nodes:
                    yield from leaves(c)

    files = 0
    seqs = {}
    for root in roots:
        first = None
        off = 0
        seq = seqs[root] = []
        for ents in leaves(root):
            for cid, ln, so in ents:
                seq.append((cid, so, ln))
                if first is None:
                    first = cid
                if cid not in m:
                    m[cid] = (first, off)
                off += ln
        files += 1
    pickle.dump(m, open(outp, "wb"))
    pickle.dump(seqs, open(outp.replace(".pkl", "-files.pkl"), "wb"))
    print({"nodes": len(nodes), "roots": len(roots), "chunks_mapped": len(m), "seconds": round(time.time() - t0, 1)})


if __name__ == "__main__":
    main()
