use crate::handles::Handles;
use crate::inode_table::InodeTable;
use crate::{Attr, Kind, NodeId, PortError, SharedPort};
use fuser::{FileAttr, FileHandle, FileType, INodeNo, ReplyEmpty};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, UNIX_EPOCH};

pub(crate) type DirectoryEpoch = (u64, u64);
pub(crate) type DirectoryEntries = Arc<Vec<(NodeId, Kind, Vec<u8>)>>;
pub(crate) type DirectoryEntriesPlus = Arc<Vec<(Attr, Vec<u8>)>>;

pub(crate) const TTL: Duration = Duration::from_secs(1);
pub(crate) const O_TRUNC: i32 = 0o1000;
pub(crate) const O_ACCMODE: i32 = 0o3;
pub(crate) const O_WRONLY: i32 = 0o1;
pub(crate) const O_RDWR: i32 = 0o2;

pub struct LayerFs {
    pub(crate) port: SharedPort,
    pub(crate) inodes: InodeTable,
    pub(crate) handles: Handles,
    pub(crate) directory_entries: Mutex<Option<(u64, DirectoryEpoch, DirectoryEntries)>>,
    pub(crate) directory_entries_plus: Mutex<Option<(u64, DirectoryEpoch, DirectoryEntriesPlus)>>,
    pub(crate) directory_cache_enabled: bool,
    pub(crate) directory_stats: [AtomicU64; 4],
    pub(crate) uid: u32,
    pub(crate) gid: u32,
}

impl LayerFs {
    pub fn new(port: SharedPort, uid: u32, gid: u32) -> Self {
        Self {
            port,
            inodes: InodeTable,
            handles: Handles::default(),
            directory_entries: Mutex::new(None),
            directory_entries_plus: Mutex::new(None),
            directory_cache_enabled: std::env::var("LAYERFS_EXPERIMENT_DIRECTORY_PAGES").as_deref() == Ok("1"),
            directory_stats: Default::default(),
            uid,
            gid,
        }
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
        writable: bool,
    ) -> std::result::Result<u64, fuser::Errno> {
        self.port.pin(node, truncate, writable).map_err(errno)?;
        Ok(self.handles.insert(node, writable))
    }

    pub(crate) fn handle(&self, handle: FileHandle) -> std::result::Result<NodeId, fuser::Errno> {
        self.handles
            .get(handle.0)
            .map(|handle| handle.node)
            .ok_or(fuser::Errno::EBADF)
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

pub(crate) fn errno(error: PortError) -> fuser::Errno {
    match error {
        PortError::NotFound => fuser::Errno::ENOENT,
        PortError::NotEmpty => fuser::Errno::ENOTEMPTY,
        PortError::Exists => fuser::Errno::EEXIST,
        PortError::NoSpace => fuser::Errno::ENOSPC,
        PortError::ReadOnly => fuser::Errno::EROFS,
        PortError::Busy => fuser::Errno::EBUSY,
        PortError::Invalid => fuser::Errno::EINVAL,
        PortError::Io => fuser::Errno::EIO,
    }
}

impl Drop for LayerFs {
    fn drop(&mut self) {
        let [requests, loads, entries, hits] = self.directory_stats.each_ref().map(|n| n.load(Ordering::Relaxed));
        eprintln!("experiment_directory_pages enabled={} requests={} loads={} loaded_entries={} hits={}", self.directory_cache_enabled, requests, loads, entries, hits);
    }
}
