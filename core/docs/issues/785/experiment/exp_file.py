"""Per-file predecessor by similarity, then positional chunk alignment.

For each sampled FULL chunk, take the file version in which it first appears.
Propose a predecessor file with the product's index rule applied to file
signatures (eight smallest distinct hashes over the file's chunks, i.e. the
whole-file product signature less boundary windows): newest earlier holder per
hash, at least 2 shared, largest overlap, newest. The base offered to the chunk
is the predecessor's chunk covering the same start offset, as the existing
rename-over route offers it. One trial; depth cap 8; strictly smaller wins.

Modes: "earlier" requires the predecessor (and base) from an earlier Commit;
"any" allows any earlier admitted file.
"""

import glob
import json
import signal
import struct
import sys
import time
from bisect import bisect_right

sys.path.insert(0, sys.argv[1])
import numpy as np
from compression import zstd
from compression.zstd import CompressionParameter as CP
import lfs

signal.alarm(112)
TOOLS, DB, OUT = sys.argv[1], sys.argv[2], sys.argv[3]
CAP = 8
OPTS = {CP.compression_level: 3, CP.checksum_flag: 1, CP.window_log: 20}
ENV = 13
u32 = lfs.u32
t0 = time.time()

ids = np.load(f"{OUT}/order.npy")
n = len(ids)
SIG = np.empty((n, 8), dtype=np.uint64)
for f in glob.glob(f"{OUT}/sig_*.npz"):
    z = np.load(f)
    SIG[int(z["lo"]):int(z["hi"])] = z["sig"]
MAXU = np.iinfo(np.uint64).max
idb = [bytes(r) for r in ids]
rank = {o: i for i, o in enumerate(idb)}
con = lfs.connect(DB)
loc = lfs.load_payload_locations(con, roles=(2,))
base = lfs.load_bases(f"{OUT}/edges.bin", roles=(2,))
depth = lfs.depths(base)
P = lfs.Payloads(con, loc, base)
packs = con.execute("SELECT pack_id, length FROM pack ORDER BY pack_id").fetchall()
cc = np.cumsum([s for (s,) in con.execute('SELECT new_stored FROM "commit" ORDER BY created_ms')])
pc = np.cumsum([l for _, l in packs]) - np.array([l for _, l in packs]) / 2
pack_commit = dict(zip([p for p, _ in packs], np.searchsorted(cc, pc).tolist()))
chunk_commit = np.array([pack_commit[loc[o][0]] for o in idb])
is_prefix = np.array([o in base for o in idb])

# ---- rebuild extent pages and file states (ordinary lane) ----
meta = {}
for oid, role, pid, g, r in con.execute(
        "SELECT object_id, role, pack_id, group_number, record_number FROM object_location WHERE role IN (3,4,5)"):
    meta.setdefault(pid, []).append((oid, role, g, r))
recs, roles, fpos = {}, {}, {}
for pid, items in meta.items():
    body = con.execute("SELECT body FROM pack WHERE pack_id=?", (pid,)).fetchone()[0]
    gs = lfs.pack_groups(body)
    for oid, role, g, r in items:
        recs[oid] = lfs.framed(gs[g], r)
        roles[oid] = role
        if role == 5:
            fpos[oid] = (pid, g, r)
canon = {}


def canonical(oid):
    want, chain = oid, []
    while oid not in canon:
        rec = recs[oid]
        if rec[0] == 0:
            canon[oid] = rec[1:]
            break
        chain.append(oid)
        oid = rec[1:33]
    for x in reversed(chain):
        rec = recs[x]
        b = canon[rec[1:33]]
        count, prog, out, cur = u32(rec, 37), rec[41:], bytearray(), 0
        for _ in range(count):
            op = prog[cur]
            cur += 1
            if op == 0:
                off, ln = u32(prog, cur), u32(prog, cur + 4)
                cur += 8
                out += b[off:off + ln]
            else:
                ln = u32(prog, cur)
                cur += 4
                out += prog[cur:cur + ln]
                cur += ln
        canon[x] = bytes(out)
    return canon[want]


page = {}


