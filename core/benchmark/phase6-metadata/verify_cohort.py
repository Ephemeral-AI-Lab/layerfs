"""Independent path/dictionary, sparse-coordinate and full-byte cohort oracle."""
import sqlite3,json
from pathlib import Path
LIVE=2**63-1
SIZE=10*1024*1024

def model(case):
    nodes={100:(-1,0,1)};bindings={};sources={};dirty=set()
    def create(parent,name,ino,data=None,changed=False):
        kind=1 if data is None else 2
        nodes[ino]=(-1,0,kind) if data is None else (ino,len(data),kind)
        bindings[parent,name.encode()]=ino
        if data is not None:sources[ino]=(0,data,len(data))
        if changed:dirty.update((parent,ino))
    if 'retained' in case or case=='cohort-overwrite-4k':
        nodes[1]=(0,SIZE,2);bindings[100,b'data.bin' if 'retained' in case else b'large.bin']=1;sources[0]=(1,b'A',SIZE)
    if case=='cohort-namespace67':
        for parent,name,ino in [(100,'packages',101),(101,'old',102),(101,'new',103),(102,'subtree',104),(104,'child',105)]:create(parent,name,ino)
        create(105,'grand.txt',106,b'grand-base');create(104,'sibling.txt',107,b'sibling-base')
        for i in range(64):create(105,f'extra{i:03}.txt',108+i,b'x')
        create(100,'.marker',172,b'baseline')
        for parent,name,dest,new in [(102,'subtree',103,'subtree'),(104,'child',104,'renamed'),(104,'renamed',104,'child'),(103,'subtree',102,'subtree'),(102,'subtree',103,'subtree')]:
            bindings[dest,new.encode()]=bindings.pop((parent,name.encode()));dirty.update((parent,dest))
        create(105,'grand.txt.next',173,b'grand-new',True)
        bindings[105,b'grand.txt']=bindings.pop((105,b'grand.txt.next'));dirty.add(105)
    elif case=='cohort-components270':
        parent=100
        for i in range(270):create(parent,'d',200+i,changed=True);parent=200+i
        create(parent,'file',470,b'leaf',True)
    elif case=='cohort-many128':
        create(100,'many',200,changed=True)
        for i in range(128):create(200,f'f{i}',201+i,f'new-{i}'.encode(),True)
    spans=[];retained_spans=[]
    if 1 in nodes:
        points={}
        if 'retained' in case:
            for i in range(4097):
                p=(104729+i*2654435761)%SIZE
                points[p]=(10000+i,1)
                sources[10000+i]=(0,bytes([ord('B')+i%24]),1)
        def partition(points):
            cursor=0;rows=[]
            for start,(source,length) in sorted(points.items()):
                if start>cursor:rows.append((cursor,start,0,cursor))
                rows.append((start,start+length,source,0));cursor=start+length
            if cursor<SIZE:rows.append((cursor,SIZE,0,cursor))
            return rows
        retained_spans=partition(points)
        if case=='cohort-one-edit-retained-4097':points[0]=(20000,1);sources[20000]=(0,b'X',1);dirty.add(1)
        if case=='cohort-overwrite-4k':points[5242880]=(20000,4096);sources[20000]=(0,b'P'*4096,4096);dirty.add(1)
        spans=partition(points)
    return nodes,bindings,sources,dirty,spans,retained_spans

def catalog(nodes,bindings,dirty,spans,g):
    rows=[]
    for ino in sorted(dirty):
        value,size,kind=nodes[ino];rows.append(f'I\t{g}\t{ino}\t{value}\t{size}\t{kind}')
        if kind==1:
            for (parent,name),child in sorted(bindings.items()):
                if parent==ino:rows.append(f'N\t{g}\t{ino}\t{child}\t{name.hex()}')
        elif ino==1:
            rows.extend(f'E\t{g}\t1\t{start}\t{end}\t{source}\t{off}' for start,end,source,off in spans)
    return rows

