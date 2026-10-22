#!/usr/bin/env python3
"""#420 payload study: does the Python zstd proxy reproduce the Store's own frames?

usage: check_proxy.py DUMPDIR MOD
For records with index % MOD == 0: FULL (tag 0) records are re-encoded with full_frame and PREFIX
(tag 1) records with prefix_frame against their stored base; reports how many proxy record widths
equal the stored width exactly, and the summed widths.
"""
import json
import sys
from collections import Counter

sys.path.insert(0, __file__.rsplit("/", 1)[0])
from common import Raw, full_frame, prefix_frame, record_width, CHUNK_WLOG, WF_WLOG  # noqa: E402


def main():
    raw = Raw(sys.argv[1])
    mod = int(sys.argv[2])
    by = {r["id"]: r for r in raw.rows}
    c = Counter()
    for i, r in enumerate(raw.rows):
        if i % mod or r["tag"] == 2:
            continue
        wlog = WF_WLOG if r["role"] == 1 else CHUNK_WLOG
        data = raw.get(r)
        if r["tag"] == 0:
            w = record_width(r["role"], len(full_frame(data, wlog)), False)
        else:
            w = record_width(r["role"], len(prefix_frame(data, raw.get(by[r["base"]]), wlog)), True)
        k = f"role{r['role']}_tag{r['tag']}"
        c[k + "_n"] += 1
        c[k + "_exact"] += w == r["rec"]
        c[k + "_stored"] += r["rec"]
        c[k + "_proxy"] += w
    print(json.dumps(dict(sorted(c.items()))))


if __name__ == "__main__":
    main()