def entries(node):
    """Ordered (chunk_rank, slice_length) under an extent node."""
    if node in page:
        return page[node]
    v = canonical(node)[ENV:]
    count = struct.unpack_from(">H", v, 13)[0]
    if v[10] == 0x08:
        out = [(rank[v[31 + 40 * i:63 + 40 * i]], struct.unpack_from(">I", v, 67 + 40 * i)[0]) for i in range(count)]
    else:
        out = []
        for i in range(count):
            out.extend(entries(v[31 + 48 * i + 16:31 + 48 * i + 48]))
    if v[10] == 0x08:
        page[node] = out
    return out


files = sorted(fpos, key=lambda o: fpos[o])  # admission order
nf = len(files)
f_chunks, f_offs = [], []
pairs_c, pairs_f = [], []
for fi, o in enumerate(files):
    v = canonical(o)[ENV:]
    ent = entries(v[61:93])
    cr = np.fromiter((c for c, _ in ent), dtype=np.int64, count=len(ent))
    ln = np.fromiter((l for _, l in ent), dtype=np.int64, count=len(ent))
    f_chunks.append(cr)
    f_offs.append(np.concatenate(([0], np.cumsum(ln)[:-1])))
    pairs_c.append(cr)
    pairs_f.append(np.full(len(cr), fi, dtype=np.int64))
pc_all, pf_all = np.concatenate(pairs_c), np.concatenate(pairs_f)
first_file = np.full(n, nf, dtype=np.int64)
np.minimum.at(first_file, pc_all, pf_all)
file_commit = np.array([pack_commit[fpos[o][0]] for o in files])
file_prefix_share = np.array([is_prefix[c].mean() if len(c) else 0 for c in f_chunks])
t_walk = time.time() - t0

# diagnostic over ALL full chunks: did their first file have any delta chunk?
full_mask = ~is_prefix
ff = first_file[full_mask]
share_of_first = file_prefix_share[ff]
diag = {
    "full_chunks": int(full_mask.sum()),
    "first_file_has_no_delta_chunk": int((share_of_first == 0).sum()),
    "first_file_mostly_delta(>=50%)": int((share_of_first >= 0.5).sum()),
    "distinct_first_files": int(len(np.unique(ff))),
}

# file signatures: 8 smallest distinct over the file's chunk signatures, folded
fsig = np.full((nf, 8), 0xffffffff, dtype=np.uint64)
for fi, cr in enumerate(f_chunks):
    u = np.unique(SIG[np.unique(cr)].reshape(-1))
    u = u[u != MAXU][:8]
    fsig[fi, :len(u)] = u & np.uint64(0xffffffff)
vals = fsig.reshape(-1)
fr = np.repeat(np.arange(nf, dtype=np.uint64), 8)
keep = vals != 0xffffffff
keys = np.sort((vals[keep] << np.uint64(32)) | fr[keep])


def propose_file(f, mode):
    q = [int(x) for x in fsig[f] if x != 0xffffffff]
    qs = set(q)
    best = None
    for v in q:
        hi = np.searchsorted(keys, (np.uint64(v) << np.uint64(32)) | np.uint64(f)) - 1
        while hi >= 0:
            k = int(keys[hi])
            if k >> 32 != v:
                break
            c = k & 0xffffffff
            hi -= 1
            if mode == "earlier" and file_commit[c] >= file_commit[f]:
                continue
            ov = len(qs.intersection(int(x) for x in fsig[c] if x != 0xffffffff))
            if ov >= 2 and (best is None or (ov, c) > best):
                best = (ov, c)
            break  # newest qualifying holder per hash, as the reference table keeps
    return None if best is None else best[1]


