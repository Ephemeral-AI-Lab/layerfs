use crate::capture::{capture_workspace, live_hard_link_authority, SemanticDigestCache};
use crate::driver::*;
use crate::workspace::topology_edge_key;
use crate::workspace::{VfsError, VfsResult};
use crate::{NativeRoute, OperationCounters};
use layerfs_core::content::rope::{read_all, read_all_bounded, FileStateRoot, ObjectRead};
use layerfs_core::inode::{
    inode_table_lookup, InodeId, InodeKind, InodeTableCounters, InodeTableRoot,
};
use layerfs_core::metadata::{decode_apple_acl, visit_metadata_entries, SUPPORTED_BSD_FLAGS};
use layerfs_core::namespace::{visit_directory_entries, DirectoryStateRoot, NamespaceCounters};
use layerfs_core::namespace_codec::{decode_inode_record, decode_namespace_root, decode_symlink};
use layerfs_core::ObjectId;
use layerfs_engine::scratch::DiskTable;
use layerfs_engine::Engine;
use std::io::{BufWriter, Write};
use std::path::Path;

pub fn materialize(
    engine: &Engine,
    driver: &dyn ProjectionDriver,
    root: ObjectId,
    path: &Path,
) -> VfsResult<()> {
    let workspace = driver.open_workspace(
        path,
        WorkspacePolicy::ExternalCooperative,
        engine.store_id()?,
    )?;
    materialize_workspace(
        engine,
        &SemanticDigestCache::default(),
        workspace.as_ref(),
        root,
    )
    .map(drop)
}

pub(crate) fn materialize_workspace(
    engine: &Engine,
    digest_cache: &SemanticDigestCache,
    workspace: &dyn ProjectionWorkspace,
    root: ObjectId,
) -> VfsResult<(OperationCounters, DiskTable, DiskTable)> {
    let mut counters = OperationCounters::default();
    counters.native.route = Some(NativeRoute::MaterializeStream);
    workspace.revalidate_root_binding()?;
    let root_handle = workspace.root_directory()?;
    if workspace
        .enumerate_at(root_handle.as_ref())?
        .next()
        .is_some()
    {
        let expected = engine.read_ref("main")?.ok_or(VfsError::InvalidState)?;
        if expected.root != root {
            return Err(VfsError::ExternalDirtyConflict);
        }
        let (verified, mut counters) = capture_workspace(
            engine,
            digest_cache,
            workspace,
            Some(&expected),
            None,
            true,
            true,
        )?;
        if verified.root != root {
            return Err(VfsError::ExternalDirtyConflict);
        }
        workspace.revalidate_root_binding()?;
        counters.native.route = Some(NativeRoute::ExactNoop);
        let (authority, topology, authority_counters) =
            live_hard_link_authority(engine, workspace, root)?;
        return Ok((counters.merge(authority_counters)?, authority, topology));
    }
    let namespace = engine.with_authenticated_canonical(root, decode_namespace_root)?;
    let table = InodeTableRoot(namespace.inode_table_root);
    let root_record = record(engine, table, namespace.root_directory_inode, &mut counters)?;
    if root_record.kind != InodeKind::Directory {
        return Err(VfsError::InvalidState);
    }
    let links = DiskTable::create_near(engine.path(), "materialize-hardlinks")?;
    let authority = DiskTable::create_near(engine.path(), "materialize-live-hardlinks")?;
    let topology = DiskTable::create_near(engine.path(), "materialize-topology-edges")?;
    let current_path = Vec::new();
    materialize_directory(
        workspace,
        engine,
        table,
        namespace.root_directory_inode,
        DirectoryStateRoot(root_record.content_root),
        root_handle.as_ref(),
        root_handle.as_ref(),
        &links,
        &authority,
        &topology,
        &current_path,
        &mut counters,
    )?;
    workspace.set_root_metadata(&metadata(engine, root_record.metadata_root, &mut counters)?)?;
    counters.native.metadata_calls = checked_add(counters.native.metadata_calls, 1)?;
    workspace.sync_directory(root_handle.as_ref())?;
    counters.native.sync_calls = checked_add(counters.native.sync_calls, 1)?;
    workspace.revalidate_root_binding()?;
    counters.add_scratch(links.observation()?)?;
    counters.add_scratch(authority.observation()?)?;
    counters.add_scratch(topology.observation()?)?;
    Ok((counters, authority, topology))
}

