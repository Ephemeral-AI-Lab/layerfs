
import json, collections
rows=json.load(open('/tmp/b4/objects.json'))
r1=[r for r in rows if r['role']==1]
byoid={r['oid']:r for r in r1}
joborder=json.load(open('/tmp/b4/role1_order.json'))
F={}
for line in open('/tmp/b4/frames_full.tsv'):
    j,n=line.split(); F[joborder[int(j)]]=int(n)
def load_pairs(index_path, frames_path):
    out={}
    idx=[l.split() for l in open(index_path)]
    fr=[l.split() for l in open(frames_path)]
    for a,b in zip(idx,fr): out[(a[1],a[2])]=int(b[1])
    return out
P=load_pairs('/tmp/b4/pairs_index.tsv','/tmp/b4/frames_pairs.tsv')
PC=load_pairs('/tmp/b4/cross_index.tsv','/tmp/b4/frames_cross.tsv')
CANON={o:byoid[o]['size']+23 for o in byoid}
DEPTH_CAP=8; CHAIN_CANON=512*1024; CHAIN_ENC=256*1024
edges=[]
for (o,b),f in list(P.items())+list(PC.items()):
    if o not in F or b not in F: continue
    save=(1+F[o])-(1+32+f)
    if save>0: edges.append((save,o,b,1+32+f))
edges.sort(key=lambda e:(-e[0],e[1],e[2]))
print('positive-saving edges', len(edges))
parent={}; height={}
def depth_of(x):
    d=0
    while x in parent:
        x=parent[x]; d+=1
        if d>64: raise RuntimeError('cycle')
    return d
chosen={}
def reaches(start, target):
    x=start; d=0
    while x in parent:
        if x==target: return True
        x=parent[x]; d+=1
        if d>64: raise RuntimeError('cycle')
    return x==target
for save,o,b,c in edges:
    if o in parent or o==b: continue
    if reaches(b, o): continue
    db=depth_of(b)
    if db+1+height.get(o,0) > DEPTH_CAP: continue
    parent[o]=b; chosen[o]=(b,c)
    # update heights up the chain
    x=b; inc=1+height.get(o,0)
    while True:
        if height.get(x,0) >= inc: break
        height[x]=inc
        if x not in parent: break
        x=parent[x]; inc+=1
# chain limits repair
def chain(o):
    can=0; enc=0; x=o
    while True:
        can+=CANON[x]; enc+=chosen[x][1] if x in chosen else (1+F[x])
        if x not in chosen: break
        x=chosen[x][0]
    return can,enc
bad=[]
for o in list(chosen):
    can,enc=chain(o)
    if can>CHAIN_CANON or enc>CHAIN_ENC: bad.append(o)
print('chain-limit violations', len(bad))
for o in bad: del chosen[o]
total=sum((chosen[o][1] if o in chosen else 1+F[o]) for o in F)
print('depth-aware savings greedy total', total, 'deltas', len(chosen), 'full', len(F)-len(chosen))
dep=collections.Counter(depth_of(o) for o in chosen)
print('depth hist', sorted(dep.items()))
json.dump({o:chosen[o][0] for o in chosen}, open('/tmp/b4/greedy_best.json','w'))
