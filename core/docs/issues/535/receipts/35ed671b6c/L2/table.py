"""Markdown rows from the L2 cell receipts (copied from K2) (Node runner and parity drivers).
usage: table.py <receipt folder>"""

import glob
import json
import os
import sys


def ms(d, name):
    for s in d.get("steps", []):
        if s["name"] == name:
            return s.get("ms")
    return None


root = sys.argv[1]
files = sorted(glob.glob(os.path.join(root, "*-C??.json")) + glob.glob(os.path.join(root, "parity", "*-C??-*.json")))
print("| Receipt | Verdict | timed exec ms | Commit ms (kind) | push ms | pull ms | load1 | started (UTC) | first error |")
print("| --- | --- | ---: | --- | ---: | ---: | ---: | --- | --- |")
for f in files:
    d = json.load(open(f))
    t = d.get("phases", {}).get("timed", {})
    kind = (t.get("commit") or {}).get("kind")
    commit = f"{t.get('commitMs')} ({kind})" if t else "-"
    err = "" if d["ok"] else (d.get("cause") or "")[:140].replace("|", "/")
    name = os.path.relpath(f, root)[:-5]
    print(
        f"| {name} | {'PASS' if d['ok'] else 'FAIL'} | {t.get('exec', {}).get('wallMs', '-')} | {commit} | "
        f"{ms(d, 'store.push') or '-'} | {ms(d, 'store.pull') or '-'} | {round(d['load1'], 1)} | {d['startedAt'][11:19]} | {err} |"
    )
