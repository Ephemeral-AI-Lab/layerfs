"""Aggregate CLI lifecycle phase times from the 2026-11-06 six-suite reports at 76a32a9a.

Read-only: parses markdown tables in layerfs-benchmark/result/2026-11-06/<suite>/76a32a9a*/pgr2-*/report.md.
Prints per (machine, database) median and min-max for each phase, over PASS rows only.
"""
import glob, statistics, sys, re

ROOT = sys.argv[1]
SUITES = ["pjdfstest", "fsx", "fsstress", "fio", "smallfile", "mdtest", "fs-bench-quick"]
PHASES = ["Create ms", "Mount ms", "Exec ms", "Commit ms", "Unmount ms", "Close ms", "Store release ms", "End-to-end ms"]

rows = []
for s in SUITES:
    for f in glob.glob(f"{ROOT}/result/2026-11-06/{s}/76a32a9a*/pgr2-*/report.md"):
        header = None
        case = None
        for line in open(f):
            line = line.rstrip("\n")
            m = re.match(r"^## ([A-Z]\d\d)\b", line)
            if m:
                case = m.group(1)
                header = None
                continue
            if line.startswith("| Machine | Database | Execution"):
                header = [c.strip() for c in line.strip("|").split("|")]
                continue
            if header and line.startswith("| ---"):
                continue
            if header and line.startswith("| "):
                cells = [c.strip() for c in line.strip("|").split("|")]
                if len(cells) != len(header):
                    header = None
                    continue
                d = dict(zip(header, cells))
                d["suite"] = s
                d["case"] = case
                rows.append(d)
            elif header:
                header = None

def num(x):
    try:
        return float(x.replace(",", ""))
    except Exception:
        return None

print(f"rows parsed: {len(rows)}; PASS: {sum(r['Execution']=='PASS' for r in rows)}")
from collections import defaultdict
agg = defaultdict(lambda: defaultdict(list))
for r in rows:
    if r["Execution"] != "PASS":
        continue
    key = (r["Machine"], r["Database"])
    for p in PHASES:
        v = num(r.get(p, ""))
        if v is not None:
            agg[key][p].append(v)
    agg[key]["_n"].append(1)

print("| Machine | DB | n PASS rows | " + " | ".join(p.replace(" ms", "") + " median (min–max)" for p in PHASES if p != "Exec ms" and p != "End-to-end ms") + " |")
for key in sorted(agg):
    d = agg[key]
    out = []
    for p in PHASES:
        if p in ("Exec ms", "End-to-end ms"):
            continue
        v = d[p]
        out.append(f"{statistics.median(v):.1f} ({min(v):.1f}–{max(v):.1f})")
    print(f"| {key[0]} | {key[1]} | {len(d['_n'])} | " + " | ".join(out) + " |")

# per suite counts
from collections import Counter
c = Counter((r["suite"], r["Execution"]) for r in rows)
print(sorted(c.items()))
