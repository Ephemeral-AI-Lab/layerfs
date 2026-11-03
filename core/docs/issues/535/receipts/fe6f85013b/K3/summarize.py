# Tabulate the K3 cell receipts in this folder: one row per receipt.
# usage: python3 summarize.py [folder]
import glob
import json
import os
import re
import sys

folder = sys.argv[1] if len(sys.argv) > 1 else os.path.dirname(os.path.abspath(__file__))
print("| Receipt | Verdict | Store (host) | Failed step | Mount | Exec | Commit (kind) | Unmount | Load |")
print("| --- | --- | --- | --- | ---: | ---: | ---: | ---: | ---: |")
counts = {}
for path in sorted(glob.glob(os.path.join(folder, "*.json"))):
    r = json.load(open(path))
    if r.get("format") != "lfs533-cell/1":
        continue
    name = os.path.basename(path)[:-5]
    t = r.get("phases", {}).get("timed") or {}
    commit = t.get("commit") or {}
    kind = commit.get("kind") or commit.get("outcome") or ("Committed" if commit.get("commit") else "-")
    store = r.get("store", {})
    where = store.get("kind", "?") + (" (" + store["host"] + ")" if store.get("host") else "")
    verdict = "PASS" if r.get("ok") else "FAIL"
    group = re.sub(r"-C\d\d", "", name)
    counts.setdefault(group, [0, 0])[0 if r.get("ok") else 1] += 1
    def ms(v):
        return "-" if v is None else "%.1f" % v
    print("| %s | %s | %s | %s | %s | %s | %s (%s) | %s | %.1f |" % (
        name, verdict, where, r.get("failedStep") or "-", ms(t.get("mountMs")),
        ms((t.get("exec") or {}).get("wallMs")), ms(t.get("commitMs")), kind, ms(t.get("unmountMs")), r.get("load1", 0)))
print()
for group, (ok, bad) in sorted(counts.items()):
    print("%s: PASS %d FAIL %d" % (group, ok, bad))
