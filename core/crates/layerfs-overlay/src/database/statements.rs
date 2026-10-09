//! Fixed statement templates shared by ordinary jobs and plan diagnostics.
pub(crate) const INODE_LOOKUP: &str =
    "SELECT serial,kind,mode,mtime_seconds,mtime_nanoseconds,nlink,size,inherited_cutoff,born,entries
    FROM inode WHERE ns=?1 AND serial=?2 AND gen<=?3 AND gen>?4 ORDER BY gen DESC LIMIT 1";
pub(crate) const INODE_CAPTURE: &str =
    "SELECT serial,kind,mode,mtime_seconds,mtime_nanoseconds,nlink,size,inherited_cutoff,born,entries
    FROM inode INDEXED BY inode_capture WHERE ns=?1 AND gen=?2 AND serial>?3 ORDER BY serial LIMIT 64";
pub(crate) const CELL_LOOKUP: &str = "SELECT epoch,data,validity FROM payload
    WHERE ns=?1 AND serial=?2 AND gen=?3 AND cell_offset=?4";
pub(crate) const CELL_PUT: &str =
    "INSERT INTO payload(ns,serial,gen,cell_offset,epoch,data,validity) VALUES(?1,?2,?3,?4,?5,?6,?7)
    ON CONFLICT(ns,serial,gen,cell_offset) DO UPDATE SET epoch=excluded.epoch,
    data=excluded.data,validity=excluded.validity";
pub(crate) const CELL_DROP: &str =
    "DELETE FROM payload WHERE ns=?1 AND serial=?2 AND gen=?3 AND cell_offset=?4";
pub(crate) const CELL_RANGE: &str = "SELECT cell_offset,epoch,data,validity FROM payload
    WHERE ns=?1 AND serial=?2 AND gen=?3 AND cell_offset>=?4 AND cell_offset<?5
    ORDER BY cell_offset";
pub(crate) const CAPTURED_CELL_METADATA: &str =
    "SELECT cell_offset,epoch,length(data),length(validity) FROM payload
    WHERE ns=?1 AND serial=?2 AND gen=?3 AND cell_offset>=?4 AND cell_offset<?5
    ORDER BY cell_offset LIMIT 1";
pub(crate) const STEP_GET: &str =
    "SELECT cell_offset,epoch FROM shrink WHERE ns=?1 AND serial=?2 AND gen=?3 AND depth=?4";
pub(crate) const STEP_PUT: &str = "INSERT INTO shrink VALUES(?1,?2,?3,?4,?5,?6)
    ON CONFLICT(ns,serial,gen,depth) DO UPDATE SET cell_offset=excluded.cell_offset,
    epoch=excluded.epoch";
pub(crate) const LAYERS: &str =
    "SELECT gen,kind,size,inherited_cutoff,epoch,height,nlink FROM inode
    WHERE ns=?1 AND serial=?2 AND gen<=?3 AND gen>?4 ORDER BY gen DESC LIMIT 4";
pub(crate) const LAYER_ACTIVE: &str = "SELECT size,inherited_cutoff,epoch,height FROM inode
    WHERE ns=?1 AND serial=?2 AND gen=?3";
pub(crate) const LAYER_LOWER: &str = "SELECT size FROM inode
    WHERE ns=?1 AND serial=?2 AND gen<?3 AND gen>?4 ORDER BY gen DESC LIMIT 1";
pub(crate) const DIRECTORY_ENTRY_LOOKUP:&str="SELECT serial,inherited FROM directory_entry WHERE ns=?1 AND parent=?2 AND name=?3 AND gen<=?4 AND gen>?5 ORDER BY gen DESC LIMIT 1";
pub(crate) const LEASE_LOOKUP: &str =
    "SELECT 1 FROM lease WHERE ns=?1 AND kind=?2 AND owner=?3 AND resource=?4";
pub(crate) const RETAINED_CAPTURE: &str = "SELECT captured,captured_revision,base_root
    FROM workspace WHERE ns=?1 AND incarnation=?2";
pub(crate) const PUBLICATION_PAGE: &str = "SELECT revision,gen FROM request
    WHERE ns=?1 AND revision>?2 ORDER BY revision LIMIT 64";
