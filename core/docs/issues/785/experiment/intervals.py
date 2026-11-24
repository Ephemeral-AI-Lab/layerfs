"""Final saving fractions and 95% bootstrap intervals from the per-sample results.

Usage: intervals.py OUT_DIR  (reads per_*.npz/npy written by the experiment scripts)
"""
import json
import sys

import numpy as np

OUT = sys.argv[1]
FULL_SAMPLE, PREFIX_SAMPLE = 16889634, 8987764  # sampled record bytes (exp_* outputs)
FULL_STORED, PREFIX_STORED = 2071395695, 2466859715  # all independent / delta chunk records
PACKS = 4754953132
rng = np.random.default_rng(1)


def ci(per, sample, population):
    f = per.sum() / sample
    boots = [per[rng.integers(0, len(per), len(per))].sum() / sample for _ in range(2000)]
    lo, hi = np.percentile(boots, [2.5, 97.5])
    gb = [x * population for x in (f, lo, hi)]
    return {"fraction": round(f, 4), "fraction_95": [round(lo, 4), round(hi, 4)],
            "bytes": int(gb[0]), "bytes_95": [int(gb[1]), int(gb[2])], "share_of_packs": round(gb[0] / PACKS, 4),
            "samples_improved": int((per > 0).sum())}


res = {}
for mode in ("earlier", "any"):
    z = np.load(f"{OUT}/per_chunk_{mode}.npz")
    for k in z.files:
        res[f"chunk_{mode}_{k}"] = ci(z[k], FULL_SAMPLE, FULL_STORED)
    res[f"file_{mode}"] = ci(np.load(f"{OUT}/per_file_{mode}.npy"), FULL_SAMPLE, FULL_STORED)
for w in ("None", "65536", "65536_fullonly"):
    z = np.load(f"{OUT}/per_policy_{w}.npz")
    res[f"policy_{w}_full_rule_b"] = ci(z["full_rule_b"], FULL_SAMPLE, FULL_STORED)
    res[f"policy_{w}_full_rule_a_conservative"] = ci(z["full_rule_a_cons"], FULL_SAMPLE, FULL_STORED)
    res[f"policy_{w}_prefix_rule_a"] = ci(z["prefix_rule_a"], PREFIX_SAMPLE, PREFIX_STORED)
print(json.dumps(res, indent=1))
