"""Structure-only: across all session files, count tool-related event shapes per file version.
Prints keys and enum tags only."""
import glob, os, subprocess, json, sys
from collections import Counter, defaultdict
sys.path.insert(0, os.path.dirname(__file__))
ROOT = os.path.expanduser("~/.dsh/sessions")
shapes = defaultdict(Counter)

def shape(o, depth=0):
    if isinstance(o, dict):
        if depth > 3:
            return "{..}"
        return "{" + ",".join(
            (f"{k}={v}" if k in ("type", "role", "kind") and isinstance(v, str) and len(v) < 40 else f"{k}:{shape(v, depth+1)}")
            for k, v in o.items()) + "}"
    if isinstance(o, list):
        return "[..]"
    return type(o).__name__

for f in glob.glob(f"{ROOT}/*/*/session*.jsonl.zstd"):
    ver = os.path.basename(f)
    out = subprocess.run(["zstd", "-dc", f], capture_output=True, timeout=60).stdout
    for line in out.splitlines():
        try:
            o = json.loads(line)
        except Exception:
            continue
        t = o.get("type", "")
        if "tool" in t or (t == "assistant/chunk" and "tool" in json.dumps((o.get("data") or {}).get("chunk", {}).get("type", ""))):
            shapes[ver][shape(o)[:400]] += 1
for ver, c in shapes.items():
    print("==", ver)
    for k, v in c.most_common(8):
        print(v, k)
