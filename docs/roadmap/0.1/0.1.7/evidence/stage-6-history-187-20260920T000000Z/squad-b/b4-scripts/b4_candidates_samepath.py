
import json, collections
from pathlib import Path
C = Path('/Users/yifanxu/Ephemeral-AI-Lab/deepseek-history-data')
man = json.loads((C/'checkpoint-manifest.json').read_bytes()); cps = man['checkpoints']
sel = sorted(set(range(1,158,10)) | {157})
states=[]
for idx in sel:
    sha=cps[idx-1]['sha']
    t={}
    for line in (C/'inputs'/sha/'manifest.tsv').read_text().splitlines():
        if not line: continue
        mode,oid,size,hexpath=line.split('\t')
        t[bytes.fromhex(hexpath).decode('utf-8','surrogateescape')]=(mode,oid,int(size))
    states.append(t)
chains=collections.defaultdict(list)
for si,t in enumerate(states):
    for p,(m,o,sz) in t.items(): chains[p].append((si,o,sz))
# first emission order: (state, path) of first appearance of each oid
first={}
for si,t in enumerate(states):
    for p in sorted(t):
        o=t[p][1]
        if o not in first: first[o]=(si,p)
# candidate pairs (same path, earlier version)
pairs=0
cand=collections.defaultdict(set)
for p,v in chains.items():
    for j in range(len(v)):
        o=v[j][1]
        for k in range(j):
            cand[o].add(v[k][1])
    pairs += len(v)*(len(v)-1)//2
print('same-path ordered pairs', pairs, 'objects with >=1 candidate', len(cand))
sizes=[len(c) for c in cand.values()]
print('candidate count per object: max', max(sizes), 'mean %.2f'%(sum(sizes)/len(sizes)))
rows=json.load(open('/tmp/b4/objects.json'))
r1={r['oid']:r for r in rows if r['role']==1}
print('whole-file objects with >=1 same-path earlier version:', sum(1 for o in r1 if o in cand))
# immediate predecessor only
prev1={}
for p,v in chains.items():
    for j in range(1,len(v)):
        prev1.setdefault(v[j][1],set()).add(v[j-1][1])
print('objects with an immediate same-path predecessor:', sum(1 for o in r1 if o in prev1))
json.dump(dict(first={o:list(v) for o,v in first.items()},
               cand={o:sorted(v) for o,v in cand.items()},
               prev1={o:sorted(v) for o,v in prev1.items()}),
          open('/tmp/b4/cand.json','w'))
