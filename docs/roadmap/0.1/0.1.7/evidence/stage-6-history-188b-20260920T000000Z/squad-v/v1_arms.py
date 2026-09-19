#!/usr/bin/env python3
"""V1 part 4: decompose the T1 squad's own negative arms (four-slot declarations)."""
import collections
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from v1_lib import Store  # noqa: E402

V016 = "benchmark-results/repository-history/stride-10/deepseek-stride10/host-runtime/store.sqlite"
ARMS = [
    ("v0.1.6", V016, "016"),
    ("default (one base)", "/tmp/r2/sample.sqlite", "c1"),
    ("four slots, measured order", "/tmp/r4/sample.sqlite", "c1"),
    ("four slots, depth-ranked", "/tmp/r5/sample.sqlite", "c1"),
    ("R1a only (pre-R2)", "/tmp/r1a/sample.sqlite", "c1"),
]


def main():
    stores = {}
    for label, path, gen in ARMS:
        if not os.path.exists(path):
            print("missing %s" % path)
            continue
        stores[label] = Store(path, gen)
    base = stores["default (one base)"]
    b = {o.hex(): r for o, r in base.lane4().items()}

    print("=" * 78)
    print("Lane-4 totals per arm")
    print("=" * 78)
    for label, s in stores.items():
        lane = s.lane4()
        body = sum(r["body_bytes"] for r in lane.values())
        deltas = sum(1 for r in lane.values() if r["tag"] in (1, 2))
        print("   %-28s objects %6d  delta %6d  full %6d  lane body %10d  store %10d" % (
            label, len(lane), deltas, len(lane) - deltas, body, os.path.getsize(s.path)))

    print()
    print("=" * 78)
    print("Each arm against the default, per object")
    print("=" * 78)
    for label in ("four slots, measured order", "four slots, depth-ranked"):
        s = stores[label]
        lane = {o.hex(): r for o, r in s.lane4().items()}
        only = set(lane) - set(b)
        gone = set(b) - set(lane)
        gain_full_to_delta = 0
        gain_bytes = 0
        lose_delta_to_full = 0
        lose_bytes = 0
        replaced = collections.Counter()
        replaced_bytes = collections.Counter()
        for o in set(lane) & set(b):
            x, y = b[o], lane[o]
            d = y["body_bytes"] - x["body_bytes"]
            xb = x["column_base"]
            yb = y["column_base"]
            if xb == yb:
                if d:
                    replaced["same base, different bytes"] += 1
                continue
            if xb is None and yb is not None:
                gain_full_to_delta += 1
                gain_bytes += d
            elif xb is not None and yb is None:
                lose_delta_to_full += 1
                lose_bytes += d
            else:
                replaced["base replaced"] += 1
                replaced_bytes["base replaced"] += d
                if d > 0:
                    replaced["base replaced, WORSE"] += 1
                    replaced_bytes["base replaced, WORSE"] += d
                elif d < 0:
                    replaced["base replaced, better"] += 1
                    replaced_bytes["base replaced, better"] += d
        total = sum(r["body_bytes"] for r in lane.values()) - sum(r["body_bytes"] for r in b.values())
        print("   %s" % label)
        print("      objects only in this arm %d (body %d)   objects only in the default %d" % (
            len(only), sum(lane[o]["body_bytes"] for o in only), len(gone)))
        print("      full -> delta (new base found): %6d objects  %+d B" % (gain_full_to_delta, gain_bytes))
        print("      delta -> full (base dropped):   %6d objects  %+d B" % (lose_delta_to_full, lose_bytes))
        print("      base replaced:                  %6d objects  %+d B" % (
            replaced["base replaced"], replaced_bytes["base replaced"]))
        print("         of which WORSE:              %6d objects  %+d B" % (
            replaced["base replaced, WORSE"], replaced_bytes["base replaced, WORSE"]))
        print("         of which better:             %6d objects  %+d B" % (
            replaced["base replaced, better"], replaced_bytes["base replaced, better"]))
        print("      lane delta %+d" % total)
        print("      closure %+d" % (gain_bytes + lose_bytes + replaced_bytes["base replaced"]
                                    + sum(lane[o]["body_bytes"] for o in only)
                                    - sum(b[o]["body_bytes"] for o in gone)))


if __name__ == "__main__":
    main()
