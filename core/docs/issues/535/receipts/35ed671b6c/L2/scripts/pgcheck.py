"""Each PostgreSQL cell's timed Commit id (or for an UpToDate cell its head)
looked up in that cell's own database on the lane's container (lane L2,
method of K2's pg-commits-in-own-database.txt). usage: pgcheck.py <receipt folder>"""

import glob
import json
import os
import subprocess
import sys
from datetime import datetime, timezone

root = sys.argv[1]
prefix = {("apple-container", "pg-loop"): "a", ("apple-container", "pg-remote"): "ar",
          ("microsandbox", "pg-loop"): "m", ("microsandbox", "pg-remote"): "mr",
          ("ssh", "pg-loop"): "s", ("ssh", "pg-remote"): "sr"}
print("# Each PostgreSQL cell's timed Commit (UpToDate: the Branch head) found in that cell's own database (lane L2)")
print(f"# check: select count(*) from layerfs.commit where encode(commit_id,'hex') = '<receipt id>' in <db>; at {datetime.now(timezone.utc).strftime('%Y-%m-%dT%H:%M:%SZ')}")
for f in sorted(glob.glob(os.path.join(root, "*-pg-*-C??.json"))):
    name = os.path.basename(f)[:-5]
    platform, rest = name.split("-pg-")
    store, cell = rest.rsplit("-", 1)
    d = json.load(open(f))
    db = prefix[(platform, "pg-" + store)] + cell.lower()
    if not d.get("ok"):
        print(f"{platform}-pg-{store} {cell} db={db} not PASS: {(d.get('cause') or '')[:120]}")
        continue
    c = d["phases"]["timed"]["commit"] or {}
    cid = (c.get("commit") or {}).get("id") or c.get("head")
    if not cid:
        print(f"{platform}-pg-{store} {cell} db={db} {c.get('kind')}  found=(no id: UpToDate with no head)")
        continue
    sql = f"select count(*) from layerfs.commit where encode(commit_id,'hex') = '{cid}'"
    n = subprocess.run(["docker", "exec", "lfs535l2-pg", "psql", "-U", "postgres", "-d", db, "-tAc", sql], capture_output=True, text=True).stdout.strip()
    print(f"{platform}-pg-{store} {cell} db={db} {c.get('kind')} {cid[:16]} found={n}")
