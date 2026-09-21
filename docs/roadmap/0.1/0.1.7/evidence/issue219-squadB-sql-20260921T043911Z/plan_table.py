#!/usr/bin/env python3
"""Condense explain.txt into a per-statement plan table (base vs sample)."""
import re, sys, collections
text = open(sys.argv[1]).read().splitlines()
db = None
label = None
plans = collections.defaultdict(dict)
cur = []
def flush():
    global cur, label, db
    if label and db:
        plans[label][db] = [l for l in cur if l.strip()]
    cur = []
for line in text:
    m = re.match(r"### DATABASE = (\S+)", line)
    if m:
        flush(); db = m.group(1); label = None; continue
    m = re.match(r"===== (\S+) \| (.*) =====$", line)
    if m:
        flush(); label = m.group(1) + " | " + m.group(2); continue
    if line.startswith("===") or line.startswith("###") or line.startswith("#"):
        continue
    if label and db:
        if line.strip() in ("QUERY PLAN",):
            continue
        cur.append(line)
flush()
keys = list(plans.keys())
print("%-58s %s" % ("STATEMENT", "PLAN (sample.sqlite)"))
print("-"*130)
for k in keys:
    p = plans[k].get("sample.sqlite", [])
    b = plans[k].get("base.sqlite", [])
    same = (p == b)
    print("%-58s %s%s" % (k.split(" | ")[0], " || ".join(x.strip() for x in p), "" if same else "   [DIFFERS FROM BASE]"))
    if not same:
        print("%-58s   base: %s" % ("", " || ".join(x.strip() for x in b)))
print()
print("=== flags ===")
for k in keys:
    p = " ".join(plans[k].get("sample.sqlite", []))
    flags = []
    if "SCAN" in p and "SEARCH" not in p: flags.append("FULL SCAN ONLY")
    if "SCAN" in p: flags.append("contains SCAN")
    if "TEMP B-TREE" in p.upper(): flags.append("TEMP B-TREE")
    if "CORRELATED" in p.upper(): flags.append("CORRELATED")
    if "CO-ROUTINE" in p.upper(): flags.append("CO-ROUTINE")
    if flags:
        print("%-50s %s" % (k.split(" | ")[0], "; ".join(flags)))
