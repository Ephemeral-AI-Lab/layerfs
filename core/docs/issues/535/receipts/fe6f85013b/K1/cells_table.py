"""K1: a Markdown table of the runner's receipts in a folder.
    python3 cells_table.py <folder> [prefix]"""
import glob, json, os, sys
rows = []
for f in sorted(glob.glob(os.path.join(sys.argv[1], (sys.argv[2] if len(sys.argv) > 2 else "") + "*-C??.json"))):
    d = json.load(open(f))
    t = d["phases"].get("timed", {})
    c = t.get("commit") or {}
    rows.append((os.path.basename(f)[:-5], "PASS" if d["ok"] else "FAIL", (t.get("exec") or {}).get("wallMs"), t.get("commitMs"), c.get("kind"), d["load1"], d["startedAt"][11:19], (d.get("cause") or "")[:120]))
print("| receipt | verdict | timed exec ms | Commit ms (kind) | load1 | start UTC | first error |")
print("| --- | --- | --- | --- | --- | --- | --- |")
for r in rows:
    print(f"| {r[0]} | {r[1]} | {r[2]} | {r[3]} ({r[4]}) | {r[5]:.2f} | {r[6]} | {r[7]} |")
print(f"\n{sum(r[1]=='PASS' for r in rows)} PASS, {sum(r[1]!='PASS' for r in rows)} FAIL of {len(rows)}")
