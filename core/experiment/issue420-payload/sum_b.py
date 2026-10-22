#!/usr/bin/env python3
"""#420 payload study: break resemble.py conversions down by content class of target and base.

usage: sum_b.py CLASSIFY_DIR ROWS_JSONL
"""
import glob
import json
import sys
from collections import defaultdict


def main():
    cls = {}
    for p in glob.glob(sys.argv[1] + "/c-*.jsonl"):
        for line in open(p):
            r = json.loads(line)
            cls[r["i"]] = r["cls"]
    t = defaultdict(lambda: [0, 0, 0])
    for line in open(sys.argv[2]):
        r = json.loads(line)
        k = (cls.get(r["i"], "?"), cls.get(r["b"], "delta-rec"))
        a = t[k]
        a[0] += 1
        a[1] += r["before"]
        a[2] += r["before"] - r["after"]
    print(f"{'target class':22} {'base class':22} {'n':>7} {'before MB':>10} {'saved MB':>9}")
    for k, a in sorted(t.items(), key=lambda kv: -kv[1][2]):
        print(f"{k[0]:22} {k[1]:22} {a[0]:7} {a[1]/1e6:10.2f} {a[2]/1e6:9.2f}")


if __name__ == "__main__":
    main()
