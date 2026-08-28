use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn capture_directory(
    workspace: &dyn ProjectionWorkspace,
    directory: &dyn DirectoryHandle,
    digest_cache: &SemanticDigestCache,
    inode: InodeId,
    root: bool,
    native_metadata: NativeMetadata,
    publication: &mut impl CaptureStore,
    table: &mut Option<GeneratedInodeTable>,
    hard_links: &DiskTable,
    entries: &DiskTable,
    existing: &DiskNamespace<'_>,
    existing_links: Option<&DiskNamespace<'_>>,
    prior_links: Option<&DiskNamespace<'_>>,
    prior_table: Option<InodeTableRoot>,
    current_path: &[u8],
    next_directory: &mut u64,
    counters: &mut OperationCounters,
) -> VfsResult<()> {
    let mut state = empty_directory(publication)?;
    let directory_key = next_directory.to_be_bytes();
    *next_directory = next_directory
        .checked_add(1)
        .ok_or(VfsError::InvalidState)?;
    for entry in workspace.enumerate_at(directory)? {
        let entry = entry?;
        let mut key = directory_key.to_vec();
        key.extend_from_slice(&entry.name);
        entries.enqueue_once(&key, &encode_entry(&entry)?)?;
    }
    while let Some((key, encoded)) = entries.pop_pending_prefix(&directory_key)? {
        let entry = decode_entry(key[8..].to_vec(), &encoded)?;
        if workspace.token_at(directory, &entry.name)? != entry.token {
            return Err(DriverError::Conflict.into());
        }
        let name = layerfs_core::CanonicalName::from_bytes(&entry.name)?;
        let path = child_path(current_path, &entry.name);
        let child_inode = match entry.kind {
            NativeKind::Directory => {
                let child_inode = match existing_inode(existing, &path, InodeKind::Directory)? {
                    Some(inode) => inode,
                    None => publication.allocate_inode_id()?,
                };
                let metadata =
                    workspace.read_metadata_at(directory, &entry.name, Some(&entry.token))?;
                let child =
                    workspace.open_directory_at(directory, &entry.name, Some(&entry.token))?;
                capture_directory(
                    workspace,
                    child.as_ref(),
                    digest_cache,
                    child_inode,
                    false,
                    metadata,
                    publication,
                    table,
                    hard_links,
                    entries,
                    existing,
                    existing_links,
                    prior_links,
                    prior_table,
                    &path,
                    next_directory,
                    counters,
                )?;
                child_inode
            }
            NativeKind::RegularFile => capture_regular(
                workspace,
                directory,
                digest_cache,
                &entry,
                &path,
                publication,
                table,
                hard_links,
                existing,
                existing_links,
                prior_links,
                prior_table,
                counters,
            )?,
            NativeKind::Symlink => {
                let child_inode = match existing_inode(existing, &path, InodeKind::Symlink)? {
                    Some(inode) => inode,
                    None => publication.allocate_inode_id()?,
                };
                let target = workspace.read_link_at(directory, &entry.name, Some(&entry.token))?;
                let content = publication.put(&encode_symlink(
                    &layerfs_core::namespace::SymlinkStateV1::new(target)?,
                )?)?;
                let metadata = put_metadata_observed(
                    publication,
                    InodeKind::Symlink,
                    &workspace.read_metadata_at(directory, &entry.name, Some(&entry.token))?,
                    counters,
                )?;
                put_record(
                    publication,
                    table,
                    child_inode,
                    InodeRecordV1 {
                        kind: InodeKind::Symlink,
                        namespace_ref_count: 1,
                        content_root: content,
                        metadata_root: metadata,
                    },
                    counters,
                )?;
                child_inode
            }
        };
        if workspace.token_at(directory, &entry.name)? != entry.token {
            return Err(DriverError::Conflict.into());
        }
        let (next, namespace_counters) = directory_insert(publication, state, name, child_inode)?;
        counters.add_namespace(namespace_counters)?;
        state = next;
    }
    let metadata = put_metadata_observed(
        publication,
        InodeKind::Directory,
        &native_metadata,
        counters,
    )?;
    put_record(
        publication,
        table,
        inode,
        InodeRecordV1 {
            kind: InodeKind::Directory,
            namespace_ref_count: if root { 0 } else { 1 },
            content_root: state.0,
            metadata_root: metadata,
        },
        counters,
    )
}
