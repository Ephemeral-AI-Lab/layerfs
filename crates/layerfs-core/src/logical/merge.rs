use super::mutate::CandidateRoot;
use super::resolver::{namespace, LogicalCounters};
use crate::inode::{merge_inode_tables, InodeId, InodeTableDiff, InodeTableRoot};
use crate::namespace::NamespaceRootV1;
use crate::namespace_codec::encode_namespace_root;
use crate::object::access::ObjectStore;
use crate::{CoreError, CoreResult, ObjectId};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MergeConflict {
    pub inode: InodeId,
    pub source: Option<ObjectId>,
    pub destination: Option<ObjectId>,
}

/// Applies the exact three-root rule for one inode-table change. Root-level
/// traversal remains streaming in `diff`; callers never need a complete inode
/// inventory to classify a change.
pub fn merge_inode_change(
    source: InodeTableDiff,
    destination: Option<ObjectId>,
) -> CoreResult<std::result::Result<Option<ObjectId>, MergeConflict>> {
    if destination == source.before {
        Ok(Ok(source.after))
    } else if destination == source.after {
        Ok(Ok(destination))
    } else {
        Ok(Err(MergeConflict {
            inode: source.inode,
            source: source.after,
            destination,
        }))
    }
}

pub fn merge_roots<S: ObjectStore>(
    store: &mut S,
    base: ObjectId,
    source: ObjectId,
    destination: ObjectId,
) -> CoreResult<std::result::Result<CandidateRoot, MergeConflict>> {
    let base_namespace = namespace(store, base)?;
    let source_namespace = namespace(store, source)?;
    let destination_namespace = namespace(store, destination)?;
    if [source_namespace, destination_namespace]
        .into_iter()
        .any(|namespace| {
            namespace.profile_id != base_namespace.profile_id
                || namespace.root_directory_inode != base_namespace.root_directory_inode
        })
    {
        return Err(CoreError::InvalidRecord("namespace identity mismatch"));
    }
    let merged = merge_inode_tables(
        store,
        InodeTableRoot(base_namespace.inode_table_root),
        InodeTableRoot(source_namespace.inode_table_root),
        InodeTableRoot(destination_namespace.inode_table_root),
    )?;
    let (table, inode_table, namespace) = match merged {
        Ok(merged) => merged,
        Err(conflict) => {
            return Ok(Err(MergeConflict {
                inode: conflict.inode,
                source: conflict.source,
                destination: conflict.destination,
            }))
        }
    };
    let root = store.put(&encode_namespace_root(NamespaceRootV1 {
        inode_table_root: table.0,
        ..destination_namespace
    })?)?;
    Ok(Ok(CandidateRoot::new(
        destination,
        root,
        LogicalCounters {
            inode_table,
            namespace,
            ..LogicalCounters::default()
        },
    )))
}
