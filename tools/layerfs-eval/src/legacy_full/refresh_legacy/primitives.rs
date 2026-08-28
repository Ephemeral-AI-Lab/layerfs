use super::super::session_legacy::{VfsError, VfsResult};
use super::super::OperationCounters;
use layerfs_core::content::rope::{diff_ranges, read_range, FileStateRoot};
use layerfs_core::inode::{InodeId, InodeKind};
use layerfs_core::ObjectId;
use layerfs_materialization::driver::DriverError;
use layerfs_storage::scratch::DiskNamespace;
use layerfs_storage::Engine;
use std::io::{Seek, SeekFrom, Write};

use super::scratch::{Snapshot, ORDERED_PATH_PREFIX_BYTES, SNAPSHOT_BYTES};
pub(super) fn ordered_path(path: &[u8], reverse: bool) -> VfsResult<Vec<u8>> {
    let depth = if path.is_empty() {
        0
    } else {
        path.iter().filter(|byte| **byte == b'/').count() + 1
    };
    let depth = u16::try_from(depth).map_err(|_| VfsError::InvalidState)?;
    let depth = if reverse { u16::MAX - depth } else { depth };
    let mut key = Vec::with_capacity(path.len() + ORDERED_PATH_PREFIX_BYTES);
    key.extend_from_slice(&depth.to_be_bytes());
    key.extend_from_slice(path);
    Ok(key)
}

pub(super) fn translate_path(path: &[u8], mappings: &DiskNamespace<'_>) -> VfsResult<Vec<u8>> {
    if path.is_empty() {
        return Ok(Vec::new());
    }
    for (index, _) in path
        .iter()
        .copied()
        .enumerate()
        .filter(|(_, byte)| *byte == b'/')
        .chain(std::iter::once((path.len(), b'/')))
    {
        let prefix = &path[..index];
        if let Some(target) = mappings.get(prefix)? {
            let suffix = path.get(index..).ok_or(VfsError::InvalidState)?;
            let mut translated = target;
            translated.extend_from_slice(suffix);
            return Ok(translated);
        }
    }
    Ok(path.to_vec())
}

pub(super) fn has_mapped_ancestor(path: &[u8], mappings: &DiskNamespace<'_>) -> VfsResult<bool> {
    for (index, _) in path.iter().enumerate().filter(|(_, byte)| **byte == b'/') {
        if mappings.get(&path[..index])?.is_some() {
            return Ok(true);
        }
    }
    Ok(false)
}

pub(super) fn mark_ambiguous(visible: &mut bool, error: &DriverError) {
    if matches!(
        error,
        DriverError::VisibilityAmbiguous | DriverError::DurabilityAmbiguous
    ) {
        *visible = true;
    }
}

pub(super) fn checked_add(left: u64, right: u64) -> VfsResult<u64> {
    left.checked_add(right).ok_or(VfsError::InvalidState)
}

pub(super) fn changed_ranges(
    engine: &Engine,
    old_root: ObjectId,
    new_root: ObjectId,
    ranges: &DiskNamespace<'_>,
    counters: &mut OperationCounters,
) -> VfsResult<(bool, u64)> {
    let mut count = 0_u64;
    let mut scratch_error = None;
    let result = diff_ranges(
        engine,
        FileStateRoot(old_root),
        FileStateRoot(new_root),
        |range| {
            if let Err(error) =
                ranges.put(&count.to_be_bytes(), &encode_range(range.start, range.end))
            {
                scratch_error = Some(error);
                return Err(layerfs_core::CoreError::Io);
            }
            count = count
                .checked_add(1)
                .ok_or(layerfs_core::CoreError::LengthOverflow)?;
            Ok(())
        },
    );
    if let Some(error) = scratch_error {
        return Err(error.into());
    }
    let (same_length, rope) = result?;
    counters.root_diff_nodes = counters
        .root_diff_nodes
        .checked_add(rope.nodes_read)
        .ok_or(VfsError::InvalidState)?;
    counters.add_rope(rope)?;
    Ok((same_length, count))
}

pub(super) fn copy_target_range(
    engine: &Engine,
    target: ObjectId,
    start: u64,
    end: u64,
    output: &mut (impl Write + Seek + ?Sized),
    counters: &mut OperationCounters,
) -> VfsResult<()> {
    let mut offset = start;
    while offset < end {
        let stop = end.min(offset + 1024 * 1024);
        let mut bytes = Vec::with_capacity((stop - offset) as usize);
        let rope = read_range(engine, FileStateRoot(target), offset..stop, &mut bytes)?;
        counters.add_rope(rope)?;
        output.seek(SeekFrom::Start(offset))?;
        output.write_all(&bytes)?;
        counters.native.bytes_written += stop - offset;
        counters.native.patch_bytes += stop - offset;
        offset = stop;
    }
    Ok(())
}

pub(super) fn encode_snapshot(value: Snapshot) -> [u8; SNAPSHOT_BYTES] {
    let mut bytes = [0; SNAPSHOT_BYTES];
    bytes[0] = value.kind as u8;
    bytes[1..33].copy_from_slice(value.inode.as_bytes());
    bytes[33..41].copy_from_slice(&value.namespace_ref_count.to_be_bytes());
    bytes[41..73].copy_from_slice(value.content_root.as_bytes());
    bytes[73..105].copy_from_slice(value.metadata_root.as_bytes());
    bytes
}

pub(super) fn decode_snapshot(bytes: &[u8]) -> VfsResult<Snapshot> {
    if bytes.len() != SNAPSHOT_BYTES {
        return Err(VfsError::InvalidState);
    }
    Ok(Snapshot {
        kind: InodeKind::try_from(bytes[0])?,
        inode: InodeId::from_slice(&bytes[1..33])?,
        namespace_ref_count: u64::from_be_bytes(bytes[33..41].try_into().unwrap()),
        content_root: ObjectId::from_bytes(&bytes[41..73])?,
        metadata_root: ObjectId::from_bytes(&bytes[73..105])?,
    })
}

pub(super) fn encode_range(start: u64, end: u64) -> [u8; 16] {
    let mut bytes = [0; 16];
    bytes[..8].copy_from_slice(&start.to_be_bytes());
    bytes[8..].copy_from_slice(&end.to_be_bytes());
    bytes
}

pub(super) fn decode_range(bytes: &[u8]) -> VfsResult<(u64, u64)> {
    if bytes.len() != 16 {
        return Err(VfsError::InvalidState);
    }
    Ok((
        u64::from_be_bytes(bytes[..8].try_into().unwrap()),
        u64::from_be_bytes(bytes[8..].try_into().unwrap()),
    ))
}
