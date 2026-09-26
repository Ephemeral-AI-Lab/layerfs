//! Checked conversion of portable Workspace results into kernel replies.
use fuser::{Errno, FileAttr, FileType, INodeNo};
use layerfs_workspace::{NodeAttributes, NodeKind, ServiceCode, WorkspaceError};
use std::time::{Duration, UNIX_EPOCH};

pub(crate) fn errno(error: WorkspaceError) -> Errno {
    if std::env::var_os("LAYERFS_FUSE_ERROR_DIAGNOSTIC").is_some() {
        eprintln!("LFS_FUSE_ERROR v=1 error={error:?}");
    }
    match error {
        WorkspaceError::InvalidInput => Errno::EINVAL,
        WorkspaceError::StaleStamp => Errno::ESTALE,
        WorkspaceError::Capacity => Errno::ENOSPC,
        WorkspaceError::Busy => Errno::EBUSY,
        WorkspaceError::Closed => Errno::ENODEV,
        WorkspaceError::NotFound => Errno::ENOENT,
        WorkspaceError::Exists => Errno::EEXIST,
        WorkspaceError::NotDirectory => Errno::ENOTDIR,
        WorkspaceError::IsDirectory => Errno::EISDIR,
        WorkspaceError::NotEmpty => Errno::ENOTEMPTY,
        WorkspaceError::WrongKind => Errno::EINVAL,
        WorkspaceError::BadHandle => Errno::EBADF,
        WorkspaceError::ReadOnly => Errno::EROFS,
        WorkspaceError::Denied => Errno::EACCES,
        WorkspaceError::Unsupported => Errno::EOPNOTSUPP,
        WorkspaceError::Deadline => Errno::ETIMEDOUT,
        WorkspaceError::Backing(failure) => match failure.kind {
            std::io::ErrorKind::StorageFull => Errno::ENOSPC,
            std::io::ErrorKind::TimedOut => Errno::ETIMEDOUT,
            std::io::ErrorKind::PermissionDenied => Errno::EACCES,
            std::io::ErrorKind::Unsupported => Errno::EOPNOTSUPP,
            std::io::ErrorKind::Interrupted => Errno::EINTR,
            _ => Errno::EIO,
        },
        WorkspaceError::Service(failure) => match failure.code {
            ServiceCode::InvalidInput => Errno::EINVAL,
            ServiceCode::Unsupported => Errno::EOPNOTSUPP,
            ServiceCode::Denied => Errno::EACCES,
            ServiceCode::Capacity => Errno::ENOSPC,
            ServiceCode::Ownership | ServiceCode::Busy => Errno::EBUSY,
            ServiceCode::PathNotFound => Errno::ENOENT,
            ServiceCode::Deadline => Errno::ETIMEDOUT,
            _ => Errno::EIO,
        },
        WorkspaceError::Io
        | WorkspaceError::Stage(_)
        | WorkspaceError::Commit(_)
        | WorkspaceError::Coherence(_) => Errno::EIO,
    }
}

pub(crate) fn kind(value: NodeKind) -> FileType {
    match value {
        NodeKind::File => FileType::RegularFile,
        NodeKind::Directory => FileType::Directory,
        NodeKind::Symlink => FileType::Symlink,
    }
}

// Swap the canonical root and serial 1; this is a reversible, allocation-free
// mapping, including roots whose serial is not 1. Canonical serials are positive.
pub(crate) fn inode(serial: u64, root: u64) -> INodeNo {
    INodeNo(if serial == root {
        1
    } else if serial == 1 {
        root
    } else {
        serial
    })
}

pub(crate) fn serial(ino: INodeNo, root: u64) -> u64 {
    inode(ino.0, root).0
}

pub(crate) fn attributes(value: NodeAttributes, root: u64) -> Result<FileAttr, Errno> {
    if value.mtime_nanoseconds >= 1_000_000_000 {
        return Err(Errno::EIO);
    }
    let seconds = Duration::from_secs(value.mtime_seconds.unsigned_abs());
    let seconds = if value.mtime_seconds < 0 {
        UNIX_EPOCH.checked_sub(seconds)
    } else {
        UNIX_EPOCH.checked_add(seconds)
    };
    let mtime = seconds
        .and_then(|time| time.checked_add(Duration::from_nanos(u64::from(value.mtime_nanoseconds))))
        .ok_or(Errno::EOVERFLOW)?;
    let nlink = match value.kind {
        NodeKind::Directory => 2,
        NodeKind::Symlink => 1,
        NodeKind::File => u32::try_from(value.references).map_err(|_| Errno::EOVERFLOW)?,
    };
    Ok(FileAttr {
        ino: inode(value.serial, root),
        size: value.size,
        blocks: value.size / 512 + u64::from(value.size % 512 != 0),
        atime: mtime,
        mtime,
        ctime: mtime,
        crtime: UNIX_EPOCH,
        kind: kind(value.kind),
        perm: u16::try_from(value.mode).map_err(|_| Errno::EIO)?,
        nlink,
        uid: value.uid,
        gid: value.gid,
        rdev: 0,
        blksize: 4096,
        flags: 0,
    })
}
