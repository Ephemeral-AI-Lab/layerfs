#!/usr/bin/env python3
import re
EV = "/Users/yifanxu/Ephemeral-AI-Lab/layerfs/docs/roadmap/0.1/0.1.7/evidence/stages-1-5-review-20260917T230700Z"
text = open(EV + "/cargo-test.log").read()
lines = text.splitlines()
target = None
rows = []
for line in lines:
    m = re.match(r"\s+Running (\S+) \((.*)\)", line)
    if m:
        target = m.group(2).split("/")[-1]
        continue
    m = re.match(r"\s+Docs?tests? (\S+)", line)
    if m:
        target = "doc:" + m.group(1)
        continue
    m = re.match(r"test result: (\w+)\. (\d+) passed; (\d+) failed; (\d+) ignored", line)
    if m:
        rows.append((target, m.group(1), int(m.group(2)), int(m.group(3)), int(m.group(4))))
print("%-46s %s" % ("target", "passed"))
fs = 0
for r in rows:
    if r[0] and r[0].startswith("filesystem"):
        print("%-46s %d" % (r[0], r[2]))
        fs += r[2]
print("stage-5 named filesystem_* targets: %d, tests: %d" % (len([r for r in rows if r[0] and r[0].startswith("filesystem")]), fs))
print("total targets %d, tests %d" % (len(rows), sum(r[2] for r in rows)))
