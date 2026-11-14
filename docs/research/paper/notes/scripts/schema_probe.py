"""Print only the structure (keys, type tags, value types) of JSONL records read from stdin.
Never prints string values except short enum-like 'type'/'role'/'name'/'kind' tags (<=40 chars, no spaces)."""
import sys, json
from collections import Counter

TAGS = {"type", "role", "kind", "event", "name", "tool", "toolName", "tool_name", "status", "stop_reason", "subtype"}

def shape(o, depth=0):
    if isinstance(o, dict):
        if depth > 3:
            return "{...}"
        parts = []
        for k, v in o.items():
            if k in TAGS and isinstance(v, str) and len(v) <= 40 and " " not in v:
                parts.append(f"{k}={v}")
            else:
                parts.append(f"{k}:{shape(v, depth+1)}")
        return "{" + ",".join(parts) + "}"
    if isinstance(o, list):
        return "[" + (shape(o[0], depth+1) if o else "") + f"]x{len(o)}" if depth < 2 else "[..]"
    return type(o).__name__

c = Counter()
examples = {}
n = 0
for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    n += 1
    try:
        o = json.loads(line)
    except Exception:
        c["<nonjson>"] += 1
        continue
    s = shape(o)
    key = s[:200]
    c[key] += 1
    examples.setdefault(key, s[:1500])
print("records", n)
for k, v in c.most_common(40):
    print(v, examples[k])
    print()
