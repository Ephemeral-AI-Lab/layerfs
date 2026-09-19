
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
    states.append(dict(idx=idx,sha=sha,tree=t))
# change classification
cls=collections.Counter(); changed_oids=set()
for k in range(1,len(states)):
    prev=states[k-1]['tree']; cur=states[k]['tree']
    for p,(m,o,sz) in cur.items():
        if p not in prev: cls['added']+=1; changed_oids.add(o)
        elif prev[p][1]!=o: cls['modified']+=1; changed_oids.add(o)
        elif prev[p][0]!=m: cls['metadata-only']+=1
    for p in prev:
        if p not in cur: cls['removed']+=1
cls['added']+=len(states[0]['tree']); changed_oids.update(o for _,o,_ in states[0]['tree'].values())
print('change classes:', dict(cls))
print('distinct oids referenced by added-or-modified paths:', len(changed_oids))
# union
union={}
for s in states:
    for p,(m,o,sz) in s['tree'].items(): union[o]=sz
print('union oids', len(union))
# per-path chains
chains=collections.defaultdict(list)
for si,s in enumerate(states):
    for p,(m,o,sz) in s['tree'].items(): chains[p].append((si,o,sz))
multi=sum(1 for p,v in chains.items() if len(v)>1)
print('paths', len(chains), 'paths with >1 version', multi)
# objects with a same-path earlier version (within selection)
have_prev={}
for p,v in chains.items():
    seen=set()
    for si,o,sz in v:
        if o not in have_prev:
            have_prev[o] = (p, si, seen.copy())
        seen.add(o)
prev_oids=set()
for p,v in chains.items():
    for j in range(1,len(v)):
        prev_oids.add(v[j][1])
print('distinct oids that are a non-first version of some path:', len(prev_oids))
# also: oids that appear at >1 path
byoid=collections.defaultdict(set)
for p,v in chains.items():
    for si,o,sz in v: byoid[o].add(p)
print('oids referenced by >1 path:', sum(1 for o,ps in byoid.items() if len(ps)>1))
json.dump(dict(states=[dict(idx=s['idx'],sha=s['sha']) for s in states],
               chains={p:v for p,v in chains.items()}), open('/tmp/b4/chains.json','w'))
