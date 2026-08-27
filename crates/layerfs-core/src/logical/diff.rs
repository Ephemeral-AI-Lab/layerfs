use super::resolver::{namespace, LogicalCounters};
use crate::inode::{diff_inode_table_entries, InodeId, InodeTableDiff, InodeTableRoot};
use crate::object::access::ObjectRead;
use crate::{CoreError, CoreResult, ObjectId};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RootDiff {
    pub inode: InodeId,
    pub before: Option<ObjectId>,
    pub after: Option<ObjectId>,
}

pub fn diff_roots<S: ObjectRead>(
    store: &S,
    old: ObjectId,
    new: ObjectId,
    mut visitor: impl FnMut(RootDiff) -> CoreResult<()>,
) -> CoreResult<LogicalCounters> {
    let old = namespace(store, old)?;
    let new = namespace(store, new)?;
    if old.profile_id != new.profile_id || old.root_directory_inode != new.root_directory_inode {
        return Err(CoreError::InvalidRecord("namespace identity mismatch"));
    }
    let inode_table = diff_inode_table_entries(
        store,
        InodeTableRoot(old.inode_table_root),
        InodeTableRoot(new.inode_table_root),
        |InodeTableDiff {
             inode,
             before,
             after,
         }| {
            visitor(RootDiff {
                inode,
                before,
                after,
            })
        },
    )?;
    Ok(LogicalCounters {
        inode_table,
        ..LogicalCounters::default()
    })
}
