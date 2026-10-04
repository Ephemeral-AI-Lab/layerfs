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
    time::Instant,
};
static NEXT: AtomicU64 = AtomicU64::new(0);
use super::connection::AllocationRelease as Release;
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
    pub(crate) fn release_checked(&self, device: u64, inode: u64) -> Result<Release, BackendError> {
        let m = self.file.metadata().map_err(error)?;
        if m.dev() != device || m.ino() != inode || m.nlink() != 1 {
            return Err(BackendError::Integrity);
        }
        self.release()
    }
    pub(crate) fn close_checked(self) -> Result<u64, BackendError> {
        let Self { file, parent: _ } = self;
        let start = Instant::now();
        let outcome = nix::unistd::close(file);
        let wall = start.elapsed().as_nanos() as u64;
        outcome.map_err(|_| BackendError::Unknown)?;
        Ok(wall)
    }
    pub(crate) fn release(&self) -> Result<Release, BackendError> {
        let before = self.file.metadata().map_err(error)?;
        let allocated = before
            .blocks()
            .checked_mul(512)
            .ok_or(BackendError::Capacity)?;
        let source = super::connection::AllocationIdentity {
            descriptor: self.file.as_raw_fd(),
            device: before.dev(),
            inode: before.ino(),
            logical_bytes: before.len(),
        };
        if allocated <= before.len() {
            return Ok(Release {
                before: allocated,
                after: allocated,
                source: Some(source),
                transfer_ns: 0,
                scratch_close_ns: 0,
                source_close_ns: 0,
            });
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
        let transfer = Instant::now();
        let result = fcntl(&self.file, FcntlArg::F_TRANSFEREXTENTS(sink.as_raw_fd()));
        let transfer_ns = transfer.elapsed().as_nanos() as u64;
        // This exclusively created scratch contains no logical Store data, even
        // on an unsuccessful transfer. Cleanup has its own definite outcome.
        let close = Instant::now();
        let closed = nix::unistd::close(sink);
        let scratch_close_ns = close.elapsed().as_nanos() as u64;
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
        Ok(Release {
            before: allocated,
            after: after
                .blocks()
                .checked_mul(512)
                .ok_or(BackendError::Capacity)?,
            source: Some(source),
            transfer_ns,
            scratch_close_ns,
            source_close_ns: 0,
        })
    }
}
