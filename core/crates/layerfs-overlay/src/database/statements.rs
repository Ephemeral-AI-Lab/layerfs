//! Fixed statement templates shared by ordinary jobs and plan diagnostics.
pub(crate) const INODE_LOOKUP: &str =
    "SELECT serial,kind,mode,mtime_seconds,mtime_nanoseconds,nlink,size,inherited_cutoff,born,entries,subdirs
    FROM inode WHERE ns=?1 AND serial=?2 AND gen<=?3 AND gen>?4 ORDER BY gen DESC LIMIT 1";
pub(crate) const INODE_CAPTURE: &str =
    "SELECT serial,kind,mode,mtime_seconds,mtime_nanoseconds,nlink,size,inherited_cutoff,born,entries,subdirs
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
/// The active row and the latest lower row of one name in one seek: the row
/// whose generation is the upper bound is active, the first below it is lower.
pub(crate) const NAME_LAYERS: &str = "SELECT gen,serial,inherited FROM directory_entry
    WHERE ns=?1 AND parent=?2 AND name=?3 AND gen<=?4 AND gen>?5 ORDER BY gen DESC LIMIT 2";
pub(crate) const DIRECTORY_ENTRY_LOOKUP:&str="SELECT serial,inherited FROM directory_entry WHERE ns=?1 AND parent=?2 AND name=?3 AND gen<=?4 AND gen>?5 ORDER BY gen DESC LIMIT 1";
pub(crate) const LEASE_LOOKUP: &str =
    "SELECT 1 FROM lease WHERE ns=?1 AND kind=?2 AND owner=?3 AND resource=?4";
pub(crate) const RETAINED_CAPTURE: &str = "SELECT captured,captured_revision,base_root
    FROM workspace WHERE ns=?1 AND incarnation=?2";
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
/// The same window with the kind of each bound name's latest local inode
/// row at the current view, or NULL when the inode has none: one seek per
/// returned name inside this one statement.
pub(crate) const SOURCE_NAMES_KINDS: &str =
    "SELECT e.parent,e.name,e.serial,e.inherited,(SELECT i.kind FROM inode i
    WHERE i.ns=e.ns AND i.serial=e.serial AND i.gen<=?5 AND i.gen>?6 ORDER BY i.gen DESC LIMIT 1)
    FROM directory_entry e INDEXED BY directory_entry_capture
    WHERE e.ns=?1 AND e.gen=?2 AND e.parent=?3 AND e.name>?4 ORDER BY e.name LIMIT 64";
/// The published reply that holds one cookie of an open directory.
pub(crate) const COOKIE_PAGE: &str = "SELECT first_cookie,names FROM native_cookie
    WHERE ns=?1 AND owner=?2 AND first_cookie<=?3 ORDER BY first_cookie DESC LIMIT 1";
/// The latest published reply of an open directory listed after one name.
pub(crate) const COOKIE_PAGE_AFTER: &str = "SELECT first_cookie,names FROM native_cookie
    INDEXED BY native_cookie_after WHERE ns=?1 AND owner=?2 AND after=?3
    ORDER BY first_cookie DESC LIMIT 1";
/// At most one more than the replies a RELEASEDIR deletes in its own job.
pub(crate) const COOKIE_PAGES: &str = "SELECT first_cookie,length(names) FROM native_cookie
    WHERE ns=?1 AND owner=?2 AND first_cookie>?3 ORDER BY first_cookie LIMIT 9";
pub(crate) const CAPTURED_DIRECTORY_ENTRY: &str =
    "SELECT serial,inherited FROM directory_entry WHERE ns=?1 AND parent=?2 AND name=?3 AND gen=?4";
pub(crate) const OPERATION_RECORD_PAGE: &str = "SELECT kind,key,value FROM operation_record
    WHERE ns=?1 AND operation=?2 AND kind=?3 AND key>?4 ORDER BY key LIMIT 64";
pub(crate) const INODE_PUT: &str = "INSERT INTO inode
    VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)
    ON CONFLICT(ns,serial,gen) DO UPDATE SET kind=excluded.kind,mode=excluded.mode,
    mtime_seconds=excluded.mtime_seconds,mtime_nanoseconds=excluded.mtime_nanoseconds,
    nlink=excluded.nlink,size=excluded.size,inherited_cutoff=excluded.inherited_cutoff,
    born=excluded.born,entries=excluded.entries,subdirs=excluded.subdirs,
    epoch=excluded.epoch,height=excluded.height";
pub(crate) const DIRECTORY_ENTRY_ACTIVE: &str =
    "SELECT serial,inherited FROM directory_entry WHERE ns=?1 AND parent=?2 AND name=?3 AND gen=?4";
pub(crate) const DIRECTORY_ENTRY_PUT: &str = "INSERT INTO directory_entry VALUES(?1,?2,?3,?4,?5,?6)
    ON CONFLICT(ns,parent,name,gen) DO UPDATE SET serial=excluded.serial,inherited=excluded.inherited";
