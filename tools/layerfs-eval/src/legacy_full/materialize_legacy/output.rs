use super::super::session_legacy::{VfsError, VfsResult};
use super::super::OperationCounters;
use layerfs_core::content::rope::{read_all_bounded, FileStateRoot, ObjectRead};
use layerfs_core::inode::{inode_table_lookup, InodeId, InodeTableCounters, InodeTableRoot};
use layerfs_core::metadata::{decode_apple_acl, visit_metadata_entries, SUPPORTED_BSD_FLAGS};
use layerfs_core::namespace_codec::decode_inode_record;
use layerfs_core::ObjectId;
use layerfs_materialization::driver::*;
use layerfs_storage::Engine;
use std::io::{BufWriter, Write};

pub(super) fn project_regular_file<T>(
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

pub(super) fn checked_add(left: u64, right: u64) -> VfsResult<u64> {
    left.checked_add(right).ok_or(VfsError::InvalidState)
}

pub(super) fn encode_link_state(remaining: u64, path: &[u8]) -> Vec<u8> {
    let mut value = Vec::with_capacity(8 + path.len());
    value.extend_from_slice(&remaining.to_be_bytes());
    value.extend_from_slice(path);
    value
}

pub(super) fn decode_link_state(value: &[u8]) -> VfsResult<(u64, &[u8])> {
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

pub(super) fn child_path(parent: &[u8], name: &[u8]) -> Vec<u8> {
    let mut path = parent.to_vec();
    if !path.is_empty() {
        path.push(b'/');
    }
    path.extend_from_slice(name);
    path
}

pub(super) fn create_hard_link_from_path(
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

pub(super) fn create_hard_link_from_components(
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

pub(super) fn finish_hard_link_from_path(
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

pub(super) fn finish_hard_link_from_components(
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

pub(super) fn record(
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
    let mut xattrs = layerfs_materialization::driver::NativeXattrs::new();
    let mut acl = None;
    let mut flags = 0;
    visit_metadata_entries(engine, root, |entries| {
        for entry in entries {
            let file_root = FileStateRoot(entry.value_file_root);
            let mut value = Vec::new();
            let rope = read_all_bounded(engine, file_root, 1024 * 1024, &mut value)?;
            super::super::add_metadata_rope(counters, rope)?;
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
    super::super::managed_edit_legacy::spooled_metadata_len(&metadata)?;
    Ok(metadata)
}
