//! Fixed statement templates shared by ordinary jobs and plan diagnostics.
pub(crate) const INODE_LOOKUP: &str = "SELECT serial,kind,mode,mtime_ns,nlink,size,inherited_cutoff
    FROM inode WHERE ns=?1 AND serial=?2 AND gen<=?3 ORDER BY gen DESC LIMIT 1";
pub(crate) const INODE_CAPTURE: &str =
    "SELECT serial,kind,mode,mtime_ns,nlink,size,inherited_cutoff
    FROM inode INDEXED BY inode_capture WHERE ns=?1 AND gen=?2 AND serial>?3 ORDER BY serial LIMIT 64";
pub(crate) const CELL_LOOKUP: &str = "SELECT data,validity FROM payload
    WHERE ns=?1 AND serial=?2 AND gen=?3 AND cell_offset=?4";
pub(crate) const SCRATCH_PAGE: &str = "SELECT kind,key,value FROM scratch
    WHERE ns=?1 AND operation=?2 AND kind=?3 AND key>?4 ORDER BY key LIMIT 64";
