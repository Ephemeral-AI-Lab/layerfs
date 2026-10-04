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
