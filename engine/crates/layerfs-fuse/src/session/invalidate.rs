//! Kernel cache invalidation for one mount, after an install whose names,
//! bytes, links, attributes or serials differ from what the Workspace showed.
//!
//! Each call is one notification write. An entry or inode the kernel does not
//! cache answers ENOENT, which fuser reports as success; nothing is retried.
//!
//! `Deferred` sends the one invalidation a WRITE reply can owe (see
//! `request/mutate.rs`) from the mount's page thread, after that reply.
use crate::attributes::Identity;
use fuser::{Errno, Notifier};
use std::{
    ffi::OsStr,
    io,
    sync::{mpsc, Arc, OnceLock},
    thread,
};

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

/// Inodes whose cached pages a WRITE reply left wrong. An `O_APPEND` write
/// the daemon placed at the Workspace's end was copied by the kernel into its
/// page cache at the end it caches; when the two differ, the inode's pages are
/// invalidated once the reply is sent. An invalidation waits for pages a
/// writer keeps locked until its request is answered, so it never runs on a
/// receive loop: one page thread per mount sends them in order. The thread
/// ends when the mount's callbacks are dropped.
#[derive(Clone, Default)]
pub(crate) struct Deferred(Arc<OnceLock<mpsc::Sender<u64>>>);

impl Deferred {
    /// Queues the inode's invalidation. Before the page thread runs nothing
    /// was replied, so nothing can be cached wrongly.
    pub(crate) fn pages(&self, serial: u64) {
        if let Some(pages) = self.0.get() {
            let _ = pages.send(serial);
        }
    }

    /// Starts the page thread over the serving connection's invalidator.
    pub(super) fn serve(&self, invalidator: Invalidator, name: String) -> io::Result<()> {
        let (pages, serials) = mpsc::channel();
        thread::Builder::new().name(name).spawn(move || {
            for serial in serials {
                let _ = invalidator.inode(serial);
            }
        })?;
        let _ = self.0.set(pages);
        Ok(())
    }
}
