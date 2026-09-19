#!/usr/bin/env python3
"""V2: exact content-level attribution of the Native lane's raw bytes to classes."""
import json, collections
from pathlib import Path
CORPUS = Path("/Users/yifanxu/Ephemeral-AI-Lab/deepseek-history-data")
T = 131_072
SELECTION = tuple(sorted(set(range(1,158,10)) | {157}))

manifest = json.loads((CORPUS/"checkpoint-manifest.json").read_bytes())
cps = manifest["checkpoints"]

def load_state(i):
    cp = cps[i-1]
    oracle = json.loads((CORPUS/"oracles"/f"{cp['sha']}.json").read_bytes())
    return {p:(int(s),int(m),str(d)) for p,(m,s,d) in oracle.items() if m.startswith("100")}

states = {i: load_state(i) for i in SELECTION}

# every chunked path-state that is a CHANGE, with its class
events = []   # (state_index, path, size, sha, class)
for pos, i in enumerate(SELECTION):
    st = states[i]
    prev = states[SELECTION[pos-1]] if pos else None
    for p,(size,mode,sha) in st.items():
        if size < T: continue
        if prev is None or p not in prev:
            cls = "first_appearance"
        else:
            psize, _, psha = prev[p]
            if psize < T:
                cls = "crossing_small_to_large"
            elif psha != sha:
                cls = "in_place_chunk_edit"
            else:
                cls = "unchanged"
        events.append((i,p,size,sha,cls))

by_class = collections.Counter(); bytes_by_class = collections.Counter()
for i,p,size,sha,cls in events:
    by_class[cls]+=1; bytes_by_class[cls]+=size

print("== path-state classes (per-state sums) ==")
for c in sorted(by_class):
    print(f"  {c:28s} {by_class[c]:5d}  {bytes_by_class[c]:12,d} B")

# dedupe by content sha256; attribute each distinct content to its FIRST class
first_class = {}
for i,p,size,sha,cls in sorted(events, key=lambda e:(e[0],)):
    if sha not in first_class:
        first_class[sha] = (cls, size, i, p)

dedup_class = collections.Counter(); dedup_bytes = collections.Counter()
for sha,(cls,size,i,p) in first_class.items():
    dedup_class[cls]+=1; dedup_bytes[cls]+=size

print()
print("== distinct contents, attributed to the class that FIRST produced them ==")
tot=0
for c in sorted(dedup_class):
    print(f"  {c:28s} {dedup_class[c]:5d}  {dedup_bytes[c]:12,d} B")
    tot += dedup_bytes[c]
print(f"  {'TOTAL':28s} {sum(dedup_class.values()):5d}  {tot:12,d} B")
print()
print("sum of per-state change bytes :", sum(bytes_by_class[c] for c in by_class if c!='unchanged'))
print("distinct content bytes        :", tot)
print("Store chunk raw (measured)    :", 22_032_441)
print("residual (distinct - store)   :", tot - 22_032_441)
print("unchanged path-states         :", by_class['unchanged'])
print("duplicate re-stores avoided   :", sum(bytes_by_class[c] for c in by_class if c!='unchanged') - tot)
