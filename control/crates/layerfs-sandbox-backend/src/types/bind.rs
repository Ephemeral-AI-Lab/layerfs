//! A host directory a Sandbox's private root binds (#535 §6.3): a sqlite
//! Store's directory is one, read-write at its own path; a PostgreSQL
//! Store's Sandbox has none.
use crate::RuntimeError;
use std::path::{Component, Path, PathBuf};

/// One host directory, seen inside a Sandbox at `guest`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Bind {
    /// The host directory, absolute.
    pub host: PathBuf,
    /// Where the Sandbox sees it, absolute.
    pub guest: PathBuf,
    /// Writable; otherwise bound read-only.
    pub read_write: bool,
}

impl Bind {
    /// A sqlite Store's bind: the directory of the exact Store file `store`,
    /// read-write at its own path, so SQLite's WAL and the writers'
    /// directory lock see the same inode as every other opener on this
    /// kernel. Refuses a relative path, `.` or `..`, or a file at `/`.
    pub fn store(store: &Path) -> Result<Self, RuntimeError> {
        let plain = store.is_absolute()
            && !(store.components()).any(|c| matches!(c, Component::ParentDir | Component::CurDir));
        let directory = (store.parent())
            .filter(|parent| plain && !parent.as_os_str().is_empty())
            .ok_or(RuntimeError::Protocol("namespaces Store file"))?;
        Ok(Self {
            host: directory.to_owned(),
            guest: directory.to_owned(),
            read_write: true,
        })
    }
}
