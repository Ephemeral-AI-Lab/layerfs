use super::*;

pub(crate) fn capture_workspace_candidate(
    working: &WorkingStore,
    digest_cache: &SemanticDigestCache,
    workspace: &dyn ProjectionWorkspace,
    base_root: layerfs_core::ObjectId,
    operation_id: Option<layerfs_workspace::OperationId>,
) -> VfsResult<(layerfs_core::ObjectId, OperationCounters)> {
    let mut counters = OperationCounters::default();
    counters.native.route = Some(NativeRoute::CaptureStream);
    counters.authority_full_scans = 1;
    workspace.revalidate_root_binding()?;
    let root_handle = workspace.root_directory()?;
    let root_token = workspace.directory_token(root_handle.as_ref())?;
    let existing_table = working
        .create_scratch_table("existing-paths")
        .map_err(working_error)?;
    let existing = existing_table.namespace(b"paths")?;
    let prior_table = seed_existing_paths(working, base_root, &existing, None, &mut counters)?;
    let seeded_links_table = working
        .create_scratch_table("existing-hardlinks")
        .map_err(working_error)?;
    let seeded_links = seeded_links_table.namespace(b"links")?;
    seed_existing_hard_links(
        workspace,
        root_handle.as_ref(),
        &existing,
        &seeded_links,
        &[],
        true,
    )?;
    let mut writer = working.begin_candidate_write().map_err(working_error)?;
    let root_inode = match existing_inode(&existing, b"", InodeKind::Directory)? {
        Some(inode) => inode,
        None => CaptureStore::allocate_inode_id(&mut writer)?,
    };
    let mut table = None;
    let hard_links = working
        .create_scratch_table("hardlinks")
        .map_err(working_error)?;
    let entries = working
        .create_scratch_table("enumeration")
        .map_err(working_error)?;
    let mut next_directory = 0_u64;
    let root_metadata = workspace.read_root_metadata()?;
    capture_directory(
        workspace,
        root_handle.as_ref(),
        digest_cache,
        root_inode,
        true,
        root_metadata,
        &mut writer,
        &mut table,
        &hard_links,
        &entries,
        &existing,
        Some(&seeded_links),
        Some(&seeded_links),
        Some(prior_table),
        &[],
        &mut next_directory,
        &mut counters,
    )?;
    if workspace.directory_token(root_handle.as_ref())? != root_token {
        return Err(DriverError::Conflict.into());
    }
    workspace.revalidate_root_binding()?;
    hard_links
        .for_each(|bytes| {
            let link = HardLink::decode(bytes).map_err(|_| {
                layerfs_workspace::StorageError::InvalidRecord("hard-link scratch value")
            })?;
            if link.observed != link.expected {
                return Err(layerfs_workspace::StorageError::InvalidRecord(
                    "external hard-link boundary",
                ));
            }
            Ok(())
        })
        .map_err(|error| {
            if matches!(
                error,
                layerfs_workspace::StorageError::InvalidRecord("external hard-link boundary")
            ) {
                VfsError::ExternalHardLinkBoundary
            } else {
                error.into()
            }
        })?;
    let namespace = encode_namespace_root(NamespaceRootV1 {
        profile_id: profile_id(),
        root_directory_inode: root_inode,
        inode_table_root: table.ok_or(VfsError::InvalidState)?.into_root().0,
    })?;
    workspace.revalidate_root_binding()?;
    let root = writer.put(&namespace)?;
    match operation_id {
        Some(operation_id) => writer
            .commit_operation_candidate(operation_id, root)
            .map_err(working_error)?,
        None => writer.commit_candidate(root).map_err(working_error)?,
    };
    for scratch in [existing_table, seeded_links_table, hard_links, entries] {
        counters.add_scratch(scratch.finish()?)?;
    }
    Ok((root, counters))
}

pub(super) fn working_error(error: layerfs_workspace::WorkingError) -> VfsError {
    VfsError::Io(std::io::Error::other(error.to_string()))
}
