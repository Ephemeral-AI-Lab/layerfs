use super::super::session_legacy::topology_edge_key;
use super::super::session_legacy::{VfsError, VfsResult};
use super::super::OperationCounters;
use layerfs_core::content::rope::{read_all, FileStateRoot, ObjectRead};
use layerfs_core::inode::{InodeId, InodeKind, InodeTableRoot};
use layerfs_core::namespace::{visit_directory_entries, DirectoryStateRoot, NamespaceCounters};
use layerfs_core::namespace_codec::{decode_namespace_root, decode_symlink};
use layerfs_core::ObjectId;
use layerfs_materialization::driver::*;
use layerfs_storage::scratch::DiskNamespace;
use layerfs_storage::Engine;
use std::io::Write;

use super::output::{
    checked_add, child_path, create_hard_link_from_path, decode_link_state, encode_link_state,
    finish_hard_link_from_path, metadata, project_regular_file, record,
};

pub(super) enum MaterializeTarget<'a> {
    Native {
        workspace: &'a dyn ProjectionWorkspace,
        workspace_root: &'a dyn DirectoryHandle,
    },
    Sink(&'a mut dyn Write),
}

#[allow(clippy::too_many_arguments)]
pub(super) fn visit_materialization_source(
    engine: &Engine,
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

#[allow(clippy::too_many_arguments)]
pub(super) fn materialize_directory(
    target: &mut MaterializeTarget<'_>,
    engine: &Engine,
    table: InodeTableRoot,
    directory_inode: InodeId,
    directory: DirectoryStateRoot,
    parent: Option<&dyn DirectoryHandle>,
    links: &DiskNamespace<'_>,
    authority: &DiskNamespace<'_>,
    topology: &DiskNamespace<'_>,
    current_path: &[u8],
    counters: &mut OperationCounters,
) -> VfsResult<()> {
    let mut preflight = match target {
        MaterializeTarget::Native { workspace, .. } => Some(workspace.begin_name_preflight()?),
        MaterializeTarget::Sink(_) => None,
    };
    let mut error = None;
    let mut namespace_counters = NamespaceCounters::default();
    let visited = visit_directory_entries(engine, directory, &mut namespace_counters, |entries| {
        for (name, _) in entries {
            if let Some(preflight) = preflight.as_mut() {
                if let Err(cause) = preflight.add(name.as_bytes()) {
                    error = Some(VfsError::Driver(cause));
                    return Err(layerfs_core::CoreError::Io);
                }
            }
        }
        Ok(())
    });
    if let Some(error) = error {
        return Err(error);
    }
    visited?;
    counters.add_namespace(namespace_counters)?;
    if let Some(preflight) = preflight {
        preflight.finish()?;
    }

    let mut error = None;
    let mut namespace_counters = NamespaceCounters::default();
    let visited = visit_directory_entries(engine, directory, &mut namespace_counters, |entries| {
        for (name, inode) in entries {
            if let Err(cause) = topology.put(
                &topology_edge_key(*inode, directory_inode, name.as_bytes()),
                &[],
            ) {
                error = Some(cause.into());
                return Err(layerfs_core::CoreError::Io);
            }
            if let Err(cause) = materialize_entry(
                target,
                engine,
                table,
                parent,
                links,
                authority,
                topology,
                current_path,
                name.as_bytes(),
                *inode,
                counters,
            ) {
                error = Some(cause);
                return Err(layerfs_core::CoreError::Io);
            }
        }
        Ok(())
    });
    if let Some(error) = error {
        return Err(error);
    }
    visited?;
    counters.add_namespace(namespace_counters)?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn materialize_entry(
    target: &mut MaterializeTarget<'_>,
    engine: &Engine,
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
            let child = match &*target {
                MaterializeTarget::Native { workspace, .. } => {
                    let child = workspace
                        .create_directory_at(parent.ok_or(VfsError::InvalidState)?, name)?;
                    counters.native.create_calls = checked_add(counters.native.create_calls, 1)?;
                    Some(child)
                }
                MaterializeTarget::Sink(_) => None,
            };
            materialize_directory(
                target,
                engine,
                table,
                inode,
                DirectoryStateRoot(record.content_root),
                child.as_deref(),
                links,
                authority,
                topology,
                &child_path(current_path, name),
                counters,
            )?;
            if let MaterializeTarget::Native { workspace, .. } = target {
                let child = child.as_deref().ok_or(VfsError::InvalidState)?;
                let parent = parent.ok_or(VfsError::InvalidState)?;
                let expected = workspace.directory_identity(child)?;
                workspace.set_entry_metadata(parent, name, &expected, &metadata)?;
                counters.native.metadata_calls = checked_add(counters.native.metadata_calls, 1)?;
                workspace.sync_directory(child)?;
                counters.native.sync_calls = checked_add(counters.native.sync_calls, 1)?;
            }
        }
        InodeKind::RegularFile => {
            let prior_link = if record.namespace_ref_count > 1 {
                links.get(inode.as_bytes())?
            } else {
                None
            };
            if let Some(value) = prior_link {
                let (remaining, source) = decode_link_state(&value)?;
                if let MaterializeTarget::Native {
                    workspace,
                    workspace_root,
                } = target
                {
                    create_hard_link_from_path(
                        *workspace,
                        *workspace_root,
                        source,
                        parent.ok_or(VfsError::InvalidState)?,
                        name,
                    )?;
                    counters.native.hard_link_calls =
                        checked_add(counters.native.hard_link_calls, 1)?;
                    if remaining == 1 {
                        finish_hard_link_from_path(*workspace, *workspace_root, source, &metadata)?;
                        counters.native.metadata_calls =
                            checked_add(counters.native.metadata_calls, 1)?;
                        counters.native.sync_calls = checked_add(counters.native.sync_calls, 1)?;
                    }
                }
                links.put(inode.as_bytes(), &encode_link_state(remaining - 1, source))?;
            } else {
                let root = FileStateRoot(record.content_root);
                let rope = match target {
                    MaterializeTarget::Native { workspace, .. } => {
                        let mut representative_metadata = metadata.clone();
                        if record.namespace_ref_count > 1 {
                            representative_metadata.bsd_flags = 0;
                        }
                        project_regular_file(
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
                        )?
                    }
                    MaterializeTarget::Sink(output) => read_all(engine, root, &mut **output)?,
                };
                counters.add_rope(rope)?;
                if record.namespace_ref_count > 1 {
                    let path = child_path(current_path, name);
                    links.put(
                        inode.as_bytes(),
                        &encode_link_state(record.namespace_ref_count - 1, &path),
                    )?;
                }
            }
            let key = match target {
                MaterializeTarget::Native { workspace, .. } => {
                    workspace.identity_at(parent.ok_or(VfsError::InvalidState)?, name)?
                }
                MaterializeTarget::Sink(_) => inode.as_bytes().to_vec(),
            };
            authority.put(&key, inode.as_bytes())?;
        }
        InodeKind::Symlink => {
            let link = engine.with_authenticated_canonical(record.content_root, decode_symlink)?;
            if let MaterializeTarget::Native { workspace, .. } = target {
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
    }
    Ok(())
}
