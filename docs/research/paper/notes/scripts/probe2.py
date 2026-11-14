"""Structure-only probe: stream element shapes in assistant/message, tool names in request/header,
and ordering of events within one step. Prints no content strings."""
import sys, json
from collections import Counter

shapes = Counter()
tools = Counter()
order = Counter()
cur = []
for line in sys.stdin:
    try:
        o = json.loads(line)
    except Exception:
        continue
    t = o.get("type")
    d = o.get("data") or {}
    if t == "assistant/message":
        for s in d.get("stream", [])[:50]:
            if isinstance(s, dict):
                shapes[tuple(sorted((k, type(v).__name__ if k not in ("type", "kind") else v) for k, v in s.items()))] += 1
    if t == "request/header":
        for tl in d.get("header", {}).get("tools", []):
            if isinstance(tl, dict):
                tools[tl.get("name") or (tl.get("function") or {}).get("name")] += 1
    if t == "step/start":
        cur = ["start"]
    elif t in ("assistant/message", "tool/call", "tool/result", "assistant/attempt"):
        cur.append(t.split("/")[1])
    elif t == "step/end":
        cur.append("end")
        order[" ".join(cur)[:120]] += 1
for k, v in shapes.most_common(15):
    print(v, k)
print(tools)
for k, v in order.most_common(10):
    print(v, k)
