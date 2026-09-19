
import sqlite3, struct, collections, json
DB='/tmp/base187/sample.sqlite'
c=sqlite3.connect('file:'+DB+'?mode=ro',uri=True)
PACK_MAGIC=b'LFPACK\x00\x00'
lane_names={1:'ordinary',2:'native',4:'wholefile',6:'pooled',7:'singleton'}
obj={}
for r in c.execute('select object_id,object_role,canonical_length,base_object_id,pack_id,group_number,record_number from objects'):
    obj[(r[4],r[5],r[6])]=dict(oid=r[0],role=r[1],canon=r[2],base=r[3],pack=r[4],group=r[5],rec=r[6])
packbytes=collections.Counter(); packcount=collections.Counter(); groupcount=collections.Counter()
per_obj=[]; agg=collections.Counter(); aggobj=collections.Counter(); aggcanon=collections.Counter()
for pack_id, data in c.execute('select pack_id, data from object_packs'):
    ver,n = struct.unpack_from('<II', data, 8)
    lane=lane_names[ver]
    packbytes[lane]+=len(data); packcount[lane]+=1; groupcount[lane]+=n
    if ver==4:
        offs=list(struct.unpack_from('<%dI'%n, data, 16))
        for gi,off in enumerate(offs):
            end = offs[gi+1] if gi+1<n else len(data)
            rec=data[off:end]
            tag=rec[0]
            o=obj[(pack_id,gi,0)]
            per_obj.append((lane,len(rec),o['canon'],o['base'] is not None,o['role'],pack_id,gi))
    elif ver==2:
        for gi in range(n):
            s,e,d,codec=struct.unpack_from('<IIIB', data, 16+16*gi)
            body=data[s:s+e]
            nrec=struct.unpack_from('<I',body,0)[0]
            ends=list(struct.unpack_from('<%dI'%nrec, body, 4))
            start=4+4*nrec
            for ri in range(nrec):
                end=ends[ri]; rec=body[start:end]; start=end
                o=obj[(pack_id,gi,ri)]
                per_obj.append((lane,len(rec),o['canon'],o['base'] is not None,o['role'],pack_id,gi))
    else:
        for gi in range(n):
            s,e,d,codec=struct.unpack_from('<IIIB', data, 16+16*gi)
            agg[(lane,'zstd' if codec==1 else 'raw')]+=e
print('packs by lane', dict(packcount))
print('groups by lane', dict(groupcount))
print('pack bytes by lane', dict(packbytes), 'total', sum(packbytes.values()))
print('metadata group-body bytes by lane', dict(agg), 'total', sum(agg.values()))
tot=collections.Counter(); cnt=collections.Counter(); canon=collections.Counter()
for lane,sb,cb,hb,role,_,_ in per_obj:
    key=(lane,'base' if hb else 'nobase')
    tot[key]+=sb; cnt[key]+=1; canon[key]+=cb
for k in sorted(tot):
    print('  %-18s objs %6d canon %10d stored %10d ratio %.2f'%(str(k),cnt[k],canon[k],tot[k],canon[k]/tot[k]))
wf=sum(v for k,v in tot.items() if k[0]=='wholefile'); na=sum(v for k,v in tot.items() if k[0]=='native')
md=sum(agg.values())
print('wholefile record bytes', wf, ' native record bytes', na, ' metadata body bytes', md, ' sum', wf+na+md)
print('pack overhead =', sum(packbytes.values())-(wf+na+md))
json.dump([[l,s,c,h,r] for l,s,c,h,r,_,_ in per_obj], open('/tmp/b4/per_obj.json','w'))
