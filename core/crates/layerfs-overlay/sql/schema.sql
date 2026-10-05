-- Disposable overlay schema v4. All owner relations lead with Workspace ns.
CREATE TABLE workspace (
    ns INTEGER PRIMARY KEY AUTOINCREMENT,
    incarnation BLOB NOT NULL UNIQUE CHECK(length(incarnation)=32),
    base_root BLOB NOT NULL CHECK(length(base_root)=32),
    active INTEGER NOT NULL CHECK(active>0),
    captured INTEGER CHECK(captured>0),
    installed INTEGER NOT NULL DEFAULT 0 CHECK(installed>=0),
    revision INTEGER NOT NULL DEFAULT 0 CHECK(revision>=0),
    lifecycle INTEGER NOT NULL DEFAULT 0 CHECK(lifecycle IN (0,1)),
    dirty_inodes INTEGER NOT NULL DEFAULT 0 CHECK(dirty_inodes>=0),
    dirty_names INTEGER NOT NULL DEFAULT 0 CHECK(dirty_names>=0)
) STRICT;
CREATE TABLE inode (
    ns INTEGER NOT NULL REFERENCES workspace(ns),
    serial INTEGER NOT NULL CHECK(serial>0),
    gen INTEGER NOT NULL CHECK(gen>0),
    kind INTEGER NOT NULL CHECK(kind IN (1,2,3)),
    mode INTEGER NOT NULL CHECK(mode>=0 AND ((kind=1 AND mode<=511) OR (kind=2 AND mode<=1023) OR (kind=3 AND mode=511))),
    mtime_seconds INTEGER NOT NULL,
    mtime_nanoseconds INTEGER NOT NULL CHECK(mtime_nanoseconds>=0 AND mtime_nanoseconds<1000000000),
    nlink INTEGER NOT NULL CHECK(nlink>=0),
    size INTEGER NOT NULL CHECK(size>=0),
    inherited_cutoff INTEGER NOT NULL CHECK(inherited_cutoff>=0),
    PRIMARY KEY(ns,serial,gen)
) STRICT, WITHOUT ROWID;
CREATE INDEX inode_capture ON inode(ns,gen,serial);
CREATE TABLE dentry (
    ns INTEGER NOT NULL REFERENCES workspace(ns),
    parent INTEGER NOT NULL CHECK(parent>0),
    name BLOB NOT NULL CHECK(length(name)>0 AND length(name)<=255),
    gen INTEGER NOT NULL CHECK(gen>0),
    serial INTEGER CHECK(serial>0),
    PRIMARY KEY(ns,parent,name,gen)
) STRICT, WITHOUT ROWID;
CREATE INDEX dentry_capture ON dentry(ns,gen,parent,name);
CREATE TABLE payload (
    rowid INTEGER PRIMARY KEY,
    ns INTEGER NOT NULL REFERENCES workspace(ns),
    serial INTEGER NOT NULL CHECK(serial>0),
    gen INTEGER NOT NULL CHECK(gen>0),
    cell_offset INTEGER NOT NULL CHECK(cell_offset>=0 AND cell_offset%4096=0),
    data BLOB NOT NULL CHECK(length(data)=4096),
    validity BLOB NOT NULL CHECK(length(validity)=512),
    UNIQUE(ns,serial,gen,cell_offset)
) STRICT;
CREATE TABLE request (
    ns INTEGER NOT NULL REFERENCES workspace(ns),
    revision INTEGER NOT NULL CHECK(revision>0),
    PRIMARY KEY(ns,revision)
) STRICT, WITHOUT ROWID;
CREATE INDEX payload_namespace_row ON payload(ns,rowid);
CREATE TABLE lease (
    ns INTEGER NOT NULL REFERENCES workspace(ns),
    kind INTEGER NOT NULL CHECK(kind BETWEEN 1 AND 4),
    owner INTEGER NOT NULL CHECK(owner>0),
    resource INTEGER NOT NULL CHECK(resource>=0),
    PRIMARY KEY(ns,kind,owner,resource)
) STRICT, WITHOUT ROWID;
CREATE TABLE scratch (
    ns INTEGER NOT NULL REFERENCES workspace(ns),
    operation INTEGER NOT NULL CHECK(operation>0),
    kind INTEGER NOT NULL CHECK(kind>=0),
    key INTEGER NOT NULL CHECK(key>=0),
    value BLOB NOT NULL CHECK(length(value)<=65536),
    PRIMARY KEY(ns,operation,kind,key)
) STRICT, WITHOUT ROWID;
CREATE TABLE reclaim (
    ns INTEGER NOT NULL REFERENCES workspace(ns),
    queue_key INTEGER NOT NULL CHECK(queue_key>0),
    target INTEGER NOT NULL CHECK(target>0),
    cursor INTEGER NOT NULL CHECK(cursor>=0),
    PRIMARY KEY(ns,queue_key)
) STRICT, WITHOUT ROWID;
PRAGMA application_id=1279676210;
CREATE INDEX reclaim_ready ON reclaim(queue_key,ns);
PRAGMA user_version=4;
