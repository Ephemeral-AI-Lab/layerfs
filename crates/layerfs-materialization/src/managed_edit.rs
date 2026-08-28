use crate::driver::*;
use crate::{NativeOperationCounters, NativeRoute, VfsError, VfsResult};
use layerfs_core::CanonicalPath;
use std::cmp::Ordering;
use std::io::{Read, Seek, SeekFrom, Write};

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
    if delete_len == replacement.len() as u64 {
        file.seek(SeekFrom::Start(start))?;
        let mut offset = 0;
        let mut buffer = [0_u8; 64 * 1024];
        while offset < replacement.len() {
            let count = (replacement.len() - offset).min(buffer.len());
            file.read_exact(&mut buffer[..count])?;
            if buffer[..count] != replacement[offset..offset + count] {
                break;
            }
            offset += count;
        }
        if offset == replacement.len() {
            return Ok((
                original_metadata,
                NativeOperationCounters {
                    route: Some(NativeRoute::ExactNoop),
                    bytes_read: replacement.len() as u64,
                    ..NativeOperationCounters::default()
                },
                false,
            ));
        }
    }
    if protected {
        return Err(VfsError::NativeProtected);
    }
    match native.clone_temp_from_regular(file.as_ref()) {
        Ok(mut temp) => {
            let mut counters = shift_file(
                temp.as_mut(),
                start,
                delete_len,
                replacement.len() as u64,
                |file, len| file.set_len(len).map_err(Into::into),
            )?;
            temp.seek(SeekFrom::Start(start))?;
            temp.write_all(replacement)?;
            temp.flush()?;
            let metadata = native.read_temp_metadata(temp.as_ref())?;
            native.set_temp_metadata(temp.as_mut(), &metadata)?;
            native.atomic_replace(temp, parent.as_ref(), name)?;
            counters.route = Some(if delete_len == replacement.len() as u64 {
                NativeRoute::ClonePatch
            } else {
                NativeRoute::CloneShift
            });
            counters.bytes_written = counters
                .bytes_written
                .checked_add(replacement.len() as u64)
                .ok_or(VfsError::InvalidState)?;
            counters.patch_bytes = replacement.len() as u64;
            counters.clone_attempts = 1;
            counters.clone_successes = 1;
            return Ok((
                native.read_metadata_at(parent.as_ref(), name, None)?,
                counters,
                false,
            ));
        }
        Err(DriverError::Unsupported) => {}
        Err(error) => return Err(error.into()),
    }
    let mut temp = native.create_temp_at(parent.as_ref())?;
    file.seek(SeekFrom::Start(0))?;
    let mut copied = 0_u64;
    let mut buffer = vec![0_u8; 1024 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        temp.write_all(&buffer[..count])?;
        copied = copied
            .checked_add(count as u64)
            .ok_or(VfsError::InvalidState)?;
    }
    native.set_temp_metadata(temp.as_mut(), &original_metadata)?;
    let mut counters = shift_file(
        temp.as_mut(),
        start,
        delete_len,
        replacement.len() as u64,
        |file, len| file.set_len(len).map_err(Into::into),
    )?;
    temp.seek(SeekFrom::Start(start))?;
    temp.write_all(replacement)?;
    temp.flush()?;
    native.atomic_replace(temp, parent.as_ref(), name)?;
    counters.route = Some(NativeRoute::FullFallback);
    counters.bytes_read = counters
        .bytes_read
        .checked_add(copied)
        .ok_or(VfsError::InvalidState)?;
    counters.bytes_written = counters
        .bytes_written
        .checked_add(copied)
        .and_then(|bytes| bytes.checked_add(replacement.len() as u64))
        .ok_or(VfsError::InvalidState)?;
    counters.patch_bytes = replacement.len() as u64;
    counters.clone_attempts = 1;
    counters.clone_fallbacks = 1;
    Ok((
        native.read_metadata_at(parent.as_ref(), name, None)?,
        counters,
        true,
    ))
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

pub(crate) fn native_parent<'a>(
    workspace: &dyn ProjectionWorkspace,
    mut directory: Box<dyn DirectoryHandle>,
    path: &'a CanonicalPath,
) -> VfsResult<(Box<dyn DirectoryHandle>, &'a [u8])> {
    let components = path.components().collect::<Vec<_>>();
    let (name, parents) = components.split_last().ok_or(VfsError::InvalidState)?;
    for component in parents {
        directory = workspace.open_directory_at(directory.as_ref(), component, None)?;
    }
    Ok((directory, name))
}

fn shift_file<F: Read + Write + Seek + ?Sized>(
    file: &mut F,
    start: u64,
    delete_len: u64,
    replacement_len: u64,
    mut set_len: impl FnMut(&mut F, u64) -> VfsResult<()>,
) -> VfsResult<NativeOperationCounters> {
    let length = file.seek(SeekFrom::End(0))?;
    let end = start
        .checked_add(delete_len)
        .ok_or(VfsError::InvalidState)?;
    if end > length {
        return Err(VfsError::InvalidState);
    }
    let next_len = length
        .checked_sub(delete_len)
        .and_then(|value| value.checked_add(replacement_len))
        .ok_or(VfsError::InvalidState)?;
    let shifted = if next_len == length { 0 } else { length - end };
    match next_len.cmp(&length) {
        Ordering::Greater => {
            set_len(file, next_len)?;
            let mut buffer = vec![0_u8; 1024 * 1024];
            let mut remaining = shifted;
            while remaining > 0 {
                let count = remaining.min(buffer.len() as u64);
                let source = end + remaining - count;
                file.seek(SeekFrom::Start(source))?;
                file.read_exact(&mut buffer[..count as usize])?;
                file.seek(SeekFrom::Start(source + (next_len - length)))?;
                file.write_all(&buffer[..count as usize])?;
                remaining -= count;
            }
        }
        Ordering::Less => {
            let mut buffer = vec![0_u8; 1024 * 1024];
            let mut source = end;
            while source < length {
                let count = (length - source).min(buffer.len() as u64);
                file.seek(SeekFrom::Start(source))?;
                file.read_exact(&mut buffer[..count as usize])?;
                file.seek(SeekFrom::Start(source - (length - next_len)))?;
                file.write_all(&buffer[..count as usize])?;
                source += count;
            }
            set_len(file, next_len)?;
        }
        Ordering::Equal => {}
    }
    Ok(NativeOperationCounters {
        bytes_read: shifted,
        bytes_written: shifted,
        suffix_bytes_shifted: shifted,
        ..NativeOperationCounters::default()
    })
}
