use super::super::session_legacy::{VfsError, VfsResult};
use super::super::OperationCounters;
use layerfs_core::content::rope::{ObjectRead, ReadPlan};
use layerfs_core::inode::{
    inode_table_lookup, InodeId, InodeKind, InodeRecordV1, InodeTableCounters, InodeTableRoot,
};
use layerfs_core::namespace::{
    directory_lookup, DirectoryStateRoot, NamespaceCounters, NamespaceRootV1,
};
use layerfs_core::namespace_codec::{decode_inode_record, decode_namespace_root};
use layerfs_core::{CanonicalName, CanonicalPath, ObjectId};
use std::sync::{Arc, Mutex};

#[derive(Default)]
pub(crate) struct ResolvedReadCache(Mutex<Option<ResolvedRead>>);

struct ResolvedRead {
    root: ObjectId,
    path: CanonicalPath,
    plan: Arc<ReadPlan>,
}

impl ResolvedReadCache {
    pub(super) fn get(
        &self,
        root: ObjectId,
        path: &CanonicalPath,
    ) -> VfsResult<Option<Arc<ReadPlan>>> {
        let cache = self.0.lock().map_err(|_| VfsError::InvalidState)?;
        Ok(cache
            .as_ref()
            .filter(|entry| entry.root == root && entry.path == *path)
            .map(|entry| entry.plan.clone()))
    }

    pub(super) fn put(
        &self,
        root: ObjectId,
        path: &CanonicalPath,
        plan: Arc<ReadPlan>,
    ) -> VfsResult<()> {
        *self.0.lock().map_err(|_| VfsError::InvalidState)? = Some(ResolvedRead {
            root,
            path: path.clone(),
            plan,
        });
        Ok(())
    }
}

pub(crate) fn namespace<S: ObjectRead>(store: &S, root: ObjectId) -> VfsResult<NamespaceRootV1> {
    Ok(store.with_authenticated_canonical(root, decode_namespace_root)?)
}

pub(crate) fn resolve<S: ObjectRead>(
    store: &S,
    namespace: NamespaceRootV1,
    path: &CanonicalPath,
    counters: &mut OperationCounters,
) -> VfsResult<(InodeId, InodeRecordV1)> {
    let table = InodeTableRoot(namespace.inode_table_root);
    let mut inode = namespace.root_directory_inode;
    let mut record = load_record(store, table, inode, counters)?;
    for component in path.components() {
        if record.kind != InodeKind::Directory {
            return Err(VfsError::InvalidState);
        }
        let mut visits = NamespaceCounters::default();
        inode = directory_lookup(
            store,
            DirectoryStateRoot(record.content_root),
            &CanonicalName::from_bytes(component)?,
            &mut visits,
        )?
        .ok_or(VfsError::InvalidState)?;
        counters.add_namespace(visits)?;
        record = load_record(store, table, inode, counters)?;
    }
    Ok((inode, record))
}

pub(super) fn resolve_parent<S: ObjectRead>(
    store: &S,
    namespace: NamespaceRootV1,
    path: &CanonicalPath,
    counters: &mut OperationCounters,
) -> VfsResult<(InodeId, InodeRecordV1, CanonicalName)> {
    let bytes = path.as_bytes();
    let (name, parent) = bytes
        .iter()
        .rposition(|byte| *byte == b'/')
        .map_or((bytes, &[][..]), |separator| {
            (&bytes[separator + 1..], &bytes[..separator])
        });
    if name.is_empty() {
        return Err(VfsError::InvalidState);
    }
    let (inode, record) = resolve(
        store,
        namespace,
        &CanonicalPath::from_bytes(parent)?,
        counters,
    )?;
    if record.kind != InodeKind::Directory {
        return Err(VfsError::InvalidState);
    }
    Ok((inode, record, CanonicalName::from_bytes(name)?))
}

pub(super) fn load_record<S: ObjectRead>(
    store: &S,
    table: InodeTableRoot,
    inode: InodeId,
    counters: &mut OperationCounters,
) -> VfsResult<InodeRecordV1> {
    let mut visits = InodeTableCounters::default();
    let id = inode_table_lookup(store, table, inode, &mut visits)?.ok_or(VfsError::InvalidState)?;
    counters.add_inode_table(visits)?;
    Ok(store.with_authenticated_canonical(id, decode_inode_record)?)
}
