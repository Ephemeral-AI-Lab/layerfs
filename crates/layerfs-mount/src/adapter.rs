use crate::handles::Handles;
use crate::inode_table::InodeTable;
use fuser::{FileAttr, FileHandle, FileType, INodeNo, ReplyEmpty};
use layerfs_sdk::{Attr, Kind, NodeId, StorageError, Workspace};
use std::sync::{Arc, Mutex};
use std::time::{Duration, UNIX_EPOCH};

pub(crate) const TTL: Duration = Duration::from_secs(1);
pub(crate) const O_TRUNC: i32 = 0o1000;

pub struct LayerFs {
    workspace: Arc<Mutex<Workspace>>,
    pub(crate) inodes: InodeTable,
    pub(crate) handles: Handles,
    pub(crate) uid: u32,
    pub(crate) gid: u32,
}

impl LayerFs {
    pub fn new(workspace: Arc<Mutex<Workspace>>, uid: u32, gid: u32) -> Self {
        Self {
            workspace,
            inodes: InodeTable,
            handles: Handles,
            uid,
            gid,
        }
    }

    pub(crate) fn lock(
        &self,
    ) -> std::result::Result<std::sync::MutexGuard<'_, Workspace>, fuser::Errno> {
        self.workspace.lock().map_err(|_| fuser::Errno::EIO)
    }

    pub(crate) fn node(&self, ino: INodeNo) -> std::result::Result<NodeId, fuser::Errno> {
        self.inodes.node(ino.0).ok_or(fuser::Errno::ENOENT)
    }

    pub(crate) fn attr(&self, attr: Attr) -> std::result::Result<FileAttr, fuser::Errno> {
        let ino = self.inodes.kernel(attr.node);
        let time = if attr.mtime_seconds >= 0 {
            UNIX_EPOCH + Duration::new(attr.mtime_seconds as u64, attr.mtime_nanoseconds)
        } else {
            UNIX_EPOCH
        };
        Ok(FileAttr {
            ino: INodeNo(ino),
            size: attr.size,
            blocks: attr.size.div_ceil(512),
            atime: time,
            mtime: time,
            ctime: time,
            crtime: UNIX_EPOCH,
            kind: file_type(attr.kind),
            perm: attr.mode as u16,
            nlink: attr.links,
            uid: self.uid,
            gid: self.gid,
            rdev: 0,
            blksize: 4096,
            flags: 0,
        })
    }

    pub(crate) fn open_handle(
        &self,
        node: NodeId,
        truncate: bool,
    ) -> std::result::Result<u64, fuser::Errno> {
        self.lock()?.pin(node, truncate).map_err(errno)?;
        Ok(self.handles.insert(node))
    }

    pub(crate) fn handle(&self, handle: FileHandle) -> std::result::Result<NodeId, fuser::Errno> {
        self.handles.get(handle.0).ok_or(fuser::Errno::EBADF)
    }
}

pub(crate) fn empty_reply(result: std::result::Result<(), fuser::Errno>, reply: ReplyEmpty) {
    match result {
        Ok(()) => reply.ok(),
        Err(error) => reply.error(error),
    }
}

pub(crate) fn file_type(kind: Kind) -> FileType {
    match kind {
        Kind::File => FileType::RegularFile,
        Kind::Directory => FileType::Directory,
        Kind::Symlink => FileType::Symlink,
    }
}

pub(crate) fn errno(error: StorageError) -> fuser::Errno {
    match error {
        StorageError::NotFound(_) => fuser::Errno::ENOENT,
        StorageError::InvalidInput("directory not empty") => fuser::Errno::ENOTEMPTY,
        StorageError::InvalidInput("name exists") => fuser::Errno::EEXIST,
        StorageError::InvalidInput("workspace spool limit") => fuser::Errno::ENOSPC,
        StorageError::InvalidInput(_) => fuser::Errno::EINVAL,
        _ => fuser::Errno::EIO,
    }
}
