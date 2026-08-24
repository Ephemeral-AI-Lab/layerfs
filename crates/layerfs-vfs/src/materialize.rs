use crate::capture::capture_workspace;
use crate::driver::*;
use crate::workspace::{VfsError, VfsResult};
use layerfs_core::content::rope::{read_all, read_all_bounded, FileStateRoot};
use layerfs_core::inode::{
    inode_table_lookup, InodeId, InodeKind, InodeTableCounters, InodeTableRoot,
};
use layerfs_core::metadata::{decode_apple_acl, visit_metadata_entries, SUPPORTED_BSD_FLAGS};
use layerfs_core::namespace::{visit_directory_entries, DirectoryStateRoot, NamespaceCounters};
use layerfs_core::namespace_codec::{decode_inode_record, decode_namespace_root, decode_symlink};
use layerfs_core::ObjectId;
use layerfs_engine::scratch::DiskTable;
use layerfs_engine::Engine;
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
    materialize_workspace(engine, workspace.as_ref(), root)
}

pub(crate) fn materialize_workspace(
    engine: &Engine,
    workspace: &dyn ProjectionWorkspace,
    root: ObjectId,
) -> VfsResult<()> {
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
        let verified = capture_workspace(engine, workspace, Some(&expected), None, true, true)?;
        if verified.root != root {
            return Err(VfsError::ExternalDirtyConflict);
        }
        workspace.revalidate_root_binding()?;
        return Ok(());
    }
    let namespace = decode_namespace_root(&engine.load_object(root)?.canonical_bytes)?;
    let table = InodeTableRoot(namespace.inode_table_root);
    let root_record = record(engine, table, namespace.root_directory_inode)?;
    if root_record.kind != InodeKind::Directory {
        return Err(VfsError::InvalidState);
    }
    let links = DiskTable::create_near(engine.path(), "materialize-hardlinks")?;
    let current_path = Vec::new();
    materialize_directory(
        workspace,
        engine,
        table,
        DirectoryStateRoot(root_record.content_root),
        root_handle.as_ref(),
        root_handle.as_ref(),
        &links,
        &current_path,
    )?;
    workspace.set_root_metadata(&metadata(engine, root_record.metadata_root)?)?;
    workspace.sync_directory(root_handle.as_ref())?;
    workspace.revalidate_root_binding()?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn materialize_directory(
    workspace: &dyn ProjectionWorkspace,
    engine: &Engine,
    table: InodeTableRoot,
    directory: DirectoryStateRoot,
    workspace_root: &dyn DirectoryHandle,
    parent: &dyn DirectoryHandle,
    links: &DiskTable,
    current_path: &[u8],
) -> VfsResult<()> {
    let mut preflight = workspace.begin_name_preflight()?;
    let mut error = None;
    let visited = visit_directory_entries(
        engine,
        directory,
        &mut NamespaceCounters::default(),
        |entries| {
            for (name, _) in entries {
                if let Err(cause) = preflight.add(name.as_bytes()) {
                    error = Some(VfsError::Driver(cause));
                    return Err(layerfs_core::CoreError::Io);
                }
            }
            Ok(())
        },
    );
    if let Some(error) = error {
        return Err(error);
    }
    visited?;
    preflight.finish()?;

    let mut error = None;
    let visited = visit_directory_entries(
        engine,
        directory,
        &mut NamespaceCounters::default(),
        |entries| {
            for (name, inode) in entries {
                if let Err(cause) = materialize_entry(
                    workspace,
                    engine,
                    table,
                    workspace_root,
                    parent,
                    links,
                    current_path,
                    name.as_bytes(),
                    *inode,
                ) {
                    error = Some(cause);
                    return Err(layerfs_core::CoreError::Io);
                }
            }
            Ok(())
        },
    );
    if let Some(error) = error {
        return Err(error);
    }
    visited?;
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
    current_path: &[u8],
    name: &[u8],
    inode: InodeId,
) -> VfsResult<()> {
    let record = record(engine, table, inode)?;
    let metadata = metadata(engine, record.metadata_root)?;
    match record.kind {
        InodeKind::Directory => {
            let child = workspace.create_directory_at(parent, name)?;
            materialize_directory(
                workspace,
                engine,
                table,
                DirectoryStateRoot(record.content_root),
                workspace_root,
                child.as_ref(),
                links,
                &child_path(current_path, name),
            )?;
            let expected = workspace.directory_identity(child.as_ref())?;
            workspace.set_entry_metadata(parent, name, &expected, &metadata)?;
            workspace.sync_directory(child.as_ref())?;
        }
        InodeKind::RegularFile => {
            if let Some(value) = links.get(inode.as_bytes())? {
                let (remaining, source) = decode_link_state(&value)?;
                create_hard_link_from_path(workspace, workspace_root, source, parent, name)?;
                if remaining == 1 {
                    finish_hard_link_from_path(workspace, workspace_root, source, &metadata)?;
                }
                links.put(inode.as_bytes(), &encode_link_state(remaining - 1, source))?;
            } else {
                let mut temp = workspace.create_temp_at(parent)?;
                let root = FileStateRoot(record.content_root);
                read_all(engine, root, &mut temp)?;
                let mut representative_metadata = metadata.clone();
                if record.namespace_ref_count > 1 {
                    representative_metadata.bsd_flags = 0;
                }
                workspace.set_temp_metadata(temp.as_mut(), &representative_metadata)?;
                workspace.atomic_replace(temp, parent, name)?;
                if record.namespace_ref_count > 1 {
                    let path = child_path(current_path, name);
                    links.put(
                        inode.as_bytes(),
                        &encode_link_state(record.namespace_ref_count - 1, &path),
                    )?;
                }
            }
        }
        InodeKind::Symlink => {
            let link = decode_symlink(&engine.load_object(record.content_root)?.canonical_bytes)?;
            workspace.create_symlink_at(parent, name, &link.target, &metadata)?;
        }
    }
    Ok(())
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
) -> VfsResult<layerfs_core::inode::InodeRecordV1> {
    let id = inode_table_lookup(engine, table, inode, &mut InodeTableCounters::default())?
        .ok_or(VfsError::InvalidState)?;
    Ok(decode_inode_record(
        &engine.load_object(id)?.canonical_bytes,
    )?)
}