sample = np.load(f"{OUT}/sample.npy").tolist()
out = {"diag": diag, "walk_s": t_walk, "files": nf}
for mode in ("earlier", "any"):
    r = {"proposed": 0, "eligible": 0, "won": 0, "full_py": 0, "best_py": 0, "lookups": 0}
    per = []
    cache = {}
    for t in sample:
        tag, frame, rec = P.parts(idb[t])
        target = zstd.decompress(frame)
        full = 5 + len(zstd.compress(target, options=OPTS))
        f = int(first_file[t])
        if f not in cache:
            cache[f] = propose_file(f, mode)
            r["lookups"] += 1
        pf = cache[f]
        best = full
        if pf is not None:
            r["proposed"] += 1
            pos = int(np.nonzero(f_chunks[f] == t)[0][0])
            start = int(f_offs[f][pos])
            j = bisect_right(f_offs[pf].tolist(), start) - 1
            if j >= 0:
                b = int(f_chunks[pf][j])
                if b != t and depth.get(idb[b], 0) < CAP and (mode == "any" or chunk_commit[b] < chunk_commit[t]):
                    r["eligible"] += 1
                    w = 37 + len(zstd.compress(target, options=OPTS,
                                               zstd_dict=zstd.ZstdDict(P.payload(idb[b]), is_raw=True)))
                    best = min(best, w)
        r["won"] += best < full
        r["full_py"] += full
        r["best_py"] += best
        per.append(full - best)
    np.save(f"{OUT}/per_file_{mode}.npy", np.array(per))
    out[mode] = r

# base locality: for 150 proposed files, how many distinct packs hold the aligned bases
import random
random.seed(5)
cand = [f for f in random.sample(range(nf), 3000) if file_prefix_share[f] == 0][:400]
loc_stats = {"files": 0, "chunks": 0, "distinct_base_packs": 0}
for f in cand:
    pf = propose_file(f, "earlier")
    if pf is None:
        continue
    offs = f_offs[pf].tolist()
    bpacks = set()
    for pos in range(len(f_chunks[f])):
        j = bisect_right(offs, int(f_offs[f][pos])) - 1
        if j >= 0:
            bpacks.add(loc[idb[int(f_chunks[pf][j])]][0])
    loc_stats["files"] += 1
    loc_stats["chunks"] += len(f_chunks[f])
    loc_stats["distinct_base_packs"] += len(bpacks)
    if loc_stats["files"] == 150:
        break
out["base_locality"] = loc_stats

# ---- classify sampled FULL chunks by why they are FULL; try cap-aware ancestors ----
from collections import Counter
cls = []
decl = []
capr = {"n": 0, "full_py": 0, "parent_py": 0, "root_py": 0, "parent_won": 0, "root_won": 0}
for t in sample:
    f = int(first_file[t])
    pre = [c for c in f_chunks[f].tolist() if is_prefix[c]]
    if not pre:
        cls.append("no_predecessor")
        decl.append(-1)
        continue
    guess = Counter(int(first_file[rank[base[idb[c]]]]) for c in pre)
    guess.pop(f, None)
    if not guess:
        cls.append("no_predecessor")
        decl.append(-1)
        continue
    pf = guess.most_common(1)[0][0]
    pos = int(np.nonzero(f_chunks[f] == t)[0][0])
    j = bisect_right(f_offs[pf].tolist(), int(f_offs[f][pos])) - 1
    if j < 0:
        cls.append("lost_or_shifted")
        decl.append(-1)
        continue
    a = int(f_chunks[pf][j])
    decl.append(a)
    if depth.get(idb[a], 0) >= CAP:
        cls.append("aligned_base_at_cap")
        tag, frame, rec = P.parts(idb[t])
        target = zstd.decompress(frame)
        full = 5 + len(zstd.compress(target, options=OPTS))
        parent = base[idb[a]]
        root = parent
        while root in base:
            root = base[root]
        wp = 37 + len(zstd.compress(target, options=OPTS, zstd_dict=zstd.ZstdDict(P.payload(parent), is_raw=True)))
        wr = 37 + len(zstd.compress(target, options=OPTS, zstd_dict=zstd.ZstdDict(P.payload(root), is_raw=True)))
        capr["n"] += 1
        capr["full_py"] += full
        capr["parent_py"] += min(full, wp)
        capr["root_py"] += min(full, wr)
        capr["parent_won"] += wp < full
        capr["root_won"] += wr < full
    else:
        cls.append("lost_or_shifted")
np.save(f"{OUT}/sample_class.npy", np.array(cls))
np.save(f"{OUT}/declared_for_sample.npy", np.array(decl))
out["sample_classes"] = dict(Counter(cls))
out["cap_aware"] = capr
out["seconds"] = time.time() - t0
json.dump(out, open(f"{OUT}/exp_file.json", "w"), indent=1)
print(json.dumps(out, indent=1))
