"""L1 (from K1) item 5 (binding parity): compare the Node runner's native sqlite
receipts with parity_cell.py's Python and CLI receipts, cell by cell.
    python3 parity_compare.py <receipt folder>
Compared: verdict, step names in order, timed exec exit code and body exit
code, timed Commit kind, setup Commit kind, verifier output, the identity
(`where`) and the body hash. Timings are not compared."""
import json, os, sys
R = sys.argv[1]
def load(p):
    return json.load(open(os.path.join(R, p)))
def facts(d):
    t = d["phases"].get("timed", {})
    c = t.get("commit") or {}
    s = (d["phases"].get("setup") or {}).get("commit") or {}
    return {
        "ok": d["ok"],
        "steps": [x["name"] for x in d["steps"]],
        "exec": (t.get("exec") or {}).get("exitCode"),
        "rc": (t.get("exec") or {}).get("rc"),
        "commit": c.get("kind") or c.get("outcome"),
        "setupCommit": s.get("kind") if s else None,
        "verify": (d["phases"].get("verify") or {}).get("stdout", "").strip(),
        "where": d.get("where"),
        "body": d["body"]["sha256"],
    }
bad = 0
for cell in ("C01", "C05", "C09", "C12"):
    rows = {"node": facts(load(f"native-sqlite-{cell}.json")), "python": facts(load(f"parity-native-python-{cell}.json")), "cli": facts(load(f"parity-native-cli-{cell}.json"))}
    same = all(rows[b] == rows["node"] for b in rows)
    bad += not same
    print(f"{cell}: {'SAME' if same else 'DIFFERENT'} ok={[rows[b]['ok'] for b in rows]} commit={rows['node']['commit']} setup={rows['node']['setupCommit']} verify={rows['node']['verify']!r} steps={len(rows['node']['steps'])}")
    if not same:
        for b in ("python", "cli"):
            for k in rows["node"]:
                if rows[b][k] != rows["node"][k]:
                    print(f"   {b}.{k}: {rows[b][k]!r} vs node {rows['node'][k]!r}")
print("PARITY", "PASS" if bad == 0 else "FAIL")
