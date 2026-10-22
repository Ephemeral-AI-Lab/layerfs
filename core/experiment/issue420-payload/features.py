#!/usr/bin/env python3
"""#420 payload study (question B): N-transform resemblance features of every payload record.

usage: features.py DUMPDIR OUTDIR LO HI
For index.pkl records in [LO, HI): polynomial rolling hash over 32-byte windows (every position,
uint64 wrap-around arithmetic via prefix sums), 12 linear transforms (a_i*h + b_i mod 2^64),
feature i = max transformed value; super-feature j = hash of features 4j..4j+3 (3 super-features).
Payloads shorter than 64 bytes get no features. Writes feat-LO.npz (idx, feats[n,12], sf[n,3]) and
prints the measured Python/numpy cost (an upper bound for a native implementation).
"""
import hashlib
import json
import sys
import time

import numpy as np

sys.path.insert(0, __file__.rsplit("/", 1)[0])
from common import Raw  # noqa: E402

W = 32
R = np.uint64(0x100000001B3)  # odd multiplier, invertible mod 2^64
MASK = (1 << 64) - 1
RINV = pow(0x100000001B3, -1, 1 << 64)
rng = np.random.default_rng(420)
A = (rng.integers(1, 2**63, 12, dtype=np.uint64) * np.uint64(2) + np.uint64(1)).astype(np.uint64)
B = rng.integers(0, 2**63, 12, dtype=np.uint64)
MAXLEN = 1 << 18
POW = np.empty(MAXLEN + 1, dtype=np.uint64)
INV = np.empty(MAXLEN + 1, dtype=np.uint64)
p, q = 1, 1
for k in range(MAXLEN + 1):
    POW[k] = p
    INV[k] = q
    p = (p * 0x100000001B3) & MASK
    q = (q * RINV) & MASK


def feats(buf):
    n = len(buf)
    if n < 64:
        return None
    if n > MAXLEN:
        buf = buf[:MAXLEN]
        n = MAXLEN
    b = np.frombuffer(buf, dtype=np.uint8).astype(np.uint64)
    with np.errstate(over="ignore"):
        pre = np.empty(n + 1, dtype=np.uint64)
        pre[0] = 0
        np.cumsum(b * INV[:n], out=pre[1:])
        # window hash at i: sum_{j=i}^{i+W-1} b[j] R^(i+W-1-j) = R^(i+W-1) * (pre[i+W]-pre[i])
        m = n - W + 1
        h = (pre[W:W + m] - pre[:m]) * POW[W - 1:W - 1 + m]
        out = np.empty(12, dtype=np.uint64)
        for i in range(12):
            out[i] = (h * A[i] + B[i]).max()
    return out


def superf(f):
    sf = np.empty(3, dtype=np.uint64)
    for j in range(3):
        sf[j] = int.from_bytes(hashlib.blake2b(f[4 * j:4 * j + 4].tobytes(), digest_size=8).digest(), "little")
    return sf


def main():
    d, outdir, lo, hi = sys.argv[1], sys.argv[2], int(sys.argv[3]), int(sys.argv[4])
    raw = Raw(d)
    hi = min(hi, len(raw.rows))
    idx, F, S = [], [], []
    nbytes = 0
    t0 = time.perf_counter()
    for i in range(lo, hi):
        data = raw.get(raw.rows[i])
        f = feats(data)
        if f is None:
            continue
        nbytes += len(data)
        idx.append(i)
        F.append(f)
        S.append(superf(f))
    dt = time.perf_counter() - t0
    np.savez(f"{outdir}/feat-{lo}.npz", idx=np.array(idx, dtype=np.int64),
             feats=np.array(F, dtype=np.uint64).reshape(-1, 12), sf=np.array(S, dtype=np.uint64).reshape(-1, 3))
    print(json.dumps({"lo": lo, "hi": hi, "n": len(idx), "bytes": nbytes, "seconds": round(dt, 2),
                      "us_per_kb": round(dt * 1e6 / max(1, nbytes / 1024), 2),
                      "us_per_record": round(dt * 1e6 / max(1, len(idx)), 1)}))


if __name__ == "__main__":
    main()
