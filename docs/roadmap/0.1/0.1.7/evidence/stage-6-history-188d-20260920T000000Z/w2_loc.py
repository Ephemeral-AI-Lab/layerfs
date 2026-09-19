#!/usr/bin/env python3
"""Production LOC for core/crates: src/*.rs + sql/*.sql, non-blank, non-comment."""
import subprocess, sys
from pathlib import Path

ROOT = Path("/Users/yifanxu/Ephemeral-AI-Lab/layerfs")

def count(text, suffix):
    n = 0
    for line in text.splitlines():
        s = line.strip()
        if not s:
            continue
        if suffix == ".rs" and (s.startswith("//")):
            continue
        if suffix == ".sql" and s.startswith("--"):
            continue
        n += 1
    return n

def files():
    out = []
    for p in sorted(ROOT.glob("core/crates/*/src/**/*.rs")):
        out.append(p)
    for p in sorted(ROOT.glob("core/crates/*/sql/**/*.sql")):
        out.append(p)
    return out

def head_text(rel):
    try:
        return subprocess.run(["git", "-C", str(ROOT), "show", f"HEAD:{rel}"],
                              capture_output=True, text=True, check=True).stdout
    except subprocess.CalledProcessError:
        return None

tot_now = tot_head = 0
rows = []
MINE = {"layerfs-storage/src/encoding/delta/candidates.rs",
        "layerfs-storage/src/cas/lifecycle.rs",
        "layerfs-storage/src/cas/store.rs",
        "layerfs-storage/src/cas/owner.rs",
        "layerfs-storage/src/cas/selection.rs",
        "layerfs-storage/src/policy.rs",
        "layerfs-storage/src/sqlite/schema.rs",
        "layerfs-storage/sql/schema.sql"}
for p in files():
    rel = str(p.relative_to(ROOT / "core" / "crates"))
    now = count(p.read_text(), p.suffix)
    old_text = head_text("core/crates/" + rel)
    old = count(old_text, p.suffix) if old_text is not None else 0
    tot_now += now
    tot_head += old
    if rel in MINE:
        rows.append((rel, old, now, now - old))
print("core/crates production LOC  HEAD -> now:", tot_head, "->", tot_now, "(delta %+d)" % (tot_now - tot_head))
print()
print("my files (HEAD -> now):")
for rel, old, now, d in rows:
    print("  %-52s %5d -> %5d  (%+d)" % (rel, old, now, d))
print("  %-52s %5d -> %5d  (%+d)" % ("SUBTOTAL (my files)", sum(r[1] for r in rows), sum(r[2] for r in rows), sum(r[3] for r in rows)))
