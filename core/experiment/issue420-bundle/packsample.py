"""#420: compressibility of pack bodies (every Nth pack, by domain), zstd levels.
Usage: python3 -I packsample.py FIXTURE N"""
import sqlite3, sys, time
from compression import zstd
db = sqlite3.connect(f"file:{sys.argv[1]}?mode=ro&immutable=1", uri=True)
n = int(sys.argv[2])
acc = {}
t = time.time()
for pid, dom, body in db.execute("SELECT pack_id, domain, body FROM pack WHERE pack_id % ? = 0", (n,)):
    a = acc.setdefault(dom, {"packs": 0, "raw": 0, 1: 0, 3: 0, 9: 0, 19: 0})
    a["packs"] += 1; a["raw"] += len(body)
    for lv in (1, 3, 9, 19):
        a[lv] += min(len(zstd.compress(body, lv)), len(body))
for d, a in sorted(acc.items()):
    print("domain", d, a, {lv: round(a[lv] / a["raw"], 4) for lv in (1, 3, 9, 19)})
print("secs", round(time.time() - t, 1))