pub(crate) const DIRECTORY_ENTRY_DROP: &str =
    "DELETE FROM directory_entry WHERE ns=?1 AND parent=?2 AND name=?3 AND gen=?4";
pub(crate) const FRONTIER_ADVANCE: &str = "UPDATE workspace SET revision=?2,
    dirty_inodes=dirty_inodes+?3,dirty_directory_entries=dirty_directory_entries+?4 WHERE ns=?1";

pub(crate) const ORPHAN_LOOKUP: &str =
    "SELECT base_root,lower_top,lower_floor FROM orphan WHERE ns=?1 AND serial=?2";

pub(crate) const FILE_REFS: &str =
    "SELECT opens+lookups+readers FROM file_custody WHERE ns=?1 AND serial=?2";

pub(crate) const FILE_OPENS_ADD: &str = "INSERT INTO file_custody(ns,serial,opens) VALUES(?1,?2,1)
    ON CONFLICT(ns,serial) DO UPDATE SET opens=opens+1";
/// Every decrement returns the references that remain, so the last one is
/// known without a second read.
pub(crate) const FILE_OPENS_DROP: &str = "UPDATE file_custody SET opens=opens-1
    WHERE ns=?1 AND serial=?2 AND opens>0 RETURNING opens+lookups+readers";
/// The descriptor and the kernel lookup reference of one created-and-opened
/// file, as one row write.
pub(crate) const FILE_OPEN_LOOKUP_ADD: &str =
    "INSERT INTO file_custody(ns,serial,opens,lookups) VALUES(?1,?2,1,1)
    ON CONFLICT(ns,serial) DO UPDATE SET opens=opens+1,lookups=lookups+1";
pub(crate) const FILE_LOOKUPS_ADD: &str =
    "INSERT INTO file_custody(ns,serial,lookups) VALUES(?1,?2,1)
    ON CONFLICT(ns,serial) DO UPDATE SET lookups=lookups+1";
pub(crate) const FILE_LOOKUPS_DROP: &str = "UPDATE file_custody SET lookups=lookups-1
    WHERE ns=?1 AND serial=?2 AND lookups>0 RETURNING opens+lookups+readers";
pub(crate) const FILE_READERS_ADD: &str =
    "INSERT INTO file_custody(ns,serial,readers) VALUES(?1,?2,1)
    ON CONFLICT(ns,serial) DO UPDATE SET readers=readers+1";
pub(crate) const FILE_READERS_DROP: &str = "UPDATE file_custody SET readers=readers-1
    WHERE ns=?1 AND serial=?2 AND readers>0 RETURNING opens+lookups+readers";

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

/// The fence of one native visit: the Workspace row, its native connection
/// and the kernel's own reference on the inode the request names, in one
/// statement. The last column is NULL when that reference is not held.
macro_rules! fence {
    ($held:literal) => {
        concat!(
            "SELECT w.active,w.captured,w.revision,w.base_root,w.dirty_inodes,",
            "w.dirty_directory_entries,w.lifecycle,w.installed,w.captured_revision,",
            "w.base_readers,w.consolidating,m.owner,m.root,m.revoked,(",
            $held,
            ") FROM workspace w LEFT JOIN native_mount m ON m.ns=w.ns ",
            "WHERE w.ns=?1 AND w.incarnation=?2"
        )
    };
}
/// A kernel lookup count on the inode.
pub(crate) const FENCE_LOOKUP: &str =
    fence!("SELECT 1 FROM native_lookup l WHERE l.ns=w.ns AND l.mount=?3 AND l.serial=?4");
/// An open regular-file descriptor of the inode; the column is its access mode.
pub(crate) const FENCE_FILE: &str = fence!(
    "SELECT f.writable FROM native_file n JOIN file_handle f ON f.ns=n.ns AND f.owner=n.owner WHERE n.ns=w.ns AND n.mount=?3 AND n.owner=?5 AND f.serial=?4"
);
/// An open file or directory descriptor of the inode.
pub(crate) const FENCE_HANDLE: &str = fence!(
    "CASE WHEN EXISTS(SELECT 1 FROM native_file n JOIN file_handle f ON f.ns=n.ns AND f.owner=n.owner WHERE n.ns=w.ns AND n.mount=?3 AND n.owner=?5 AND f.serial=?4) OR EXISTS(SELECT 1 FROM native_directory d WHERE d.ns=w.ns AND d.owner=?5 AND d.mount=?3 AND d.serial=?4 AND d.closed=0) THEN 1 END"
);
/// An open directory descriptor of the inode.
pub(crate) const FENCE_DIRECTORY: &str = fence!(
    "SELECT 1 FROM native_directory d WHERE d.ns=w.ns AND d.owner=?5 AND d.mount=?3 AND d.serial=?4 AND d.closed=0"
);
