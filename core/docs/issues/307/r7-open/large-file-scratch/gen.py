import sys
n,cell,per,name=int(sys.argv[1]),int(sys.argv[2]),int(sys.argv[3]),sys.argv[4]
out=open(name,'w')
out.write("PRAGMA page_size=4096; PRAGMA auto_vacuum=NONE; PRAGMA journal_mode=MEMORY; PRAGMA synchronous=OFF; PRAGMA locking_mode=EXCLUSIVE; PRAGMA mmap_size=0; PRAGMA foreign_keys=ON; PRAGMA temp_store=FILE; PRAGMA cache_size=-2048;\n")
schema=open('schema.sql').read()
if cell>4096: schema=schema.replace("length(data) BETWEEN 1 AND 4096","length(data) BETWEEN 1 AND 1048576")
out.write(schema+"\n"+open('accounting.sql').read()+"\n")
out.write("INSERT INTO workspace(incarnation,base_root,active) VALUES(randomblob(32),randomblob(32),1);\n")
out.write("INSERT INTO inode VALUES(1,5,1,1,420,0,0,1,0,0,1,0,0,0);\n")
for t in range(n):
    out.write("BEGIN IMMEDIATE;\nUPDATE inode SET size=%d WHERE ns=1 AND serial=5 AND gen=1;\n"%((t+1)*cell*per))
    for i in range(per):
        out.write("INSERT INTO payload(ns,serial,gen,cell_offset,epoch,data,validity) VALUES(1,5,1,%d,0,randomblob(%d),NULL) ON CONFLICT(ns,serial,gen,cell_offset) DO UPDATE SET epoch=excluded.epoch,data=excluded.data,validity=excluded.validity;\n"%((t*per+i)*cell,cell))
    out.write("UPDATE workspace SET revision=%d WHERE ns=1;\nCOMMIT;\n"%(t+1))
out.write(".stats on\nSELECT 'done';\n")
