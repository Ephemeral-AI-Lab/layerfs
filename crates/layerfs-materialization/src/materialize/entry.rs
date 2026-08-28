use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn materialize_entry(
    target: &mut MaterializeTarget<'_>,
    engine: &impl ObjectRead,
    table: InodeTableRoot,
    parent: Option<&dyn DirectoryHandle>,
    links: &DiskNamespace<'_>,
    authority: &DiskNamespace<'_>,
    topology: &DiskNamespace<'_>,
    current_path: &[u8],
    name: &[u8],
    inode: InodeId,
    counters: &mut OperationCounters,
) -> VfsResult<()> {
    let record = record(engine, table, inode, counters)?;
    let metadata = metadata(engine, record.metadata_root, counters)?;
    match record.kind {
        InodeKind::Directory => {
            let MaterializeTarget::Native { workspace, .. } = &*target;
            let child =
                workspace.create_directory_at(parent.ok_or(VfsError::InvalidState)?, name)?;
            counters.native.create_calls = checked_add(counters.native.create_calls, 1)?;
            materialize_directory(
                target,
                engine,
                table,
                inode,
                DirectoryStateRoot(record.content_root),
                Some(child.as_ref()),
                links,
                authority,
                topology,
                &child_path(current_path, name),
                counters,
            )?;
            let MaterializeTarget::Native { workspace, .. } = target;
            let parent = parent.ok_or(VfsError::InvalidState)?;
            let expected = workspace.directory_identity(child.as_ref())?;
            workspace.set_entry_metadata(parent, name, &expected, &metadata)?;
            counters.native.metadata_calls = checked_add(counters.native.metadata_calls, 1)?;
            workspace.sync_directory(child.as_ref())?;
            counters.native.sync_calls = checked_add(counters.native.sync_calls, 1)?;
        }
        InodeKind::RegularFile => {
            let prior_link = if record.namespace_ref_count > 1 {
                links.get(inode.as_bytes())?
            } else {
                None
            };
            if let Some(value) = prior_link {
                let (remaining, source) = decode_link_state(&value)?;
                let MaterializeTarget::Native {
                    workspace,
                    workspace_root,
                } = target;
                create_hard_link_from_path(
                    *workspace,
                    *workspace_root,
                    source,
                    parent.ok_or(VfsError::InvalidState)?,
                    name,
                )?;
                counters.native.hard_link_calls = checked_add(counters.native.hard_link_calls, 1)?;
                if remaining == 1 {
                    finish_hard_link_from_path(*workspace, *workspace_root, source, &metadata)?;
                    counters.native.metadata_calls =
                        checked_add(counters.native.metadata_calls, 1)?;
                    counters.native.sync_calls = checked_add(counters.native.sync_calls, 1)?;
                }
                links.put(inode.as_bytes(), &encode_link_state(remaining - 1, source))?;
            } else {
                let root = FileStateRoot(record.content_root);
                let MaterializeTarget::Native { workspace, .. } = target;
                let mut representative_metadata = metadata.clone();
                if record.namespace_ref_count > 1 {
                    representative_metadata.bsd_flags = 0;
                }
                let rope = project_regular_file(
                    *workspace,
                    parent.ok_or(VfsError::InvalidState)?,
                    name,
                    &representative_metadata,
                    DirectoryDurability::DeferredToIncompleteTreeBoundary,
                    |output| {
                        let rope = read_all(engine, root, output)?;
                        Ok((rope, rope.payload_bytes_read))
                    },
                    counters,
                )?;
                counters.add_rope(rope)?;
                if record.namespace_ref_count > 1 {
                    let path = child_path(current_path, name);
                    links.put(
                        inode.as_bytes(),
                        &encode_link_state(record.namespace_ref_count - 1, &path),
                    )?;
                }
            }
            let MaterializeTarget::Native { workspace, .. } = target;
            let key = workspace.identity_at(parent.ok_or(VfsError::InvalidState)?, name)?;
            authority.put(&key, inode.as_bytes())?;
        }
        InodeKind::Symlink => {
            let link = engine.with_authenticated_canonical(record.content_root, decode_symlink)?;
            let MaterializeTarget::Native { workspace, .. } = target;
            workspace.create_symlink_at(
                parent.ok_or(VfsError::InvalidState)?,
                name,
                &link.target,
                &metadata,
            )?;
            counters.native.create_calls = checked_add(counters.native.create_calls, 1)?;
            counters.native.metadata_calls = checked_add(counters.native.metadata_calls, 1)?;
        }
    }
    Ok(())
}
