# Count the worker-connection trace lines (LAYERFS_TRACE, #535 item 6 R1) of
# each cell: one line per connection to a machine's worker, by op.
# usage: python3 traces.py [folder]  (reads <folder>/trace/*.jsonl)
import collections
import glob
import json
import os
import sys

folder = sys.argv[1] if len(sys.argv) > 1 else os.path.dirname(os.path.abspath(__file__))
groups = collections.OrderedDict()
for path in sorted(glob.glob(os.path.join(folder, "trace", "*.jsonl"))):
    name = os.path.basename(path)[:-6]
    ops = collections.Counter()
    bad = 0
    for line in open(path):
        if not line.strip():
            continue
        v = json.loads(line)
        if v.get("event") != "worker.connection" or not all(k in v for k in ("at", "op", "worker", "sent", "ms")):
            bad += 1
        ops[v.get("op")] += 1
    print("%s: %d lines, malformed %d, ops %s" % (name, sum(ops.values()), bad, dict(sorted(ops.items()))))