pub(crate) const BASE_SOURCE_LOOKUP: &str =
    "SELECT base_root FROM base_source WHERE ns=?1 AND owner=?2 AND kind=?3";
pub(crate) const BASE_SOURCE_INSERT: &str =
    "INSERT INTO base_source(ns,owner,base_root,kind) VALUES(?1,?2,?3,?4)";
pub(crate) const BASE_SOURCE_DELETE: &str =
    "DELETE FROM base_source WHERE ns=?1 AND owner=?2 AND base_root=?3 AND kind=?4";
pub(crate) const BASE_SOURCE_INCREMENT: &str =
    "UPDATE workspace SET base_readers=base_readers+1 WHERE ns=?1";
pub(crate) const BASE_SOURCE_DECREMENT: &str =
    "UPDATE workspace SET base_readers=base_readers-1 WHERE ns=?1";
pub(crate) const DIRECTORY_ENTRY_CAPTURE: &str = "SELECT parent,name,serial,inherited
    FROM directory_entry INDEXED BY directory_entry_capture WHERE ns=?1 AND gen=?2
    AND (parent,name)>(?3,?4) ORDER BY parent,name LIMIT 64";
pub(crate) const SOURCE_NAMES: &str =
    "SELECT parent,name,serial,inherited FROM directory_entry INDEXED BY directory_entry_capture
    WHERE ns=?1 AND gen=?2 AND parent=?3 AND name>?4 ORDER BY name LIMIT 64";
pub(crate) const CAPTURED_DIRECTORY_ENTRY: &str =
    "SELECT serial,inherited FROM directory_entry WHERE ns=?1 AND parent=?2 AND name=?3 AND gen=?4";
pub(crate) const OPERATION_RECORD_PAGE: &str = "SELECT kind,key,value FROM operation_record
    WHERE ns=?1 AND operation=?2 AND kind=?3 AND key>?4 ORDER BY key LIMIT 64";
pub(crate) const INODE_PUT: &str = "INSERT INTO inode
    VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)
    ON CONFLICT(ns,serial,gen) DO UPDATE SET kind=excluded.kind,mode=excluded.mode,
    mtime_seconds=excluded.mtime_seconds,mtime_nanoseconds=excluded.mtime_nanoseconds,
    nlink=excluded.nlink,size=excluded.size,inherited_cutoff=excluded.inherited_cutoff,
    born=excluded.born,entries=excluded.entries,epoch=excluded.epoch,height=excluded.height";
pub(crate) const DIRECTORY_ENTRY_ACTIVE: &str =
    "SELECT serial,inherited FROM directory_entry WHERE ns=?1 AND parent=?2 AND name=?3 AND gen=?4";
pub(crate) const DIRECTORY_ENTRY_PUT: &str = "INSERT INTO directory_entry VALUES(?1,?2,?3,?4,?5,?6)
    ON CONFLICT(ns,parent,name,gen) DO UPDATE SET serial=excluded.serial,inherited=excluded.inherited";
pub(crate) const DIRECTORY_ENTRY_DROP: &str =
    "DELETE FROM directory_entry WHERE ns=?1 AND parent=?2 AND name=?3 AND gen=?4";
pub(crate) const TICKET_PUT: &str = "INSERT INTO request(ns,revision,gen) VALUES(?1,?2,?3)";
pub(crate) const FRONTIER_ADVANCE: &str = "UPDATE workspace SET revision=?2,
    dirty_inodes=dirty_inodes+?3,dirty_directory_entries=dirty_directory_entries+?4 WHERE ns=?1";

pub(crate) const ORPHAN_LOOKUP: &str =
    "SELECT base_root,lower_top,lower_floor FROM orphan WHERE ns=?1 AND serial=?2";

pub(crate) const FILE_REFS: &str =
    "SELECT opens+lookups+readers FROM file_custody WHERE ns=?1 AND serial=?2";

pub(crate) const FILE_OPENS_ADD: &str = "INSERT INTO file_custody(ns,serial,opens) VALUES(?1,?2,1)
    ON CONFLICT(ns,serial) DO UPDATE SET opens=opens+1";
