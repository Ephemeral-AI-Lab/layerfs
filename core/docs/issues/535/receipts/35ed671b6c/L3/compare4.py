# item 4 (Linux part): compare `machine platforms` (from item4.log) with
# connect/SUPPORT.json's linux rows, field by field, and the three create
# refusals with the table's "refusal" text. usage: python3 compare4.py <item4.log> <SUPPORT.json>
import json, sys
log = open(sys.argv[1]).read().split("\n")
sup = json.load(open(sys.argv[2]))
obs = next(json.loads(l) for l in log if l.startswith('{"host"'))
row = next(h for h in sup["hosts"] if h["host"] == obs["host"])
print("observed host:", obs["host"])
o = {p["platform"]: p for p in obs["platforms"]}
t = {p["platform"]: p for p in row["platforms"]}
print("platform sets equal:", set(o) == set(t), sorted(o))
diffs = 0
for name, tp in t.items():
    op = o.get(name, {})
    for k, v in tp.items():
        if k in op and op[k] != v:
            diffs += 1
            print("DIFF", name, k, "observed", op[k], "table", v)
        elif k not in op:
            print("table-only field", name, k, "=", json.dumps(v)[:120])
    print(name, "ready=", op.get("ready"), "missing=", op.get("missing"), "version=", op.get("version"))
print("static-field differences:", diffs)
refusals = [json.loads(l) for l in log if l.startswith('{"code"')]
for r in refusals:
    plat = r["message"].split(" ")[0]
    tp = t.get(plat, {})
    want = [tp.get("postgresql", {}).get("reason")]
    print("create", plat, r["code"], repr(r["message"]), "table:", want, "equal:", r["message"] in want)