#[allow(clippy::too_many_arguments)]
fn materialize_directory(
    workspace: &dyn ProjectionWorkspace,
    engine: &Engine,
    table: InodeTableRoot,
    directory_inode: InodeId,
    directory: DirectoryStateRoot,
    workspace_root: &dyn DirectoryHandle,
    parent: &dyn DirectoryHandle,
    links: &DiskTable,
    authority: &DiskTable,
    topology: &DiskTable,
    current_path: &[u8],
    counters: &mut OperationCounters,
) -> VfsResult<()> {
    let mut preflight = workspace.begin_name_preflight()?;
    let mut error = None;
    let mut namespace_counters = NamespaceCounters::default();
    let visited = visit_directory_entries(engine, directory, &mut namespace_counters, |entries| {
        for (name, _) in entries {
            if let Err(cause) = preflight.add(name.as_bytes()) {
                error = Some(VfsError::Driver(cause));
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
    preflight.finish()?;

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
                workspace,
                engine,
                table,
                workspace_root,
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
fn materialize_entry(
    workspace: &dyn ProjectionWorkspace,
    engine: &Engine,
    table: InodeTableRoot,
    workspace_root: &dyn DirectoryHandle,
    parent: &dyn DirectoryHandle,
    links: &DiskTable,
    authority: &DiskTable,
    topology: &DiskTable,
    current_path: &[u8],
    name: &[u8],
    inode: InodeId,
    counters: &mut OperationCounters,
) -> VfsResult<()> {
    let record = record(engine, table, inode, counters)?;
    let metadata = metadata(engine, record.metadata_root, counters)?;
    match record.kind {
        InodeKind::Directory => {
            let child = workspace.create_directory_at(parent, name)?;
            counters.native.create_calls = checked_add(counters.native.create_calls, 1)?;
            materialize_directory(
                workspace,
                engine,
                table,
                inode,
                DirectoryStateRoot(record.content_root),
                workspace_root,
                child.as_ref(),
                links,
                authority,
                topology,
                &child_path(current_path, name),
                counters,
            )?;
            let expected = workspace.directory_identity(child.as_ref())?;
            workspace.set_entry_metadata(parent, name, &expected, &metadata)?;
            counters.native.metadata_calls = checked_add(counters.native.metadata_calls, 1)?;
            workspace.sync_directory(child.as_ref())?;
            counters.native.sync_calls = checked_add(counters.native.sync_calls, 1)?;
        }
        InodeKind::RegularFile => {
            if let Some(value) = links.get(inode.as_bytes())? {
                let (remaining, source) = decode_link_state(&value)?;
                create_hard_link_from_path(workspace, workspace_root, source, parent, name)?;
                counters.native.hard_link_calls = checked_add(counters.native.hard_link_calls, 1)?;
                if remaining == 1 {
                    finish_hard_link_from_path(workspace, workspace_root, source, &metadata)?;
                    counters.native.metadata_calls =
                        checked_add(counters.native.metadata_calls, 1)?;
                    counters.native.sync_calls = checked_add(counters.native.sync_calls, 1)?;
                }
                links.put(inode.as_bytes(), &encode_link_state(remaining - 1, source))?;
            } else {
                let mut temp = workspace.create_temp_at(parent)?;
                counters.native.temp_calls = checked_add(counters.native.temp_calls, 1)?;
                let root = FileStateRoot(record.content_root);
                let mut output = BufWriter::with_capacity(1024 * 1024, temp.as_mut());
                let rope = read_all(engine, root, &mut output)?;
                output.flush()?;
                drop(output);
                counters.native.bytes_written = counters
                    .native
                    .bytes_written
                    .checked_add(rope.payload_bytes_read)
                    .ok_or(VfsError::InvalidState)?;
                counters.add_rope(rope)?;
                let mut representative_metadata = metadata.clone();
                if record.namespace_ref_count > 1 {
                    representative_metadata.bsd_flags = 0;
                }
                workspace.set_temp_metadata(temp.as_mut(), &representative_metadata)?;
                counters.native.metadata_calls = checked_add(counters.native.metadata_calls, 1)?;
                workspace.atomic_replace(temp, parent, name)?;
                counters.native.replace_calls = checked_add(counters.native.replace_calls, 1)?;
                counters.native.sync_calls = checked_add(counters.native.sync_calls, 2)?;
                if record.namespace_ref_count > 1 {
                    let path = child_path(current_path, name);
                    links.put(
                        inode.as_bytes(),
                        &encode_link_state(record.namespace_ref_count - 1, &path),
                    )?;
                }
            }
            let key = workspace.identity_at(parent, name)?;
            authority.put(&key, inode.as_bytes())?;
        }
        InodeKind::Symlink => {
            let link = engine.with_authenticated_canonical(record.content_root, decode_symlink)?;
            workspace.create_symlink_at(parent, name, &link.target, &metadata)?;
            counters.native.create_calls = checked_add(counters.native.create_calls, 1)?;
            counters.native.metadata_calls = checked_add(counters.native.metadata_calls, 1)?;
        }
    }
    Ok(())
}

fn checked_add(left: u64, right: u64) -> VfsResult<u64> {
    left.checked_add(right).ok_or(VfsError::InvalidState)
}

fn encode_link_state(remaining: u64, path: &[u8]) -> Vec<u8> {
    let mut value = Vec::with_capacity(8 + path.len());
    value.extend_from_slice(&remaining.to_be_bytes());
    value.extend_from_slice(path);
    value
}

fn decode_link_state(value: &[u8]) -> VfsResult<(u64, &[u8])> {
    let remaining = u64::from_be_bytes(
        value
            .get(..8)
            .ok_or(VfsError::InvalidState)?
            .try_into()
            .unwrap(),
    );
    let path = value.get(8..).ok_or(VfsError::InvalidState)?;
    if remaining == 0 || path.is_empty() {
        return Err(VfsError::InvalidState);
    }
    Ok((remaining, path))
}

fn child_path(parent: &[u8], name: &[u8]) -> Vec<u8> {
    let mut path = parent.to_vec();
    if !path.is_empty() {
        path.push(b'/');
    }
    path.extend_from_slice(name);
    path
}

fn create_hard_link_from_path(
    workspace: &dyn ProjectionWorkspace,
    current: &dyn DirectoryHandle,
    path: &[u8],
    target_parent: &dyn DirectoryHandle,
    target: &[u8],
) -> VfsResult<()> {
    let mut components = path.split(|byte| *byte == b'/');
    let first = components.next().ok_or(VfsError::InvalidState)?;
    create_hard_link_from_components(
        workspace,
        current,
        first,
        components.collect::<Vec<_>>().as_slice(),
        target_parent,
        target,
    )
}

fn create_hard_link_from_components(
    workspace: &dyn ProjectionWorkspace,
    current: &dyn DirectoryHandle,
    component: &[u8],
    remaining: &[&[u8]],
    target_parent: &dyn DirectoryHandle,
    target: &[u8],
) -> VfsResult<()> {
    if remaining.is_empty() {
        let expected = workspace.identity_at(current, component)?;
        return Ok(workspace.create_hard_link_at(
            current,
            component,
            &expected,
            target_parent,
            target,
        )?);
    }
    let expected = workspace.token_at(current, component)?;
    let child = workspace.open_directory_at(current, component, Some(&expected))?;
    create_hard_link_from_components(
        workspace,
        child.as_ref(),
        remaining[0],
        &remaining[1..],
        target_parent,
        target,
    )
}

fn finish_hard_link_from_path(
    workspace: &dyn ProjectionWorkspace,
    current: &dyn DirectoryHandle,
    path: &[u8],
    metadata: &NativeMetadata,
) -> VfsResult<()> {
    let mut components = path.split(|byte| *byte == b'/');
    let first = components.next().ok_or(VfsError::InvalidState)?;
    finish_hard_link_from_components(
        workspace,
        current,
        first,
        components.collect::<Vec<_>>().as_slice(),
        metadata,
    )
}

fn finish_hard_link_from_components(
    workspace: &dyn ProjectionWorkspace,
    current: &dyn DirectoryHandle,
    component: &[u8],
    remaining: &[&[u8]],
    metadata: &NativeMetadata,
) -> VfsResult<()> {
    if remaining.is_empty() {
        let expected = workspace.identity_at(current, component)?;
        return Ok(workspace.finish_hard_link_at(current, component, &expected, metadata)?);
    }
    let expected = workspace.token_at(current, component)?;
    let child = workspace.open_directory_at(current, component, Some(&expected))?;
    finish_hard_link_from_components(
        workspace,
        child.as_ref(),
        remaining[0],
        &remaining[1..],
        metadata,
    )
}

fn record(
    engine: &Engine,
    table: InodeTableRoot,
    inode: InodeId,
    counters: &mut OperationCounters,
) -> VfsResult<layerfs_core::inode::InodeRecordV1> {
    let mut inode_counters = InodeTableCounters::default();
    let id = inode_table_lookup(engine, table, inode, &mut inode_counters)?
        .ok_or(VfsError::InvalidState)?;
    counters.add_inode_table(inode_counters)?;
    Ok(engine.with_authenticated_canonical(id, decode_inode_record)?)
}

pub(crate) fn metadata(
    engine: &Engine,
    root: ObjectId,
    counters: &mut OperationCounters,
) -> VfsResult<NativeMetadata> {
    let mut mode = None;
    let mut seconds = None;
    let mut nanos = None;
    let mut xattrs = crate::driver::NativeXattrs::new();
    let mut acl = None;
    let mut flags = 0;
    visit_metadata_entries(engine, root, |entries| {
        for entry in entries {
            let file_root = FileStateRoot(entry.value_file_root);
            let mut value = Vec::new();
            let rope = read_all_bounded(engine, file_root, 1024 * 1024, &mut value)?;
            counters.add_metadata_rope(rope)?;
            match (entry.key.domain.as_str(), entry.key.key.as_slice()) {
                ("portable", b"mode") if value.len() == 4 => {
                    mode = Some(u32::from_be_bytes(value.try_into().unwrap()))
                }
                ("portable", b"mtime") if value.len() == 12 => {
                    seconds = Some(i64::from_be_bytes(value[..8].try_into().unwrap()));
                    nanos = Some(u32::from_be_bytes(value[8..].try_into().unwrap()));
                }
                ("apple.xattr", name) => {
                    xattrs.push(name, &value).map_err(|_| {
                        layerfs_core::CoreError::InvalidRecord("metadata xattr bytes")
                    })?;
                }
                ("apple.acl", b"") => {
                    decode_apple_acl(&value)?;
                    acl = Some(value);
                }
                ("apple.bsd-flags", b"") if value.len() == 4 => {
                    flags = u32::from_be_bytes(value.try_into().unwrap());
                    if flags & !SUPPORTED_BSD_FLAGS != 0 {
                        return Err(layerfs_core::CoreError::InvalidRecord("BSD flags"));
                    }
                }
                _ => return Err(layerfs_core::CoreError::InvalidRecord("metadata value")),
            }
        }
        Ok(())
    })?;
    let metadata = NativeMetadata {
        mode: mode.ok_or(VfsError::InvalidState)?,
        mtime_seconds: seconds.ok_or(VfsError::InvalidState)?,
        mtime_nanoseconds: nanos.ok_or(VfsError::InvalidState)?,
        xattrs,
        acl,
        bsd_flags: flags,
    };
    crate::managed_edit::spooled_metadata_len(&metadata)?;
    Ok(metadata)
}
