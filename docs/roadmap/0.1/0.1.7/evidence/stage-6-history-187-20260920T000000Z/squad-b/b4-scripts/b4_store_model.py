
import json, collections
rows=json.load(open('/tmp/b4/objects.json'))
r1=[r for r in rows if r['role']==1]
byoid={r['oid']:r for r in r1}
joborder=json.load(open('/tmp/b4/role1_order.json'))
cand=json.load(open('/tmp/b4/cand.json'))
first={o:(v[0],v[1]) for o,v in cand['first'].items()}
order_emit=[r['oid'] for r in sorted(r1, key=lambda r: first[r['oid']])]
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
def frame(o,b):
    if b is None: return F[o]
    if (o,b) in P: return P[(o,b)]
    return PC[(o,b)]
CANON={o:byoid[o]['size']+23 for o in byoid}

# ---- pack assembly, whole-file lane (real grammar) ----
HEADER=16; WF_ENTRY=4; PACK_LIMIT=262144; GROUP_COUNT_LIMIT=256
def assemble_wf(records_in_order):
    packs=[]; cur=HEADER; groups=0; total=0
    for body in records_in_order:
        if groups and (cur+WF_ENTRY+body > PACK_LIMIT or groups>=GROUP_COUNT_LIMIT):
            packs.append(cur+WF_ENTRY*groups); total+=cur+WF_ENTRY*groups; cur=HEADER; groups=0
        cur+=WF_ENTRY+body; groups+=1
    if groups: packs.append(cur+WF_ENTRY*groups); total+=cur+WF_ENTRY*groups
    return total, packs

# real assignment: use the recorded stored record sizes (validated: model reproduces them within 8 B)
REAL_STORED={r['oid']:r['stored'] for r in r1}
def real_records():
    return [REAL_STORED[o] for o in order_emit]
def records_for(assignment):
    out=[]; nb=0; nf=0; t=0
    for o in order_emit:
        b=assignment.get(o)
        f=frame(o,b) if b else F[o]
        body=1+(32 if b else 0)+f
        out.append(body); t+=body
        if b: nb+=1
        else: nf+=1
    return out, t, nb, nf

# native + metadata constants (measured from the real pack directory)
NATIVE_PACK=5698270
META_PACK=2047483+1023900
OVERFLOW_RATE=1089261/119894291
OBJ_ROWS=52032; REAL_BASE_ROWS=19261; OTHER_BASE_ROWS=917
def sqlite_nonpack(pack_bytes, nbase_total, overflow_rate=OVERFLOW_RATE):
    objects_tbl = 52032*55.70 + 32*nbase_total
    locations   = 2547712
    bases_idx   = 81.87*nbase_total
    mvg         = 118784
    schema      = 8192
    slack       = int(pack_bytes*overflow_rate)
    return objects_tbl+locations+bases_idx+mvg+schema+slack, dict(objects_tbl=objects_tbl,locations=locations,bases_idx=bases_idx,mvg=mvg,schema=schema,slack=slack)

def store_total(assignment, label, verbose=True):
    recs, rec_total, nb, nf = records_for(assignment)
    wf_pack, packs = assemble_wf(recs)
    pack_total = wf_pack+NATIVE_PACK+META_PACK
    nonpack, parts = sqlite_nonpack(pack_total, nb+OTHER_BASE_ROWS)
    total = pack_total+nonpack
    if verbose:
        print('%-22s wf_records %10d  wf_packs %10d (%d packs)  native %8d  meta %8d  pack_total %10d  nonpack %9d  TOTAL %10d  bases %d full %d'%(
            label, rec_total, wf_pack, len(packs), NATIVE_PACK, META_PACK, pack_total, nonpack, total, nb, nf))
    return dict(label=label, wf_record_bytes=rec_total, wf_pack_bytes=wf_pack, wf_packs=len(packs),
                native_pack_bytes=NATIVE_PACK, meta_pack_bytes=META_PACK, pack_total=pack_total,
                sqlite_nonpack=nonpack, sqlite_parts=parts, total_apparent=total, bases=nb, full=nf)

def store_total_records(recs, nbase, label):
    wf_pack, packs = assemble_wf(recs)
    pack_total = wf_pack+NATIVE_PACK+META_PACK
    nonpack, parts = sqlite_nonpack(pack_total, nbase+OTHER_BASE_ROWS)
    total = pack_total+nonpack
    print('%-22s wf_records %10d  wf_packs %10d (%d packs)  native %8d  meta %8d  pack_total %10d  nonpack %9d  TOTAL %10d  bases %d full %d'%(
        label, sum(recs), wf_pack, len(packs), NATIVE_PACK, META_PACK, pack_total, nonpack, total, nbase, len(recs)-nbase))
    return dict(label=label, wf_record_bytes=sum(recs), wf_pack_bytes=wf_pack, wf_packs=len(packs),
                native_pack_bytes=NATIVE_PACK, meta_pack_bytes=META_PACK, pack_total=pack_total,
                sqlite_nonpack=nonpack, sqlite_parts=parts, total_apparent=total, bases=nbase, full=len(recs)-nbase)
out={}
recs_real=real_records()
nbase_real=sum(1 for r in r1 if r['base'])
out['real']=store_total_records(recs_real, nbase_real, 'REAL policy')
print('  real pack directory says: 437 packs, 111124638 wf pack bytes, 119894291 total pack bodies')
out['full_only']=store_total({}, 'FULL only')
s1=json.load(open('/tmp/b4/base_S3_samepath_cross_emit.json'))
s1={o:(v or None) for o,v in s1.items()}
# S1 and S2 assignments: recompute quickly
prev1={o:set(v) for o,v in cand['prev1'].items()}
samepath={o:set(v) for o,v in cand['cand'].items()}
cx=json.load(open('/tmp/b4/cross_cand.json'))
cross={o:set(v) for o,v in cx.items()}
def merge(*maps):
    out=collections.defaultdict(set)
    for m in maps:
        for o,v in m.items(): out[o].update(v)
    return out
DEPTH_CAP=8; CHAIN_CANON=512*1024; CHAIN_ENC=256*1024
def onepass(candmap, order):
    pos={o:i for i,o in enumerate(order)}
    depth={}; cc={}; ce={}; base={}
    for o in order:
        c=CANON[o]; best=1+F[o]; bb=None
        for b in candmap.get(o,()):
            if b not in base: continue
            if pos[b]>=pos[o] or depth[b]>=DEPTH_CAP: continue
            k=1+32+frame(o,b)
            if k>=best: continue
            if cc[b]+c>CHAIN_CANON or ce[b]+c>CHAIN_ENC: continue
            best=k; bb=b
        base[o]=bb
        if bb is None: depth[o]=0; cc[o]=c; ce[o]=c
        else: depth[o]=depth[bb]+1; cc[o]=cc[bb]+c; ce[o]=ce[bb]+c
    return base
a1=onepass(prev1, order_emit)
a2=onepass(samepath, order_emit)
a3=onepass(merge(samepath,cross), order_emit)
g=json.load(open('/tmp/b4/greedy_best.json'))
out['S1']=store_total(a1,'S1 prev-only')
out['S2']=store_total(a2,'S2 same-path any')
out['S3']=store_total(a3,'S3 same+cross 1-pass')
out['S4']=store_total(g,'S4 global savings')
json.dump(out, open('/tmp/b4/store_totals.json','w'), indent=1)
