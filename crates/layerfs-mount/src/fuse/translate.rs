use crate::workspace::{MountedError, MountedFileType, MountedNodeId, ROOT_NODE};
use fuser::FileType;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub(super) fn file_type(kind: MountedFileType) -> FileType {
    match kind {
        MountedFileType::RegularFile => FileType::RegularFile,
        MountedFileType::Directory => FileType::Directory,
        MountedFileType::Symlink => FileType::Symlink,
    }
}

pub(super) fn errno(error: MountedError) -> fuser::Errno {
    match error {
        MountedError::NotFound => fuser::Errno::ENOENT,
        MountedError::AlreadyExists => fuser::Errno::EEXIST,
        MountedError::NotDirectory => fuser::Errno::ENOTDIR,
        MountedError::IsDirectory => fuser::Errno::EISDIR,
        MountedError::NotEmpty => fuser::Errno::ENOTEMPTY,
        MountedError::InvalidName | MountedError::InvalidRange => fuser::Errno::EINVAL,
        MountedError::PermissionDenied => fuser::Errno::EACCES,
        MountedError::ReadOnly => fuser::Errno::EROFS,
        MountedError::NoSpace => fuser::Errno::ENOSPC,
        MountedError::TooManyOpenFiles => fuser::Errno::EMFILE,
        MountedError::ResourceExhausted => fuser::Errno::ENOSPC,
        MountedError::Busy => fuser::Errno::EBUSY,
        MountedError::StaleHandle => fuser::Errno::ESTALE,
        MountedError::InvalidHandle => fuser::Errno::EBADF,
        MountedError::Conflict
        | MountedError::CommittedCleanup
        | MountedError::Corrupt
        | MountedError::Indeterminate
        | MountedError::Startup(_, _) => fuser::Errno::EIO,
        MountedError::Unsupported => fuser::Errno::EOPNOTSUPP,
        MountedError::Io(_) => fuser::Errno::EIO,
    }
}

pub(super) fn system_time(seconds: i64, nanoseconds: u32) -> SystemTime {
    if seconds >= 0 {
        UNIX_EPOCH + Duration::new(seconds as u64, nanoseconds)
    } else {
        UNIX_EPOCH
            .checked_sub(Duration::new(seconds.unsigned_abs(), nanoseconds))
            .unwrap_or(UNIX_EPOCH)
    }
}

pub(super) fn timestamp(time: SystemTime) -> Option<(i64, u32)> {
    match time.duration_since(UNIX_EPOCH) {
        Ok(duration) => Some((
            i64::try_from(duration.as_secs()).ok()?,
            duration.subsec_nanos(),
        )),
        Err(error) => Some((
            -i64::try_from(error.duration().as_secs()).ok()?,
            error.duration().subsec_nanos(),
        )),
    }
}

pub fn root_node() -> MountedNodeId {
    ROOT_NODE
}
