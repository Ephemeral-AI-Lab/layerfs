//! Kernel cache invalidation for one mount, after an install whose names,
//! bytes, links, attributes or serials differ from what the Workspace showed.
//!
//! Each call is one notification write. An entry or inode the kernel does not
//! cache answers ENOENT, which fuser reports as success; nothing is retried.
use crate::attributes::Identity;
use fuser::{Errno, Notifier};
use std::{ffi::OsStr, io};

/// Sends entry and inode invalidations to one mounted connection.
#[derive(Clone)]
pub struct Invalidator {
    notifier: Notifier,
    identity: Identity,
}

fn errno(errno: Errno) -> io::Error {
    io::Error::from_raw_os_error(errno.code())
}

impl Invalidator {
    pub(super) fn new(notifier: Notifier, identity: Identity) -> Self {
        Self { notifier, identity }
    }

    /// Drops the attributes and every cached page (or directory listing) of
    /// the inode with this serial.
    pub fn inode(&self, serial: u64) -> io::Result<()> {
        let inode = self.identity.inode(serial).map_err(errno)?;
        self.notifier.inval_inode(inode, 0, 0)
    }

    /// Drops the cached binding of `name` in the directory with this serial.
    pub fn entry(&self, parent: u64, name: &str) -> io::Result<()> {
        let inode = self.identity.inode(parent).map_err(errno)?;
        self.notifier.inval_entry(inode, OsStr::new(name))
    }
}
