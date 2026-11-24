"""Per-chunk similarity bases for independent (FULL) chunks.

For a random sample of FULL chunks, propose a base the way the product's
content index proposes one for whole files (encoding/delta/candidates.rs):
for each of the target's 8 signature hashes take the newest earlier object
holding it, keep candidates sharing at least 2 hashes, prefer the largest
overlap, then the newest. Earlier = admission order (pack, group, record).

Variants: unbounded; window of the last 65,536 admitted chunks; window of the
last 8,192 (the whole-file index's size); and best-of-3 trials (unbounded).
Eligibility follows the selector: base depth strictly below the chunk cap (8).
Each trial compresses at zstd level 3 with checksum, the base payload as a raw
prefix, and is kept only when the record is strictly smaller than the FULL one.
"""

import glob
import json
import random
import signal
import sys
import time

sys.path.insert(0, sys.argv[1])
import numpy as np
from compression import zstd
from compression.zstd import CompressionParameter as CP
import lfs

signal.alarm(112)
TOOLS, DB, OUT, N = sys.argv[1], sys.argv[2], sys.argv[3], int(sys.argv[4])
MODE = sys.argv[5]
CAP = 8
OPTS = {CP.compression_level: 3, CP.checksum_flag: 1, CP.window_log: 20}

t0 = time.time()
ids = np.load(f"{OUT}/order.npy")
n = len(ids)
SIG = np.empty((n, 8), dtype=np.uint64)
for f in glob.glob(f"{OUT}/sig_*.npz"):
    z = np.load(f)
    SIG[int(z["lo"]):int(z["hi"])] = z["sig"]
MAXU = np.iinfo(np.uint64).max
F = (SIG & np.uint64(0xffffffff)).astype(np.uint64)
F[SIG == MAXU] = 0xffffffff
idb = [bytes(r) for r in ids]
rank = {o: i for i, o in enumerate(idb)}
con = lfs.connect(DB)
loc = lfs.load_payload_locations(con, roles=(2,))
base = lfs.load_bases(f"{OUT}/edges.bin", roles=(2,))
depth = lfs.depths(base)
P = lfs.Payloads(con, loc, base)

# pack -> commit index by cumulative stored bytes (commits in creation order)
packs = con.execute("SELECT pack_id, length FROM pack ORDER BY pack_id").fetchall()
cc = np.cumsum([s for (s,) in con.execute('SELECT new_stored FROM "commit" ORDER BY created_ms')])
pc = np.cumsum([l for _, l in packs]) - np.array([l for _, l in packs]) / 2
pack_commit = dict(zip([p for p, _ in packs], np.searchsorted(cc, pc).tolist()))
commit_of = np.array([pack_commit[loc[o][0]] for o in idb])

# sorted (value, rank) keys for "newest earlier holder of a hash"
vals = F.reshape(-1)
rk = np.repeat(np.arange(n, dtype=np.uint64), 8)
keep = vals != 0xffffffff
keys = np.sort((vals[keep] << np.uint64(32)) | rk[keep])
t_index = time.time() - t0


def holders(v, t, limit, window=None):
    """Up to `limit` newest earlier ranks holding folded hash v, newest first.

    In "earlier" mode a holder from the target's own Commit is skipped, not
    counted: the index a Save consults holds only earlier Saves' objects.
    """
    hi = np.searchsorted(keys, (np.uint64(v) << np.uint64(32)) | np.uint64(t)) - 1
    out = []
    while hi >= 0 and len(out) < limit:
        k = int(keys[hi])
        if k >> 32 != v:
            break
        c = k & 0xffffffff
        hi -= 1
        if window is not None and t - c > window:
            break
        if MODE == "earlier" and commit_of[c] >= commit_of[t]:
            continue
        out.append(c)
    return out


