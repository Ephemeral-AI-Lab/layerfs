use crate::inode::{
    inode_table_lookup, InodeId, InodeRecordV1, InodeTableCounters, InodeTableRoot,
};
use crate::namespace::{directory_lookup, DirectoryStateRoot, NamespaceCounters, NamespaceRootV1};
use crate::namespace_codec::{decode_inode_record, decode_namespace_root};
use crate::object::access::ObjectRead;
use crate::{CanonicalName, CanonicalPath, CoreError, CoreResult, ObjectId};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LogicalCounters {
    pub namespace: NamespaceCounters,
    pub inode_table: InodeTableCounters,
    pub rope: crate::content::rope::RopeCounters,
    pub structural_deferred_peak_bytes: u64,
    pub structural_deferred_prunes: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Resolved {
    pub inode: InodeId,
    pub record: InodeRecordV1,
}

pub fn namespace<S: ObjectRead>(store: &S, root: ObjectId) -> CoreResult<NamespaceRootV1> {
    store.with_authenticated_canonical(root, decode_namespace_root)
}

pub fn resolve<S: ObjectRead>(
    store: &S,
    root: ObjectId,
    path: &CanonicalPath,
    counters: &mut LogicalCounters,
) -> CoreResult<Resolved> {
    let namespace = namespace(store, root)?;
    let table = InodeTableRoot(namespace.inode_table_root);
    let mut inode = namespace.root_directory_inode;
    let mut record = load_record(store, table, inode, counters)?;
    for component in path.components() {
        if record.kind != crate::inode::InodeKind::Directory {
            return Err(CoreError::InvalidRecord(
                "path component is not a directory",
            ));
        }
        inode = directory_lookup(
            store,
            DirectoryStateRoot(record.content_root),
            &CanonicalName::from_bytes(component)?,
            &mut counters.namespace,
        )?
        .ok_or(CoreError::MissingObject)?;
        record = load_record(store, table, inode, counters)?;
    }
    Ok(Resolved { inode, record })
}

pub fn resolve_parent<S: ObjectRead>(
    store: &S,
    root: ObjectId,
    path: &CanonicalPath,
    counters: &mut LogicalCounters,
) -> CoreResult<(Resolved, CanonicalName)> {
    let bytes = path.as_bytes();
    let (name, parent) = bytes
        .iter()
        .rposition(|byte| *byte == b'/')
        .map_or((bytes, &[][..]), |separator| {
            (&bytes[separator + 1..], &bytes[..separator])
        });
    if name.is_empty() {
        return Err(CoreError::InvalidRecord("empty basename"));
    }
    let parent = resolve(store, root, &CanonicalPath::from_bytes(parent)?, counters)?;
    if parent.record.kind != crate::inode::InodeKind::Directory {
        return Err(CoreError::InvalidRecord("parent is not a directory"));
    }
    Ok((parent, CanonicalName::from_bytes(name)?))
}

fn load_record<S: ObjectRead>(
    store: &S,
    table: InodeTableRoot,
    inode: InodeId,
    counters: &mut LogicalCounters,
) -> CoreResult<InodeRecordV1> {
    let id = inode_table_lookup(store, table, inode, &mut counters.inode_table)?
        .ok_or(CoreError::MissingObject)?;
    store.with_authenticated_canonical(id, decode_inode_record)
}
