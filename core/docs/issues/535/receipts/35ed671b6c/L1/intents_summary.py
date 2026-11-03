"""L1 (from K1) item 7: one line per kill from intents.sh's logs: the point hit, the
entity record's placement and op, the copies on both sides, the Sandbox's
runtime left, the entity's doctor findings, and whether the reads changed
any file.   python3 intents_summary.py <out dir>"""
import json, os, re, sys
O = sys.argv[1]
names = sorted({f.split(".")[0] for f in os.listdir(O) if f.endswith(".records.log")}, key=lambda n: (n[:2], os.path.getmtime(os.path.join(O, n + ".records.log"))))
for n in names:
    ev = open(os.path.join(O, n + ".events.log")).read()
    hit = re.findall(r"SIGKILL (.*)", ev)
    phases = re.findall(r'progress \{"phase": "(\w+)"', ev)
    rec = open(os.path.join(O, n + ".records.log")).read()
    kind = "sandbox" if n.startswith("k-") else "store"
    m = re.search(rf"--- {kind}/{re.escape(n)}/record.json\n(.*)\n", rec)
    r = json.loads(m.group(1)) if m else None
    op = (r or {}).get("op")
    home = rec.split("--- home files")[1].split("\n---")[0].split()
    worker = rec.split("--- worker /layerfs/data")[1].split("\n---")[0].split()
    mine = lambda xs: [x for x in xs if f"/{n}/" in x and ("store.sqlite" in x or "record.json" in x and kind == "sandbox" or "/open/" in x)]
    reads = open(os.path.join(O, n + ".reads.log")).read()
    doc = re.search(r"\$ layerfs doctor\n(\{.*\})", reads)
    finds = [f for f in json.loads(doc.group(1))["findings"] if f["entity"]["name"] == n] if doc else []
    changed = open(os.path.join(O, n + ".before-reads")).read() != open(os.path.join(O, n + ".after-reads")).read()
    daemons = rec.split("--- native daemons")[1].split("--- containers")[0].strip()
    cont = rec.split("--- containers")[1].strip()
    print(f"## {n}: hit {hit[0] if hit else 'NO KILL'}; phases seen {list(dict.fromkeys(phases))}")
    if r is None:
        print("   record: none")
    else:
        print(f"   record: machine={r.get('machine')} op={json.dumps({k: op[k] for k in ('name','phase','uncertain')}) if op else None} state-fields={ {k: r[k] for k in ('runtime_id','id') if k in r} }")
    print(f"   home: {mine(home)}")
    print(f"   worker: {mine(worker)}")
    if kind == "sandbox":
        print(f"   native daemon procs: {daemons.count(chr(10)) + 1 if daemons else 0}; containers: {[c for c in cont.splitlines() if n in c]}")
    print(f"   doctor: {[(f['severity'], f['message'][:150], f['fix'][:120]) for f in finds]}")
    print(f"   reads changed files: {changed}")
