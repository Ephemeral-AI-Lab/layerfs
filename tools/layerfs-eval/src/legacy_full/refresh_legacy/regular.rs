use super::super::managed_edit_legacy::native_parent;
use super::super::materialize_legacy::metadata;
use super::super::session_legacy::{VfsError, VfsResult};
use super::super::{AcceptedSplice, NativeRoute, OperationCounters};
use layerfs_core::content::rope::{read_all, FileStateRoot};
use layerfs_core::CanonicalPath;
use layerfs_materialization::driver::{DriverError, ProjectionWorkspace};
use layerfs_storage::scratch::DiskNamespace;
use layerfs_storage::Engine;
use std::io::{Seek, SeekFrom};

use super::primitives::{
    changed_ranges, checked_add, copy_target_range, decode_range, mark_ambiguous,
};
use super::scratch::{RefreshScratch, Snapshot};
#[allow(clippy::too_many_arguments)]
pub(super) fn refresh_regular(
    engine: &Engine,
    scratch: &RefreshScratch,
    native: &dyn ProjectionWorkspace,
    authority: &DiskNamespace<'_>,
    path: &CanonicalPath,
    before: Snapshot,
    after: Snapshot,
    accepted: Option<&AcceptedSplice>,
    visible: &mut bool,
    counters: &mut OperationCounters,
) -> VfsResult<()> {
    if before.content_root == after.content_root {
        if before.metadata_root == after.metadata_root {
            return Ok(());
        }
        let root = native.root_directory()?;
        let (parent, name) = native_parent(native, root, path)?;
        let identity = native.identity_at(parent.as_ref(), name)?;
        ensure_unprotected(native, parent.as_ref(), name, counters)?;
        *visible = true;
        native.set_entry_metadata(
            parent.as_ref(),
            name,
            &identity,
            &metadata(engine, after.metadata_root, counters)?,
        )?;
        let mut file = native.open_regular_at(parent.as_ref(), name, Some(&identity))?;
        native.sync_regular(file.as_mut())?;
        counters.native.metadata_calls = checked_add(counters.native.metadata_calls, 2)?;
        counters.native.sync_calls = checked_add(counters.native.sync_calls, 1)?;
        return Ok(());
    }

    let ranges = scratch.table("ranges")?;
    let (same_length, count) = changed_ranges(
        engine,
        before.content_root,
        after.content_root,
        &ranges,
        counters,
    )?;
    let root = native.root_directory()?;
    let (parent, name) = native_parent(native, root, path)?;
    let old_key = native.identity_at(parent.as_ref(), name)?;
    let old_token = native.token_at(parent.as_ref(), name)?;
    if authority.get(&old_key)?.as_deref() != Some(before.inode.as_bytes()) {
        return Err(VfsError::InvalidState);
    }
    ensure_unprotected(native, parent.as_ref(), name, counters)?;
    if !same_length {
        if let Some(accepted) = accepted.filter(|splice| splice.path() == path) {
            return refresh_splice(
                engine,
                native,
                authority,
                parent.as_ref(),
                name,
                &old_key,
                &old_token,
                before,
                after,
                accepted,
                visible,
                counters,
            );
        }
        return full_fallback(
            engine,
            native,
            authority,
            parent.as_ref(),
            name,
            &old_key,
            &old_token,
            before,
            after,
            visible,
            counters,
        );
    }
    let source = native.open_regular_read_at(parent.as_ref(), name, Some(&old_token))?;
    let clone = if before.namespace_ref_count == 1 && after.namespace_ref_count == 1 {
        native.clone_temp_from_regular(source.as_ref())
    } else {
        Err(DriverError::Unsupported)
    };
    match clone {
        Ok(mut temp) => {
            counters.native.clone_attempts = checked_add(counters.native.clone_attempts, 1)?;
            counters.native.clone_successes = checked_add(counters.native.clone_successes, 1)?;
            counters.native.temp_calls = checked_add(counters.native.temp_calls, 1)?;
            for index in 0..count {
                let (start, end) = decode_range(
                    &ranges
                        .get(&index.to_be_bytes())?
                        .ok_or(VfsError::InvalidState)?,
                )?;
                copy_target_range(
                    engine,
                    after.content_root,
                    start,
                    end,
                    temp.as_mut(),
                    counters,
                )?;
            }
            native.set_temp_metadata(
                temp.as_mut(),
                &metadata(engine, after.metadata_root, counters)?,
            )?;
            counters.native.metadata_calls = checked_add(counters.native.metadata_calls, 2)?;
            match native.atomic_replace_checked(temp, parent.as_ref(), name, Some(&old_key)) {
                Ok(()) => *visible = true,
                Err(error) => {
                    mark_ambiguous(visible, &error);
                    return Err(error.into());
                }
            }
            counters.native.replace_calls = checked_add(counters.native.replace_calls, 1)?;
            counters.native.sync_calls = checked_add(counters.native.sync_calls, 2)?;
            counters.native.route = Some(NativeRoute::ClonePatch);
            let next_key = native.identity_at(parent.as_ref(), name)?;
            authority.remove(&old_key)?;
            authority.put(&next_key, before.inode.as_bytes())?;
        }
        Err(DriverError::Unsupported) => {
            counters.native.clone_attempts = checked_add(counters.native.clone_attempts, 1)?;
            counters.native.clone_fallbacks = checked_add(counters.native.clone_fallbacks, 1)?;
            let mut file = native.open_regular_at(parent.as_ref(), name, Some(&old_token))?;
            for index in 0..count {
                let (start, end) = decode_range(
                    &ranges
                        .get(&index.to_be_bytes())?
                        .ok_or(VfsError::InvalidState)?,
                )?;
                *visible = true;
                copy_target_range(
                    engine,
                    after.content_root,
                    start,
                    end,
                    file.as_mut(),
                    counters,
                )?;
            }
            native.set_entry_metadata(
                parent.as_ref(),
                name,
                &old_key,
                &metadata(engine, after.metadata_root, counters)?,
            )?;
            native.sync_regular(file.as_mut())?;
            counters.native.metadata_calls = checked_add(counters.native.metadata_calls, 2)?;
            counters.native.sync_calls = checked_add(counters.native.sync_calls, 1)?;
            counters.native.route = Some(NativeRoute::InPlacePatch);
        }
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn refresh_splice(
    engine: &Engine,
    native: &dyn ProjectionWorkspace,
    authority: &DiskNamespace<'_>,
    parent: &dyn layerfs_materialization::driver::DirectoryHandle,
    name: &[u8],
    old_key: &[u8],
    old_token: &[u8],
    before: Snapshot,
    after: Snapshot,
    accepted: &AcceptedSplice,
    visible: &mut bool,
    counters: &mut OperationCounters,
) -> VfsResult<()> {
    let end = accepted
        .start
        .checked_add(accepted.delete_len)
        .ok_or(VfsError::InvalidState)?;
    let replacement_end = accepted
        .start
        .checked_add(accepted.insert_len)
        .ok_or(VfsError::InvalidState)?;
    if end > accepted.old_len
        || accepted
            .old_len
            .checked_sub(accepted.delete_len)
            .and_then(|length| length.checked_add(accepted.insert_len))
            != Some(accepted.new_len)
        || replacement_end > accepted.new_len
    {
        return Err(VfsError::InvalidState);
    }
    let target_metadata = metadata(engine, after.metadata_root, counters)?;
    let mut source = native.open_regular_read_at(parent, name, Some(old_token))?;
    if source.seek(SeekFrom::End(0))? != accepted.old_len {
        return Err(VfsError::ExternalDirtyConflict);
    }
    let suffix = accepted
        .old_len
        .checked_sub(end)
        .ok_or(VfsError::InvalidState)?;
    if suffix != 0 && before.namespace_ref_count == 1 && after.namespace_ref_count == 1 {
        counters.native.clone_attempts = checked_add(counters.native.clone_attempts, 1)?;
        match native.clone_temp_from_regular(source.as_ref()) {
            Ok(mut temp) => {
                if temp.seek(SeekFrom::End(0))? != accepted.old_len {
                    return Err(VfsError::ExternalDirtyConflict);
                }
                counters.native.clone_successes = checked_add(counters.native.clone_successes, 1)?;
                counters.native.temp_calls = checked_add(counters.native.temp_calls, 1)?;
                super::super::add_native(
                    counters,
                    super::super::managed_edit_legacy::shift_temp(
                        temp.as_mut(),
                        accepted.start,
                        accepted.delete_len,
                        accepted.insert_len,
                    )?,
                )?;
                copy_target_range(
                    engine,
                    after.content_root,
                    accepted.start,
                    replacement_end,
                    temp.as_mut(),
                    counters,
                )?;
                native.set_temp_metadata(temp.as_mut(), &target_metadata)?;
                counters.native.metadata_calls = checked_add(counters.native.metadata_calls, 2)?;
                match native.atomic_replace_checked(temp, parent, name, Some(old_key)) {
                    Ok(()) => *visible = true,
                    Err(error) => {
                        mark_ambiguous(visible, &error);
                        return Err(error.into());
                    }
                }
                counters.native.replace_calls = checked_add(counters.native.replace_calls, 1)?;
                counters.native.sync_calls = checked_add(counters.native.sync_calls, 2)?;
                counters.native.route = Some(NativeRoute::CloneShift);
                let next_key = native.identity_at(parent, name)?;
                authority.remove(old_key)?;
                authority.put(&next_key, after.inode.as_bytes())?;
                return Ok(());
            }
            Err(DriverError::Unsupported) => {
                counters.native.clone_fallbacks = checked_add(counters.native.clone_fallbacks, 1)?;
            }
            Err(error) => return Err(error.into()),
        }
    }
    let mut file = native.open_regular_at(parent, name, Some(old_token))?;
    if file.seek(SeekFrom::End(0))? != accepted.old_len {
        return Err(VfsError::ExternalDirtyConflict);
    }
    *visible = true;
    super::super::add_native(
        counters,
        super::super::managed_edit_legacy::shift_regular(
            native,
            file.as_mut(),
            accepted.start,
            accepted.delete_len,
            accepted.insert_len,
        )?,
    )?;
    copy_target_range(
        engine,
        after.content_root,
        accepted.start,
        replacement_end,
        file.as_mut(),
        counters,
    )?;
    native.set_entry_metadata(parent, name, old_key, &target_metadata)?;
    native.sync_regular(file.as_mut())?;
    counters.native.metadata_calls = checked_add(counters.native.metadata_calls, 2)?;
    counters.native.sync_calls = checked_add(counters.native.sync_calls, 1)?;
    counters.native.route = Some(NativeRoute::InPlaceShift);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn full_fallback(
    engine: &Engine,
    native: &dyn ProjectionWorkspace,
    authority: &DiskNamespace<'_>,
    parent: &dyn layerfs_materialization::driver::DirectoryHandle,
    name: &[u8],
    old_key: &[u8],
    old_token: &[u8],
    before: Snapshot,
    after: Snapshot,
    visible: &mut bool,
    counters: &mut OperationCounters,
) -> VfsResult<()> {
    let target_metadata = metadata(engine, after.metadata_root, counters)?;
    if before.namespace_ref_count > 1 || after.namespace_ref_count > 1 {
        let mut file = native.open_regular_at(parent, name, Some(old_token))?;
        *visible = true;
        native.set_regular_len(file.as_mut(), 0)?;
        file.seek(SeekFrom::Start(0))?;
        let rope = read_all(engine, FileStateRoot(after.content_root), file.as_mut())?;
        counters.native.bytes_written =
            checked_add(counters.native.bytes_written, rope.payload_bytes_read)?;
        counters.add_rope(rope)?;
        native.set_entry_metadata(parent, name, old_key, &target_metadata)?;
        native.sync_regular(file.as_mut())?;
        counters.native.metadata_calls = checked_add(counters.native.metadata_calls, 1)?;
        counters.native.sync_calls = checked_add(counters.native.sync_calls, 1)?;
    } else {
        let mut temp = native.create_temp_at(parent)?;
        counters.native.temp_calls = checked_add(counters.native.temp_calls, 1)?;
        let rope = read_all(engine, FileStateRoot(after.content_root), temp.as_mut())?;
        counters.native.bytes_written =
            checked_add(counters.native.bytes_written, rope.payload_bytes_read)?;
        counters.add_rope(rope)?;
        native.set_temp_metadata(temp.as_mut(), &target_metadata)?;
        counters.native.metadata_calls = checked_add(counters.native.metadata_calls, 1)?;
        match native.atomic_replace_checked(temp, parent, name, Some(old_key)) {
            Ok(()) => *visible = true,
            Err(error) => {
                mark_ambiguous(visible, &error);
                return Err(error.into());
            }
        }
        counters.native.replace_calls = checked_add(counters.native.replace_calls, 1)?;
        counters.native.sync_calls = checked_add(counters.native.sync_calls, 2)?;
        let next_key = native.identity_at(parent, name)?;
        authority.remove(old_key)?;
        authority.put(&next_key, after.inode.as_bytes())?;
    }
    counters.native.route = Some(NativeRoute::FullFallback);
    counters.full_fallback_files = checked_add(counters.full_fallback_files, 1)?;
    Ok(())
}

pub(super) fn ensure_unprotected(
    native: &dyn ProjectionWorkspace,
    parent: &dyn layerfs_materialization::driver::DirectoryHandle,
    name: &[u8],
    counters: &mut OperationCounters,
) -> VfsResult<()> {
    let token = native.token_at(parent, name)?;
    let metadata = native.read_metadata_at(parent, name, Some(&token))?;
    counters.native.metadata_calls = checked_add(counters.native.metadata_calls, 1)?;
    if metadata.bsd_flags & 0x6 != 0 {
        return Err(VfsError::NativeProtected);
    }
    Ok(())
}
