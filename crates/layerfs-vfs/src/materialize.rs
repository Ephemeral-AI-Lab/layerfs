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
use layerfs_engine::scratch::{DiskNamespace, DiskTable};
use layerfs_engine::Engine;
use std::io::{BufWriter, Read, Write};
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
) -> VfsResult<(OperationCounters, DiskTable)> {
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
        let (live_scratch, authority_counters) = live_hard_link_authority(engine, workspace, root)?;
        return Ok((counters.merge(authority_counters)?, live_scratch));
    }
    let scratch = engine.create_scratch_table("materialize")?;
    let links = scratch.namespace(b"hard-links")?;
    let authority = scratch.namespace(b"authority")?;
    let topology = scratch.namespace(b"topology")?;
    let mut target = MaterializeTarget::Native {
        workspace,
        workspace_root: root_handle.as_ref(),
    };
    let root_metadata = visit_materialization_source(
        engine,
        root,
        &mut target,
        Some(root_handle.as_ref()),
        &links,
        &authority,
        &topology,
        &mut counters,
    )?;
    workspace.set_root_metadata(&root_metadata)?;
    counters.native.metadata_calls = checked_add(counters.native.metadata_calls, 1)?;
    workspace.sync_directory(root_handle.as_ref())?;
    counters.native.sync_calls = checked_add(counters.native.sync_calls, 1)?;
    workspace.revalidate_root_binding()?;
    counters.add_scratch(scratch.observation()?)?;
    Ok((counters, scratch))
}

/// Runs the exact canonical source side of full materialization and sends each
/// unique regular-file payload to `output`, without opening a native workspace.
pub fn materialize_authenticated_to<W: Write>(
    engine: &Engine,
    root: ObjectId,
    mut output: W,
) -> VfsResult<OperationCounters> {
    let mut counters = OperationCounters::default();
    let scratch = engine.create_scratch_table("materialize")?;
    let links = scratch.namespace(b"hard-links")?;
    let authority = scratch.namespace(b"authority")?;
    let topology = scratch.namespace(b"topology")?;
    let mut target = MaterializeTarget::Sink(&mut output);
    visit_materialization_source(
        engine,
        root,
        &mut target,
        None,
        &links,
        &authority,
        &topology,
        &mut counters,
    )?;
    output.flush()?;
    counters.add_scratch(scratch.observation()?)?;
    Ok(counters)
}

impl crate::workspace::LayerVfs {
    pub fn materialize_authenticated_to<W: Write>(
        &self,
        root: ObjectId,
        output: W,
    ) -> VfsResult<OperationCounters> {
        let reservation = self.operation_q.reserve();
        let mut counters = materialize_authenticated_to(&self.engine, root, output)?;
        reservation.finish(&mut counters);
        Ok(counters)
    }

    pub fn native_durable_output<R: Read>(
        &self,
        path: &Path,
        name: &[u8],
        metadata: &NativeMetadata,
        logical_len: u64,
        input: R,
    ) -> VfsResult<OperationCounters> {
        let reservation = self.operation_q.reserve();
        let mut counters = native_durable_output(
            self.projection_driver(),
            self.engine.store_id()?,
            path,
            name,
            metadata,
            logical_len,
            input,
        )?;
        reservation.finish(&mut counters);
        Ok(counters)
    }
}

/// Projects one exact-length bounded stream through the same native regular-file
/// temp, metadata, install, and sync route used by full materialization.
pub fn native_durable_output<R: Read>(
    driver: &dyn ProjectionDriver,
    store_id: [u8; 32],
    path: &Path,
    name: &[u8],
    metadata: &NativeMetadata,
    logical_len: u64,
    input: R,
) -> VfsResult<OperationCounters> {
    let projection_before = driver.projection_facts();
    let workspace = driver.open_workspace(path, WorkspacePolicy::ExternalCooperative, store_id)?;
    workspace.revalidate_root_binding()?;
    let root = workspace.root_directory()?;
    if workspace
        .enumerate_at(root.as_ref())?
        .next()
        .transpose()?
        .is_some()
    {
        return Err(VfsError::InvalidState);
    }
    let mut preflight = workspace.begin_name_preflight()?;
    preflight.add(name)?;
    preflight.finish()?;

    let mut counters = OperationCounters::default();
    counters.native.route = Some(NativeRoute::NativeDurableOutput);
    let mut input = input.take(logical_len);
    project_regular_file(
        workspace.as_ref(),
        root.as_ref(),
        name,
        metadata,
        DirectoryDurability::ImmediateDirectoryDurability,
        |output| {
            let written = std::io::copy(&mut input, output)?;
            if written != logical_len {
                return Err(VfsError::Io(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "native durable source ended before its declared length",
                )));
            }
            Ok(((), written))
        },
        &mut counters,
    )?;
    workspace.revalidate_root_binding()?;
    counters.projection = driver
        .projection_facts()
        .checked_delta(projection_before)
        .ok_or(VfsError::InvalidState)?;
    Ok(counters)
}

enum MaterializeTarget<'a> {
    Native {
        workspace: &'a dyn ProjectionWorkspace,
        workspace_root: &'a dyn DirectoryHandle,
    },
    Sink(&'a mut dyn Write),
}

#[allow(clippy::too_many_arguments)]
fn visit_materialization_source(
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
fn materialize_directory(
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
fn materialize_entry(
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

fn project_regular_file<T>(
    workspace: &dyn ProjectionWorkspace,
    parent: &dyn DirectoryHandle,
    name: &[u8],
    metadata: &NativeMetadata,
    requested_directory_durability: DirectoryDurability,
    write: impl FnOnce(&mut dyn Write) -> VfsResult<(T, u64)>,
    counters: &mut OperationCounters,
) -> VfsResult<T> {
    let mut temp = workspace.create_temp_at(parent)?;
    counters.native.temp_calls = checked_add(counters.native.temp_calls, 1)?;
    let mut output = BufWriter::with_capacity(1024 * 1024, temp.as_mut());
    let (result, written) = write(&mut output)?;
    output.flush()?;
    drop(output);
    counters.native.bytes_written = checked_add(counters.native.bytes_written, written)?;
    workspace.set_temp_metadata(temp.as_mut(), metadata)?;
    counters.native.metadata_calls = checked_add(counters.native.metadata_calls, 1)?;
    let achieved_directory_durability = workspace.atomic_replace_with_directory_durability(
        temp,
        parent,
        name,
        requested_directory_durability,
    )?;
    counters.native.replace_calls = checked_add(counters.native.replace_calls, 1)?;
    let file_syncs = 1 + u64::from(metadata.bsd_flags != 0);
    let directory_syncs = u64::from(
        achieved_directory_durability == DirectoryDurability::ImmediateDirectoryDurability,
    );
    counters.native.sync_calls =
        checked_add(counters.native.sync_calls, file_syncs + directory_syncs)?;
    Ok(result)
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
