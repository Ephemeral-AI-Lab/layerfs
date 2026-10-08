//! Stable kernel identities and checked projection of portable attributes.
use crate::operations::Declined;
use fuser::{Errno, FileAttr, FileType, INodeNo};
use layerfs_content::object::inode_leaf::InodeKind;
use layerfs_workspace::{Refusal, ViewStat};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy, Debug)]
pub struct Identity {
    pub root: u64,
    pub uid: u32,
    pub gid: u32,
}
impl Identity {
    /// Kernel inode 1 is reserved for the canonical root. Other serials use a
    /// reversible offset, even when a committed root's serial is not itself 1.
    pub fn inode(self, serial: u64) -> Result<INodeNo, Errno> {
        if serial == 0 {
            return Err(Errno::ESTALE);
        }
        if serial == self.root {
            Ok(INodeNo(1))
        } else {
            serial.checked_add(1).map(INodeNo).ok_or(Errno::EOVERFLOW)
        }
    }
    pub fn serial(self, inode: INodeNo) -> Result<u64, Errno> {
        if inode.0 == 1 {
            return Ok(self.root);
        }
        match inode.0.checked_sub(1) {
            Some(serial) if serial != 0 && serial != self.root => Ok(serial),
            _ => Err(Errno::ESTALE),
        }
    }
    pub fn attributes(self, value: &ViewStat) -> Result<FileAttr, Errno> {
        let time = timestamp(
            value.metadata.mtime_seconds,
            value.metadata.mtime_nanoseconds,
        )?;
        let kind = match value.kind {
            InodeKind::RegularFile => FileType::RegularFile,
            InodeKind::Directory => FileType::Directory,
            InodeKind::Symlink => FileType::Symlink,
        };
        let nlink = if kind == FileType::Directory {
            // A directory's count is projected, not stored: 2 while it has a
            // name (the root always has), 0 once it is removed.
            if value.namespace_refs == 0 && value.serial != self.root {
                0
            } else {
                2
            }
        } else {
            u32::try_from(value.namespace_refs).map_err(|_| Errno::EOVERFLOW)?
        };
        let perm = u16::try_from(value.metadata.mode).map_err(|_| Errno::EOVERFLOW)?;
        Ok(FileAttr {
            ino: self.inode(value.serial)?,
            size: value.logical_len,
            blocks: value.logical_len / 512 + u64::from(value.logical_len % 512 != 0),
            atime: time,
            mtime: time,
            ctime: time,
            crtime: time,
            kind,
            perm,
            nlink,
            uid: self.uid,
            gid: self.gid,
            rdev: 0,
            blksize: 4096,
            flags: 0,
        })
    }
}
fn timestamp(seconds: i64, nanos: u32) -> Result<SystemTime, Errno> {
    if nanos >= 1_000_000_000 {
        return Err(Errno::EOVERFLOW);
    }
    let value = if seconds >= 0 {
        UNIX_EPOCH.checked_add(Duration::new(seconds as u64, nanos))
    } else if nanos == 0 {
        UNIX_EPOCH.checked_sub(Duration::from_secs(seconds.unsigned_abs()))
    } else {
        UNIX_EPOCH.checked_sub(Duration::new(
            seconds.unsigned_abs() - 1,
            1_000_000_000 - nanos,
        ))
    };
    value.ok_or(Errno::EOVERFLOW)
}
pub(crate) fn refusal(value: Refusal) -> Errno {
    match value {
        Refusal::Exists => Errno::EEXIST,
        Refusal::Missing => Errno::ENOENT,
        Refusal::NotDirectory => Errno::ENOTDIR,
        Refusal::IsDirectory => Errno::EISDIR,
        Refusal::NotEmpty => Errno::ENOTEMPTY,
        Refusal::NotPermitted => Errno::EPERM,
        Refusal::Unsupported => Errno::EOPNOTSUPP,
        Refusal::Invalid | Refusal::AncestryRequired | Refusal::AncestryMismatch => Errno::EINVAL,
        Refusal::TooManyLinks => Errno::EMLINK,
        Refusal::TooLarge => Errno::EFBIG,
    }
}
/// A definite no-effect answer of a mutation as the errno the caller sees.
pub(crate) fn declined(value: Declined) -> Errno {
    match value {
        Declined::Refused(reason) => refusal(reason),
        Declined::NoSpace => Errno::ENOSPC,
        Declined::Contended => Errno::EAGAIN,
        Declined::TargetTooLong => Errno::ENAMETOOLONG,
    }
}
