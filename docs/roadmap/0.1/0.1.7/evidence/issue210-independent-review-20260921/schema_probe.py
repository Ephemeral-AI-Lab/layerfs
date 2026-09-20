from pathlib import Path
import sqlite3

SCHEMA = Path('/Users/yifanxu/.codex/worktrees/pair2-history-implementation/layerfs/core/crates/layerfs-history/sql/schema-v1.sql').read_text()
def tagged(tag, n, width): return bytes([tag])+bytes([n])*(width-1)
s,s2=tagged(0x31,1,17),tagged(0x31,2,17)
g,g2,l,x=[tagged(0x32,n,33) for n in (1,2,3,4)]
a,b=[tagged(0x11,n,17) for n in (1,2)]
k,k2,k3,kx=[tagged(0x12,n,33) for n in (1,2,3,4)]
r=bytes(32)
def database():
    db=sqlite3.connect(':memory:');db.execute('PRAGMA foreign_keys=ON');db.executescript(SCHEMA)
    db.execute('INSERT INTO history_meta VALUES(1,?,1,?,1,1)',(r,b'binding'))
    for stack,base,text in ((s,g,'one'),(s2,g2,'two')):
        db.execute('INSERT INTO layer_stacks VALUES(?,?,?,?,?)',(stack,text,r,r,base))
        db.execute('INSERT INTO layers VALUES(?,?,NULL,?,NULL,NULL)',(base,stack,r))
    for commit,stack,base in ((k,s,g),(k2,s,g),(k3,s2,g2)):
        db.execute('INSERT INTO commits VALUES(?,?,?,NULL,?)',(commit,stack,r,base))
    for branch,commit,text in ((a,k,'a'),(b,k2,'b')):
        db.execute('INSERT INTO branches VALUES(?,?,?,?,?)',(branch,s,text,g,commit))
    db.execute('INSERT INTO layers VALUES(?,?,?,?,?,?)',(l,s,g,r,a,k))
    db.execute('UPDATE layer_stacks SET head_layer_id=? WHERE layer_stack_id=?',(l,s));db.commit()
    return db

db=database()
print('tables:',[r[0] for r in db.execute("SELECT name FROM sqlite_schema WHERE type='table' ORDER BY name")])
print('application_id:',db.execute('PRAGMA application_id').fetchone()[0],'user_version:',db.execute('PRAGMA user_version').fetchone()[0])
print('triggers:',db.execute("SELECT count(*) FROM sqlite_schema WHERE type='trigger'").fetchone()[0])
for name,sql,args in [
    ('second genesis','INSERT INTO layers VALUES(?,?,NULL,?,NULL,NULL)',(x,s,r)),
    ('second child','INSERT INTO layers VALUES(?,?,?,?,?,?)',(x,s,g,r,b,k2)),
    ('second source publication','INSERT INTO layers VALUES(?,?,?,?,?,?)',(x,s,l,r,a,k)),
    ('genesis with source','INSERT INTO layers VALUES(?,?,NULL,?,?,NULL)',(x,s,r,a)),
    ('child missing source commit','INSERT INTO layers VALUES(?,?,?,?,?,NULL)',(x,s,l,r,b)),
    ('cross-stack commit base','INSERT INTO commits VALUES(?,?,?,NULL,?)',(kx,s,r,g2)),
    ('cross-stack commit parent','INSERT INTO commits VALUES(?,?,?,?,?)',(kx,s,r,k3,g)),
    ('cross-stack branch base','UPDATE branches SET base_layer_id=? WHERE branch_id=?',(g2,a)),
    ('cross-stack branch head','UPDATE branches SET head_commit_id=? WHERE branch_id=?',(k3,a)),
]:
    db=database()
    try: db.execute(sql,args);db.commit()
    except sqlite3.IntegrityError as error: print(name+':',str(error));db.rollback()
    else: raise AssertionError('constraint absent: '+name)
print('DDL constraint probes complete')
