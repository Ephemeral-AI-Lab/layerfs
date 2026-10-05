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
pub(crate) const STEP_GET: &str =
    "SELECT cell_offset,epoch FROM shrink WHERE ns=?1 AND serial=?2 AND gen=?3 AND depth=?4";
pub(crate) const STEP_PUT: &str = "INSERT INTO shrink VALUES(?1,?2,?3,?4,?5,?6)
    ON CONFLICT(ns,serial,gen,depth) DO UPDATE SET cell_offset=excluded.cell_offset,
    epoch=excluded.epoch";
pub(crate) const LAYERS: &str = "SELECT gen,kind,size,inherited_cutoff,epoch,height FROM inode
    WHERE ns=?1 AND serial=?2 AND gen<=?3 AND gen>?4 ORDER BY gen DESC";
pub(crate) const LAYER_ACTIVE: &str = "SELECT size,inherited_cutoff,epoch,height FROM inode
    WHERE ns=?1 AND serial=?2 AND gen=?3";
pub(crate) const LAYER_LOWER: &str = "SELECT size FROM inode
    WHERE ns=?1 AND serial=?2 AND gen<?3 AND gen>?4 ORDER BY gen DESC LIMIT 1";
pub(crate) const DENTRY_LOOKUP:&str="SELECT serial FROM dentry WHERE ns=?1 AND parent=?2 AND name=?3 AND gen<=?4 AND gen>?5 ORDER BY gen DESC LIMIT 1";
pub(crate) const LEASE_LOOKUP: &str =
    "SELECT 1 FROM lease WHERE ns=?1 AND kind=?2 AND owner=?3 AND resource=?4";
pub(crate) const RETAINED_CAPTURE: &str = "SELECT captured,captured_revision,base_root
    FROM workspace WHERE ns=?1 AND incarnation=?2";
pub(crate) const PUBLICATION_PAGE: &str = "SELECT revision,gen FROM request
    WHERE ns=?1 AND revision>?2 ORDER BY revision LIMIT 64";
pub(crate) const BASE_SOURCE_LOOKUP: &str =
    "SELECT base_root FROM base_source WHERE ns=?1 AND owner=?2";
pub(crate) const BASE_SOURCE_INSERT: &str =
    "INSERT INTO base_source(ns,owner,base_root) VALUES(?1,?2,?3)";
pub(crate) const BASE_SOURCE_DELETE: &str =
    "DELETE FROM base_source WHERE ns=?1 AND owner=?2 AND base_root=?3";
pub(crate) const BASE_SOURCE_INCREMENT: &str =
    "UPDATE workspace SET base_readers=base_readers+1 WHERE ns=?1";
pub(crate) const BASE_SOURCE_DECREMENT: &str =
    "UPDATE workspace SET base_readers=base_readers-1 WHERE ns=?1";
pub(crate) const DENTRY_CAPTURE: &str = "SELECT parent,name,serial
    FROM dentry INDEXED BY dentry_capture WHERE ns=?1 AND gen=?2
    AND (parent,name)>(?3,?4) ORDER BY parent,name LIMIT 64";
pub(crate) const SOURCE_NAMES: &str =
    "SELECT parent,name,serial FROM dentry INDEXED BY dentry_capture
    WHERE ns=?1 AND gen=?2 AND parent=?3 AND name>?4 ORDER BY name LIMIT 64";
pub(crate) const SCRATCH_PAGE: &str = "SELECT kind,key,value FROM scratch
    WHERE ns=?1 AND operation=?2 AND kind=?3 AND key>?4 ORDER BY key LIMIT 64";
pub(crate) const INODE_PUT: &str = "INSERT INTO inode
    VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)
    ON CONFLICT(ns,serial,gen) DO UPDATE SET kind=excluded.kind,mode=excluded.mode,
    mtime_seconds=excluded.mtime_seconds,mtime_nanoseconds=excluded.mtime_nanoseconds,
    nlink=excluded.nlink,size=excluded.size,inherited_cutoff=excluded.inherited_cutoff,
    born=excluded.born,entries=excluded.entries,epoch=excluded.epoch,height=excluded.height";
pub(crate) const DENTRY_ACTIVE: &str =
    "SELECT serial FROM dentry WHERE ns=?1 AND parent=?2 AND name=?3 AND gen=?4";
pub(crate) const DENTRY_PUT: &str = "INSERT INTO dentry VALUES(?1,?2,?3,?4,?5)
    ON CONFLICT(ns,parent,name,gen) DO UPDATE SET serial=excluded.serial";
pub(crate) const DENTRY_DROP: &str =
    "DELETE FROM dentry WHERE ns=?1 AND parent=?2 AND name=?3 AND gen=?4";
pub(crate) const TICKET_PUT: &str = "INSERT INTO request(ns,revision,gen) VALUES(?1,?2,?3)";
pub(crate) const FRONTIER_ADVANCE: &str = "UPDATE workspace SET revision=?2,
    dirty_inodes=dirty_inodes+?3,dirty_names=dirty_names+?4 WHERE ns=?1";
