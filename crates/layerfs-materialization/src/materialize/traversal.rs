use super::*;

pub(super) enum MaterializeTarget<'a> {
    Native {
        workspace: &'a dyn ProjectionWorkspace,
        workspace_root: &'a dyn DirectoryHandle,
    },
}

#[allow(clippy::too_many_arguments)]
pub(super) fn visit_materialization_source(
    engine: &impl ObjectRead,
    root: ObjectId,
    target: &mut MaterializeTarget<'_>,
    root_parent: Option<&dyn DirectoryHandle>,
    links: &DiskNamespace<'_>,
    authority: &DiskNamespace<'_>,
    topology: &DiskNamespace<'_>,
    counters: &mut OperationCounters,
) -> VfsResult<NativeMetadata> {
    let namespace = engine.with_authenticated_canonical(root, decode_namespace_root)?;
    let table = InodeTableRoot(namespace.inode_table_root);
    let root_record = record(engine, table, namespace.root_directory_inode, counters)?;
    if root_record.kind != InodeKind::Directory {
        return Err(VfsError::InvalidState);
    }
    materialize_directory(
        target,
        engine,
        table,
        namespace.root_directory_inode,
        DirectoryStateRoot(root_record.content_root),
        root_parent,
        links,
        authority,
        topology,
        &[],
        counters,
    )?;
    metadata(engine, root_record.metadata_root, counters)
}
