"""L1 item 3 (#535 phase 8 item 4, macOS part): `layerfs machine platforms`
against connect/SUPPORT.json's macOS rows, and every runner receipt in the
folder against its machine's row.
    python3 platforms.py <live machine-platforms.json> <SUPPORT.json> <receipt folder>"""
import glob, json, os, sys
live, sup, folder = json.load(open(sys.argv[1])), json.load(open(sys.argv[2])), sys.argv[3]
rows = next(h for h in sup["hosts"] if h["host"] == "macos")["platforms"]
print("host:", live["host"])
names = [p["platform"] for p in live["platforms"]]
print("platform sets equal:", set(names) == {r["platform"] for r in rows}, names)
print("order equal:", names == [r["platform"] for r in rows])
bad = []
for r in rows:
    l = next(p for p in live["platforms"] if p["platform"] == r["platform"])
    bad += [(r["platform"], k) for k in r if l.get(k) != r[k]]
    extra = sorted(set(l) - set(r))
    print(f"  {r['platform']}: live-only {extra} ready={l.get('ready')} missing={l.get('missing')} version={l.get('version')!r}")
print("every SUPPORT.json field equal in the live answer:", not bad, bad)
print("\n## observed results on native and docker vs their rows")
row = {r["platform"]: r for r in rows}
for m in ("native", "docker"):
    r = row[m]
    print(f"  {m} row: backend={r['backend']} kernel={r['kernel']} sqlite={r['sqlite']} postgresql={r['postgresql']}")
allok = True
for f in sorted(glob.glob(os.path.join(folder, "*-C??.json"))):
    d = json.load(open(f))
    m = d["machine"]; r = row[m]; sb = d.get("sandbox") or {}
    st = d.get("store", {}).get("kind")
    pushed = any(s["name"] == "store.push" for s in d["steps"])
    ok = sb.get("backend") == r["backend"] and sb.get("route") == r["kernel"] and (st != "sqlite" or pushed == (r["sqlite"] == "worker")) and (st != "postgresql" or r["postgresql"]["sandbox"])
    allok &= ok and d["ok"]
    print(f"  {os.path.basename(f)}: backend={sb.get('backend')} route={sb.get('route')} store={st} pushed={pushed} ok={d['ok']} -> {'MATCH' if ok else 'MISMATCH'}")
print("ALL RECEIPTS MATCH THEIR ROWS:", allok)
