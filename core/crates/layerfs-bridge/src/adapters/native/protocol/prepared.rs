//! Bounded prepared filesystem and history request encoding.
use super::metadata::{put_optional, take_array, take_optional};
use super::{Decoder, Encoder};
use crate::contract::*;

/// Writes the final directory bindings of one prepared filesystem update.
pub(super) fn put_directories(
    e: &mut Encoder,
    directories: &[DirectoryChange],
) -> Result<(), Failure> {
    e.count(directories.len())?;
    for d in directories {
        e.u64(d.parent)?;
        e.count(d.changes.len())?;
        for (name, serial) in &d.changes {
            e.blob(name)?;
            e.u64(serial.unwrap_or(0))?;
        }
    }
    Ok(())
}

/// Reads the final directory bindings of one prepared filesystem update.
pub(super) fn take_directories(d: &mut Decoder<'_>) -> Result<Vec<DirectoryChange>, Failure> {
    let count = d.count(128, 10)?;
    let mut directories = Vec::with_capacity(count);
    let mut total = 0;
    for _ in 0..count {
        let parent = d.u64()?;
        let n = d.count(128 - total, 10)?;
        total += n;
        let mut changes = Vec::with_capacity(n);
        for _ in 0..n {
            let name = d.blob(255)?;
            let serial = d.u64()?;
            changes.push((name, (serial != 0).then_some(serial)));
        }
        directories.push(DirectoryChange { parent, changes });
    }
    Ok(directories)
}

/// Writes the typed final inode values of one prepared filesystem update.
pub(super) fn put_inodes(e: &mut Encoder, inodes: &[InodeChange]) -> Result<(), Failure> {
    e.count(inodes.len())?;
    for i in inodes {
        e.u64(i.serial)?;
        e.u8(i.kind)?;
        e.put(&i.content)?;
        e.put(&i.metadata)?;
    }
    Ok(())
}

/// Reads the typed final inode values of one prepared filesystem update.
pub(super) fn take_inodes(d: &mut Decoder<'_>) -> Result<Vec<InodeChange>, Failure> {
    let n = d.count(128, 73)?;
    let mut inodes = Vec::with_capacity(n);
    for _ in 0..n {
        inodes.push(InodeChange {
            serial: d.u64()?,
            kind: d.u8()?,
            content: d.root()?,
            metadata: d.root()?,
        });
    }
    Ok(inodes)
}

/// An absent trailer preserves the original prepared-update bytes exactly.
pub(super) fn put_additions(
    e: &mut Encoder,
    new_directories: &[DirectoryMetadata],
    directory_metadata: &[DirectoryMetadata],
    new_file_serials: &[u64],
) -> Result<(), Failure> {
    if new_directories.is_empty() && directory_metadata.is_empty() && new_file_serials.is_empty() {
        return Ok(());
    }
    e.u8(if new_file_serials.is_empty() { 1 } else { 2 })?;
    for records in [new_directories, directory_metadata] {
        e.count(records.len())?;
        for directory in records {
            e.u64(directory.serial)?;
            e.u32(directory.mode)?;
            e.u64(directory.mtime_seconds as u64)?;
            e.u32(directory.mtime_nanoseconds)?;
        }
    }
    if !new_file_serials.is_empty() {
        e.count(new_file_serials.len())?;
        for serial in new_file_serials {
            e.u64(*serial)?;
        }
    }
    Ok(())
}
type PreparedAdditions = (Vec<DirectoryMetadata>, Vec<DirectoryMetadata>, Vec<u64>);
pub(super) fn take_additions(
    d: &mut Decoder<'_>,
    inode_count: usize,
) -> Result<PreparedAdditions, Failure> {
    if d.is_empty() {
        return Ok((Vec::new(), Vec::new(), Vec::new()));
    }
    let version = d.u8()?;
    if version != 1 && version != 2 {
        return Err(Code::Unsupported.into());
    }
    let maximum = 128 - inode_count;
    let new_directories = take_directory_records(d, maximum)?;
    let directory_metadata = take_directory_records(d, maximum - new_directories.len())?;
    let mut new_file_serials = Vec::new();
    if version == 2 {
        let count = d.count(inode_count, 8)?;
        if count == 0 {
            return Err(Code::InvalidInput.into());
        }
        new_file_serials = Vec::with_capacity(count);
        for _ in 0..count {
            new_file_serials.push(d.u64()?);
        }
    } else if new_directories.is_empty() && directory_metadata.is_empty() {
        return Err(Code::InvalidInput.into());
    }
    Ok((new_directories, directory_metadata, new_file_serials))
}
fn take_directory_records(
    d: &mut Decoder<'_>,
    maximum: usize,
) -> Result<Vec<DirectoryMetadata>, Failure> {
    let count = d.count(maximum, 24)?;
    let mut directories = Vec::with_capacity(count);
    for _ in 0..count {
        directories.push(DirectoryMetadata {
            serial: d.u64()?,
            mode: d.u32()?,
            mtime_seconds: d.u64()? as i64,
            mtime_nanoseconds: d.u32()?,
        });
    }
    Ok(directories)
}

/// Writes the prepared filesystem update history carries.
pub(super) fn put_prepared(e: &mut Encoder, changes: &PreparedChanges) -> Result<(), Failure> {
    e.put(&changes.workspace)?;
    e.put(&changes.branch)?;
    put_optional(e, changes.expected_head.as_ref())?;
    e.put(&changes.expected_base)?;
    e.u64(changes.generation)?;
    e.put(&changes.base)?;
    e.put(&changes.scope)?;
    e.u64(changes.root_serial)?;
    put_directories(e, &changes.directories)?;
    put_inodes(e, &changes.inodes)?;
    put_additions(
        e,
        &changes.new_directories,
        &changes.directory_metadata,
        &changes.new_file_serials,
    )
}

pub(super) fn take_prepared(d: &mut Decoder<'_>) -> Result<PreparedChanges, Failure> {
    let mut changes = PreparedChanges {
        workspace: take_array::<32>(d)?,
        branch: take_array::<17>(d)?,
        expected_head: take_optional::<33>(d)?,
        expected_base: take_array::<33>(d)?,
        generation: d.u64()?,
        base: d.root()?,
        scope: d.root()?,
        root_serial: d.u64()?,
        directories: take_directories(d)?,
        inodes: take_inodes(d)?,
        new_directories: Vec::new(),
        directory_metadata: Vec::new(),
        new_file_serials: Vec::new(),
    };
    (
        changes.new_directories,
        changes.directory_metadata,
        changes.new_file_serials,
    ) = take_additions(d, changes.inodes.len())?;
    Ok(changes)
}
