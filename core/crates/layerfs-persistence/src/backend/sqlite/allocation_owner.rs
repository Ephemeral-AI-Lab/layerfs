//! Profile-specific allocation lifetime with original-file identity custody.
use super::{allocation::AllocationFile, connection::AllocationRelease};
use crate::backend::records::BackendError;
use std::{
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};

pub(crate) enum AllocationOwner {
    Retained(AllocationFile),
    OnDemand {
        path: PathBuf,
        device: u64,
        inode: u64,
    },
}
fn metadata(path: &Path, device: u64, inode: u64) -> Result<std::fs::Metadata, BackendError> {
    let m = std::fs::symlink_metadata(path).map_err(|e| BackendError::Filesystem(e.kind()))?;
    if !m.is_file() || m.nlink() != 1 || m.dev() != device || m.ino() != inode {
        return Err(BackendError::Integrity);
    }
    Ok(m)
}
impl AllocationOwner {
    pub(crate) fn open(path: &Path, temporary: bool) -> Result<Self, BackendError> {
        if !temporary {
            return AllocationFile::open(path).map(Self::Retained);
        }
        let path = if path.is_absolute() {
            path.to_owned()
        } else {
            std::env::current_dir()
                .map_err(|e| BackendError::Filesystem(e.kind()))?
                .join(path)
        };
        let m = std::fs::symlink_metadata(&path).map_err(|e| BackendError::Filesystem(e.kind()))?;
        metadata(&path, m.dev(), m.ino())?;
        Ok(Self::OnDemand {
            path,
            device: m.dev(),
            inode: m.ino(),
        })
    }
    pub(crate) fn before_pack(&self, capacity: usize) -> Result<(u64, u64), BackendError> {
        let Self::OnDemand {
            path,
            device,
            inode,
        } = self
        else {
            return Err(BackendError::Integrity);
        };
        if capacity > layerfs_storage::policy::SINGLETON_PACK_LIMIT {
            return Err(BackendError::Capacity);
        }
        const MIB: u64 = 1 << 20;
        let before = metadata(path, *device, *inode)?;
        let allocated = before
            .blocks()
            .checked_mul(512)
            .ok_or(BackendError::Capacity)?;
        let wanted = before
            .len()
            .checked_add(capacity as u64)
            .and_then(|n| n.checked_add(2 * MIB))
            .and_then(|n| n.checked_add(MIB - 1))
            .map(|n| n / MIB * MIB)
            .ok_or(BackendError::Capacity)?;
        let amount = wanted.saturating_sub(allocated);
        if amount == 0 {
            return Ok((0, 0));
        }
        if amount > layerfs_storage::policy::SINGLETON_PACK_LIMIT as u64 + 3 * MIB {
            return Err(BackendError::Capacity);
        }
        let file = AllocationFile::open(path)?;
        let result = file.preallocate_checked(*device, *inode, amount);
        let close = file.close_checked()?;
        metadata(path, *device, *inode)?;
        result.map(|()| (amount, close))
    }
    pub(crate) fn release(&self) -> Result<AllocationRelease, BackendError> {
        let Self::OnDemand {
            path,
            device,
            inode,
        } = self
        else {
            let Self::Retained(file) = self else {
                unreachable!()
            };
            return file.release();
        };
        let before = metadata(path, *device, *inode)?;
        let allocated = before
            .blocks()
            .checked_mul(512)
            .ok_or(BackendError::Capacity)?;
        if allocated <= before.len() {
            return Ok(AllocationRelease {
                before: allocated,
                after: allocated,
                source: None,
                transfer_ns: 0,
                scratch_close_ns: 0,
                source_close_ns: 0,
            });
        }
        let file = AllocationFile::open(path)?;
        let result = file.release_checked(*device, *inode);
        // Always close this owned descriptor; close failure has unknown outcome.
        let closed = file.close_checked()?;
        metadata(path, *device, *inode)?;
        result.map(|mut release| {
            release.source_close_ns = closed;
            release
        })
    }
}
