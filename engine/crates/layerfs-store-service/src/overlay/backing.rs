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
    // The deletion identity check reads these on unix and Windows; the
    // remaining platforms refuse deletion before reaching them.
    #[cfg_attr(not(any(unix, target_os = "windows")), allow(dead_code))]
    device: u64,
    #[cfg_attr(not(any(unix, target_os = "windows")), allow(dead_code))]
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
        #[cfg(target_os = "windows")]
        let (device, inode, links) = {
            // The audited probe in the overlay engine reads the entry's real
            // volume, file index and link count.
            let identity = layerfs_overlay::windows_handle_identity(&file).map_err(|error| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("overlay file identity: {error}"),
                )
            })?;
            (identity.device, identity.index, identity.links)
        };
        #[cfg(not(any(unix, target_os = "windows")))]
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
        #[cfg(unix)]
        {
            let metadata = std::fs::symlink_metadata(&self.path)?;
            let original = self
                .file
                .as_ref()
                .expect("retained identity file")
                .metadata()?;
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
            std::fs::remove_file(&self.path)
        }
        #[cfg(target_os = "windows")]
        {
            // The same discipline through the audited Windows identity: the
            // entry at the path and the retained open handle must both still
            // be the acquisition's file (volume serial + file index, sole
            // link) before the removal. The retained handle was opened with
            // FILE_SHARE_DELETE (std's share mode), so the removal takes
            // effect when Drop closes that last handle, immediately after.
            let path_identity = layerfs_overlay::windows_path_identity(&self.path)
                .map_err(|error| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("overlay deletion identity: {error}"),
                    )
                })?;
            let retained = layerfs_overlay::windows_handle_identity(
                self.file.as_ref().expect("retained identity file"),
            )
            .map_err(|error| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("overlay deletion identity: {error}"),
                )
            })?;
            if path_identity.device != self.device
                || path_identity.index != self.inode
                || retained.device != self.device
                || retained.index != self.inode
                || path_identity.links != 1
                || retained.links != 1
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "overlay deletion identity",
                ));
            }
            std::fs::remove_file(&self.path)
        }
        #[cfg(not(any(unix, target_os = "windows")))]
        {
            let _ = &self.path;
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "overlay deletion platform",
            ))
        }
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
