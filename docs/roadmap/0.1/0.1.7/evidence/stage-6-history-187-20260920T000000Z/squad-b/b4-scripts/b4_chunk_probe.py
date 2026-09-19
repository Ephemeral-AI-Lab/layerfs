
import sqlite3, struct, collections, json
DB='/tmp/base187/sample.sqlite'
c=sqlite3.connect('file:'+DB+'?mode=ro',uri=True)
lane_names={1:'ordinary',2:'native',4:'wholefile',6:'pooled',7:'singleton'}
raw={}
for pack_id, data in c.execute('select pack_id, data from object_packs'):
    ver,n = struct.unpack_from('<II', data, 8)
    if ver!=2: continue
    for gi in range(n):
        s,e,d,codec=struct.unpack_from('<IIIB', data, 16+16*gi)
        body=data[s:s+e]
        nrec=struct.unpack_from('<I',body,0)[0]
        ends=list(struct.unpack_from('<%dI'%nrec, body, 4))
        st=4+4*nrec
        for ri in range(nrec):
            en=ends[ri]; rec=body[st:en]; st=en
            tag=rec[0]; rl=struct.unpack_from('<I',rec,1)[0]
            raw[(pack_id,gi,ri)]=rl
print('native records', len(raw), 'sum raw_length', sum(raw.values()))
# object canonical for those
tot=0; cnt=0
for r in c.execute("select object_id, canonical_length, pack_id, group_number, record_number from objects where object_role=2"):
    tot+=r[1]; cnt+=1
print('role2 objects', cnt, 'sum canonical', tot)
# how do canonical_length compare with raw_length?
diff=collections.Counter()
for r in c.execute("select canonical_length, pack_id, group_number, record_number from objects where object_role=2"):
    diff[r[0]-raw[(r[1],r[2],r[3])]]+=1
print('canonical - raw_length histogram (role 2):', sorted(diff.items())[:10])
