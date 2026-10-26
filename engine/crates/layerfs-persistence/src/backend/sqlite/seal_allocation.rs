//! One-shot release of unused macOS extents after the sole SQLite owner closes.
use layerfs_storage::port::PersistenceError;
use nix::fcntl::{fcntl, FcntlArg};
use std::{
    fs::{self, File, Metadata, OpenOptions},
    os::{
        fd::AsRawFd,
        unix::fs::{MetadataExt, OpenOptionsExt},
    },
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);

pub(super) struct Identity {
    device: u64,
    inode: u64,
}
impl Identity {
    pub(super) fn capture(path: &Path) -> Result<Self, PersistenceError> {
        let metadata = fs::symlink_metadata(path).map_err(|error| refused("identity", error))?;
        if !metadata.is_file() || metadata.nlink() != 1 {
            return Err(refused("identity", "exclusive regular Store required"));
        }
        Ok(Self::from(&metadata))
    }
    fn from(metadata: &Metadata) -> Self {
        Self {
            device: metadata.dev(),
            inode: metadata.ino(),
        }
    }
    fn matches(&self, metadata: &Metadata) -> bool {
        metadata.is_file()
            && metadata.nlink() == 1
            && metadata.dev() == self.device
            && metadata.ino() == self.inode
    }
    fn check(&self, path: &Path, file: &File) -> Result<Metadata, PersistenceError> {
        let descriptor = file
            .metadata()
            .map_err(|error| refused("descriptor", error))?;
        let named = fs::symlink_metadata(path).map_err(|error| refused("path", error))?;
        if !self.matches(&descriptor) || !self.matches(&named) || descriptor.len() != named.len() {
            return Err(refused("identity", "Store file changed during seal"));
        }
        Ok(descriptor)
    }
}

fn refused(action: &str, error: impl std::fmt::Display) -> PersistenceError {
    PersistenceError::Refused {
        status: format!("seal allocation {action}: {error}"),
    }
}

pub(super) fn release(path: &Path, identity: Identity) -> Result<u64, PersistenceError> {
    let source = OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(nix::libc::O_NOFOLLOW)
        .open(path)
        .map_err(|error| refused("open", error))?;
    let result = (|| {
        let before = identity.check(path, &source)?;
        let allocated = before
            .blocks()
            .checked_mul(512)
            .ok_or_else(|| refused("size", "overflow"))?;
        if allocated > before.len() {
            transfer(path, &source, &identity, before.len())?;
        }
        let after = identity
            .check(path, &source)
            .map_err(|_| PersistenceError::Uncertain)?;
        if after.len() != before.len() || after.blocks() > before.blocks() {
            return Err(PersistenceError::Uncertain);
        }
        Ok(after.len())
    })();
    if nix::unistd::close(source).is_err() {
        return Err(PersistenceError::Uncertain);
    }
    result
}

fn transfer(
    path: &Path,
    source: &File,
    identity: &Identity,
    length: u64,
) -> Result<(), PersistenceError> {
    let parent = path
        .parent()
        .ok_or_else(|| refused("parent", "missing parent"))?;
    let temporary = parent.join(format!(
        ".layerfs-allocation-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let target = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temporary)
        .map_err(|error| refused("temporary creation", error))?;
    let target_identity = match target.metadata() {
        Ok(metadata) => Identity::from(&metadata),
        Err(_) => {
            let _ = nix::unistd::close(target);
            return Err(PersistenceError::Uncertain);
        }
    };
    let transferred = fcntl(source, FcntlArg::F_TRANSFEREXTENTS(target.as_raw_fd()));
    if nix::unistd::close(target).is_err() {
        return Err(PersistenceError::Uncertain);
    }
    let known = match transferred {
        Ok(_) => Ok(()),
        Err(
            error @ (nix::errno::Errno::ENOTSUP
            | nix::errno::Errno::EXDEV
            | nix::errno::Errno::EPERM
            | nix::errno::Errno::EINVAL),
        ) => Err(refused("extent transfer", error)),
        Err(_) => return Err(PersistenceError::Uncertain),
    };
    // Unknown outcomes retain the exclusively created sibling. No cleanup is
    // inferred from an error or attempted again after a failed close/unlink.
    let original = identity
        .check(path, source)
        .map_err(|_| PersistenceError::Uncertain)?;
    let sibling = fs::symlink_metadata(&temporary).map_err(|_| PersistenceError::Uncertain)?;
    if original.len() != length || !target_identity.matches(&sibling) {
        return Err(PersistenceError::Uncertain);
    }
    fs::remove_file(&temporary).map_err(|_| PersistenceError::Uncertain)?;
    known
}
