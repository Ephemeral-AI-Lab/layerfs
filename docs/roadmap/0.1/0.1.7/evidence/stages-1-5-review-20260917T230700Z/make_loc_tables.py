#!/usr/bin/env python3
"""Review tool: per-file and per-directory production LOC tables from loc-*-files.txt."""
import os, sys, collections

EV = "/Users/yifanxu/Ephemeral-AI-Lab/layerfs/docs/roadmap/0.1/0.1.7/evidence/stages-1-5-review-20260917T230700Z"
SNAPS = ["pre1", "pre5", "rev"]

def parse(snap):
    table = {}
    for line in open(os.path.join(EV, "loc-%s-files.txt" % snap)):
        line = line.rstrip("\n")
        if not line or line.startswith("path "):
            continue
        parts = line.split()
        if len(parts) < 4:
            continue
        table[" ".join(parts[:-3])] = {
            "scope": parts[-3],
            "prod": int(parts[-2]),
            "phys": int(parts[-1]),
        }
    return table

maps = {s: parse(s) for s in SNAPS}
paths = sorted(set().union(*[set(m) for m in maps.values()]))

def g(snap, path, key):
    return maps[snap][path][key] if path in maps[snap] else 0

rows = []
for path in paths:
    rows.append({
        "path": path,
        "scope": (maps["rev"].get(path) or maps["pre5"].get(path) or maps["pre1"].get(path))["scope"],
        "pre1": g("pre1", path, "prod"),
        "pre5": g("pre5", path, "prod"),
        "rev": g("rev", path, "prod"),
        "phys": g("rev", path, "phys"),
    })

def emit_table(sel, title):
    print("\n### " + title)
    print("%-64s %8s %8s %8s %9s %8s %7s" % ("path", "pre1", "pre5", "rev", "cum", "s5", "phys"))
    tot = dict(pre1=0, pre5=0, rev=0, phys=0)
    for row in sel:
        print("%-64s %8d %8d %8d %+9d %+8d %7d" % (
            row["path"], row["pre1"], row["pre5"], row["rev"],
            row["rev"] - row["pre1"], row["rev"] - row["pre5"], row["phys"]))
        tot["pre1"] += row["pre1"]; tot["pre5"] += row["pre5"]
        tot["rev"] += row["rev"]; tot["phys"] += row["phys"]
    print("%-64s %8d %8d %8d %+9d %+8d %7d" % (
        "TOTAL %d files" % len(sel), tot["pre1"], tot["pre5"], tot["rev"],
        tot["rev"] - tot["pre1"], tot["rev"] - tot["pre5"], tot["phys"]))

core = [r for r in rows if r["scope"] == "core"]
emit_table([r for r in core if r["path"].startswith("core/crates/layerfs-content")],
           "C1 layerfs-content (production LOC)")
emit_table([r for r in core if r["path"].startswith("core/crates/layerfs-storage")],
           "C2 layerfs-storage (production LOC, includes sql/schema.sql)")

print("\n### Directory rollups, core (recursive, children included once)")
dirs = collections.defaultdict(lambda: dict(files=0, pre1=0, pre5=0, rev=0, phys=0))
for row in core:
    parts = row["path"].split("/")
    for index in range(1, len(parts)):
        key = "/".join(parts[:index])
        entry = dirs[key]
        entry["files"] += 1
        entry["pre1"] += row["pre1"]; entry["pre5"] += row["pre5"]
        entry["rev"] += row["rev"]; entry["phys"] += row["phys"]
print("%-56s %5s %8s %8s %8s %9s %8s" % ("directory", "files", "pre1", "pre5", "rev", "cum", "s5"))
for key in sorted(dirs):
    if key.count("/") > 3:
        continue
    e = dirs[key]
    print("%-56s %5d %8d %8d %8d %+9d %+8d" % (
        key, e["files"], e["pre1"], e["pre5"], e["rev"], e["rev"] - e["pre1"], e["rev"] - e["pre5"]))

print("\n### Reference scope")
ref = [r for r in rows if r["scope"] == "reference"]
print("reference files %d pre1 %d pre5 %d rev %d cum %+d s5 %+d" % (
    len(ref), sum(r["pre1"] for r in ref), sum(r["pre5"] for r in ref), sum(r["rev"] for r in ref),
    sum(r["rev"] - r["pre1"] for r in ref), sum(r["rev"] - r["pre5"] for r in ref)))
print("\n### Combined")
print("combined pre1 %d pre5 %d rev %d cum %+d s5 %+d" % (
    sum(r["pre1"] for r in rows), sum(r["pre5"] for r in rows), sum(r["rev"] for r in rows),
    sum(r["rev"] - r["pre1"] for r in rows), sum(r["rev"] - r["pre5"] for r in rows)))
