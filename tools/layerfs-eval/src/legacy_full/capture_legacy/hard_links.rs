use super::super::session_legacy::{topology_edge_key, VfsError, VfsResult};
use super::super::OperationCounters;
use layerfs_core::content::rope::ObjectRead;
use layerfs_core::inode::{
    inode_table_lookup, InodeId, InodeKind, InodeRecordV1, InodeTableCounters, InodeTableRoot,
};
use layerfs_core::namespace::{visit_directory_entries, DirectoryStateRoot, NamespaceCounters};
use layerfs_core::namespace_codec::{decode_inode_record, decode_namespace_root};
use layerfs_materialization::driver::*;
use layerfs_storage::scratch::{DiskNamespace, DiskTable};
use layerfs_storage::Engine;
pub(super) fn child_path(parent: &[u8], name: &[u8]) -> Vec<u8> {
    let mut path = parent.to_vec();
    if !path.is_empty() {
        path.push(b'/');
    }
    path.extend_from_slice(name);
    path
}

pub(super) fn encode_existing(inode: InodeId, kind: InodeKind) -> [u8; 33] {
    let mut value = [0_u8; 33];
    value[0] = kind as u8;
    value[1..].copy_from_slice(inode.as_bytes());
    value
}

pub(super) fn existing_inode(
    existing: &DiskNamespace<'_>,
    path: &[u8],
    kind: InodeKind,
) -> VfsResult<Option<InodeId>> {
    existing
        .get(path)?
        .map(|value| {
            if value.len() != 33 || value[0] != kind as u8 {
                return Ok(None);
            }
            Ok(Some(InodeId::from_slice(&value[1..])?))
        })
        .transpose()
        .map(Option::flatten)
}

pub(super) fn seed_existing_hard_links(
    workspace: &dyn ProjectionWorkspace,
    directory: &dyn DirectoryHandle,
    existing: &DiskNamespace<'_>,
    links: &DiskNamespace<'_>,
    current_path: &[u8],
    allow_ambiguous: bool,
) -> VfsResult<()> {
    for entry in workspace.enumerate_at(directory)? {
        let entry = entry?;
        let path = child_path(current_path, &entry.name);
        match entry.kind {
            NativeKind::Directory => {
                let child =
                    workspace.open_directory_at(directory, &entry.name, Some(&entry.token))?;
                seed_existing_hard_links(
                    workspace,
                    child.as_ref(),
                    existing,
                    links,
                    &path,
                    allow_ambiguous,
                )?;
            }
            NativeKind::RegularFile => {
                if let (Some(key), Some(inode)) = (
                    entry.hard_link_key,
                    existing_inode(existing, &path, InodeKind::RegularFile)?,
                ) {
                    if let Some(prior) = links.get(&key)? {
                        if prior.len() == 32 && prior.as_slice() != inode.as_bytes() {
                            if allow_ambiguous {
                                links.put(&key, &[])?;
                                continue;
                            }
                            return Err(VfsError::InvalidState);
                        }
                    } else {
                        links.put(&key, inode.as_bytes())?;
                    }
                }
            }
            NativeKind::Symlink => {}
        }
    }
    Ok(())
}

pub(crate) fn live_hard_link_authority(
    engine: &Engine,
    workspace: &dyn ProjectionWorkspace,
    root: layerfs_core::ObjectId,
) -> VfsResult<(DiskTable, OperationCounters)> {
    let mut counters = OperationCounters::default();
    let scratch = engine.create_scratch_table("live")?;
    let paths = scratch.namespace(b"paths")?;
    let topology = scratch.namespace(b"topology")?;
    let links = scratch.namespace(b"authority")?;
    seed_existing_paths(engine, root, &paths, Some(&topology), &mut counters)?;
    let directory = workspace.root_directory()?;
    seed_existing_hard_links(workspace, directory.as_ref(), &paths, &links, &[], false)?;
    super::super::add_scratch(&mut counters, scratch.observation()?)?;
    Ok((scratch, counters))
}

pub(super) fn seed_existing_paths(
    engine: &Engine,
    root: layerfs_core::ObjectId,
    paths: &DiskNamespace<'_>,
    topology: Option<&DiskNamespace<'_>>,
    counters: &mut OperationCounters,
) -> VfsResult<InodeTableRoot> {
    let namespace = engine.with_authenticated_canonical(root, decode_namespace_root)?;
    let table = InodeTableRoot(namespace.inode_table_root);
    seed_existing_directory(
        engine,
        table,
        namespace.root_directory_inode,
        Vec::new(),
        paths,
        topology,
        counters,
    )?;
    Ok(table)
}

pub(super) fn seed_existing_directory(
    engine: &Engine,
    table: InodeTableRoot,
    inode: InodeId,
    path: Vec<u8>,
    paths: &DiskNamespace<'_>,
    topology: Option<&DiskNamespace<'_>>,
    counters: &mut OperationCounters,
) -> VfsResult<()> {
    let record = existing_record(engine, table, inode, counters)?;
    if record.kind != InodeKind::Directory {
        return Err(VfsError::InvalidState);
    }
    paths.put(&path, &encode_existing(inode, record.kind))?;
    let mut callback_error = None;
    let mut namespace_counters = NamespaceCounters::default();
    let visited = visit_directory_entries(
        engine,
        DirectoryStateRoot(record.content_root),
        &mut namespace_counters,
        |entries| {
            for (name, child) in entries {
                if let Some(topology) = topology {
                    if let Err(error) =
                        topology.put(&topology_edge_key(*child, inode, name.as_bytes()), &[])
                    {
                        callback_error = Some(error.into());
                        return Err(layerfs_core::CoreError::Io);
                    }
                }
                let child_record = match existing_record(engine, table, *child, counters) {
                    Ok(record) => record,
                    Err(error) => {
                        callback_error = Some(error);
                        return Err(layerfs_core::CoreError::Io);
                    }
                };
                let child_path = child_path(&path, name.as_bytes());
                if child_record.kind == InodeKind::Directory {
                    if let Err(error) = seed_existing_directory(
                        engine, table, *child, child_path, paths, topology, counters,
                    ) {
                        callback_error = Some(error);
                        return Err(layerfs_core::CoreError::Io);
                    }
                } else if let Err(error) =
                    paths.put(&child_path, &encode_existing(*child, child_record.kind))
                {
                    callback_error = Some(error.into());
                    return Err(layerfs_core::CoreError::Io);
                }
            }
            Ok(())
        },
    );
    if let Some(error) = callback_error {
        return Err(error);
    }
    visited?;
    counters.add_namespace(namespace_counters)?;
    Ok(())
}

pub(super) fn existing_record<S: ObjectRead>(
    source: &S,
    table: InodeTableRoot,
    inode: InodeId,
    counters: &mut OperationCounters,
) -> VfsResult<InodeRecordV1> {
    let mut inode_counters = InodeTableCounters::default();
    let id = inode_table_lookup(source, table, inode, &mut inode_counters)?
        .ok_or(VfsError::InvalidState)?;
    counters.add_inode_table(inode_counters)?;
    Ok(source.with_authenticated_canonical(id, decode_inode_record)?)
}
