//! Release only physical extents beyond logical EOF after a complete checkpoint.
use crate::backend::records::BackendError;
use nix::fcntl::{fcntl, FcntlArg};
use std::{
    fs::{File, OpenOptions},
    os::{
        fd::AsRawFd,
        unix::fs::{MetadataExt, OpenOptionsExt},
    },
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
pub(crate) struct AllocationFile {
    file: File,
    parent: PathBuf,
}
fn error(e: std::io::Error) -> BackendError {
    BackendError::Filesystem(e.kind())
}
impl AllocationFile {
    pub(crate) fn open(path: &Path) -> Result<Self, BackendError> {
        let path = if path.is_absolute() {
            path.to_owned()
        } else {
            std::env::current_dir().map_err(error)?.join(path)
        };
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(nix::libc::O_NOFOLLOW)
            .open(&path)
            .map_err(error)?;
        if !file.metadata().map_err(error)?.is_file() {
            return Err(BackendError::Integrity);
        }
        Ok(Self {
            file,
            parent: path.parent().ok_or(BackendError::Integrity)?.to_owned(),
        })
    }
    pub(crate) fn release(&self) -> Result<(u64, u64), BackendError> {
        let before = self.file.metadata().map_err(error)?;
        let allocated = before
            .blocks()
            .checked_mul(512)
            .ok_or(BackendError::Capacity)?;
        if allocated <= before.len() {
            return Ok((allocated, allocated));
        }
        // No payload is copied. The kernel transfers only unused extra extents;
        // this operation never changes the source's logical length or bytes.
        let path = self.parent.join(format!(
            ".layerfs-allocation-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let sink = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
            .map_err(error)?;
        let result = fcntl(&self.file, FcntlArg::F_TRANSFEREXTENTS(sink.as_raw_fd()));
        // This exclusively created scratch contains no logical Store data, even
        // on an unsuccessful transfer. Cleanup has its own definite outcome.
        let closed = nix::unistd::close(sink);
        let cleanup = std::fs::remove_file(&path);
        if closed.is_err() {
            return Err(BackendError::Unknown);
        }
        if let Err(errno) = result {
            let outcome = match errno {
                nix::errno::Errno::ENOTSUP
                | nix::errno::Errno::EXDEV
                | nix::errno::Errno::EPERM
                | nix::errno::Errno::EINVAL => {
                    BackendError::Filesystem(std::io::ErrorKind::Unsupported)
                }
                _ => BackendError::Unknown,
            };
            if outcome == BackendError::Unknown {
                return Err(outcome);
            }
            cleanup.map_err(error)?;
            return Err(outcome);
        }
        cleanup.map_err(error)?;
        let after = self.file.metadata().map_err(|_| BackendError::Unknown)?;
        if after.len() != before.len() {
            return Err(BackendError::Busy);
        }
        Ok((
            allocated,
            after
                .blocks()
                .checked_mul(512)
                .ok_or(BackendError::Capacity)?,
        ))
    }
}
