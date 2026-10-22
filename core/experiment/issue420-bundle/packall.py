"""#420: compressibility of every pack body at the given zstd levels, by domain
and size class. Usage: python3 -I packall.py FIXTURE LEVEL[,LEVEL] [MOD]
(MOD k: only packs with (pack_id*2654435761) % k == 0, a hash sample)."""
import sqlite3, sys, time
from compression import zstd
db = sqlite3.connect(f"file:{sys.argv[1]}?mode=ro&immutable=1", uri=True)
levels = [int(x) for x in sys.argv[2].split(",")]
mod = int(sys.argv[3]) if len(sys.argv) > 3 else 1
acc = {}
t = time.time()
for pid, dom, body in db.execute("SELECT pack_id, domain, body FROM pack"):
    if (pid * 2654435761) % mod:
        continue
    cls = "<=8K" if len(body) <= 8192 else "<=256K" if len(body) <= 262144 else ">256K"
    a = acc.setdefault((dom, cls), {"packs": 0, "raw": 0, **{lv: 0 for lv in levels}})
    a["packs"] += 1; a["raw"] += len(body)
    for lv in levels:
        a[lv] += min(len(zstd.compress(body, lv)), len(body))
tot = {"raw": 0, **{lv: 0 for lv in levels}}
for k, a in sorted(acc.items()):
    print(k, a, {lv: round(a[lv] / a["raw"], 4) for lv in levels})
    for x in tot: tot[x] += a[x]
print("total", tot, {lv: round(tot[lv] / tot["raw"], 4) for lv in levels}, "secs", round(time.time() - t, 1))
