"""Measured delta gain on a random sample of PREFIX payload records.

For each sampled PREFIX record (chunk or whole file): rebuild its raw payload by
decoding its base chain (zstd frames against the base payload as a raw-content
prefix), re-encode the payload independently at zstd level 3, and compare that
independent width with the PREFIX record actually stored. Also re-encodes a
sample of FULL records to check this zstd matches the stored frame widths.
"""

import json
import random
import signal
import sqlite3
import struct
import sys
from compression import zstd

signal.alarm(115)
DB, EDGES, OUT, N = sys.argv[1], sys.argv[2], sys.argv[3], int(sys.argv[4])
u32 = lambda b, o: struct.unpack_from("<I", b, o)[0]
con = sqlite3.connect(f"file:{DB}?immutable=1", uri=True)
loc = {o: (p, g, r, role, c) for o, p, g, r, role, c in con.execute(
    "SELECT object_id, pack_id, group_number, record_number, role, canonical_length FROM object_location WHERE role IN (1,2)")}
data = open(EDGES, "rb").read()
base = {data[i:i + 32]: data[i + 32:i + 64] for i in range(0, len(data), 65) if data[i + 64] in (1, 2)}
random.seed(20261124)
pre = {1: [o for o in base if loc[o][3] == 1], 2: [o for o in base if loc[o][3] == 2]}
full = {1: [], 2: []}
for o, v in loc.items():
    if o not in base:
        full[v[3]].append(o)
sample = {k: random.sample(v, min(N, len(v))) for k, v in pre.items()}
fsample = {k: random.sample(v, min(N, len(v))) for k, v in full.items()}

need = set()
for k in (1, 2):
    for o in sample[k] + fsample[k]:
        while True:
            need.add(o)
            if o not in base:
                break
            o = base[o]
bypack = {}
for o in need:
    bypack.setdefault(loc[o][0], []).append(o)
recs = {}
for pid, items in bypack.items():
    body = con.execute("SELECT body FROM pack WHERE pack_id=?", (pid,)).fetchone()[0]
    ver = u32(body, 8)
    n = u32(body, 12)
    groups = []
    if ver == 23:
        starts = [u32(body, 24 + 4 * i) for i in range(n)]
        used = u32(body, 16)
        for i, s in enumerate(starts):
            groups.append(body[s:(starts[i + 1] if i + 1 < n else used)])
    else:
        for i in range(n):
            o = 24 + 16 * i
            s, enc = u32(body, o), u32(body, o + 4)
            groups.append(body[s:s + enc])
    for o in items:
        g = groups[loc[o][1]]
        k = loc[o][2]
        cnt = u32(g, 0)
        st = 4 + 4 * cnt + (u32(g, 4 + 4 * (k - 1)) if k else 0)
        recs[o] = g[st:4 + 4 * cnt + u32(g, 4 + 4 * k)]

raw = {}


def payload(o):
    if o in raw:
        return raw[o]
    rec = recs[o]
    role = loc[o][3]
    tag = rec[0]
    if role == 2:
        frame = rec[5 + (32 if tag == 1 else 0):]
    else:
        frame = rec[1 + (32 if tag == 1 else 0):]
    if tag == 2:
        out = frame
    elif tag == 0:
        out = zstd.decompress(frame)
    else:
        d = zstd.ZstdDict(payload(base[o]), is_raw=True)
        out = zstd.decompress(frame, zstd_dict=d)
    raw[o] = out
    return out


res = {}
for k, name in ((1, "WholeFile"), (2, "Chunk")):
    hdr = 5 if k == 2 else 1
    tot_raw = tot_pre = tot_ind = 0
    for o in sample[k]:
        p = payload(o)
        tot_raw += len(p)
        tot_pre += len(recs[o])
        tot_ind += min(len(zstd.compress(p, 3)), len(p)) + hdr
    f_raw = f_stored = f_re = 0
    for o in fsample[k]:
        p = payload(o)
        f_raw += len(p)
        f_stored += len(recs[o])
        f_re += (len(zstd.compress(p, 3)) if recs[o][0] == 0 else len(p)) + hdr
    res[name] = {"prefix_sample": len(sample[k]), "raw": tot_raw, "prefix_records": tot_pre,
                 "independent_zstd3_records": tot_ind,
                 "full_sample": len(fsample[k]), "full_raw": f_raw, "full_stored": f_stored,
                 "full_reencoded": f_re}
json.dump(res, open(OUT, "w"), indent=1)
print(json.dumps(res, indent=1))
