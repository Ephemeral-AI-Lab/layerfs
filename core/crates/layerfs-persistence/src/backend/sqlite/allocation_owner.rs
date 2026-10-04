//! Profile-specific allocation lifetime with original-file identity custody.
use super::{allocation::AllocationFile, connection::AllocationRelease};
use crate::backend::records::BackendError;
use std::{
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};

pub(crate) struct AllocationOwner {
    path: PathBuf,
    device: u64,
    inode: u64,
    retained: Option<AllocationFile>,
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
        let path = if path.is_absolute() {
            path.to_owned()
        } else {
            std::env::current_dir()
                .map_err(|e| BackendError::Filesystem(e.kind()))?
                .join(path)
        };
        let m = std::fs::symlink_metadata(&path).map_err(|e| BackendError::Filesystem(e.kind()))?;
        metadata(&path, m.dev(), m.ino())?;
        Ok(Self {
            retained: if temporary {
                None
            } else {
                Some(AllocationFile::open(&path)?)
            },
            path,
            device: m.dev(),
            inode: m.ino(),
        })
    }
    pub(crate) fn before_pack(&self, capacity: usize) -> Result<(u64, u64), BackendError> {
        if capacity > layerfs_storage::policy::SINGLETON_PACK_LIMIT {
            return Err(BackendError::Capacity);
        }
        const MIB: u64 = 1 << 20;
        let before = metadata(&self.path, self.device, self.inode)?;
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
        let (_, close) =
            self.with_file(|file| file.preallocate_checked(self.device, self.inode, amount))?;
        Ok((amount, close))
    }
    fn with_file<T>(
        &self,
        operation: impl FnOnce(&AllocationFile) -> Result<T, BackendError>,
    ) -> Result<(T, u64), BackendError> {
        metadata(&self.path, self.device, self.inode)?;
        let (result, close) = if let Some(file) = &self.retained {
            (operation(file), 0)
        } else {
            let file = AllocationFile::open(&self.path)?;
            let result = operation(&file);
            // Always close the temporary descriptor, including on operation failure.
            let close = file.close_checked()?;
            (result, close)
        };
        let custody = metadata(&self.path, self.device, self.inode);
        if result.as_ref().err() == Some(&BackendError::Unknown) {
            return Err(BackendError::Unknown);
        }
        custody?;
        result.map(|value| (value, close))
    }
    pub(crate) fn release(&self) -> Result<AllocationRelease, BackendError> {
        let before = metadata(&self.path, self.device, self.inode)?;
        let allocated = before
            .blocks()
            .checked_mul(512)
            .ok_or(BackendError::Capacity)?;
        if self.retained.is_none() && allocated <= before.len() {
            return Ok(AllocationRelease {
                before: allocated,
                after: allocated,
                source: None,
                transfer_ns: 0,
                scratch_close_ns: 0,
                source_close_ns: 0,
            });
        }
        let (mut release, closed) =
            self.with_file(|file| file.release_checked(self.device, self.inode))?;
        release.source_close_ns = closed;
        Ok(release)
    }
}