fn metadata(engine: &Engine, root: ObjectId) -> VfsResult<NativeMetadata> {
    let mut mode = None;
    let mut seconds = None;
    let mut nanos = None;
    let mut xattrs = Vec::new();
    let mut xattr_bytes = 0_usize;
    let mut acl = None;
    let mut flags = 0;
    visit_metadata_entries(engine, root, |entries| {
        for entry in entries {
            let file_root = FileStateRoot(entry.value_file_root);
            let mut value = Vec::new();
            read_all_bounded(engine, file_root, 1024 * 1024, &mut value)?;
            match (entry.key.domain.as_str(), entry.key.key.as_slice()) {
                ("portable", b"mode") if value.len() == 4 => {
                    mode = Some(u32::from_be_bytes(value.try_into().unwrap()))
                }
                ("portable", b"mtime") if value.len() == 12 => {
                    seconds = Some(i64::from_be_bytes(value[..8].try_into().unwrap()));
                    nanos = Some(u32::from_be_bytes(value[8..].try_into().unwrap()));
                }
                ("apple.xattr", name) => {
                    xattr_bytes = xattr_bytes
                        .checked_add(name.len())
                        .and_then(|total| total.checked_add(value.len()))
                        .filter(|total| *total <= MAX_NATIVE_XATTR_BYTES)
                        .ok_or(layerfs_core::CoreError::InvalidRecord(
                            "metadata xattr bytes",
                        ))?;
                    xattrs.push((name.to_vec(), value));
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
    Ok(NativeMetadata {
        mode: mode.ok_or(VfsError::InvalidState)?,
        mtime_seconds: seconds.ok_or(VfsError::InvalidState)?,
        mtime_nanoseconds: nanos.ok_or(VfsError::InvalidState)?,
        xattrs,
        acl,
        bsd_flags: flags,
    })
}
