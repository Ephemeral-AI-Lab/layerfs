#!/usr/bin/env python3
"""#420 payload study (question B): resemblance-delta simulation over the fixture's payload records.

usage: resemble.py DUMPDIR FEATDIR CLASSIFY_DIR ROLE OUT_JSON OUT_ROWS_JSONL [TRIALS]
ROLE 2 = Chunk (depth cap 8, window log 20), 1 = WholeFile (depth cap 16, window log 18; the
fixture's store_policy). Records are visited in insertion order (pack, group, record). Every visited
record of ROLE with features enters a super-feature index (3 super-features -> most recent holder).
Targets are records stored FULL or STORED today (WholeFile records of class zlib are excluded: they
are question A). For a target the candidates are earlier records sharing >= 1 super-feature, ranked
by shared count then recency; TRIALS (default 1, as the product's one-trial rule) candidates are
tried. A candidate is eligible when chain depth(base, walked over the simulated base pointers) + 1 +
the target's own descendant height <= cap and the chain canonical (<= 512 KiB) and encoded
(<= 256 KiB) budgets hold with the target's canonical length added. The proxy PREFIX record
(level 3 zstd, base raw payload as prefix, same frame flags) replaces the stored record only when
strictly smaller. Rows of converted targets go to OUT_ROWS_JSONL. Environment CAP overrides the cap
(sensitivity runs only; existing deeper chains are left as they are); MAXIDX bounds each of the three
super-feature tables to that many entries, oldest insertion evicted first.
"""
import glob
import json
import os
import sys
import time
from collections import Counter

import numpy as np

sys.path.insert(0, __file__.rsplit("/", 1)[0])
from common import Raw, prefix_frame, record_width, CHUNK_WLOG, WF_WLOG  # noqa: E402
from compression import zstd  # noqa: E402
from compression.zstd import ZstdDict  # noqa: E402


def main():
    d, fdir, cdir, role, outp, rowsp = sys.argv[1:7]
    role = int(role)
    trials = int(sys.argv[7]) if len(sys.argv) > 7 else 1
    cap = 8 if role == 2 else 16
    maxidx = int(os.environ.get("MAXIDX", "0"))
    if os.environ.get("CAP"):  # sensitivity: a lower depth cap for every chain this simulation creates
        cap = int(os.environ["CAP"])
    wlog = CHUNK_WLOG if role == 2 else WF_WLOG
    t0 = time.time()
    raw = Raw(d)
    rows = raw.rows
    n = len(rows)
    sf = {}
    for p in glob.glob(fdir + "/feat-*.npz"):
        z = np.load(p)
        for i, s in zip(z["idx"].tolist(), z["sf"].tolist()):
            sf[i] = s
    zl = set()
    if role == 1:
        for p in glob.glob(cdir + "/c-*.jsonl"):
            for line in open(p):
                if '"cls":"zlib"' in line:
                    zl.add(json.loads(line)["i"])
    pos = {r["id"]: i for i, r in enumerate(rows)}
    base = [pos[r["base"]] if r["tag"] == 1 else -1 for r in rows]
    width = [r["rec"] for r in rows]
    height = [0] * n
    for i in range(n - 1, -1, -1):
        b = base[i]
        if b >= 0:
            height[b] = max(height[b], height[i] + 1)
    index = [dict(), dict(), dict()]
    c = Counter()
    out = open(rowsp, "w")
    sav_hist = Counter()
    base_depth = Counter()
    t_enc = 0.0
    t_dec = 0.0
    enc_bytes = 0

    def chain(b):
        depth, canon, enc = 0, 0, 0
        x = b
        while x >= 0:
            canon += rows[x]["clen"]
            enc += width[x]
            if base[x] >= 0:
                depth += 1
            x = base[x]
        return depth, canon, enc

    for i, r in enumerate(rows):
        if r["role"] != role:
            continue
        s = sf.get(i)
        target = r["tag"] != 1 and s is not None and i not in zl
        if target:
            c["targets"] += 1
            c["targets_rec"] += r["rec"]
            cand = Counter()
            for j in range(3):
                h = index[j].get(s[j])
                if h is not None:
                    cand[h] += 1
            if not cand:
                c["no_candidate"] += 1
            ranked = sorted(cand.items(), key=lambda kv: (-kv[1], -kv[0]))[:trials]
            best = None
            data = None
            for b, shared in ranked:
                depth, canon, enc = chain(b)
                if depth + 1 + height[i] > cap:
                    c["refused_depth"] += 1
                    continue
                if canon + r["clen"] > 512 * 1024 or enc + r["clen"] > 256 * 1024:
                    c["refused_budget"] += 1
                    continue
                if data is None:
                    data = raw.get(r)
                braw = raw.get(rows[b])
                t1 = time.perf_counter()
                fr = prefix_frame(data, braw, wlog)
                t_enc += time.perf_counter() - t1
                enc_bytes += len(data)
                c["trials"] += 1
                w = record_width(role, len(fr), True)
                if w < width[i] and (best is None or w < best[0]):
                    best = (w, b, shared, depth, fr, braw)
            if best is not None:
                w, b, shared, depth, fr, braw = best
                if len(braw) >= 8:
                    t1 = time.perf_counter()
                    back = zstd.decompress(fr, zstd_dict=ZstdDict(braw, is_raw=True).as_prefix)
                    t_dec += time.perf_counter() - t1
                    assert back == data
                saved = width[i] - w
                c["converted"] += 1
                c["converted_rec_before"] += width[i]
                c["converted_rec_after"] += w
                c[f"converted_from_tag{r['tag']}"] += 1
                c[f"saved_from_tag{r['tag']}"] += saved
                c["base_is_delta"] += base[b] >= 0
                base_depth[depth] += 1
                frac = saved / width[i]
                sav_hist[min(9, int(frac * 10))] += 1
                out.write(json.dumps({"i": i, "b": b, "shared": shared, "before": width[i], "after": w,
                                      "raw": r["raw"], "bdepth": depth}) + "\n")
                base[i] = b
                width[i] = w
        if s is not None:
            for j in range(3):
                index[j].pop(s[j], None)
                index[j][s[j]] = i
                if maxidx and len(index[j]) > maxidx:  # FIFO bound: drop the oldest insertion
                    index[j].pop(next(iter(index[j])))
    out.close()
    res = {"role": role, "trials_per_target": trials, "cap": cap, "seconds": round(time.time() - t0, 1),
           "counts": dict(sorted(c.items())),
           "saved_bytes": c["converted_rec_before"] - c["converted_rec_after"],
           "saving_fraction_hist_deciles": {f"{k*10}-{k*10+10}%": v for k, v in sorted(sav_hist.items())},
           "base_depth_hist": dict(sorted(base_depth.items())),
           "encode_us_per_kb": round(t_enc * 1e6 / max(1, enc_bytes / 1024), 2),
           "encode_us_per_trial": round(t_enc * 1e6 / max(1, c["trials"]), 1),
           "decode_us_per_converted": round(t_dec * 1e6 / max(1, c["converted"]), 1),
           "index_entries": sum(len(x) for x in index)}
    json.dump(res, open(outp, "w"), indent=1)
    print(json.dumps(res, indent=1))


if __name__ == "__main__":
    main()