pub(crate) const FILE_OPENS_DROP: &str = "UPDATE file_custody SET opens=opens-1
    WHERE ns=?1 AND serial=?2 AND opens>0";
pub(crate) const FILE_LOOKUPS_ADD: &str =
    "INSERT INTO file_custody(ns,serial,lookups) VALUES(?1,?2,1)
    ON CONFLICT(ns,serial) DO UPDATE SET lookups=lookups+1";
pub(crate) const FILE_LOOKUPS_DROP: &str = "UPDATE file_custody SET lookups=lookups-1
    WHERE ns=?1 AND serial=?2 AND lookups>0";
pub(crate) const FILE_READERS_ADD: &str =
    "INSERT INTO file_custody(ns,serial,readers) VALUES(?1,?2,1)
    ON CONFLICT(ns,serial) DO UPDATE SET readers=readers+1";
pub(crate) const FILE_READERS_DROP: &str = "UPDATE file_custody SET readers=readers-1
    WHERE ns=?1 AND serial=?2 AND readers>0";

pub(crate) const OPERATION_RECORD_DELETE: &str =
    "DELETE FROM operation_record WHERE ns=?1 AND operation=?2 AND kind=?3 AND key=?4";
pub(crate) const OWNED_OPERATION_RECORD_DELETE: &str =
    "DELETE FROM owned_operation_record WHERE ns=?1 AND operation=?2 AND kind=?3 AND key=?4";

pub(crate) const OPERATION_CUSTODY: &str = "SELECT 1 FROM operation_owner WHERE ns=?1 AND owner=?2";

pub(crate) const INDEXED_OPERATION_RECORD_CONTAINS: &str = "SELECT 1 FROM indexed_operation_record
    WHERE ns=?1 AND operation=?2 AND file_scope=?3 AND kind=?4 AND key=?5";
pub(crate) const INDEXED_OPERATION_RECORD_GET: &str = "SELECT value FROM indexed_operation_record
    WHERE ns=?1 AND operation=?2 AND file_scope=?3 AND kind=?4 AND key=?5";
pub(crate) const INDEXED_OPERATION_RECORD_PUT: &str =
    "INSERT INTO indexed_operation_record VALUES(?1,?2,?3,?4,?5,?6)
    ON CONFLICT(ns,operation,file_scope,kind,key) DO UPDATE SET value=excluded.value";
pub(crate) const INDEXED_OPERATION_RECORD_DELETE: &str = "DELETE FROM indexed_operation_record
    WHERE ns=?1 AND operation=?2 AND file_scope=?3 AND kind=?4 AND key=?5";
pub(crate) const INDEXED_OPERATION_RECORD_KEYS: &str = "SELECT key FROM indexed_operation_record
    WHERE ns=?1 AND operation=?2 AND file_scope=?3 AND kind=?4 AND key<>?5
    ORDER BY key LIMIT 64";
pub(crate) const INDEXED_OPERATION_RECORD_ALL_KEYS: &str =
    "SELECT key FROM indexed_operation_record
    WHERE ns=?1 AND operation=?2 AND file_scope=?3 AND kind=?4
    ORDER BY key LIMIT 64";
pub(crate) const INDEXED_OPERATION_RECORD_KEYS_AFTER: &str =
    "SELECT key FROM indexed_operation_record
    WHERE ns=?1 AND operation=?2 AND file_scope=?3 AND kind=?4 AND key>?5
    ORDER BY key LIMIT 64";
pub(crate) const INDEXED_OPERATION_RECORD_RECLAIM_OPERATION: &str =
    "SELECT file_scope,kind,key,length(value)
    FROM indexed_operation_record WHERE ns=?1 AND operation=?2 ORDER BY file_scope,kind,key LIMIT 64";
pub(crate) const INDEXED_OPERATION_RECORD_RECLAIM_NAMESPACE: &str =
    "SELECT operation,file_scope,kind,key,length(value)
    FROM indexed_operation_record WHERE ns=?1 ORDER BY operation,file_scope,kind,key LIMIT 64";

pub(crate) const GENERATION_HELD: &str = "SELECT 1 FROM lease INDEXED BY lease_resource
            WHERE ns=?1 AND kind IN(1,6) AND resource=?2 LIMIT 1";
