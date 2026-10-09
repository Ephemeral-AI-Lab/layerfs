-- Disposable overlay schema v25. All owner relations lead with Workspace ns.
CREATE TABLE workspace (
    ns INTEGER PRIMARY KEY AUTOINCREMENT,
    incarnation BLOB NOT NULL UNIQUE CHECK(length(incarnation)=32),
    base_root BLOB NOT NULL CHECK(length(base_root)=32),
    active INTEGER NOT NULL CHECK(active>0),
    captured INTEGER CHECK(captured>0),
    captured_revision INTEGER CHECK(captured_revision>=0),
    installed INTEGER NOT NULL DEFAULT 0 CHECK(installed>=0),
    revision INTEGER NOT NULL DEFAULT 0 CHECK(revision>=0),
    lifecycle INTEGER NOT NULL DEFAULT 0 CHECK(lifecycle IN (0,1)),
    dirty_inodes INTEGER NOT NULL DEFAULT 0 CHECK(dirty_inodes>=0),
    dirty_directory_entries INTEGER NOT NULL DEFAULT 0 CHECK(dirty_directory_entries>=0),
    base_readers INTEGER NOT NULL DEFAULT 0 CHECK(base_readers>=0),
    consolidating INTEGER CHECK(consolidating>0),
    CHECK(captured IS NULL OR consolidating IS NULL),
    CHECK((captured IS NULL)=(captured_revision IS NULL))
) STRICT;
CREATE TABLE inode (
    ns INTEGER NOT NULL REFERENCES workspace(ns),
    serial INTEGER NOT NULL CHECK(serial>0),
    gen INTEGER NOT NULL CHECK(gen=-1 OR gen>0),
    kind INTEGER NOT NULL CHECK(kind BETWEEN 1 AND 3),
    mode INTEGER NOT NULL CHECK(mode>=0 AND ((kind=1 AND mode<=511) OR (kind=2 AND mode<=1023) OR (kind=3 AND mode=511))),
    mtime_seconds INTEGER NOT NULL,
    mtime_nanoseconds INTEGER NOT NULL CHECK(mtime_nanoseconds>=0 AND mtime_nanoseconds<1000000000),
    nlink INTEGER NOT NULL CHECK(nlink>=0),
    size INTEGER NOT NULL CHECK(size>=0),
    inherited_cutoff INTEGER NOT NULL CHECK(inherited_cutoff>=0),
    born INTEGER NOT NULL CHECK(born>=0 AND (gen=-1 OR born<=gen)),
    entries INTEGER NOT NULL CHECK(entries>=0 AND (kind=2 OR entries=0)),
    subdirs INTEGER NOT NULL CHECK(subdirs>=0 AND (kind=2 OR subdirs=0)),
    epoch INTEGER NOT NULL CHECK(epoch>=0),
    height INTEGER NOT NULL CHECK(height>=0 AND height<=epoch),
    PRIMARY KEY(ns,serial,gen)
) STRICT, WITHOUT ROWID;
CREATE INDEX inode_capture ON inode(ns,gen,serial);
CREATE TABLE directory_entry (
    ns INTEGER NOT NULL REFERENCES workspace(ns),
    parent INTEGER NOT NULL CHECK(parent>0),
    name BLOB NOT NULL CHECK(length(name)>0 AND length(name)<=255),
    gen INTEGER NOT NULL CHECK(gen>0),
    serial INTEGER CHECK(serial>0),
    inherited INTEGER NOT NULL CHECK(inherited IN(0,1)),
    PRIMARY KEY(ns,parent,name,gen)
) STRICT, WITHOUT ROWID;
CREATE INDEX directory_entry_capture ON directory_entry(ns,gen,parent,name);
CREATE TABLE payload (
    rowid INTEGER PRIMARY KEY,
    ns INTEGER NOT NULL REFERENCES workspace(ns),
    serial INTEGER NOT NULL CHECK(serial>0),
    gen INTEGER NOT NULL CHECK(gen=-1 OR gen>0),
    cell_offset INTEGER NOT NULL CHECK(cell_offset>=0 AND cell_offset%4096=0),
    epoch INTEGER NOT NULL CHECK(epoch>=0),
    -- One cell at most, or a dense run of whole cells inside one aligned 32768-byte slot.
    data BLOB NOT NULL CHECK(length(data) BETWEEN 1 AND 32768 AND (length(data)<=4096 OR
        (validity IS NULL AND length(data)%4096=0 AND cell_offset/32768=(cell_offset+length(data)-1)/32768))),
    validity BLOB CHECK(validity IS NULL OR length(validity)=(length(data)+7)/8),
    UNIQUE(ns,serial,gen,cell_offset)
) STRICT;
CREATE TABLE shrink (
    ns INTEGER NOT NULL REFERENCES workspace(ns),
    serial INTEGER NOT NULL CHECK(serial>0),
    gen INTEGER NOT NULL CHECK(gen=-1 OR gen>0),
    depth INTEGER NOT NULL CHECK(depth>0),
    cell_offset INTEGER NOT NULL CHECK(cell_offset>=0 AND cell_offset%4096=0),
    epoch INTEGER NOT NULL CHECK(epoch>0),
    PRIMARY KEY(ns,serial,gen,depth)
) STRICT, WITHOUT ROWID;
CREATE INDEX payload_namespace_row ON payload(ns,rowid);
CREATE INDEX payload_generation ON payload(ns,gen,serial,cell_offset);
CREATE INDEX shrink_generation ON shrink(ns,gen,serial,depth);
CREATE TABLE lease (
    ns INTEGER NOT NULL REFERENCES workspace(ns),
    kind INTEGER NOT NULL CHECK(kind BETWEEN 1 AND 6 OR kind=8),
    owner INTEGER NOT NULL CHECK(owner>0),
    resource INTEGER NOT NULL CHECK(resource>=0),
    PRIMARY KEY(ns,kind,owner,resource)
) STRICT, WITHOUT ROWID;
CREATE TABLE base_source (
    ns INTEGER NOT NULL REFERENCES workspace(ns),
    kind INTEGER NOT NULL DEFAULT 0 CHECK(kind BETWEEN 0 AND 2),
    owner INTEGER NOT NULL CHECK(owner>0),
    base_root BLOB NOT NULL CHECK(length(base_root)=32),
    PRIMARY KEY(ns,kind,owner)
) STRICT, WITHOUT ROWID;
CREATE TABLE operation_record (
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
CREATE INDEX lease_resource ON lease(ns,kind,resource,owner);
CREATE TABLE maintenance (
    ns INTEGER NOT NULL REFERENCES workspace(ns),
    kind INTEGER NOT NULL CHECK(kind BETWEEN 1 AND 11),
    resource INTEGER NOT NULL CHECK(resource>=0),
    target INTEGER NOT NULL CHECK(target<>0),
    phase INTEGER NOT NULL DEFAULT 0 CHECK(phase>=0),
    cursor INTEGER NOT NULL DEFAULT 0 CHECK(cursor>=-1),
    aux INTEGER NOT NULL DEFAULT -1 CHECK(aux>=-1),
    name BLOB NOT NULL DEFAULT X'' CHECK(length(name)<=255),
    ready INTEGER NOT NULL DEFAULT 1 CHECK(ready IN(0,1)),
    PRIMARY KEY(ns,kind,resource,target)
) STRICT, WITHOUT ROWID;
CREATE INDEX maintenance_ready ON maintenance(ready,ns,kind,resource,target);
CREATE INDEX maintenance_generation ON maintenance(ns,target,kind,resource);


CREATE TABLE file_custody (
    ns INTEGER NOT NULL REFERENCES workspace(ns),
    serial INTEGER NOT NULL CHECK(serial>0),
    opens INTEGER NOT NULL DEFAULT 0 CHECK(opens>=0),
    lookups INTEGER NOT NULL DEFAULT 0 CHECK(lookups>=0),
    readers INTEGER NOT NULL DEFAULT 0 CHECK(readers>=0),
    PRIMARY KEY(ns,serial)
) STRICT, WITHOUT ROWID;
-- One open regular-file descriptor. A native descriptor names its mount and
-- the kernel request that received it (any 64 bits); any other has mount 0
-- and names its caller's request.
CREATE TABLE file_handle (
    ns INTEGER NOT NULL REFERENCES workspace(ns),
    mount INTEGER NOT NULL CHECK(mount>=0),
    request INTEGER NOT NULL CHECK(mount>0 OR request>0),
    owner INTEGER NOT NULL CHECK(owner>0),
    serial INTEGER NOT NULL CHECK(serial>0),
    writable INTEGER NOT NULL CHECK(writable IN(0,1)),
    PRIMARY KEY(ns,owner), UNIQUE(ns,mount,request)
) STRICT, WITHOUT ROWID;
CREATE TABLE file_read (
    ns INTEGER NOT NULL REFERENCES workspace(ns),
    request INTEGER NOT NULL CHECK(request<>0),
    owner INTEGER NOT NULL CHECK(owner>0),
    serial INTEGER NOT NULL CHECK(serial>0),
    PRIMARY KEY(ns,owner), UNIQUE(ns,request)
) STRICT, WITHOUT ROWID;
CREATE TABLE captured_reader (
    ns INTEGER NOT NULL REFERENCES workspace(ns),
    request INTEGER NOT NULL CHECK(request>0),
    owner INTEGER NOT NULL CHECK(owner>0),
    gen INTEGER NOT NULL CHECK(gen>0),
    revision INTEGER NOT NULL CHECK(revision>=0),
    base_root BLOB NOT NULL CHECK(length(base_root)=32),
    installed INTEGER NOT NULL CHECK(installed>=0),
    PRIMARY KEY(ns,owner), UNIQUE(ns,request)
) STRICT, WITHOUT ROWID;
CREATE TABLE orphan (
    ns INTEGER NOT NULL REFERENCES workspace(ns),
    serial INTEGER NOT NULL CHECK(serial>0),
    base_root BLOB NOT NULL CHECK(length(base_root)=32),
    lower_top INTEGER NOT NULL CHECK(lower_top>=0),
    lower_floor INTEGER NOT NULL CHECK(lower_floor>=0 AND lower_floor<=lower_top),
    PRIMARY KEY(ns,serial)
) STRICT, WITHOUT ROWID;

CREATE TABLE operation_owner (
    ns INTEGER NOT NULL REFERENCES workspace(ns),
    request INTEGER NOT NULL CHECK(request>0),
    owner INTEGER NOT NULL CHECK(owner>0),
    PRIMARY KEY(ns,owner), UNIQUE(ns,request)
) STRICT, WITHOUT ROWID;
CREATE TABLE owned_operation_record (
    ns INTEGER NOT NULL REFERENCES workspace(ns),
    operation INTEGER NOT NULL CHECK(operation>0),
    kind INTEGER NOT NULL CHECK(kind>=0),
    key INTEGER NOT NULL CHECK(key>=0),
    value BLOB NOT NULL CHECK(length(value)<=65536),
    PRIMARY KEY(ns,operation,kind,key)
) STRICT, WITHOUT ROWID;
CREATE TABLE indexed_operation_record (
    ns INTEGER NOT NULL REFERENCES workspace(ns),
    operation INTEGER NOT NULL CHECK(operation>0),
    file_scope BLOB NOT NULL CHECK(length(file_scope)=8),
    kind INTEGER NOT NULL CHECK(kind BETWEEN 0 AND 4294967295),
    key BLOB NOT NULL CHECK(length(key)=32),
    value BLOB NOT NULL CHECK(length(value)<=65536),
    PRIMARY KEY(ns,operation,file_scope,kind,key)
) STRICT, WITHOUT ROWID;
CREATE TABLE lookup_owner (
    ns INTEGER NOT NULL REFERENCES workspace(ns),
    request INTEGER NOT NULL CHECK(request>0),
    owner INTEGER NOT NULL CHECK(owner>0),
    serial INTEGER NOT NULL CHECK(serial>0),
    PRIMARY KEY(ns,owner), UNIQUE(ns,request)
) STRICT, WITHOUT ROWID;
CREATE TABLE orphan_wait (
    ns INTEGER NOT NULL REFERENCES workspace(ns),
    gen INTEGER NOT NULL CHECK(gen>0),
    serial INTEGER NOT NULL CHECK(serial>0),
    PRIMARY KEY(ns,gen,serial)
) STRICT, WITHOUT ROWID;
CREATE TABLE native_mount (
    ns INTEGER PRIMARY KEY REFERENCES workspace(ns),
    owner INTEGER NOT NULL CHECK(owner>0),
    root INTEGER NOT NULL CHECK(root>0),
    revoked INTEGER NOT NULL CHECK(revoked IN(0,1)),
    UNIQUE(ns,owner)
) STRICT;
CREATE TABLE native_lookup (
    ns INTEGER NOT NULL REFERENCES workspace(ns),
    mount INTEGER NOT NULL CHECK(mount>0),
    serial INTEGER NOT NULL CHECK(serial>0),
    owner INTEGER NOT NULL CHECK(owner>0),
    nlookup INTEGER NOT NULL CHECK(nlookup>=0),
    implicit INTEGER NOT NULL CHECK(implicit IN(0,1)),
    CHECK(nlookup>0 OR implicit=1),
    PRIMARY KEY(ns,mount,serial), UNIQUE(ns,owner)
) STRICT, WITHOUT ROWID;
CREATE TABLE native_source (
    ns INTEGER NOT NULL REFERENCES workspace(ns),
    mount INTEGER NOT NULL CHECK(mount>0),
    request BLOB NOT NULL CHECK(length(request)=8),
    owner INTEGER NOT NULL CHECK(owner>0),
    serial INTEGER NOT NULL CHECK(serial>0),
    decided INTEGER NOT NULL CHECK(decided IN(0,1)),
    PRIMARY KEY(ns,mount,request), UNIQUE(ns,owner)
) STRICT, WITHOUT ROWID;
CREATE TABLE native_read (
    ns INTEGER NOT NULL REFERENCES workspace(ns),
    mount INTEGER NOT NULL CHECK(mount>0),
    request BLOB NOT NULL CHECK(length(request)=8),
    owner INTEGER NOT NULL CHECK(owner>0),
    PRIMARY KEY(ns,mount,request), UNIQUE(ns,owner)
) STRICT, WITHOUT ROWID;
CREATE TABLE native_parent (
    ns INTEGER NOT NULL,
    mount INTEGER NOT NULL CHECK(mount>0),
    serial INTEGER NOT NULL CHECK(serial>0),
    parent INTEGER NOT NULL CHECK(parent>0),
    PRIMARY KEY(ns,serial),
    FOREIGN KEY(ns,serial) REFERENCES file_custody(ns,serial) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;
CREATE TABLE native_directory (
    ns INTEGER NOT NULL REFERENCES workspace(ns),
    mount INTEGER NOT NULL CHECK(mount>0),
    owner INTEGER NOT NULL CHECK(owner>0),
    serial INTEGER NOT NULL CHECK(serial>0),
    request BLOB NOT NULL CHECK(length(request)=8),
    closed INTEGER NOT NULL DEFAULT 0 CHECK(closed IN(0,1)),
    PRIMARY KEY(ns,owner), UNIQUE(ns,mount,request)
) STRICT, WITHOUT ROWID;
CREATE INDEX native_directory_open ON native_directory(ns,mount,owner) WHERE closed=0;
-- One row per published READDIR reply of one open directory: the names the
-- reply buffer accepted, each one byte of length and its bytes, in order.
-- Cookie first_cookie+i resumes strictly after the i-th name. `after` is
-- the name the reply was listed after (empty at the start).
CREATE TABLE native_cookie (
    ns INTEGER NOT NULL,
    owner INTEGER NOT NULL CHECK(owner>0),
    first_cookie INTEGER NOT NULL CHECK(first_cookie>=3),
    after BLOB NOT NULL CHECK(length(after)<=255),
    names BLOB NOT NULL CHECK(length(names)>=2 AND length(names)<=16384),
    PRIMARY KEY(ns,owner,first_cookie),
    FOREIGN KEY(ns,owner) REFERENCES native_directory(ns,owner)
) STRICT, WITHOUT ROWID;
CREATE INDEX native_cookie_after ON native_cookie(ns,owner,after,first_cookie);
PRAGMA user_version=29;
