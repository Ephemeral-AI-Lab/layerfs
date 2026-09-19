
import json, zlib, collections, os
rows=json.load(open('/tmp/b4/objects.json'))
r1=[r for r in rows if r['role']==1]
objs=[r['oid'] for r in r1]
size={r['oid']:r['size'] for r in r1}
K=32
def sketch(b):
    n=len(b)
    if n<=16: return [zlib.crc32(b)]
    step=max(16, n//400)
    hs=set()
    i=0
    while i+16<=n and len(hs)<800:
        hs.add(zlib.crc32(b[i:i+16])); i+=step
    if n>=16: hs.add(zlib.crc32(b[-16:]))
    return sorted(hs)[:K]
S={}
for o in objs:
    S[o]=sketch(open('/tmp/b4/payload/'+o,'rb').read())
print('sketches built')
inv=collections.defaultdict(list)
for o,sk in S.items():
    for h in sk: inv[h].append(o)
print('distinct shingle hashes', len(inv))
pair=collections.Counter()
for h,os_ in inv.items():
    if len(os_)<2 or len(os_)>48: continue
    for i in range(len(os_)):
        for j in range(i+1,len(os_)):
            pair[(os_[i],os_[j])]+=1
print('raw candidate pairs', len(pair))
best=collections.defaultdict(list)
for (a,b),c in pair.items():
    best[a].append((c,b)); best[b].append((c,a))
N=6
candx=collections.defaultdict(set)
for o,lst in best.items():
    lst.sort(key=lambda t:(-t[0], size[t[1]]))
    for c,b in lst[:N]: candx[o].add(b)
print('objects with cross-path candidates', len(candx), 'pairs', sum(len(v) for v in candx.values()))
json.dump({o:sorted(v) for o,v in candx.items()}, open('/tmp/b4/cross_cand.json','w'))
