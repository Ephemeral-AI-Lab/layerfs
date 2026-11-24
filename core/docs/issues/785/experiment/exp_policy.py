"""Which selection rule captures the gain, and does it hurt existing deltas?

Rule (a), one trial: compare signature overlap of the declared base (the
predecessor's aligned chunk) with the index proposal; try only the higher one.
Rule (b), at most two trials: try the declared base; if it loses or is absent,
try the index proposal.

Measured on (1) the 3,000 sampled FULL chunks (declared base = the aligned chunk
of the guessed predecessor file, from exp_file's classification) and (2) 3,000
random PREFIX chunks (declared base = the stored base, exactly). Bases must come
from an earlier Commit; unbounded index; depth cap 8.
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
TOOLS, DB, OUT = sys.argv[1], sys.argv[2], sys.argv[3]
import os
FULLONLY = os.environ.get("FULLONLY") == "1"
WINDOW = int(sys.argv[4]) if len(sys.argv) > 4 and sys.argv[4] != "none" else None
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
packs = con.execute("SELECT pack_id, length FROM pack ORDER BY pack_id").fetchall()
cc = np.cumsum([s for (s,) in con.execute('SELECT new_stored FROM "commit" ORDER BY created_ms')])
pc = np.cumsum([l for _, l in packs]) - np.array([l for _, l in packs]) / 2
pack_commit = dict(zip([p for p, _ in packs], np.searchsorted(cc, pc).tolist()))
commit_of = np.array([pack_commit[loc[o][0]] for o in idb])
vals = F.reshape(-1)
rk = np.repeat(np.arange(n, dtype=np.uint64), 8)
keep = vals != 0xffffffff
keys = np.sort((vals[keep] << np.uint64(32)) | rk[keep])


def fset(r):
    return {int(x) for x in F[r] if x != 0xffffffff}


def propose(t):
    q = fset(t)
    best = None
    for v in q:
        hi = np.searchsorted(keys, (np.uint64(v) << np.uint64(32)) | np.uint64(t)) - 1
        while hi >= 0:
            k = int(keys[hi])
            if k >> 32 != v:
                break
            c = k & 0xffffffff
            hi -= 1
            if commit_of[c] >= commit_of[t]:
                continue
            if WINDOW is not None and t - c > WINDOW:
                break
            if FULLONLY and idb[c] in base:
                continue
            ov = len(q & fset(c))
            if ov >= 2 and (best is None or (ov, c) > best):
                best = (ov, c)
            break
    return best


def width(target, b):
    if b is None or depth.get(idb[b], 0) >= CAP:
        return None
    return 37 + len(zstd.compress(target, options=OPTS, zstd_dict=zstd.ZstdDict(P.payload(idb[b]), is_raw=True)))


# declared bases for the FULL sample: re-derive the guessed aligned chunk
exp_file_ns = {}
sample = np.load(f"{OUT}/sample.npy").tolist()
cls = np.load(f"{OUT}/sample_class.npy").tolist()
declared_full = json.load(open(f"{OUT}/declared_full.json")) if False else None

per = {"full_rule_b": [], "full_rule_a_cons": [], "prefix_rule_a": []}
res = {"full": {"n": 0, "full": 0, "rule_a": 0, "rule_a_conservative": 0, "rule_b_proposal_after_loss": 0, "a_switch": 0},
       "prefix": {"n": 0, "declared": 0, "rule_a": 0, "a_switch": 0, "a_better": 0, "a_worse": 0}}
decl = np.load(f"{OUT}/declared_for_sample.npy")
for t, d in zip(sample, decl.tolist()):
    tag, frame, rec = P.parts(idb[t])
    target = zstd.decompress(frame)
    full = 5 + len(zstd.compress(target, options=OPTS))
    p = propose(t)
    d = None if d < 0 else d
    od = len(fset(t) & fset(d)) if d is not None else -1
    wd = width(target, d) if d is not None else None
    wp = width(target, p[1]) if p else None
    # rule (b): declared first, proposal if declared absent or lost
    b_best = full
    if wd is not None and wd < full:
        b_best = wd
    elif wp is not None:
        b_best = min(full, wp)
    # rule (a): one trial on the higher-overlap candidate
    if p and p[0] > od:
        res["full"]["a_switch"] += 1
        a_best = min(full, wp) if wp is not None else full
    else:
        a_best = min(full, wd) if wd is not None else full
    r = res["full"]
    r["n"] += 1
    r["full"] += full
    r["rule_a"] += a_best
    # stored FULL means the real declared trial lost: (a) gains only where it switched
    r["rule_a_conservative"] += (min(full, wp) if (p and p[0] > od and wp is not None) else full)
    r["rule_b_proposal_after_loss"] += (min(full, wp) if wp is not None else full)
    per["full_rule_b"].append(full - (min(full, wp) if wp is not None else full))
    per["full_rule_a_cons"].append(full - (min(full, wp) if (p and p[0] > od and wp is not None) else full))

random.seed(99)
pre = random.sample([i for i in range(n) if idb[i] in base], 3000)
for t in pre:
    target = P.payload(idb[t])
    d = rank[base[idb[t]]]
    wd = 37 + len(zstd.compress(target, options=OPTS, zstd_dict=zstd.ZstdDict(P.payload(idb[d]), is_raw=True)))
    full = 5 + len(zstd.compress(target, options=OPTS))
    cur = min(wd, full)
    p = propose(t)
    od = len(fset(t) & fset(d))
    a = cur
    if p and p[0] > od and p[1] != d:
        res["prefix"]["a_switch"] += 1
        wp = width(target, p[1])
        a = min(full, wp) if wp is not None else full
        res["prefix"]["a_better"] += a < cur
        res["prefix"]["a_worse"] += a > cur
    r = res["prefix"]
    r["n"] += 1
    r["declared"] += cur
    r["rule_a"] += a
    per["prefix_rule_a"].append(cur - a)
res["seconds"] = time.time() - t0
np.savez(f"{OUT}/per_policy_{WINDOW}{'_fullonly' if FULLONLY else ''}.npz", **{k: np.array(v) for k, v in per.items()})
json.dump(res, open(f"{OUT}/exp_policy_{WINDOW}{'_fullonly' if FULLONLY else ''}.json", "w"), indent=1)
print(json.dumps(res, indent=1))
