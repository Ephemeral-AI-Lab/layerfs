use super::super::session_legacy::{VfsError, VfsResult};
use super::super::{NativeOperationCounters, NativeRoute};
use layerfs_core::CanonicalPath;
use layerfs_materialization::driver::*;
use std::io::{Read, Seek, SeekFrom, Write};

use super::shift::{native_parent, replace_native};
pub enum ManagedEdit {
    Replace {
        path: CanonicalPath,
        start: u64,
        delete_len: u64,
        spool_offset: u64,
        replacement_len: u64,
        metadata_offset: u64,
        metadata_len: u64,
        sync_required: bool,
        native_identity: Vec<u8>,
    },
    Rename {
        from: CanonicalPath,
        to: CanonicalPath,
        source_metadata_offset: u64,
        source_metadata_len: u64,
        target_metadata_offset: u64,
        target_metadata_len: u64,
    },
}

pub fn native_hard_link_key(
    native: &dyn ProjectionWorkspace,
    path: &CanonicalPath,
) -> VfsResult<Vec<u8>> {
    let root = native.root_directory()?;
    let (parent, name) = native_parent(native, root, path)?;
    native
        .identity_at(parent.as_ref(), name)
        .map_err(Into::into)
}

pub fn mutate_native(
    native: &dyn ProjectionWorkspace,
    path: &CanonicalPath,
    start: u64,
    delete_len: u64,
    replacement: &[u8],
) -> VfsResult<(NativeMetadata, NativeOperationCounters, bool)> {
    let root = native.root_directory()?;
    let (parent, name) = native_parent(native, root, path)?;
    let original_metadata = native.read_metadata_at(parent.as_ref(), name, None)?;
    let protected = original_metadata.bsd_flags & 0x0000_0006 != 0;
    let mut file = if protected {
        native.open_regular_read_at(parent.as_ref(), name, None)?
    } else {
        native.open_regular_at(parent.as_ref(), name, None)?
    };
    let length = file.seek(SeekFrom::End(0))?;
    let end = start
        .checked_add(delete_len)
        .ok_or(VfsError::InvalidState)?;
    if end > length {
        return Err(VfsError::InvalidState);
    }
    if protected {
        if delete_len == replacement.len() as u64 {
            file.seek(SeekFrom::Start(start))?;
            let mut offset = 0;
            let mut buffer = [0_u8; 64 * 1024];
            while offset < replacement.len() {
                let count = (replacement.len() - offset).min(buffer.len());
                file.read_exact(&mut buffer[..count])?;
                if buffer[..count] != replacement[offset..offset + count] {
                    return Err(VfsError::NativeProtected);
                }
                offset += count;
            }
            return Ok((
                original_metadata,
                NativeOperationCounters {
                    route: Some(NativeRoute::ProtectedExactNoop),
                    bytes_read: replacement.len() as u64,
                    ..NativeOperationCounters::default()
                },
                false,
            ));
        }
        return Err(VfsError::NativeProtected);
    }
    if delete_len == replacement.len() as u64 {
        match native.clone_temp_from_regular(file.as_ref()) {
            Ok(mut temp) => {
                temp.seek(SeekFrom::Start(start))?;
                temp.write_all(replacement)?;
                temp.flush()?;
                let metadata = native.read_temp_metadata(temp.as_ref())?;
                native.set_temp_metadata(temp.as_mut(), &metadata)?;
                native.atomic_replace(temp, parent.as_ref(), name)?;
                return Ok((
                    native.read_metadata_at(parent.as_ref(), name, None)?,
                    NativeOperationCounters {
                        route: Some(NativeRoute::ClonePatch),
                        bytes_written: replacement.len() as u64,
                        patch_bytes: replacement.len() as u64,
                        clone_attempts: 1,
                        clone_successes: 1,
                        ..NativeOperationCounters::default()
                    },
                    false,
                ));
            }
            Err(DriverError::Unsupported) => {}
            Err(error) => return Err(error.into()),
        }
    }
    let mut counters = replace_native(native, file.as_mut(), start, delete_len, replacement)?;
    if delete_len == replacement.len() as u64 {
        counters.clone_attempts = 1;
        counters.clone_fallbacks = 1;
    }
    Ok((
        native.read_metadata_at(parent.as_ref(), name, None)?,
        counters,
        true,
    ))
}

pub fn sync_pending(native: &dyn ProjectionWorkspace, edits: &[ManagedEdit]) -> VfsResult<()> {
    let mut synced = std::collections::BTreeSet::new();
    for (index, edit) in edits.iter().enumerate().rev() {
        let ManagedEdit::Replace {
            path,
            sync_required: true,
            native_identity,
            ..
        } = edit
        else {
            continue;
        };
        let path = translate_later_renames(path, &edits[index + 1..])?;
        if !synced.insert(path.clone()) {
            continue;
        }
        let root = native.root_directory()?;
        let (parent, name) = native_parent(native, root, &path)?;
        if native.identity_at(parent.as_ref(), name)? != *native_identity {
            return Err(VfsError::Indeterminate);
        }
        let current_token = native.token_at(parent.as_ref(), name)?;
        let mut file = native.open_regular_at(parent.as_ref(), name, Some(&current_token))?;
        native.sync_regular(file.as_mut())?;
        if native.token_at(parent.as_ref(), name)? != current_token
            || native.identity_at(parent.as_ref(), name)? != *native_identity
        {
            return Err(VfsError::Indeterminate);
        }
    }
    Ok(())
}

fn translate_later_renames(
    path: &CanonicalPath,
    edits: &[ManagedEdit],
) -> VfsResult<CanonicalPath> {
    let mut bytes = path.as_bytes().to_vec();
    for edit in edits {
        let ManagedEdit::Rename { from, to, .. } = edit else {
            continue;
        };
        let source = from.as_bytes();
        if bytes == source
            || bytes
                .strip_prefix(source)
                .is_some_and(|suffix| suffix.first() == Some(&b'/'))
        {
            let suffix = &bytes[source.len()..];
            let mut translated = Vec::with_capacity(to.as_bytes().len() + suffix.len());
            translated.extend_from_slice(to.as_bytes());
            translated.extend_from_slice(suffix);
            bytes = translated;
        }
    }
    Ok(CanonicalPath::from_bytes(&bytes)?)
}

pub fn rename_native(
    native: &dyn ProjectionWorkspace,
    from: &CanonicalPath,
    to: &CanonicalPath,
) -> VfsResult<(NativeMetadata, NativeMetadata)> {
    let (source_parent, source) = native_parent(native, native.root_directory()?, from)?;
    let (target_parent, target) = native_parent(native, native.root_directory()?, to)?;
    match native.token_at(target_parent.as_ref(), target) {
        Ok(_) => return Err(VfsError::InvalidState),
        Err(DriverError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    if native
        .read_metadata_at(source_parent.as_ref(), source, None)?
        .bsd_flags
        & 0x0000_0006
        != 0
        || native
            .read_directory_metadata(source_parent.as_ref())?
            .bsd_flags
            & 0x0000_0006
            != 0
        || native
            .read_directory_metadata(target_parent.as_ref())?
            .bsd_flags
            & 0x0000_0006
            != 0
    {
        return Err(VfsError::NativeProtected);
    }
    native.rename_at(
        source_parent.as_ref(),
        source,
        target_parent.as_ref(),
        target,
    )?;
    Ok((
        native.read_directory_metadata(source_parent.as_ref())?,
        native.read_directory_metadata(target_parent.as_ref())?,
    ))
}