def propose(t, window=None, deep=1):
    q = [int(x) for x in F[t] if x != 0xffffffff]
    qs = set(q)
    cands = set()
    for v in q:
        cands.update(holders(v, t, deep, window))
    scored = []
    for c in cands:
        if window is not None and t - c > window:
            continue
        if MODE == "earlier" and commit_of[c] >= commit_of[t]:
            continue
        ov = len(qs.intersection(int(x) for x in F[c] if x != 0xffffffff))
        if ov >= 2:
            scored.append((ov, c))
    scored.sort(reverse=True)  # overlap, then newest
    return scored


random.seed(20261124)
full_ranks = [i for i in range(n) if idb[i] not in base]
pool = random.sample(full_ranks, int(N * 1.05))
sample = []
for t in pool:
    tag, frame, rec = P.parts(idb[t])
    if tag == 0:
        sample.append(t)
    if len(sample) == N:
        break
np.save(f"{OUT}/sample.npy", np.array(sample))

per = {k: [] for k in ("unbounded", "win65536", "win8192", "best_of_3")}
res = {k: {"proposed": 0, "eligible": 0, "won": 0, "full_py": 0, "best_py": 0, "stored_full": 0}
       for k in ("unbounded", "win65536", "win8192", "best_of_3")}
timing = {"full_compress": [], "prefix_compress": [], "base_fetch": [], "base_depth": [], "propose": []}
for t in sample:
    o = idb[t]
    tag, frame, rec = P.parts(o)
    target = zstd.decompress(frame)
    a = time.perf_counter()
    full = 5 + len(zstd.compress(target, options=OPTS))
    timing["full_compress"].append(time.perf_counter() - a)
    a = time.perf_counter()
    unb = propose(t, deep=3)
    timing["propose"].append(time.perf_counter() - a)
    variants = {
        "unbounded": [c for _, c in unb[:1]],
        "win65536": [c for _, c in propose(t, 65536)[:1]],
        "win8192": [c for _, c in propose(t, 8192)[:1]],
        "best_of_3": [c for _, c in unb[:3]],
    }
    trials = {}
    for name, cs in variants.items():
        r = res[name]
        r["full_py"] += full
        r["stored_full"] += len(rec)
        best = full
        if cs:
            r["proposed"] += 1
        el = [c for c in cs if depth.get(idb[c], 0) < CAP]
        if el:
            r["eligible"] += 1
        for c in el:
            if c not in trials:
                P.raw.clear()
                a = time.perf_counter()
                b = P.payload(idb[c])
                timing["base_fetch"].append(time.perf_counter() - a)
                timing["base_depth"].append(depth.get(idb[c], 0))
                a = time.perf_counter()
                w = 37 + len(zstd.compress(target, options=OPTS, zstd_dict=zstd.ZstdDict(b, is_raw=True)))
                timing["prefix_compress"].append(time.perf_counter() - a)
                trials[c] = w
            best = min(best, trials[c])
        if best < full:
            r["won"] += 1
        r["best_py"] += best
        per[name].append(full - best)

med = lambda x: float(np.median(x)) * 1e6 if x else None
tim = {k: med(v) for k, v in timing.items() if k != "base_depth"}
fetch_by_depth = {}
for d, s in zip(timing["base_depth"], timing["base_fetch"]):
    fetch_by_depth.setdefault(d, []).append(s)
tim["base_fetch_by_depth_us"] = {d: round(med(v), 1) for d, v in sorted(fetch_by_depth.items())}
tim["mean_base_depth"] = float(np.mean(timing["base_depth"])) if timing["base_depth"] else None
out = {"sample": len(sample), "full_chunks": len(full_ranks), "results": res,
       "median_us": tim, "index_build_s": t_index, "seconds": time.time() - t0}
np.savez(f"{OUT}/per_chunk_{MODE}.npz", **{k: np.array(v) for k, v in per.items()})
json.dump(out, open(f"{OUT}/exp_chunk_{MODE}.json", "w"), indent=1)
print(json.dumps(out, indent=1))
