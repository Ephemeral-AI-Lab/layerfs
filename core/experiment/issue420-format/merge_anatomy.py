#!/usr/bin/env python3
"""Merge anatomy slices into one table (bytes by role)."""
import glob, json, sys
from collections import defaultdict
d = sys.argv[1]
tot = defaultdict(float); roles = defaultdict(lambda: defaultdict(float))
for f in sorted(glob.glob(f"{d}/slice*.json")):
    j = json.load(open(f))
    for k, v in j["totals"].items(): tot[k] += v
    for r, m in j["roles"].items():
        for k, v in m.items(): roles[r][k] += v
out = {"totals": tot, "roles": roles}
json.dump(out, open(f"{d}/anatomy.json", "w"), indent=1)
print("totals:", {k: round(v) for k, v in sorted(tot.items())})
print(f"{'role':16}{'records':>10}{'canonical':>15}{'rec_bytes':>13}{'stored':>13}{'full':>9}{'delta':>9}{'stored%':>8}{'baseid':>11}")
T = sum(m["stored_alloc"] for m in roles.values())
for r, m in sorted(roles.items(), key=lambda x: -x[1]["stored_alloc"]):
    print(f"{r:16}{m['records']:>10.0f}{m['canonical']:>15.0f}{m['record_bytes']:>13.0f}{m['stored_alloc']:>13.0f}{m.get('tag0',0)+m.get('tag2',0):>9.0f}{m.get('tag1',0):>9.0f}{100*m['stored_alloc']/T:>8.2f}{m['base_id_alloc']:>11.0f}")
print("records stored total", round(T))
