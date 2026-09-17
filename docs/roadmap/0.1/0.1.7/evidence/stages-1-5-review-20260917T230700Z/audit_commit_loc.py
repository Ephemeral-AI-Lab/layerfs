#!/usr/bin/env python3
"""Review tool: audit per-commit Production LOC disclosure on the Stage 1-5 range."""
import subprocess, re, json, sys

REPO = "/Users/yifanxu/Ephemeral-AI-Lab/layerfs"
EV = "/Users/yifanxu/Ephemeral-AI-Lab/layerfs/docs/roadmap/0.1/0.1.7/evidence/stages-1-5-review-20260917T230700Z"
rng = sys.argv[1] if len(sys.argv) > 1 else "579831eb6..HEAD"
out = subprocess.run(["git", "-C", REPO, "log", "--reverse", "--format=%H%x01%h%x01%s%x01%B%x02", rng],
                     capture_output=True, text=True).stdout
commits = []
for chunk in out.split("\x02"):
    chunk = chunk.strip("\n")
    if not chunk:
        continue
    parts = chunk.split("\x01")
    if len(parts) < 4:
        continue
    full, short, subj = parts[0], parts[1], parts[2]
    body = "\x01".join(parts[3:])
    m = re.search(r"Production LOC:\s*([\d,]+)\s*->\s*([\d,]+)\s*\(delta\s*([+-]?[\d,]+)\)", body)
    commits.append(dict(full=full, short=short, subject=subj,
                        loc=[m.group(1), m.group(2), m.group(3)] if m else None))
print("commits since pre1:", len(commits))
missing = [c for c in commits if not c["loc"]]
print("commits without Production LOC line:", len(missing))
for c in missing:
    print("   MISSING", c["short"], c["subject"][:90])
# chain consistency: after(n) == before(n+1) for the ones that disclose
prev = None
breaks = []
for c in commits:
    if not c["loc"]:
        continue
    before = int(c["loc"][0].replace(",", ""))
    after = int(c["loc"][1].replace(",", ""))
    delta = int(c["loc"][2].replace(",", "").replace("+", ""))
    if after - before != delta:
        breaks.append((c["short"], "delta mismatch", before, after, delta))
    if prev is not None and before != prev[1]:
        breaks.append((c["short"], "chain break", prev[0], prev[1], before))
    prev = (c["short"], after)
print("disclosed:", len(commits) - len(missing), "chained-check breaks:", len(breaks))
for b in breaks[:40]:
    print("   ", b)
json.dump(commits, open(EV + "/commit-loc.json", "w"), indent=1)
