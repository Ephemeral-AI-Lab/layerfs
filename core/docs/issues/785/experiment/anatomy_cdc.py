"""CDC sharing among distinct chunked file versions (read-only, immutable URI).

Rebuilds every ExtentLeaf/ExtentBranch page (FULL or COPY/INSERT program) and
reads every FileState, then compares:
  L  = sum of logical lengths of distinct chunked file versions
       (what whole-file-only dedup would keep, before compression)
  U  = payload bytes of the unique chunks those versions reference
and walks each file's tree to count how many distinct versions share a chunk.
"""

import json
import signal
import sqlite3
import struct
import sys
import time
from collections import Counter
from compression import zstd

signal.alarm(115)
DB, OUT = sys.argv[1], sys.argv[2]
u32 = lambda b, o: struct.unpack_from("<I", b, o)[0]
ENV = 13  # LFSO + kind + payload_len + value_len
CHUNK_OVER = ENV + 8  # + LFS4CHK\0


def groups_of(body):
    n = u32(body, 12)
    out = []
    for i in range(n):
        o = 24 + 16 * i
        s, enc, dec, codec = u32(body, o), u32(body, o + 4), u32(body, o + 8), body[o + 12]
        raw = body[s:s + enc]
        out.append(zstd.decompress(raw) if codec == 1 else raw)
    return out


def record(group, k):
    n = u32(group, 0)
    start = 4 + 4 * n + (u32(group, 4 + 4 * (k - 1)) if k else 0)
    return group[start:4 + 4 * n + u32(group, 4 + 4 * k)]


t0 = time.time()
con = sqlite3.connect(f"file:{DB}?immutable=1", uri=True)
chunk_len = {oid: c - CHUNK_OVER for oid, c in
             con.execute("SELECT object_id, canonical_length FROM object_location WHERE role=2")}
locs = {}
for oid, role, pid, g, r in con.execute(
        "SELECT object_id, role, pack_id, group_number, record_number FROM object_location WHERE role IN (3,4,5)"):
    locs.setdefault(pid, []).append((oid, role, g, r))
recs, roles = {}, {}
for pid, items in locs.items():
    body = con.execute("SELECT body FROM pack WHERE pack_id=?", (pid,)).fetchone()[0]
    gs = groups_of(body)
    for oid, role, g, r in items:
        recs[oid] = record(gs[g], r)
        roles[oid] = role
t1 = time.time()

canon = {}


def canonical(oid):
    want = oid
    chain = []
    while oid not in canon:
        rec = recs[oid]
        if rec[0] == 0:
            canon[oid] = rec[1:]
            break
        chain.append(oid)
        oid = rec[1:33]
    for x in reversed(chain):
        rec = recs[x]
        base = canon[rec[1:33]]
        count = u32(rec, 37)
        prog = rec[41:]
        out, cur = bytearray(), 0
        for _ in range(count):
            op = prog[cur]; cur += 1
            if op == 0:
                off, ln = u32(prog, cur), u32(prog, cur + 4); cur += 8
                out += base[off:off + ln]
            else:
                ln = u32(prog, cur); cur += 4
                out += prog[cur:cur + ln]; cur += ln
        canon[x] = bytes(out)
    return canon[want]


leaf_chunks = {}
branch_kids = {}
for oid, role in roles.items():
    if role == 5:
        continue
    v = canonical(oid)[ENV:]
    count = struct.unpack_from(">H", v, 13)[0]
    if role == 3:
        leaf_chunks[oid] = [(v[31 + 40 * i:63 + 40 * i], struct.unpack_from(">I", v, 67 + 40 * i)[0])
                            for i in range(count)]
    else:
        branch_kids[oid] = [v[31 + 48 * i + 16:31 + 48 * i + 48] for i in range(count)]
t2 = time.time()

files = [(oid, canonical(oid)[ENV:]) for oid, r in roles.items() if r == 5]
L = sum(struct.unpack_from(">Q", v, 12)[0] for _, v in files)
slices = sum(struct.unpack_from(">Q", v, 20)[0] for _, v in files)
used = set()
for lst in leaf_chunks.values():
    used.update(c for c, _ in lst)
U = sum(chunk_len[c] for c in used)
import pickle; pickle.dump(used, open(OUT + ".used.pkl", "wb"))

# per-chunk count of distinct file versions referencing it
leaf_set = {}


def chunks_under(node):
    if node in leaf_chunks:
        return {c for c, _ in leaf_chunks[node]}
    if node in leaf_set:
        return leaf_set[node]
    s = set()
    for k in branch_kids[node]:
        s |= chunks_under(k)
    leaf_set[node] = s
    return s


share = Counter()
for _, v in files:
    for c in chunks_under(v[61:93]):
        share[c] += 1
hist = Counter()
for c, n in share.items():
    b = 1 if n == 1 else 2 if n == 2 else 5 if n <= 5 else 20 if n <= 20 else 100 if n <= 100 else 1000
    hist[b] += 1
bytes_by_share = Counter()
for c, n in share.items():
    b = 1 if n == 1 else 2 if n == 2 else 5 if n <= 5 else 20 if n <= 20 else 100 if n <= 100 else 1000
    bytes_by_share[b] += chunk_len[c]
fsizes = Counter()
for _, v in files:
    fsizes[struct.unpack_from(">Q", v, 12)[0].bit_length()] += 1
res = {"file_versions": len(files), "logical_versions_L": L, "slices": slices,
       "unique_chunks": len(used), "unique_chunk_payload_U": U,
       "chunks_in_store": len(chunk_len), "chunk_payload_in_store": sum(chunk_len.values()),
       "share_hist_chunks": dict(hist), "share_hist_bytes": dict(bytes_by_share),
       "max_share": max(share.values()), "file_size_log2": dict(fsizes),
       "t_load": t1 - t0, "t_pages": t2 - t1, "t_total": time.time() - t0}
json.dump(res, open(OUT, "w"), indent=1)
print(json.dumps(res, indent=1))
