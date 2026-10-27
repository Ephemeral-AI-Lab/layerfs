//! Last-owner file deletion after checked connection close and terminal fencing.
use super::workspaces::Workspaces;
use std::{
    io,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Weak,
    },
};

pub struct Backing {
    pub path: PathBuf,
    file: Option<std::fs::File>,
    namespace: i64,
    incarnation: [u8; 32],
    device: u64,
    inode: u64,
    pool: Weak<Workspaces>,
    terminal: AtomicBool,
    closed: AtomicBool,
}
impl Backing {
    pub fn new(
        path: PathBuf,
        namespace: i64,
        incarnation: [u8; 32],
        pool: Weak<Workspaces>,
    ) -> io::Result<Self> {
        let file = std::fs::File::open(&path)?;
        let metadata = file.metadata()?;
        #[cfg(unix)]
        let named = std::fs::symlink_metadata(&path)?;
        #[cfg(unix)]
        let (device, inode, links) = {
            use std::os::unix::fs::MetadataExt;
            if !named.is_file() || (named.dev(), named.ino()) != (metadata.dev(), metadata.ino()) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "overlay path identity",
                ));
            }
            (metadata.dev(), metadata.ino(), metadata.nlink())
        };
        #[cfg(not(unix))]
        let (device, inode, links) = (0, 0, 0);
        if !metadata.is_file() || links != 1 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "overlay file identity",
            ));
        }
        Ok(Self {
            path,
            file: Some(file),
            namespace,
            incarnation,
            device,
            inode,
            pool,
            terminal: AtomicBool::new(false),
            closed: AtomicBool::new(false),
        })
    }
    pub fn terminal(&self) {
        self.terminal.store(true, Ordering::Release);
    }
    pub fn closed(&self) {
        self.closed.store(true, Ordering::Release);
    }
    fn delete(&self) -> io::Result<()> {
        let metadata = std::fs::symlink_metadata(&self.path)?;
        let original = self
            .file
            .as_ref()
            .expect("retained identity file")
            .metadata()?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if !metadata.is_file()
                || metadata.nlink() != 1
                || original.nlink() != 1
                || (original.dev(), original.ino()) != (self.device, self.inode)
                || (metadata.dev(), metadata.ino()) != (self.device, self.inode)
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "overlay deletion identity",
                ));
            }
        }
        #[cfg(not(unix))]
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "overlay deletion platform",
        ));
        #[cfg(unix)]
        std::fs::remove_file(&self.path)
    }
}
impl Drop for Backing {
    fn drop(&mut self) {
        // A failed or unestablished close/terminal fence never authorizes unlink.
        if !self.terminal.load(Ordering::Acquire) || !self.closed.load(Ordering::Acquire) {
            return;
        }
        let deleted = self.delete();
        let file = self.file.take().expect("retained identity file");
        let result = match deleted {
            Ok(()) => {
                drop(file);
                Ok(())
            }
            Err(cause) => Err((cause, file)),
        };
        if let Some(pool) = self.pool.upgrade() {
            pool.deleted(self.namespace, self.incarnation, result);
        }
    }
}