def check_catalog(path,expected):
    with path.open() as f:
        for row in expected:assert f.readline().rstrip('\n')==row,(path,row)
        assert f.readline()=='',(path,'extra result rows')

def verify(case,folder):
    db=sqlite3.connect(f'file:{folder/"sample.sqlite"}?mode=ro',uri=True)
    assert db.execute('PRAGMA integrity_check').fetchall()==[('ok',)]
    assert not db.execute('PRAGMA foreign_key_check').fetchall()
    for table in ['changed','captures','pins']:
        assert db.execute(f'SELECT count(*) FROM {table}').fetchone()[0]==0,table
    nodes,bindings,sources,dirty,spans,retained_spans=model(case)
    actual_sources={id:(fill,data,length) for id,fill,data,length in db.execute('SELECT * FROM sources')}
    assert actual_sources==sources,'source values differ'
    actual_kinds=dict(db.execute('SELECT * FROM kinds'))
    assert actual_kinds=={ino:v[2] for ino,v in nodes.items()}
    actual_names=db.execute('SELECT parent,name,ino FROM names ORDER BY parent,name').fetchall()
    assert actual_names==[(parent,name,ino) for (parent,name),ino in sorted(bindings.items())]
    assert db.execute('SELECT count(*) FROM names WHERE dead<>?',(LIVE,)).fetchone()[0]==0
    actual_nodes=db.execute('SELECT ino,value,size FROM inodes ORDER BY ino').fetchall()
    assert actual_nodes==[(ino,v[0],v[1]) for ino,v in sorted(nodes.items())]
    assert db.execute('SELECT count(*) FROM inodes WHERE dead<>?',(LIVE,)).fetchone()[0]==0
    orphan_rows=db.execute('SELECT ino FROM retained_orphans').fetchall()
    assert orphan_rows==([(106,)] if case=='cohort-namespace67' else [])
    actual_spans=db.execute('SELECT start,end,source,source_offset FROM extents ORDER BY start').fetchall()
    assert actual_spans==spans,'extent source coordinates differ'
    assert db.execute('SELECT count(*) FROM extents WHERE dead<>?',(LIVE,)).fetchone()[0]==0
    g=2 if 'retained' in case else 1
    check_catalog(folder/'final.catalog',catalog(nodes,bindings,dirty,spans,g))
    if 'retained' in case:
        expected=catalog(nodes,bindings,{1},retained_spans,1)
        check_catalog(folder/'prelude.catalog',expected)
        check_catalog(folder/'retained.catalog',expected)
    verified=0
    if spans:
        actual=bytearray(SIZE);end=0
        for start,stop,source,off in actual_spans:
            assert start==end and start<stop<=SIZE
            fill,data,length=actual_sources[source]
            assert off+stop-start<=length
            actual[start:stop]=data*(stop-start) if fill else data[off:off+stop-start]
            end=stop
        assert end==SIZE
        expected=bytearray(b'A'*SIZE)
        if 'retained' in case:
            for i in range(4097):expected[(104729+i*2654435761)%SIZE]=ord('B')+i%24
        if case=='cohort-one-edit-retained-4097':expected[0]=ord('X')
        if case=='cohort-overwrite-4k':expected[5242880:5246976]=b'P'*4096
        assert actual==expected,'full byte oracle differs'
        verified=SIZE
    expected_generation=3 if 'retained' in case else 2
    assert db.execute('SELECT w,active,pending,counter FROM workspace').fetchall()==[(1,expected_generation,None,0)]
    receipt=json.loads((folder/'operation.json').read_text())
    assert receipt['dirty_inodes']==len(dirty)
    if case=='cohort-clean-retained-4097':assert receipt['catalog_rows']==0
    assert receipt['obsolete_versions']==0
    db.close()
    return {'status':'PASS','regular_logical_bytes_verified':verified,'final_bindings_verified':len(bindings),'retained_pin_checked':'retained' in case,'retained_orphans':len(orphan_rows),'scope':'complete adapted metadata/source coordinates/10MiB logical byte oracle; no CAS/FUSE/Commit proof','readonly_engine':sqlite3.sqlite_version}
