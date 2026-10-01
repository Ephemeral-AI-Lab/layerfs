#!/usr/bin/env python3
"""Independent metadata/byte oracle; never imported by the measured Rust code."""
import json, sqlite3, sys, time
from pathlib import Path
LIVE=2**63-1

def name(prefix,i):
    return f'{prefix}{i:07}'.encode()

def expected_extents(case):
    fragmented=case in ('extent-fragmented-8192','extent-truncate-regrow')
    size=65536 if fragmented else 32768 if case=='extent-dispersed-512' else 16384
    owners=[((i//8)%2+1,i%8) if fragmented else (0,i) for i in range(size)]
    if case=='extent-repeated-4097':
        for i in range(4097): owners[4096:4128]=[(i+1,o) for o in range(32)]
    elif case=='extent-append-512':
        for i in range(512): owners.extend((i+1,o) for o in range(32))
    elif case=='extent-dispersed-512':
        for i in range(512):
            start=((i*61)%1024)*32
            owners[start:start+32]=[(i+1,o) for o in range(32)]
    elif case=='extent-fragmented-8192': owners=[(1,o) for o in range(65536)]
    elif case=='extent-truncate-regrow': owners=owners[:4096]+[(-1,0)]*(65536-4096)
    return owners

def verify(case,folder):
    if case.startswith('cohort-'):
        from verify_cohort import verify as cohort_verify
        return cohort_verify(case,folder)
    db=sqlite3.connect(f'file:{folder / "sample.sqlite"}?mode=ro',uri=True)
    assert db.execute('PRAGMA integrity_check').fetchall()==[('ok',)]
    assert not db.execute('PRAGMA foreign_key_check').fetchall()
    assert db.execute('SELECT count(*) FROM captures').fetchone()[0]==0
    assert db.execute('SELECT count(*) FROM workspace WHERE pending IS NOT NULL').fetchone()[0]==0
    selected=0
    if case.startswith('namespace-'):
        n=int(case.split('-')[1])
        expected=[(1,1,name('f',i),1,LIVE,i+2) for i in range(n)]
        expected += [(1,1,name('r',i),1,LIVE,n+2+i) for i in range(64)]
        actual=db.execute('SELECT w,parent,name,born,dead,ino FROM names ORDER BY w,parent,name,born').fetchall()
        assert actual==sorted(expected), 'namespace bindings differ'
        expected_i=[(1,1,1,LIVE,0,0)]+[(1,i+2,1,LIVE,i,0) for i in range(n)]+[(1,n+2+i,1,LIVE,100000+i,0) for i in range(64)]
        assert db.execute('SELECT * FROM inodes ORDER BY w,ino,born').fetchall()==expected_i
    elif case=='deep-270':
        expected=[(1,i+1,name('r' if i==0 else 'd',i),1,LIVE,i+2) for i in range(270)]
        assert db.execute('SELECT * FROM names ORDER BY w,parent,name,born').fetchall()==expected
        assert db.execute('SELECT * FROM inodes ORDER BY w,ino,born').fetchall()==[(1,i+1,1,LIVE,i,0) for i in range(271)]
    elif case.startswith('lifecycle-'):
        assert db.execute('SELECT w,active,pending,counter FROM workspace').fetchall()==[(1,1,None,128)]
    elif case.startswith('extent-'):
        expected=expected_extents(case)
        size=db.execute('SELECT size FROM inodes WHERE dead=?',(LIVE,)).fetchone()[0]
        assert size==len(expected)
        end=0
        for start,stop,source,offset,born,dead in db.execute('SELECT start,end,source,source_offset,born,dead FROM extents ORDER BY start'):
            assert born==1 and dead==LIVE and start==end and start<stop<=size
            for i in range(start,stop):
                actual=(-1,0) if source==-1 else (source,offset+i-start)
                assert actual==expected[i], (i,actual,expected[i])
                byte=0 if source==-1 else (source*17+offset+i-start)%251
                src,off=expected[i]
                assert byte==(0 if src==-1 else (src*17+off)%251)
            end=stop
        assert end==size
        if case=='extent-repeated-4097': assert db.execute('SELECT COUNT(*) FROM extents').fetchone()[0]==3
        if case=='extent-fragmented-8192': assert db.execute('SELECT COUNT(*) FROM extents').fetchone()[0]==1
    elif case.startswith(('generation-','overlap-')):
        count=int(case.split('-')[1]) if case.startswith('overlap-') else 1
        rounds=8 if case.startswith('generation-') else 1
        with (folder/'selected.tsv').open() as f:
            for round in range(1,rounds+1):
                for w in range(1,count+1):
                    for ino in range(1,10001):
                        got=tuple(map(int,f.readline().split()))
                        expected_value=(round-1)*100000+ino if ino<=128 and round>1 else ino
                        assert got==(w,round,ino,expected_value),(got,w,round,ino)
                        selected+=1
            assert f.readline()==''
        expected=[]
        for w in range(1,count+1):
            for ino in range(1,10001):
                expected.append((w,ino,rounds+1 if ino<=128 else 1,LIVE,rounds*100000+ino if ino<=128 else ino,0))
        assert db.execute('SELECT * FROM inodes ORDER BY w,ino,born').fetchall()==expected
        assert db.execute('SELECT w,active,pending,counter FROM workspace ORDER BY w').fetchall()==[(w,rounds+1,None,0) for w in range(1,count+1)]
        if case.startswith('overlap-'):
            receipt=json.loads((folder/'operation.json').read_text())
            assert receipt['coordinated_mutators_completed']==count
    else: raise ValueError(case)
    db.close()
    return {'status':'PASS','selected_rows_verified':selected,'scope':'complete final metadata / source-coordinate byte oracle; no canonical root or provider payload claim','readonly_engine':sqlite3.sqlite_version}

if __name__=='__main__':
    started=time.monotonic_ns()
    try:
        result=verify(sys.argv[1],Path(sys.argv[2]))
    except Exception as exc:
        result={'status':'FAIL','error':repr(exc)}
    result['verifier_ns']=time.monotonic_ns()-started
    print(json.dumps(result,sort_keys=True))
    sys.exit(0 if result['status']=='PASS' else 1)
