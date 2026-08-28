use super::state::{LayerFuse, O_TRUNC, TTL};
use super::translate::{errno, file_type, timestamp};
use crate::workspace::{MountedHandleId, MountedNodeId, MAX_REQUEST_BYTES};
use fuser::{
    AccessFlags, BsdFileFlags, FileHandle, Filesystem, FopenFlags, Generation, INodeNo,
    KernelConfig, LockOwner, OpenFlags, RenameFlags, ReplyAttr, ReplyCreate, ReplyData,
    ReplyDirectory, ReplyEmpty, ReplyEntry, ReplyOpen, ReplyStatfs, ReplyWrite, Request, TimeOrNow,
    WriteFlags,
};
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::time::{Duration, SystemTime};

impl Filesystem for LayerFuse {
    lifecycle_callbacks!();
    namespace_callbacks!();
    file_callbacks!();
    directory_callbacks!();
    query_callbacks!();
}
