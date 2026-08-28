use super::super::session_legacy::{VfsError, VfsResult};
use super::super::{NativeOperationCounters, NativeRoute};
use layerfs_core::CanonicalPath;
use layerfs_materialization::driver::*;
use std::cmp::Ordering;
use std::io::{Read, Seek, SeekFrom, Write};

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

pub(super) fn replace_native(
    workspace: &dyn ProjectionWorkspace,
    file: &mut dyn RegularFileHandle,
    start: u64,
    delete_len: u64,
    replacement: &[u8],
) -> VfsResult<NativeOperationCounters> {
    let replacement_len = u64::try_from(replacement.len()).map_err(|_| VfsError::InvalidState)?;
    let mut counters = shift_file(file, start, delete_len, replacement_len, |file, len| {
        workspace.set_regular_len(file, len).map_err(Into::into)
    })?;
    file.seek(SeekFrom::Start(start))?;
    file.write_all(replacement)?;
    file.flush()?;
    counters.route = Some(if delete_len == replacement_len {
        NativeRoute::InPlacePatch
    } else {
        NativeRoute::InPlaceShift
    });
    counters.bytes_written = counters
        .bytes_written
        .checked_add(replacement_len)
        .ok_or(VfsError::InvalidState)?;
    counters.patch_bytes = replacement_len;
    Ok(counters)
}

pub(crate) fn shift_regular(
    workspace: &dyn ProjectionWorkspace,
    file: &mut dyn RegularFileHandle,
    start: u64,
    delete_len: u64,
    replacement_len: u64,
) -> VfsResult<NativeOperationCounters> {
    shift_file(file, start, delete_len, replacement_len, |file, len| {
        workspace.set_regular_len(file, len).map_err(Into::into)
    })
}

pub(crate) fn shift_temp(
    file: &mut dyn OwnedTempHandle,
    start: u64,
    delete_len: u64,
    replacement_len: u64,
) -> VfsResult<NativeOperationCounters> {
    shift_file(file, start, delete_len, replacement_len, |file, len| {
        file.set_len(len).map_err(Into::into)
    })
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
